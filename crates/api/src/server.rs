//! The JSON-RPC 2.0 server over the local IPC socket.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fergents_core::{CoreError, Kernel};
use fergents_events::{now_ms, topic_matches, Subscription};
use fergents_protocol::{
    BrainArchiveParams, BrainAttachParams, BrainCloseParams, BrainCreateParams, BrainDeleteParams,
    BrainDetachParams, BrainDuplicateParams, BrainExportParams, BrainGetParams, BrainImportParams,
    BrainListParams, BrainMoveParams, BrainOpenParams, BrainReindexParams, BrainTemplateSaveParams,
    BrainUnarchiveParams, BrainUpdateParams, DailyOpenParams, DocWriteParams, ErrorCode,
    ErrorResponse, GraphAlgoParams, GraphEntitiesMergeParams, GraphNeighborsParams,
    GraphQueryParams, GraphSubgraphParams, HelloParams, JsonRpcError, Notification, PingResult,
    ReplayParams, Request, RequestId, Response, SearchQueryParams, SubscribeParams,
    TemplateCreateParams, TemplateRenderParams, UnsubscribeParams, WelcomeResult, KERNEL_VERSION,
    PROTOCOL_VERSION,
};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::sync::{watch, Mutex, RwLock};

use crate::error::ApiError;
use crate::transport::{BoxStream, IpcListener, SocketPath};

/// The handshake timeout: a client must send `hello` within this window.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

type Writer = Arc<Mutex<WriteHalf<BoxStream>>>;

/// A running kernel server.
pub struct ServerHandle {
    handle: tokio::task::JoinHandle<Result<(), ApiError>>,
}

impl ServerHandle {
    /// Spawn the server on the current Tokio runtime.
    ///
    /// The `shutdown` watch channel is flipped to `true` when a client sends
    /// `kernel.shutdown`; the caller observes the same channel to stop.
    pub fn spawn(
        kernel: Arc<Kernel>,
        socket: SocketPath,
        shutdown_tx: watch::Sender<bool>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        let handle = tokio::spawn(serve(kernel, socket, shutdown_tx, shutdown_rx));
        Self { handle }
    }

    /// Wait for the server to stop.
    pub async fn join(self) -> Result<(), ApiError> {
        self.handle
            .await
            .map_err(|e| ApiError::Io(std::io::Error::other(e)))?
    }
}

/// Run the accept loop until `shutdown_rx` flips to `true`.
pub async fn serve(
    kernel: Arc<Kernel>,
    socket: SocketPath,
    shutdown_tx: watch::Sender<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), ApiError> {
    let mut listener = IpcListener::bind(&socket)?;
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        tokio::select! {
            changed = shutdown_rx.changed() => {
                let _ = changed;
                break;
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok(stream) => {
                        let kernel = kernel.clone();
                        let shutdown_tx = shutdown_tx.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_connection(kernel, stream, shutdown_tx).await {
                                tracing::debug!(error = %err, "client connection ended");
                            }
                        });
                    }
                    Err(err) => {
                        if is_fatal_accept(&err) {
                            tracing::error!(error = %err, "accept loop failed unrecoverably");
                            let _ = kernel.announce_error(&err.to_string());
                            return Err(err);
                        }
                        tracing::warn!(error = %err, "accept failed");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Distinguish transient accept errors (retryable) from fatal ones.
///
/// Only I/O errors carry a recoverable `ErrorKind`; any other `ApiError`
/// surfaced by `accept` is treated as fatal.
fn is_fatal_accept(err: &ApiError) -> bool {
    match err {
        ApiError::Io(io) => !matches!(
            io.kind(),
            std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
        ),
        _ => true,
    }
}

async fn handle_connection(
    kernel: Arc<Kernel>,
    stream: BoxStream,
    shutdown_tx: watch::Sender<bool>,
) -> Result<(), ApiError> {
    let (read_half, write_half) = tokio::io::split(stream);
    let writer: Writer = Arc::new(Mutex::new(write_half));
    let mut reader = BufReader::new(read_half);

    let topics: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

    // --- Handshake (must be `hello`, within HANDSHAKE_TIMEOUT) ---
    let mut line = String::new();
    let read = tokio::time::timeout(HANDSHAKE_TIMEOUT, reader.read_line(&mut line)).await;
    let n = match read {
        Ok(Ok(n)) => n,
        Ok(Err(err)) => return Err(ApiError::Io(err)),
        Err(_) => {
            respond_error(
                &writer,
                None,
                ErrorCode::HandshakeRequired,
                "handshake timeout",
                None,
            )
            .await?;
            return Ok(());
        }
    };
    if n == 0 {
        return Ok(());
    }
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(request) => request,
        Err(_) => {
            respond_error(&writer, None, ErrorCode::ParseError, "parse error", None).await?;
            return Ok(());
        }
    };
    if request.method != "hello" {
        respond_error(
            &writer,
            Some(request.id),
            ErrorCode::HandshakeRequired,
            "handshake required",
            None,
        )
        .await?;
        return Ok(());
    }
    let params: HelloParams =
        match serde_json::from_value(request.params.clone().unwrap_or(Value::Null)) {
            Ok(params) => params,
            Err(_) => {
                respond_error(
                    &writer,
                    Some(request.id),
                    ErrorCode::InvalidParams,
                    "invalid hello params",
                    None,
                )
                .await?;
                return Ok(());
            }
        };
    if params.protocol_version != PROTOCOL_VERSION {
        respond_error(
            &writer,
            Some(request.id),
            ErrorCode::ProtocolVersionUnsupported,
            "protocol version unsupported",
            Some(serde_json::json!({
                "client": params.protocol_version,
                "kernel": PROTOCOL_VERSION,
            })),
        )
        .await?;
        return Ok(());
    }
    let capabilities = params.capabilities.clone().unwrap_or_default();
    let session_id = kernel.session_start(
        &params.client_kind,
        params.client_version.as_deref(),
        params.protocol_version,
        &capabilities,
    )?;

    // Start pushing subscribed events to this client.
    let subscription = kernel.bus().subscribe();
    let forwarder = tokio::spawn(forward_events(subscription, topics.clone(), writer.clone()));

    respond_result(
        &writer,
        request.id,
        &WelcomeResult {
            protocol_version: PROTOCOL_VERSION,
            kernel_version: KERNEL_VERSION.to_string(),
            session_id: session_id.clone(),
            server_capabilities: Some(vec!["events".to_string(), "health".to_string()]),
        },
    )
    .await?;

    // --- Command loop ---
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let request: Request = match serde_json::from_str(line.trim()) {
            Ok(request) => request,
            Err(_) => {
                respond_error(&writer, None, ErrorCode::ParseError, "parse error", None).await?;
                continue;
            }
        };
        let id = request.id.clone();
        match dispatch(&kernel, &request, &topics, &shutdown_tx).await {
            Ok(result) => respond_result(&writer, id, &result).await?,
            Err((code, message)) => respond_error(&writer, Some(id), code, &message, None).await?,
        }
    }

    forwarder.abort();
    kernel.session_end(&session_id)?;
    Ok(())
}

type DispatchResult = Result<Value, (ErrorCode, String)>;

async fn dispatch(
    kernel: &Arc<Kernel>,
    request: &Request,
    topics: &Arc<RwLock<HashSet<String>>>,
    shutdown_tx: &watch::Sender<bool>,
) -> DispatchResult {
    let params = request.params.clone().unwrap_or(Value::Null);
    if let Some(capability) = required_capability(&request.method) {
        kernel
            .capabilities()
            .require(capability)
            .map_err(map_capability_error)?;
    }
    match request.method.as_str() {
        "ping" => Ok(to_value(&PingResult {
            pong: true,
            ts: now_ms(),
        })),
        "system.info" => Ok(to_value(&kernel.system_info())),
        "system.health" => Ok(to_value(&kernel.health())),
        "events.subscribe" => {
            let p: SubscribeParams = from_params(&params)?;
            let mut set = topics.write().await;
            for topic in p.topics {
                set.insert(topic);
            }
            Ok(serde_json::json!({ "subscribed": set.len() }))
        }
        "events.unsubscribe" => {
            let p: UnsubscribeParams = from_params(&params)?;
            let mut set = topics.write().await;
            for topic in p.topics {
                set.remove(&topic);
            }
            Ok(serde_json::json!({ "subscribed": set.len() }))
        }
        "events.replay" => {
            let p: ReplayParams = from_params(&params)?;
            let events = kernel
                .bus()
                .replay(&p.topics, p.from_id.as_deref(), p.limit)
                .map_err(|e| (ErrorCode::InternalError, e.to_string()))?;
            Ok(serde_json::json!({ "events": events }))
        }
        "brain.list" => {
            let include_archived = if params.is_null() {
                false
            } else {
                from_params::<BrainListParams>(&params)?
                    .include_archived
                    .unwrap_or(false)
            };
            let service = kernel.brains_service().clone();
            let result = run_blocking(move || service.brains(include_archived)).await?;
            Ok(to_value(&result))
        }
        "brain.create" => {
            let p: BrainCreateParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.create(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.open" => {
            let p: BrainOpenParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.open(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.close" => {
            let p: BrainCloseParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.close(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.get" => {
            let p: BrainGetParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.get(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.update" => {
            let p: BrainUpdateParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.update(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.archive" => {
            let p: BrainArchiveParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.archive(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.unarchive" => {
            let p: BrainUnarchiveParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.unarchive(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.delete" => {
            let p: BrainDeleteParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.delete(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.duplicate" => {
            let p: BrainDuplicateParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.duplicate(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.move" => {
            let p: BrainMoveParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.mv(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.export" => {
            let p: BrainExportParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.export(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.import" => {
            let p: BrainImportParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.import(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.attach" => {
            let p: BrainAttachParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.attach(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.detach" => {
            let p: BrainDetachParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.detach(&p))
                .await
                .map(|r| to_value(&r))
        }
        "brain.reindex" => {
            let p: BrainReindexParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.reindex(&p.brain_id))
                .await
                .map(|r| to_value(&r))
        }
        "brain.template.save" => {
            let p: BrainTemplateSaveParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.save_template(&p))
                .await
                .map(|r| to_value(&r))
        }
        "daily.open" => {
            let p: DailyOpenParams = if params.is_null() {
                DailyOpenParams { date: None }
            } else {
                from_params(&params)?
            };
            let service = kernel.brains_service().clone();
            run_blocking(move || service.open_daily(&p))
                .await
                .map(|r| to_value(&r))
        }
        "template.list" => {
            let brain_id = params
                .get("brain")
                .or_else(|| params.get("brain_id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let service = kernel.brains_service().clone();
            run_blocking(move || service.list_templates(brain_id.as_deref()))
                .await
                .map(|r| to_value(&r))
        }
        "template.create" => {
            let p: TemplateCreateParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.create_template(&p))
                .await
                .map(|r| to_value(&r))
        }
        "template.render" => {
            let p: TemplateRenderParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.render_template(&p))
                .await
                .map(|r| to_value(&r))
        }
        "doc.write" => {
            let p: DocWriteParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.write_note(&p))
                .await
                .map(|r| to_value(&r))
        }
        "search.query" => {
            let p: SearchQueryParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.search_query(&p))
                .await
                .map(|r| to_value(&r))
        }
        "graph.query" => {
            let p: GraphQueryParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.graph_query(&p))
                .await
                .map(|r| to_value(&r))
        }
        "graph.algo" => {
            let p: GraphAlgoParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.graph_algo(&p))
                .await
                .map(|r| to_value(&r))
        }
        "graph.neighbors" => {
            let p: GraphNeighborsParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.graph_neighbors(&p))
                .await
                .map(|r| to_value(&r))
        }
        "graph.subgraph" => {
            let p: GraphSubgraphParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.graph_subgraph(&p))
                .await
                .map(|r| to_value(&r))
        }
        "graph.entities.merge" => {
            let p: GraphEntitiesMergeParams = from_params(&params)?;
            let service = kernel.brains_service().clone();
            run_blocking(move || service.graph_merge_entities(&p))
                .await
                .map(|r| to_value(&r))
        }
        "capability.list" => Ok(serde_json::json!({
            "capabilities": kernel.capabilities().list(),
        })),
        "capability.get" => {
            let id = capability_id_param(&params)?;
            kernel
                .capabilities()
                .view(id)
                .map(|view| to_value(&view))
                .map_err(map_capability_error)
        }
        "capability.set" => {
            let id = capability_id_param(&params)?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or((ErrorCode::InvalidParams, "missing enabled".to_string()))?;
            let auto_enable_deps = params
                .get("auto_enable_deps")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            kernel
                .capabilities()
                .set_enabled(id, enabled, auto_enable_deps)
                .map(|view| to_value(&view))
                .map_err(map_capability_error)
        }
        "capability.register" => {
            let capability: fergents_capabilities::Capability = from_params(&params)?;
            kernel
                .capabilities()
                .register(capability)
                .map(|view| to_value(&view))
                .map_err(map_capability_error)
        }
        "agent.list" => Ok(serde_json::json!({ "agents": kernel.agents().list() })),
        "agent.get" => {
            let id = agent_id_param(&params)?;
            kernel
                .agents()
                .get(id)
                .map(|definition| to_value(&definition))
                .ok_or_else(|| (ErrorCode::InvalidParams, format!("unknown agent: {id}")))
        }
        "agent.create" => {
            let markdown = params
                .get("definition")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing definition".to_string()))?;
            kernel
                .agents()
                .create(markdown)
                .map(|definition| to_value(&definition))
                .map_err(map_agent_error)
        }
        "agent.delete" => {
            let id = agent_id_param(&params)?;
            kernel
                .agents()
                .delete(id)
                .map(|_| serde_json::json!({ "deleted": id }))
                .map_err(map_agent_error)
        }
        "agent.update" => {
            let markdown = params
                .get("definition")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing definition".to_string()))?;
            kernel
                .agents()
                .update(markdown)
                .map(|definition| serde_json::json!({ "agent": to_value(&definition) }))
                .map_err(map_agent_error)
        }
        "agent.enable" | "agent.disable" => {
            let id = agent_id_param(&params)?;
            let enabled = request.method == "agent.enable";
            kernel
                .agents()
                .set_enabled(id, enabled)
                .map(|definition| {
                    serde_json::json!({ "id": definition.id, "enabled": definition.enabled })
                })
                .map_err(map_agent_error)
        }
        "agent.spawn" => {
            let agent_id = agent_id_param(&params)?;
            let task = required_str(&params, "task")?;
            let parent = optional_str(&params, "parent_session_id");
            let title = optional_str(&params, "title");
            kernel
                .subagents()
                .spawn(agent_id, task, parent, title)
                .map(|child| serde_json::json!({ "child": to_value(&child) }))
                .map_err(map_subagent_error)
        }
        "agent.call" => {
            let agent_id = agent_id_param(&params)?;
            let task = required_str(&params, "task")?;
            let parent = optional_str(&params, "parent_session_id");
            let title = optional_str(&params, "title");
            kernel
                .subagents()
                .call(agent_id, task, parent, title)
                .await
                .map(|child| serde_json::json!({ "child": to_value(&child) }))
                .map_err(map_subagent_error)
        }
        "agent.children" => {
            let parent = optional_str(&params, "parent_session_id");
            Ok(serde_json::json!({ "children": kernel.subagents().list(parent) }))
        }
        "agent.collect" => {
            let session_id = session_id_param(&params)?;
            kernel
                .subagents()
                .get(&session_id)
                .map(|child| serde_json::json!({ "child": to_value(&child) }))
                .map_err(map_subagent_error)
        }
        "agent.cancel" => {
            let session_id = session_id_param(&params)?;
            kernel
                .subagents()
                .cancel(&session_id)
                .map(|cancelled| serde_json::json!({ "cancelled": cancelled }))
                .map_err(map_subagent_error)
        }
        "agent.message" => {
            let session_id = session_id_param(&params)?;
            let content = required_str(&params, "content")?;
            kernel
                .subagents()
                .message(&session_id, content)
                .map(|_| serde_json::json!({ "session_id": session_id, "appended": true }))
                .map_err(map_subagent_error)
        }
        "agent.handoff" => {
            let from_session_id = required_str(&params, "from_session_id")?.to_string();
            let to_agent_id = required_str(&params, "to_agent_id")?;
            let task = required_str(&params, "task")?;
            kernel
                .subagents()
                .handoff(&from_session_id, to_agent_id, task)
                .await
                .map(|child| serde_json::json!({ "child": to_value(&child) }))
                .map_err(map_subagent_error)
        }
        "agent.run" => {
            let id = agent_id_param(&params)?.to_string();
            let task = params
                .get("task")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing task".to_string()))?
                .to_string();
            let service = kernel.agents_service().clone();
            let observer = fergents_agents::NoopObserver;
            let outcome = service
                .run(&id, &task, &observer, None)
                .await
                .map_err(map_agent_error)?;
            Ok(to_value(&outcome))
        }
        "agent.session.list" => {
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            kernel
                .agents()
                .sessions(limit)
                .map(|sessions| serde_json::json!({ "sessions": sessions }))
                .map_err(map_agent_error)
        }
        "agent.session.messages" => {
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing session_id".to_string()))?;
            kernel
                .agents()
                .session_messages(session_id)
                .map(|messages| serde_json::json!({ "messages": messages }))
                .map_err(map_agent_error)
        }
        "memory.remember" => {
            let content = params
                .get("content")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing content".to_string()))?;
            let importance = params
                .get("importance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32;
            kernel
                .memory()
                .remember(
                    &memory_scope_param(&params),
                    memory_kind_param(&params).unwrap_or(fergents_agents::MemoryKind::Fact),
                    content,
                    importance,
                )
                .map(|entry| to_value(&entry))
                .map_err(map_agent_error)
        }
        "memory.recall" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing query".to_string()))?;
            let k = params.get("k").and_then(Value::as_u64).unwrap_or(8) as usize;
            kernel
                .memory()
                .recall(&memory_scope_param(&params), query, k)
                .map(|entries| serde_json::json!({ "entries": entries }))
                .map_err(map_agent_error)
        }
        "memory.forget" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?;
            kernel
                .memory()
                .forget(id)
                .map(|removed| serde_json::json!({ "removed": removed }))
                .map_err(map_agent_error)
        }
        "memory.list" => kernel
            .memory()
            .list(&memory_scope_param(&params), memory_kind_param(&params))
            .map(|entries| serde_json::json!({ "entries": entries }))
            .map_err(map_agent_error),
        "context.inspect" => {
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing session_id".to_string()))?
                .to_string();
            let agent_id = params
                .get("agent_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let input = params
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || kernel.build_context(&session_id, agent_id.as_deref(), &input))
                .await
                .map(|snapshot| to_value(&snapshot))
        }
        "context.set_section" => {
            let section = params
                .get("section")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing section".to_string()))?
                .to_string();
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or((ErrorCode::InvalidParams, "missing enabled".to_string()))?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .context()
                    .set_section(&section, enabled)
                    .map_err(CoreError::from)
            })
            .await
            .map(|settings| to_value(&settings))
        }
        "context.set_budget" => {
            let budget = params
                .get("budget")
                .and_then(Value::as_u64)
                .ok_or((ErrorCode::InvalidParams, "missing budget".to_string()))?
                as u32;
            let kernel = kernel.clone();
            run_blocking(move || kernel.context().set_budget(budget).map_err(CoreError::from))
                .await
                .map(|settings| to_value(&settings))
        }
        "spec.start" => {
            let slug = params
                .get("slug")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing slug".to_string()))?
                .to_string();
            let task = params
                .get("task")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing task".to_string()))?
                .to_string();
            let mode = fergents_agents::AutonomyMode::parse(
                params
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("supervised"),
            );
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .start(fergents_agents::StartWorkflow {
                        slug,
                        task,
                        session_id,
                        mode,
                    })
                    .map_err(CoreError::from)
            })
            .await
            .map(|run| to_value(&run))
        }
        "spec.get" => {
            let run_id = run_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                let run = kernel
                    .workflow()
                    .get(&run_id)
                    .map_err(CoreError::from)?
                    .ok_or_else(|| {
                        CoreError::Agent(fergents_agents::AgentError::UnknownRun(run_id.clone()))
                    })?;
                let mut artifacts = serde_json::Map::new();
                for gate in [
                    fergents_agents::Gate::Spec,
                    fergents_agents::Gate::Plan,
                    fergents_agents::Gate::Task,
                ] {
                    if let Some(content) = kernel
                        .workflow()
                        .artifact(&run_id, gate)
                        .map_err(CoreError::from)?
                    {
                        artifacts.insert(gate.as_str().to_string(), Value::String(content));
                    }
                }
                Ok(serde_json::json!({ "run": run, "artifacts": artifacts }))
            })
            .await
        }
        "spec.approve" => {
            let run_id = run_id_param(&params)?;
            let gate = params
                .get("gate")
                .and_then(Value::as_str)
                .and_then(fergents_agents::Gate::parse);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .approve(&run_id, gate)
                    .map_err(CoreError::from)
            })
            .await
            .map(|run| to_value(&run))
        }
        "run.list" => {
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .list(limit)
                    .map(|runs| serde_json::json!({ "runs": runs }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "run.get" => {
            let run_id = run_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .get(&run_id)
                    .map_err(CoreError::from)?
                    .ok_or_else(|| {
                        CoreError::Agent(fergents_agents::AgentError::UnknownRun(run_id.clone()))
                    })
            })
            .await
            .map(|run| to_value(&run))
        }
        "run.approve" => {
            let run_id = run_id_param(&params)?;
            let gate = params
                .get("gate")
                .and_then(Value::as_str)
                .and_then(fergents_agents::Gate::parse);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .approve(&run_id, gate)
                    .map_err(CoreError::from)
            })
            .await
            .map(|run| to_value(&run))
        }
        "run.reject" => {
            let run_id = run_id_param(&params)?;
            let reason = params
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .workflow()
                    .reject(&run_id, reason.as_deref())
                    .map_err(CoreError::from)
            })
            .await
            .map(|run| to_value(&run))
        }
        "permission.mode.get" => {
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let mode = kernel.permissions().mode(session_id.as_deref());
            Ok(serde_json::json!({ "mode": mode.as_str() }))
        }
        "permission.mode.set" => {
            let mode = params
                .get("mode")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing mode".to_string()))?;
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            let mode = fergents_permissions::Mode::parse(mode);
            run_blocking(move || {
                kernel
                    .permissions()
                    .set_mode(session_id.as_deref(), mode)
                    .map_err(CoreError::from)?;
                Ok(serde_json::json!({ "mode": mode.as_str() }))
            })
            .await
        }
        "permission.policy.get" => {
            let policy = kernel.permissions().policy();
            Ok(serde_json::json!({
                "toml": policy.to_toml().unwrap_or_default(),
                "rules": policy.rules,
            }))
        }
        "permission.policy.set" => {
            let toml = params
                .get("toml")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing toml".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .permissions()
                    .set_policy_toml(&toml)
                    .map(|policy| serde_json::json!({ "rules": policy.rules }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "permission.check" => {
            let tool = params
                .get("tool")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing tool".to_string()))?
                .to_string();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let mut request = fergents_permissions::PermissionRequest::new(tool, arguments);
            request.target = params
                .get("target")
                .and_then(Value::as_str)
                .map(str::to_string);
            request.session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            request.agent_id = params
                .get("agent_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .permissions()
                    .check(&request)
                    .map(|resolved| to_value(&resolved))
                    .map_err(CoreError::from)
            })
            .await
        }
        "backup.snapshot" => {
            let path = params
                .get("path")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing path".to_string()))?
                .to_string();
            let run_id = params
                .get("run_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let session_id = params
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let agent_id = params
                .get("agent_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                let mut context = fergents_reversibility::BackupContext::new();
                context.run_id = run_id;
                context.session_id = session_id;
                context.agent_id = agent_id;
                kernel
                    .reversibility()
                    .snapshot(&path, &context)
                    .map(|backup| to_value(&backup))
                    .map_err(CoreError::from)
            })
            .await
        }
        "backup.list" => {
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            let run_id = params
                .get("run_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || match run_id {
                Some(run_id) => kernel
                    .reversibility()
                    .backups_for_run(&run_id)
                    .map(|backups| serde_json::json!({ "backups": backups }))
                    .map_err(CoreError::from),
                None => kernel
                    .reversibility()
                    .list(limit)
                    .map(|backups| serde_json::json!({ "backups": backups }))
                    .map_err(CoreError::from),
            })
            .await
        }
        "backup.restore" => {
            let backup_id = params
                .get("backup_id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing backup_id".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .reversibility()
                    .restore(&backup_id)
                    .map(|path| serde_json::json!({ "restored": path }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "backup.restore_run" => {
            let run_id = run_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .reversibility()
                    .restore_run(&run_id)
                    .map(|paths| serde_json::json!({ "restored": paths }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.create" => {
            let agent_id = params
                .get("agent_id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing agent_id".to_string()))?
                .to_string();
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string);
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .create_session(&agent_id, title.as_deref(), model.as_deref())
                    .map(|session| to_value(&session))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.list" => {
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .list(limit)
                    .map(|sessions| serde_json::json!({ "sessions": sessions }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.get" => {
            let session_id = session_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .get(&session_id)
                    .map_err(CoreError::from)?
                    .ok_or_else(|| {
                        CoreError::Chat(fergents_chat::ChatError::UnknownSession(
                            session_id.clone(),
                        ))
                    })
            })
            .await
            .map(|session| to_value(&session))
        }
        "chat.session.rename" => {
            let session_id = session_id_param(&params)?;
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing title".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .rename(&session_id, &title)
                    .map(|session| to_value(&session))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.model" => {
            let session_id = session_id_param(&params)?;
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .set_model(&session_id, model.as_deref())
                    .map(|session| to_value(&session))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.delete" => {
            let session_id = session_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .delete(&session_id)
                    .map(|_| serde_json::json!({ "deleted": session_id }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.session.search" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing query".to_string()))?
                .to_string();
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .search(&query, limit)
                    .map(|hits| serde_json::json!({ "hits": hits }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.message.send" => {
            let session_id = session_id_param(&params)?;
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing text".to_string()))?
                .to_string();
            let session = kernel
                .chat()
                .get(&session_id)
                .map_err(|err| map_core_error(CoreError::from(err)))?
                .ok_or_else(|| {
                    (
                        ErrorCode::InvalidParams,
                        format!("unknown session: {session_id}"),
                    )
                })?;
            let service = kernel.agents_service().clone();
            let observer = fergents_agents::NoopObserver;
            service
                .run_in_session(&session.agent_id, &text, &session_id, &observer, None)
                .await
                .map_err(map_agent_error)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .messages(&session_id)
                    .map(|messages| {
                        serde_json::json!({ "session_id": session_id, "messages": messages })
                    })
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.message.retry" => {
            let session_id = session_id_param(&params)?;
            let session = kernel
                .chat()
                .get(&session_id)
                .map_err(|err| map_core_error(CoreError::from(err)))?
                .ok_or_else(|| {
                    (
                        ErrorCode::InvalidParams,
                        format!("unknown session: {session_id}"),
                    )
                })?;
            let prompt = kernel
                .chat()
                .regenerate(&session_id)
                .map_err(|err| map_core_error(CoreError::from(err)))?
                .ok_or_else(|| {
                    (
                        ErrorCode::InvalidParams,
                        "no user turn to retry".to_string(),
                    )
                })?;
            let service = kernel.agents_service().clone();
            let observer = fergents_agents::NoopObserver;
            service
                .resume_in_session(
                    &session.agent_id,
                    &prompt.content,
                    &session_id,
                    &observer,
                    None,
                )
                .await
                .map_err(map_agent_error)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .messages(&session_id)
                    .map(|messages| {
                        serde_json::json!({ "session_id": session_id, "messages": messages })
                    })
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.message.list" => {
            let session_id = session_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .messages(&session_id)
                    .map(|messages| serde_json::json!({ "messages": messages }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.message.regenerate" => {
            let session_id = session_id_param(&params)?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .regenerate(&session_id)
                    .map(|message| serde_json::json!({ "prompt": message }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.message.fork" => {
            let session_id = session_id_param(&params)?;
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .fork(&session_id, title.as_deref())
                    .map(|session| to_value(&session))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.continue" => {
            let session_id = session_id_param(&params)?;
            let requested = params.get("requested").and_then(Value::as_u64).unwrap_or(1) as u32;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .auto_continue(&session_id, requested)
                    .map(|turns| serde_json::json!({ "turns": turns }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.attach" => {
            let session_id = session_id_param(&params)?;
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing name".to_string()))?
                .to_string();
            let media_type = params
                .get("media_type")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream")
                .to_string();
            let size = params.get("size").and_then(Value::as_u64).unwrap_or(0);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .attach(
                        &session_id,
                        fergents_chat::Attachment::new(name, media_type, size),
                    )
                    .map(|routing| to_value(&routing))
                    .map_err(CoreError::from)
            })
            .await
        }
        "prompt.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .list_prompts()
                    .map(|prompts| serde_json::json!({ "prompts": prompts }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "prompt.create" => {
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing title".to_string()))?
                .to_string();
            let body = params
                .get("body")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing body".to_string()))?
                .to_string();
            let tags: Vec<String> = params
                .get("tags")
                .and_then(Value::as_array)
                .map(|tags| {
                    tags.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .create_prompt(&title, &body, &tags)
                    .map(|prompt| to_value(&prompt))
                    .map_err(CoreError::from)
            })
            .await
        }
        "prompt.delete" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .chat()
                    .delete_prompt(&id)
                    .map(|deleted| serde_json::json!({ "deleted": deleted }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.open" => {
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .open(&path)
                    .map(|project| {
                        // Watch for external changes as soon as a project opens.
                        let _ = kernel.files().watch(&project.id);
                        serde_json::json!({ "project": to_value(&project) })
                    })
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .list()
                    .map(|projects| serde_json::json!({ "projects": to_value(&projects) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.get" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .get(&project_id)
                    .map(|project| serde_json::json!({ "project": to_value(&project) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.close" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .close(&project_id)
                    .map(|closed| {
                        kernel.files().unwatch(&project_id);
                        serde_json::json!({ "closed": closed })
                    })
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.trust" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let trusted = params
                .get("trusted")
                .and_then(Value::as_bool)
                .ok_or((ErrorCode::InvalidParams, "missing trusted".to_string()))?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .set_trust(&project_id, trusted)
                    .map(|project| serde_json::json!({ "project": to_value(&project) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.update" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let name = optional_str(&params, "name").map(str::to_string);
            let brain_id = params
                .get("brain_id")
                .map(|value| value.as_str().map(str::to_string));
            let settings = params
                .get("settings")
                .filter(|value| !value.is_null())
                .cloned();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .update(&project_id, name.as_deref(), brain_id, settings)
                    .map(|project| serde_json::json!({ "project": to_value(&project) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.files" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let query = optional_str(&params, "query")
                .unwrap_or_default()
                .to_string();
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .map(|v| v as usize);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .files(&project_id, &query, limit)
                    .map(|files| serde_json::json!({ "files": files }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "project.search" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let query = required_str(&params, "query")?.to_string();
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .map(|v| v as usize);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .projects()
                    .search(&project_id, &query, limit)
                    .map(|hits| serde_json::json!({ "hits": to_value(&hits) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.read" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .read(&project_id, &path)
                    .map(|file| serde_json::json!({ "file": to_value(&file) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.write" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let content = required_str(&params, "content")?.to_string();
            let base_etag = optional_str(&params, "base_etag").map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .write(&project_id, &path, &content, base_etag.as_deref())
                    .map(|file| serde_json::json!({ "file": to_value(&file) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.create" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let content = optional_str(&params, "content")
                .unwrap_or_default()
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .create(&project_id, &path, &content)
                    .map(|file| serde_json::json!({ "file": to_value(&file) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.mkdir" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .mkdir(&project_id, &path)
                    .map(|entry| serde_json::json!({ "entry": to_value(&entry) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.rename" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let from = required_str(&params, "from")?.to_string();
            let to = required_str(&params, "to")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .rename(&project_id, &from, &to)
                    .map(|entry| serde_json::json!({ "entry": to_value(&entry) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.delete" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .delete(&project_id, &path)
                    .map(|deleted| serde_json::json!({ "deleted": deleted }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.list" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = optional_str(&params, "path")
                .unwrap_or_default()
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .list(&project_id, &path)
                    .map(|entries| serde_json::json!({ "entries": to_value(&entries) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "file.stat" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .files()
                    .stat(&project_id, &path)
                    .map(|entry| serde_json::json!({ "entry": to_value(&entry) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .servers()
                    .map(|servers| serde_json::json!({ "servers": to_value(&servers) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.add" => {
            let language_id = required_str(&params, "language_id")?.to_string();
            let command = required_str(&params, "command")?.to_string();
            let args: Vec<String> = params
                .get("args")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            let env: std::collections::BTreeMap<String, String> = params
                .get("env")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            let source = optional_str(&params, "source")
                .unwrap_or("user")
                .to_string();
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .add_server(&language_id, &command, &args, &env, &source, enabled)
                    .map(|server| serde_json::json!({ "server": to_value(&server) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.remove" => {
            let id = required_str(&params, "id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .remove_server(&id)
                    .map(|removed| serde_json::json!({ "removed": removed }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.start" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let language_id = required_str(&params, "language_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .start(&project_id, &language_id)
                    .map(|server| serde_json::json!({ "server": to_value(&server) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.stop" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let language_id = required_str(&params, "language_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .stop(&project_id, &language_id)
                    .map(|stopped| serde_json::json!({ "stopped": stopped }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.restart" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let language_id = required_str(&params, "language_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .restart(&project_id, &language_id)
                    .map(|server| serde_json::json!({ "server": to_value(&server) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.server.status" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let language_id = required_str(&params, "language_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .status(&project_id, &language_id)
                    .map(|server| serde_json::json!({ "server": to_value(&server) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.document.open" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let text = required_str(&params, "text")?.to_string();
            let language_id = optional_str(&params, "language_id").map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .document_open(&project_id, &path, &text, language_id.as_deref())
                    .map(|()| serde_json::json!({ "ok": true }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.document.change" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let text = required_str(&params, "text")?.to_string();
            let range = params
                .get("range")
                .filter(|value| !value.is_null())
                .cloned();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .document_change(&project_id, &path, &text, range)
                    .map(|version| serde_json::json!({ "version": version }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.document.save" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let text = optional_str(&params, "text").map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .document_save(&project_id, &path, text.as_deref())
                    .map(|()| serde_json::json!({ "ok": true }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.document.close" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .document_close(&project_id, &path)
                    .map(|()| serde_json::json!({ "ok": true }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.completion" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .completion(&project_id, &path, line, character)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.completion_resolve" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let item = params
                .get("item")
                .cloned()
                .ok_or((ErrorCode::InvalidParams, "missing item".to_string()))?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .completion_resolve(&project_id, &path, item)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.hover" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .hover(&project_id, &path, line, character)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.definition" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .definition(&project_id, &path, line, character)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.references" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .references(&project_id, &path, line, character)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.rename" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let new_name = required_str(&params, "new_name")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .rename(&project_id, &path, line, character, &new_name)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.symbols" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .symbols(&project_id, &path)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.workspace_symbols" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let query = required_str(&params, "query")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .workspace_symbols(&project_id, &path, &query)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.format" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let tab_size = params.get("tab_size").and_then(Value::as_u64).unwrap_or(4) as u32;
            let insert_spaces = params
                .get("insert_spaces")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .format(&project_id, &path, tab_size, insert_spaces)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.code_action" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let range = params
                .get("range")
                .cloned()
                .ok_or((ErrorCode::InvalidParams, "missing range".to_string()))?;
            let diagnostics = params
                .get("diagnostics")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .code_action(&project_id, &path, range, diagnostics)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.signature_help" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = required_str(&params, "path")?.to_string();
            let line = required_u32(&params, "line")?;
            let character = required_u32(&params, "character")?;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .lsp()
                    .signature_help(&project_id, &path, line, character)
                    .map(|result| serde_json::json!({ "result": result }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "lsp.diagnostics.list" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let path = optional_str(&params, "path").map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                Ok(serde_json::json!({
                    "diagnostics": to_value(&kernel.lsp().diagnostics(&project_id, path.as_deref())),
                }))
            })
            .await
        }
        "term.open" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let shell = optional_str(&params, "shell").map(str::to_string);
            let cwd = optional_str(&params, "cwd").map(str::to_string);
            let cols = params.get("cols").and_then(Value::as_u64).map(|v| v as u16);
            let rows = params.get("rows").and_then(Value::as_u64).map(|v| v as u16);
            let actor = optional_str(&params, "actor").unwrap_or("user").to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .open(
                        &project_id,
                        shell.as_deref(),
                        cwd.as_deref(),
                        cols,
                        rows,
                        &actor,
                    )
                    .map(|session| serde_json::json!({ "session": to_value(&session) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.write" => {
            let session_id = required_str(&params, "session_id")?.to_string();
            let data = required_str(&params, "data")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .write(&session_id, &data)
                    .map(|()| serde_json::json!({ "ok": true }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.resize" => {
            let session_id = required_str(&params, "session_id")?.to_string();
            let cols = required_u32(&params, "cols")? as u16;
            let rows = required_u32(&params, "rows")? as u16;
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .resize(&session_id, cols, rows)
                    .map(|()| serde_json::json!({ "ok": true }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.close" => {
            let session_id = required_str(&params, "session_id")?.to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .close(&session_id)
                    .map(|closed| serde_json::json!({ "ok": closed }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.list" => {
            let project_id = optional_str(&params, "project_id").map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .list(project_id.as_deref())
                    .map(|sessions| serde_json::json!({ "sessions": to_value(&sessions) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.read" => {
            let session_id = required_str(&params, "session_id")?.to_string();
            let max_lines = params
                .get("max_lines")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let max_bytes = params
                .get("max_bytes")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .read(&session_id, max_lines, max_bytes)
                    .map(|output| serde_json::json!({ "output": output }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "term.exec" => {
            let project_id = required_str(&params, "project_id")?.to_string();
            let command = required_str(&params, "command")?.to_string();
            let actor = optional_str(&params, "actor")
                .unwrap_or("agent")
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .terminal()
                    .exec(&project_id, &command, &actor)
                    .map(|session| serde_json::json!({ "session": to_value(&session) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "secret.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .secrets()
                    .list()
                    .map(|secrets| serde_json::json!({ "secrets": to_value(&secrets) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "secret.set" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing name".to_string()))?
                .to_string();
            let value = params
                .get("value")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing value".to_string()))?
                .to_string();
            let provider = params
                .get("provider")
                .and_then(Value::as_str)
                .map(str::to_string);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .secrets()
                    .set(&name, &value, provider.as_deref())
                    .map(|secret| to_value(&secret))
                    .map_err(CoreError::from)
            })
            .await
        }
        "secret.delete" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing name".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .secrets()
                    .delete(&name)
                    .map(|deleted| serde_json::json!({ "deleted": deleted }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "secret.test" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing name".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .secrets()
                    .test(&name)
                    .map(|test| to_value(&test))
                    .map_err(CoreError::from)
            })
            .await
        }
        "provider.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .providers()
                    .list()
                    .map(|providers| serde_json::json!({ "providers": to_value(&providers) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "provider.add" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            let kind = params
                .get("kind")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing kind".to_string()))?
                .to_string();
            let base_url = params
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::to_string);
            let secret_name = params
                .get("secret_name")
                .and_then(Value::as_str)
                .map(str::to_string);
            let models: Vec<String> = params
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .providers()
                    .add(
                        &id,
                        &kind,
                        base_url.as_deref(),
                        secret_name.as_deref(),
                        &models,
                    )
                    .map(|provider| to_value(&provider))
                    .map_err(CoreError::from)
            })
            .await
        }
        "provider.remove" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .providers()
                    .remove(&id)
                    .map(|deleted| serde_json::json!({ "deleted": deleted }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "provider.test" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            kernel
                .providers()
                .test(&id)
                .await
                .map(|test| to_value(&test))
                .map_err(|err| map_core_error(CoreError::from(err)))
        }
        "provider.models" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            kernel
                .providers()
                .models(&id)
                .await
                .map(|models| serde_json::json!({ "models": models }))
                .map_err(|err| map_core_error(CoreError::from(err)))
        }
        "provider.route.get" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .providers()
                    .routes()
                    .map(|routes| serde_json::json!({ "routes": to_value(&routes) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "provider.route.set" => {
            let task = params
                .get("task")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing task".to_string()))?
                .to_string();
            let models: Vec<String> = params
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .providers()
                    .set_route(&task, &models)
                    .map(|route| to_value(&route))
                    .map_err(CoreError::from)
            })
            .await
        }
        "chat.compare.start" => {
            let agent_id = params
                .get("agent_id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing agent_id".to_string()))?
                .to_string();
            let prompt = params
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing prompt".to_string()))?
                .to_string();
            let models: Vec<String> = params
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string);
            let sides = kernel
                .compare(&agent_id, &prompt, &models, title.as_deref())
                .await
                .map_err(map_core_error)?;
            let sides: Vec<Value> = sides
                .into_iter()
                .map(|side| {
                    serde_json::json!({
                        "session_id": side.session_id,
                        "model": side.model,
                        "content": side.message.map(|message| message.content),
                        "error": side.error,
                    })
                })
                .collect();
            Ok(serde_json::json!({ "sides": sides }))
        }
        "mcp.server.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .mcp()
                    .list()
                    .map(|servers| serde_json::json!({ "servers": to_value(&servers) }))
                    .map_err(CoreError::from)
            })
            .await
        }
        "mcp.server.add" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            let command = params
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_string);
            let args: Vec<String> = params
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let allow = params.get("allow").and_then(Value::as_bool).unwrap_or(true);
            let kernel = kernel.clone();
            run_blocking(move || {
                kernel
                    .mcp()
                    .add(&id, command.as_deref(), &args, allow)
                    .map(|server| to_value(&server))
                    .map_err(CoreError::from)
            })
            .await
        }
        "mcp.server.remove" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            kernel
                .mcp()
                .remove(&id)
                .await
                .map(|deleted| serde_json::json!({ "deleted": deleted }))
                .map_err(|err| map_core_error(CoreError::from(err)))
        }
        "mcp.server.test" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            kernel
                .mcp()
                .test(&id)
                .await
                .map(|test| to_value(&test))
                .map_err(|err| map_core_error(CoreError::from(err)))
        }
        "mcp.server.health" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or((ErrorCode::InvalidParams, "missing id".to_string()))?
                .to_string();
            kernel
                .mcp()
                .health(&id)
                .await
                .map(|health| to_value(&health))
                .map_err(|err| map_core_error(CoreError::from(err)))
        }
        "mcp.tool.list" => {
            let kernel = kernel.clone();
            run_blocking(move || {
                Ok(serde_json::json!({ "tools": to_value(&kernel.mcp().tools()) }))
            })
            .await
        }
        "kernel.shutdown" => {
            let _ = shutdown_tx.send(true);
            Ok(serde_json::json!({ "shutting_down": true }))
        }
        other => Err((
            ErrorCode::MethodNotFound,
            format!("method not found: {other}"),
        )),
    }
}

async fn forward_events(
    mut subscription: Subscription,
    topics: Arc<RwLock<HashSet<String>>>,
    writer: Writer,
) {
    loop {
        let event = match subscription.recv().await {
            Ok(event) => event,
            Err(_) => break,
        };
        let matches = {
            let set = topics.read().await;
            set.is_empty() || set.iter().any(|topic| topic_matches(&event.kind, topic))
        };
        if matches
            && write_json(&writer, &Notification::event(&event))
                .await
                .is_err()
        {
            break;
        }
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn from_params<T: serde::de::DeserializeOwned>(params: &Value) -> Result<T, (ErrorCode, String)> {
    serde_json::from_value(params.clone())
        .map_err(|e| (ErrorCode::InvalidParams, format!("invalid params: {e}")))
}

/// The capability a command belongs to, if it is gated by one.
///
/// A disabled capability's commands are refused before dispatch.
fn required_capability(method: &str) -> Option<&'static str> {
    match method {
        "search.query" => Some("capability.search"),
        "chat.session.search" => Some("capability.chat-search"),
        "chat.compare.start" => Some("capability.compare"),
        "agent.spawn" | "agent.call" | "agent.children" | "agent.collect" | "agent.cancel" => {
            Some("capability.sub-agents")
        }
        "agent.message" | "agent.handoff" => Some("capability.agent-sync"),
        method if method.starts_with("graph.") => Some("capability.graph"),
        method if method.starts_with("agent.") => Some("capability.agents"),
        method if method.starts_with("chat.") => Some("capability.chat"),
        method if method.starts_with("memory.") => Some("capability.memory"),
        "spec.start" | "spec.get" | "spec.approve" | "run.list" | "run.get" | "run.approve"
        | "run.reject" => Some("capability.spec-workflow"),
        method if method.starts_with("permission.") => Some("capability.agents"),
        method if method.starts_with("backup.") => Some("capability.agents"),
        method if method.starts_with("project.") => Some("capability.ide"),
        method if method.starts_with("file.") => Some("capability.ide"),
        method if method.starts_with("lsp.") => Some("capability.lsp"),
        method if method.starts_with("term.") => Some("capability.terminal"),
        method if method.starts_with("prompt.") => Some("capability.chat"),
        method if method.starts_with("provider.") => Some("capability.llm"),
        method if method.starts_with("mcp.") => Some("capability.mcp"),
        _ => None,
    }
}

/// Read the `run_id` field from run command params.
fn run_id_param(params: &Value) -> Result<String, (ErrorCode, String)> {
    params
        .get("run_id")
        .or_else(|| params.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| (ErrorCode::InvalidParams, "missing run_id".to_string()))
}

/// Read the `session_id` field from chat command params.
fn session_id_param(params: &Value) -> Result<String, (ErrorCode, String)> {
    params
        .get("session_id")
        .or_else(|| params.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| (ErrorCode::InvalidParams, "missing session_id".to_string()))
}

/// Read the `scope` field from memory command params.
fn memory_scope_param(params: &Value) -> fergents_agents::MemoryScope {
    params
        .get("scope")
        .and_then(Value::as_str)
        .map(fergents_agents::MemoryScope::parse)
        .unwrap_or(fergents_agents::MemoryScope::Global)
}

/// Read the `kind` field from memory command params.
fn memory_kind_param(params: &Value) -> Option<fergents_agents::MemoryKind> {
    params
        .get("kind")
        .and_then(Value::as_str)
        .map(fergents_agents::MemoryKind::parse)
}

/// Read the `id` field from agent command params.
fn agent_id_param(params: &Value) -> Result<&str, (ErrorCode, String)> {
    params
        .get("agent_id")
        .or_else(|| params.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| (ErrorCode::InvalidParams, "missing agent_id".to_string()))
}

/// Read a required string field.
fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, (ErrorCode, String)> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| (ErrorCode::InvalidParams, format!("missing {key}")))
}

/// Read an optional string field.
fn optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

/// Read a required unsigned field as `u32`.
fn required_u32(params: &Value, key: &str) -> Result<u32, (ErrorCode, String)> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .ok_or_else(|| (ErrorCode::InvalidParams, format!("missing {key}")))
}

/// Map a sub-agent error to the closest wire error code.
fn map_subagent_error(err: fergents_core::SubAgentError) -> (ErrorCode, String) {
    use fergents_core::SubAgentError;
    let code = match &err {
        SubAgentError::Agent(_) | SubAgentError::UnknownSession(_) | SubAgentError::Unknown(_) => {
            ErrorCode::InvalidParams
        }
        SubAgentError::Event(_) => ErrorCode::InternalError,
    };
    (code, format!("{}: {}", err.code(), err))
}

/// Map an agent error to the closest wire error code.
fn map_agent_error(err: fergents_agents::AgentError) -> (ErrorCode, String) {
    use fergents_agents::AgentError;
    let code = match &err {
        AgentError::Storage(_) => ErrorCode::StorageError,
        AgentError::InvalidDefinition(_)
        | AgentError::UnknownAgent(_)
        | AgentError::UnknownTool(_)
        | AgentError::UnknownSession(_)
        | AgentError::UnknownRun(_)
        | AgentError::PermissionDenied { .. } => ErrorCode::InvalidParams,
        _ => ErrorCode::InternalError,
    };
    (code, format!("{}: {}", err.code(), err))
}

/// Read the `id` field from capability command params.
fn capability_id_param(params: &Value) -> Result<&str, (ErrorCode, String)> {
    params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| (ErrorCode::InvalidParams, "missing id".to_string()))
}

/// Map a capability error to the closest wire error code.
fn map_capability_error(err: fergents_capabilities::CapabilityError) -> (ErrorCode, String) {
    use fergents_capabilities::CapabilityError;
    let code = match &err {
        CapabilityError::Storage(_) => ErrorCode::StorageError,
        CapabilityError::Disabled(_) => ErrorCode::CapabilityDisabled,
        CapabilityError::Event(_) | CapabilityError::Serde(_) => ErrorCode::InternalError,
        _ => ErrorCode::InvalidParams,
    };
    (code, format!("{}: {}", err.code(), err))
}

/// Run a synchronous [`fergents_core`] brain operation on the blocking pool.
///
/// The brain subsystems do blocking I/O (SQLite, the filesystem, graph), so
/// dispatch must never call them directly on the async runtime.
async fn run_blocking<T, F>(f: F) -> Result<T, (ErrorCode, String)>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CoreError> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| {
            (
                ErrorCode::InternalError,
                format!("blocking task failed: {e}"),
            )
        })?
        .map_err(map_core_error)
}

/// Map a core error to the closest wire error code.
fn map_core_error(err: CoreError) -> (ErrorCode, String) {
    match &err {
        CoreError::Storage(_) => (ErrorCode::StorageError, err.to_string()),
        CoreError::Project(fergents_ide::ProjectError::Storage(_)) => {
            (ErrorCode::StorageError, err.to_string())
        }
        CoreError::Project(fergents_ide::ProjectError::Untrusted(_)) => {
            (ErrorCode::ProjectUntrusted, err.to_string())
        }
        CoreError::Project(fergents_ide::ProjectError::Conflict(_)) => {
            (ErrorCode::FileConflict, err.to_string())
        }
        CoreError::Project(_) => (ErrorCode::InvalidParams, err.to_string()),
        CoreError::Lsp(fergents_lsp::LspError::Project(fergents_ide::ProjectError::Untrusted(
            _,
        ))) => (ErrorCode::ProjectUntrusted, err.to_string()),
        CoreError::Lsp(fergents_lsp::LspError::Storage(_)) => {
            (ErrorCode::StorageError, err.to_string())
        }
        CoreError::Lsp(
            fergents_lsp::LspError::Project(_)
            | fergents_lsp::LspError::UnknownServer(_)
            | fergents_lsp::LspError::NotInstalled(_)
            | fergents_lsp::LspError::DocumentNotOpen(_),
        ) => (ErrorCode::InvalidParams, err.to_string()),
        CoreError::Lsp(_) => (ErrorCode::InternalError, err.to_string()),
        CoreError::Terminal(fergents_terminal::TerminalError::Project(
            fergents_ide::ProjectError::Untrusted(_),
        )) => (ErrorCode::ProjectUntrusted, err.to_string()),
        CoreError::Terminal(fergents_terminal::TerminalError::Storage(_)) => {
            (ErrorCode::StorageError, err.to_string())
        }
        CoreError::Terminal(
            fergents_terminal::TerminalError::Project(_)
            | fergents_terminal::TerminalError::UnknownSession(_)
            | fergents_terminal::TerminalError::NotRunning(_)
            | fergents_terminal::TerminalError::ShellNotAllowed(_)
            | fergents_terminal::TerminalError::Invalid(_),
        ) => (ErrorCode::InvalidParams, err.to_string()),
        CoreError::Terminal(_) => (ErrorCode::InternalError, err.to_string()),
        CoreError::Capability(cap) => {
            let code = if matches!(cap, fergents_capabilities::CapabilityError::Disabled(_)) {
                ErrorCode::CapabilityDisabled
            } else {
                ErrorCode::InternalError
            };
            (code, err.to_string())
        }
        CoreError::Brain(fergents_brain::BrainError::Invalid(_))
        | CoreError::Index(
            fergents_index::IndexError::Invalid(_) | fergents_index::IndexError::InvalidQuery(_),
        )
        | CoreError::Graph(fergents_graph::GraphError::Invalid(_)) => {
            (ErrorCode::InvalidParams, err.to_string())
        }
        CoreError::Permission(fergents_permissions::PermissionError::InvalidPolicy(_))
        | CoreError::Agent(fergents_agents::AgentError::InvalidDefinition(_))
        | CoreError::Reversibility(fergents_reversibility::ReversibilityError::UnknownBackup(_))
        | CoreError::Chat(
            fergents_chat::ChatError::UnknownSession(_) | fergents_chat::ChatError::Invalid(_),
        )
        | CoreError::Secret(
            fergents_secrets::SecretError::InvalidName(_)
            | fergents_secrets::SecretError::Unknown(_),
        )
        | CoreError::Provider(
            fergents_core::ProviderRegistryError::InvalidId(_)
            | fergents_core::ProviderRegistryError::Unknown(_)
            | fergents_core::ProviderRegistryError::InvalidKind(_)
            | fergents_core::ProviderRegistryError::MissingBaseUrl(_)
            | fergents_core::ProviderRegistryError::MissingSecret(_),
        )
        | CoreError::Mcp(
            fergents_core::McpServiceError::InvalidId(_)
            | fergents_core::McpServiceError::Unknown(_),
        ) => (ErrorCode::InvalidParams, err.to_string()),
        _ => (ErrorCode::InternalError, err.to_string()),
    }
}

async fn respond_result<T: Serialize>(
    writer: &Writer,
    id: RequestId,
    result: &T,
) -> Result<(), ApiError> {
    let response = Response {
        jsonrpc: fergents_protocol::JSONRPC_VERSION.to_string(),
        id,
        result: to_value(result),
    };
    write_json(writer, &response).await?;
    Ok(())
}

async fn respond_error(
    writer: &Writer,
    id: Option<RequestId>,
    code: ErrorCode,
    message: &str,
    data: Option<Value>,
) -> Result<(), ApiError> {
    let response = ErrorResponse {
        jsonrpc: fergents_protocol::JSONRPC_VERSION.to_string(),
        id,
        error: JsonRpcError {
            code: code.as_i32(),
            message: message.to_string(),
            data,
        },
    };
    write_json(writer, &response).await?;
    Ok(())
}

async fn write_json<T: Serialize>(writer: &Writer, value: &T) -> Result<(), ApiError> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    let mut guard = writer.lock().await;
    guard.write_all(&bytes).await?;
    guard.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_fatal_accept;
    use crate::error::ApiError;
    use std::io::{Error, ErrorKind};

    #[test]
    fn transient_accept_errors_are_not_fatal() {
        for kind in [
            ErrorKind::ConnectionAborted,
            ErrorKind::ConnectionReset,
            ErrorKind::Interrupted,
            ErrorKind::WouldBlock,
            ErrorKind::TimedOut,
        ] {
            let err = ApiError::Io(Error::from(kind));
            assert!(!is_fatal_accept(&err), "{kind:?}");
        }
    }

    #[test]
    fn other_accept_errors_are_fatal() {
        assert!(is_fatal_accept(&ApiError::Io(Error::from(
            ErrorKind::PermissionDenied
        ))));
        assert!(is_fatal_accept(&ApiError::HandshakeRequired));
    }
}
