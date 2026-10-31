//! Webhook triggers (spec §2.3).
//!
//! The listener binds the loopback interface only. Routing, authentication,
//! rate limiting, and idempotency live in [`WebhookRouter`], which is pure
//! enough to test without a socket; [`WebhookListener`] is the thin HTTP/1.1
//! adapter around it.
//!
//! Authentication deliberately answers `404` for a missing or incorrect
//! token: an attacker must not be able to tell an unknown route from a known
//! one, so the endpoint is not discoverable.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::document::WorkflowDocument;
use crate::error::WorkflowError;
use crate::executor::Cancellation;
use crate::trigger::{Clock, FireFn, SystemClock, TriggerFire};

/// The largest accepted request body (spec §2.3: 1 MiB).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The most requests one trigger accepts per minute (spec §2.3).
pub const RATE_LIMIT_PER_MINUTE: usize = 60;

/// The rate-limit window.
pub const RATE_WINDOW_MS: i64 = 60_000;

/// How long an `Idempotency-Key` suppresses a repeat (spec §2.3: 10 min).
pub const IDEMPOTENCY_WINDOW_MS: i64 = 10 * 60 * 1000;

/// The default port from settings (spec §2.3).
pub const DEFAULT_PORT: u16 = 8787;

/// Where a webhook trigger's bearer token comes from.
///
/// The kernel backs this with the keychain; tests and the kernel's in-memory
/// cache use [`MemoryTokenStore`].
pub trait TokenStore: Send + Sync {
    /// The token for a trigger, or `None` when the trigger has none.
    fn token(&self, workflow_id: &str, trigger_id: &str) -> Option<String>;
}

/// An in-memory token store.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    tokens: Mutex<HashMap<(String, String), String>>,
}

impl MemoryTokenStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set (or rotate) a trigger's token.
    pub fn set(&self, workflow_id: &str, trigger_id: &str, token: impl Into<String>) {
        let mut tokens = self.tokens.lock().expect("token store poisoned");
        tokens.insert(
            (workflow_id.to_string(), trigger_id.to_string()),
            token.into(),
        );
    }

    /// Forget a trigger's token.
    pub fn remove(&self, workflow_id: &str, trigger_id: &str) {
        let mut tokens = self.tokens.lock().expect("token store poisoned");
        tokens.remove(&(workflow_id.to_string(), trigger_id.to_string()));
    }
}

impl TokenStore for MemoryTokenStore {
    fn token(&self, workflow_id: &str, trigger_id: &str) -> Option<String> {
        let tokens = self.tokens.lock().expect("token store poisoned");
        tokens
            .get(&(workflow_id.to_string(), trigger_id.to_string()))
            .cloned()
    }
}

/// A parsed HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookRequest {
    /// The request method, upper-case.
    pub method: String,
    /// The request target, without a query string.
    pub path: String,
    /// Header names lower-cased.
    pub headers: BTreeMap<String, String>,
    /// The raw body.
    pub body: Vec<u8>,
}

impl WebhookRequest {
    /// Parse a complete HTTP/1.1 request.
    pub fn parse(raw: &[u8]) -> Result<Self, WorkflowError> {
        let head_end = find_headers_end(raw)
            .ok_or_else(|| WorkflowError::Invalid("incomplete http head".to_string()))?;
        let head = std::str::from_utf8(&raw[..head_end - 4])
            .map_err(|_| WorkflowError::Invalid("http head is not utf-8".to_string()))?;
        let mut lines = head.split("\r\n");

        let request_line = lines
            .next()
            .ok_or_else(|| WorkflowError::Invalid("missing request line".to_string()))?;
        let mut parts = request_line.split_whitespace();
        let method = parts
            .next()
            .ok_or_else(|| WorkflowError::Invalid("missing method".to_string()))?
            .to_ascii_uppercase();
        let target = parts
            .next()
            .ok_or_else(|| WorkflowError::Invalid("missing target".to_string()))?;
        let path = target
            .split_once('?')
            .map_or(target, |(path, _query)| path)
            .to_string();

        let mut headers = BTreeMap::new();
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }

        let mut body = raw[head_end..].to_vec();
        if let Some(length) = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
        {
            if body.len() < length {
                return Err(WorkflowError::Invalid("incomplete http body".to_string()));
            }
            body.truncate(length);
        }

        Ok(Self {
            method,
            path,
            headers,
            body,
        })
    }

    /// A header value, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// The body parsed as JSON, when it is JSON.
    pub fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

/// A response to write back.
#[derive(Debug, Clone, PartialEq)]
pub struct WebhookResponse {
    /// The status code.
    pub status: u16,
    /// The JSON body.
    pub body: Value,
}

impl WebhookResponse {
    /// A response with `status` and `body`.
    pub fn new(status: u16, body: Value) -> Self {
        Self { status, body }
    }

    /// Serialise the response, including the `Connection: close` header.
    pub fn to_http(&self) -> Vec<u8> {
        let body = serde_json::to_vec(&self.body).unwrap_or_else(|_| b"{}".to_vec());
        let head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status,
            reason(self.status),
            body.len()
        );
        let mut out = head.into_bytes();
        out.extend_from_slice(&body);
        out
    }
}

/// The outcome of routing one request, before it becomes an HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FireOutcome {
    /// A run started.
    Started(String),
    /// A run with this idempotency key is already in flight.
    Duplicate(String),
    /// The request was rejected with a status and a wire code.
    Rejected {
        /// The HTTP status to answer with.
        status: u16,
        /// The wire code, or `not_found` for an undiscoverable endpoint.
        code: &'static str,
    },
}

impl FireOutcome {
    /// The HTTP status for this outcome.
    pub fn status(&self) -> u16 {
        match self {
            FireOutcome::Started(_) | FireOutcome::Duplicate(_) => 202,
            FireOutcome::Rejected { status, .. } => *status,
        }
    }
}

struct Route {
    workflow_id: String,
    trigger_id: String,
    method: String,
    path: String,
}

/// Idempotency keys seen in the window: key → (run id, first seen at).
type SeenKeys = HashMap<(String, String, String), (String, i64)>;

/// Routes loopback webhook requests to runs (spec §2.3).
pub struct WebhookRouter {
    routes: Vec<Route>,
    tokens: Arc<dyn TokenStore>,
    fire: FireFn,
    clock: Arc<dyn Clock>,
    rate: Mutex<HashMap<(String, String), Vec<i64>>>,
    seen: Mutex<SeenKeys>,
}

impl WebhookRouter {
    /// A router firing through `fire`, reading tokens from `tokens`.
    pub fn new(tokens: Arc<dyn TokenStore>, fire: FireFn) -> Self {
        Self::with_clock(tokens, fire, Arc::new(SystemClock))
    }

    /// A router with an injected clock.
    pub fn with_clock(tokens: Arc<dyn TokenStore>, fire: FireFn, clock: Arc<dyn Clock>) -> Self {
        Self {
            routes: Vec::new(),
            tokens,
            fire,
            clock,
            rate: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Register every webhook trigger declared by `doc`. Returns the count.
    ///
    /// The canonical route is `/hooks/<workflow-id>/<trigger-id>` (spec §2.3);
    /// the declared `path` is a label the validator checks, not the route.
    pub fn add_workflow(&mut self, doc: &WorkflowDocument) -> usize {
        let mut added = 0;
        for trigger in &doc.triggers {
            if let crate::document::TriggerKind::Webhook { method, .. } = &trigger.kind {
                self.routes.push(Route {
                    workflow_id: doc.id.clone(),
                    trigger_id: trigger.id.clone(),
                    method: method.to_ascii_uppercase(),
                    path: format!("/hooks/{}/{}", doc.id, trigger.id),
                });
                added += 1;
            }
        }
        added
    }

    /// The registered route count.
    pub fn routes(&self) -> usize {
        self.routes.len()
    }

    /// Route a request and answer it.
    pub fn handle(&self, request: &WebhookRequest) -> WebhookResponse {
        match self.dispatch(request) {
            FireOutcome::Started(run_id) => WebhookResponse::new(202, json!({ "run_id": run_id })),
            FireOutcome::Duplicate(run_id) => {
                WebhookResponse::new(202, json!({ "run_id": run_id, "duplicate": true }))
            }
            FireOutcome::Rejected { status, code } => {
                WebhookResponse::new(status, json!({ "error": code }))
            }
        }
    }

    /// Route a request without answering it.
    pub fn dispatch(&self, request: &WebhookRequest) -> FireOutcome {
        let Some(route) = self
            .routes
            .iter()
            .find(|route| route.method == request.method && route.path == request.path)
        else {
            return not_found();
        };

        let Some(expected) = self.tokens.token(&route.workflow_id, &route.trigger_id) else {
            return not_found();
        };
        let Some(provided) = bearer(request) else {
            return not_found();
        };
        if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
            return not_found();
        }

        if request.body.len() > MAX_BODY_BYTES {
            return FireOutcome::Rejected {
                status: 413,
                code: "payload_too_large",
            };
        }

        let now = self.clock.now_ms();
        let key = request.header("idempotency-key").map(str::to_string);
        if let Some(key) = key.as_deref() {
            let cache = self.seen.lock().expect("idempotency cache poisoned");
            let slot = (
                route.workflow_id.clone(),
                route.trigger_id.clone(),
                key.to_string(),
            );
            if let Some((run_id, at)) = cache.get(&slot) {
                if now - at < IDEMPOTENCY_WINDOW_MS {
                    return FireOutcome::Duplicate(run_id.clone());
                }
            }
        }

        if !self.allow(&route.workflow_id, &route.trigger_id, now) {
            return FireOutcome::Rejected {
                status: 429,
                code: "rate_limited",
            };
        }

        let body = request
            .json()
            .unwrap_or_else(|| Value::String(String::from_utf8_lossy(&request.body).to_string()));
        let headers = request
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect::<serde_json::Map<_, _>>();

        let fired = (self.fire)(TriggerFire {
            workflow_id: route.workflow_id.clone(),
            trigger_id: route.trigger_id.clone(),
            kind: "webhook",
            actor: format!("trigger:{}", route.trigger_id),
            inputs: json!({ "body": body, "headers": headers }),
        });

        match fired {
            Ok(run_id) => {
                if let Some(key) = key {
                    let mut cache = self.seen.lock().expect("idempotency cache poisoned");
                    cache.retain(|_, (_, at)| now - *at < IDEMPOTENCY_WINDOW_MS);
                    cache.insert(
                        (route.workflow_id.clone(), route.trigger_id.clone(), key),
                        (run_id.clone(), now),
                    );
                }
                FireOutcome::Started(run_id)
            }
            Err(err) => match err {
                WorkflowError::QueueFull(_) => FireOutcome::Rejected {
                    status: 429,
                    code: "workflow_queue_full",
                },
                other => FireOutcome::Rejected {
                    status: 500,
                    code: other.code(),
                },
            },
        }
    }

    /// Record a request and report whether it is inside the rate limit.
    fn allow(&self, workflow_id: &str, trigger_id: &str, now: i64) -> bool {
        let mut buckets = self.rate.lock().expect("rate limiter poisoned");
        let bucket = buckets
            .entry((workflow_id.to_string(), trigger_id.to_string()))
            .or_default();
        bucket.retain(|at| now - *at < RATE_WINDOW_MS);
        if bucket.len() >= RATE_LIMIT_PER_MINUTE {
            return false;
        }
        bucket.push(now);
        true
    }
}

fn not_found() -> FireOutcome {
    FireOutcome::Rejected {
        status: 404,
        code: "not_found",
    }
}

/// Extract the bearer token from the `Authorization` header.
fn bearer(request: &WebhookRequest) -> Option<&str> {
    let value = request.header("authorization")?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Compare two byte strings without leaking the first difference.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

/// The reason phrase for a status code.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        _ => "Internal Server Error",
    }
}

/// The loopback listener that drives a [`WebhookRouter`].
pub struct WebhookListener {
    router: Arc<WebhookRouter>,
    listener: TcpListener,
}

impl WebhookListener {
    /// Bind `addr` (spec §2.3: `127.0.0.1:<port>`; never `0.0.0.0`).
    pub async fn bind(addr: &str, router: Arc<WebhookRouter>) -> Result<Self, WorkflowError> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { router, listener })
    }

    /// The bound address, useful when the port is `0`.
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, WorkflowError> {
        Ok(self.listener.local_addr()?)
    }

    /// Accept requests until `cancel` is requested.
    pub async fn serve(self, cancel: Cancellation) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                accepted = self.listener.accept() => {
                    let Ok((stream, _peer)) = accepted else {
                        tracing::warn!("webhook listener accept failed");
                        break;
                    };
                    let router = Arc::clone(&self.router);
                    tokio::spawn(async move {
                        respond(router, stream).await;
                    });
                }
            }
        }
    }
}

async fn respond(router: Arc<WebhookRouter>, mut stream: TcpStream) {
    let response = match read_request(&mut stream).await {
        Ok(raw) => match WebhookRequest::parse(&raw) {
            Ok(request) => router.handle(&request),
            Err(_) => WebhookResponse::new(400, json!({ "error": "bad_request" })),
        },
        Err(WorkflowError::Invalid(_)) => {
            WebhookResponse::new(413, json!({ "error": "payload_too_large" }))
        }
        // The peer went away mid-request: there is nobody to answer.
        Err(_) => return,
    };
    let _ = stream.write_all(&response.to_http()).await;
    let _ = stream.shutdown().await;
}

async fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, WorkflowError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.len() > MAX_BODY_BYTES + 64 * 1024 {
            return Err(WorkflowError::Invalid("request is too large".to_string()));
        }
        if let Some(head_end) = find_headers_end(&buf) {
            let head = std::str::from_utf8(&buf[..head_end - 4]).unwrap_or_default();
            let length = head
                .split("\r\n")
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if length > MAX_BODY_BYTES {
                return Err(WorkflowError::Invalid("request is too large".to_string()));
            }
            if buf.len() - head_end >= length {
                buf.truncate(head_end + length);
                break;
            }
        }
    }
    Ok(buf)
}

/// The index just past the `\r\n\r\n` separator.
fn find_headers_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_a_request_keeps_the_body_verbatim() {
        let raw = b"POST /hooks/hello/hook HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\n{\"a\":1}";
        let parsed = WebhookRequest::parse(raw).expect("parses");
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.path, "/hooks/hello/hook");
        assert_eq!(parsed.header("host"), Some("localhost"));
        assert_eq!(parsed.json(), Some(json!({ "a": 1 })));
    }

    #[test]
    fn constant_time_equality_matches_sliced_strings() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret-"));
    }
}
