//! The JSON-RPC 2.0 server over the local IPC socket.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fergents_core::Kernel;
use fergents_events::{now_ms, topic_matches, Subscription};
use fergents_protocol::{
    BrainListResult, ErrorCode, ErrorResponse, HelloParams, JsonRpcError, Notification, PingResult,
    ReplayParams, Request, RequestId, Response, SubscribeParams, UnsubscribeParams, WelcomeResult,
    KERNEL_VERSION, PROTOCOL_VERSION,
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
            let brains = kernel
                .list_brains()
                .map_err(|e| (ErrorCode::StorageError, e.to_string()))?;
            Ok(to_value(&BrainListResult { brains }))
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
