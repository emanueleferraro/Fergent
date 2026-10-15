//! A kernel client: connect, handshake, call commands, and receive events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use fergents_protocol::{
    ErrorResponse, Event, HelloParams, JsonRpcError, Request, RequestId, WelcomeResult,
    KERNEL_VERSION, PROTOCOL_VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::ApiError;
use crate::transport::{connect_stream, BoxStream, SocketPath};

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, JsonRpcError>>>>>;

/// A client connected to a running kernel.
pub struct KernelClient {
    writer: Arc<Mutex<WriteHalf<BoxStream>>>,
    pending: Pending,
    events: mpsc::Receiver<Event>,
    next_id: AtomicI64,
    session_id: String,
}

impl KernelClient {
    /// Connect with the `cli` client kind.
    pub async fn connect(socket: &SocketPath) -> Result<Self, ApiError> {
        Self::connect_as(socket, "cli", KERNEL_VERSION).await
    }

    /// Connect and complete the `hello` handshake as `client_kind`.
    pub async fn connect_as(
        socket: &SocketPath,
        client_kind: &str,
        client_version: &str,
    ) -> Result<Self, ApiError> {
        let stream = connect_stream(socket).await?;
        let (read_half, write_half) = tokio::io::split(stream);
        let writer = Arc::new(Mutex::new(write_half));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel(1024);

        tokio::spawn(reader_loop(
            BufReader::new(read_half),
            pending.clone(),
            event_tx,
        ));

        let mut client = Self {
            writer,
            pending,
            events: event_rx,
            next_id: AtomicI64::new(1),
            session_id: String::new(),
        };

        let welcome: WelcomeResult = client
            .call_typed(
                "hello",
                &HelloParams {
                    protocol_version: PROTOCOL_VERSION,
                    client_kind: client_kind.to_string(),
                    client_version: Some(client_version.to_string()),
                    capabilities: Some(Vec::new()),
                },
            )
            .await?;
        client.session_id = welcome.session_id;
        Ok(client)
    }

    /// The negotiated session id.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Send a command and await its JSON result.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, ApiError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let request = Request::new(RequestId::Number(id), method, params);
        self.write(&request).await?;
        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(ApiError::Rpc {
                code: error.code,
                message: error.message,
            }),
            Err(_) => Err(ApiError::ConnectionClosed),
        }
    }

    /// Send a command and deserialize its result into `T`.
    pub async fn call_typed<T: DeserializeOwned>(
        &self,
        method: &str,
        params: &impl Serialize,
    ) -> Result<T, ApiError> {
        let params = serde_json::to_value(params)?;
        let value = self.call(method, params).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Subscribe this connection to a set of event topics.
    pub async fn subscribe(&self, topics: &[String]) -> Result<(), ApiError> {
        self.call("events.subscribe", serde_json::json!({ "topics": topics }))
            .await?;
        Ok(())
    }

    /// Await the next event pushed by the kernel.
    pub async fn next_event(&mut self) -> Option<Event> {
        self.events.recv().await
    }

    async fn write<T: Serialize>(&self, value: &T) -> Result<(), ApiError> {
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        let mut guard = self.writer.lock().await;
        guard.write_all(&bytes).await?;
        guard.flush().await?;
        Ok(())
    }
}

async fn reader_loop<R: tokio::io::AsyncBufRead + Unpin + Send + 'static>(
    mut reader: R,
    pending: Pending,
    event_tx: mpsc::Sender<Event>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(id) = value.get("id").and_then(Value::as_i64) {
            let sender = pending.lock().await.remove(&id);
            if let Some(sender) = sender {
                if value.get("error").is_some() {
                    let response: Result<ErrorResponse, _> = serde_json::from_value(value);
                    let error = match response {
                        Ok(response) => response.error,
                        Err(_) => JsonRpcError {
                            code: -32603,
                            message: "malformed error response".to_string(),
                            data: None,
                        },
                    };
                    let _ = sender.send(Err(error));
                } else {
                    let result = value.get("result").cloned().unwrap_or(Value::Null);
                    let _ = sender.send(Ok(result));
                }
            }
            continue;
        }

        if value.get("method").and_then(Value::as_str) == Some("event") {
            if let Some(params) = value.get("params") {
                if let Ok(event) = serde_json::from_value::<Event>(params.clone()) {
                    if event_tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
    use fergents_protocol::HealthSnapshot;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use std::time::Instant;
    use tokio::sync::watch;

    /// Start a kernel + server on a temp socket and wait until it accepts.
    async fn start_stack() -> (
        tempfile::TempDir,
        SocketPath,
        crate::ServerHandle,
        Arc<Kernel>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let kernel = Kernel::start(&KernelConfig {
            data_dir: dir.path().to_path_buf(),
            migrations_dir,
            event_capacity: 128,
        })
        .unwrap();
        let socket =
            SocketPath::from_path(dir.path().join("t.sock").to_string_lossy().into_owned());
        let (tx, rx) = watch::channel(false);
        let handle = crate::ServerHandle::spawn(kernel.clone(), socket.clone(), tx, rx);

        // Wait for the listener to be ready.
        for _ in 0..100 {
            if connect_stream(&socket).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        (dir, socket, handle, kernel)
    }

    #[tokio::test]
    async fn health_roundtrip_and_shutdown() {
        let (_dir, socket, handle, _kernel) = start_stack().await;
        let client = KernelClient::connect(&socket).await.unwrap();
        assert!(!client.session_id().is_empty());
        let health: HealthSnapshot = client.call_typed("system.health", &()).await.unwrap();
        assert_eq!(health.kernel_version, KERNEL_VERSION);
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
        handle.join().await.unwrap();
    }

    #[tokio::test]
    async fn unknown_method_returns_rpc_error() {
        let (_dir, socket, handle, _kernel) = start_stack().await;
        let client = KernelClient::connect(&socket).await.unwrap();
        let err = client
            .call("nope.nope", serde_json::json!({}))
            .await
            .unwrap_err();
        match err {
            ApiError::Rpc { code, .. } => assert_eq!(code, -32601),
            other => panic!("unexpected error: {other:?}"),
        }
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
        handle.join().await.unwrap();
    }

    #[tokio::test]
    async fn subscription_delivers_events_live() {
        let (_dir, socket, handle, kernel) = start_stack().await;
        let mut client = KernelClient::connect(&socket).await.unwrap();
        client.subscribe(&["kernel.".to_string()]).await.unwrap();
        // Give the server a moment to register the subscription, then publish.
        tokio::time::sleep(Duration::from_millis(50)).await;
        kernel
            .bus()
            .publish("kernel.heartbeat", "kernel", serde_json::json!({}))
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), client.next_event())
            .await
            .expect("event timeout")
            .expect("event stream ended");
        assert_eq!(event.kind, "kernel.heartbeat");
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
        handle.join().await.unwrap();
    }

    #[tokio::test]
    async fn handshake_rejects_unsupported_protocol_version() {
        let (_dir, socket, handle, _kernel) = start_stack().await;
        let stream = connect_stream(&socket).await.unwrap();
        let (read_half, write_half) = tokio::io::split(stream);
        let mut writer = write_half;
        let mut reader = BufReader::new(read_half);
        let request = Request::new(
            RequestId::Number(1),
            "hello",
            serde_json::json!({ "protocol_version": 999, "client_kind": "cli" }),
        );
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        writer.write_all(&bytes).await.unwrap();
        writer.flush().await.unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(
            value
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(Value::as_i64),
            Some(-32001)
        );
        let _ = handle;
    }

    #[tokio::test]
    async fn ping_roundtrips_in_under_fifty_milliseconds() {
        let (_dir, socket, handle, _kernel) = start_stack().await;
        let client = KernelClient::connect(&socket).await.unwrap();
        // Warm up so the measurement excludes first-call setup.
        let _: fergents_protocol::PingResult = client.call_typed("ping", &()).await.unwrap();
        let start = Instant::now();
        let pong: fergents_protocol::PingResult = client.call_typed("ping", &()).await.unwrap();
        let elapsed = start.elapsed();
        assert!(pong.pong);
        assert!(
            elapsed < Duration::from_millis(50),
            "ping round-trip took {elapsed:?}"
        );
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
        handle.join().await.unwrap();
    }

    #[tokio::test]
    async fn two_clients_get_independent_subscriptions() {
        let (_dir, socket, handle, kernel) = start_stack().await;
        let mut a = KernelClient::connect(&socket).await.unwrap();
        let mut b = KernelClient::connect(&socket).await.unwrap();
        a.subscribe(&["a.".to_string()]).await.unwrap();
        b.subscribe(&["b.".to_string()]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        kernel
            .bus()
            .publish("b.thing", "kernel", serde_json::json!({}))
            .unwrap();

        let seen = tokio::time::timeout(Duration::from_secs(2), b.next_event())
            .await
            .expect("b should receive its topic")
            .expect("b stream ended");
        assert_eq!(seen.kind, "b.thing");
        // `a` filters on a different topic and must not see it.
        assert!(
            tokio::time::timeout(Duration::from_millis(200), a.next_event())
                .await
                .is_err(),
            "a must not receive a topic it did not subscribe to"
        );

        kernel
            .bus()
            .publish("a.thing", "kernel", serde_json::json!({}))
            .unwrap();
        let seen = tokio::time::timeout(Duration::from_secs(2), a.next_event())
            .await
            .expect("a should receive its own topic")
            .expect("a stream ended");
        assert_eq!(seen.kind, "a.thing");

        let _ = a.call("kernel.shutdown", serde_json::json!({})).await;
        handle.join().await.unwrap();
    }
}
