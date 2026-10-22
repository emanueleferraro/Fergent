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
        method if method.starts_with("graph.") => Some("capability.graph"),
        method if method.starts_with("agent.") => Some("capability.agents"),
        method if method.starts_with("chat.") => Some("capability.chat"),
        method if method.starts_with("memory.") => Some("capability.memory"),
        _ => None,
    }
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

/// Map an agent error to the closest wire error code.
fn map_agent_error(err: fergents_agents::AgentError) -> (ErrorCode, String) {
    use fergents_agents::AgentError;
    let code = match &err {
        AgentError::Storage(_) => ErrorCode::StorageError,
        AgentError::InvalidDefinition(_)
        | AgentError::UnknownAgent(_)
        | AgentError::UnknownTool(_)
        | AgentError::UnknownSession(_)
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
