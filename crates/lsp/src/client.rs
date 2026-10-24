//! A framed JSON-RPC client over a child process's stdio.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::codec::{read_message, write_message};
use crate::error::LspError;

/// Called for every server notification (or unhandled server request).
pub type NotificationHandler = Arc<dyn Fn(Value) + Send + Sync>;

/// Env var names containing these fragments are removed from the child.
const SECRET_MARKERS: [&str; 5] = ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY"];

/// One language server process and its request correlation table.
pub struct LspClient {
    child: Mutex<Child>,
    outgoing: mpsc::Sender<Vec<u8>>,
    pending: Arc<Mutex<HashMap<i64, mpsc::Sender<Value>>>>,
    next_id: AtomicI64,
    alive: Arc<AtomicBool>,
}

impl LspClient {
    /// Spawn `command` with `args`, pinned to `cwd`, with secrets scrubbed
    /// from the environment.
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::BTreeMap<String, String>,
        cwd: &Path,
        on_notification: NotificationHandler,
    ) -> Result<Self, LspError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, _) in std::env::vars() {
            if is_secret_key(&key) {
                cmd.env_remove(&key);
            }
        }
        for (key, value) in env {
            if !is_secret_key(key) {
                cmd.env(key, value);
            }
        }

        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Protocol("server stdin unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Protocol("server stdout unavailable".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LspError::Protocol("server stderr unavailable".to_string()))?;

        let alive = Arc::new(AtomicBool::new(true));
        let pending: Arc<Mutex<HashMap<i64, mpsc::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (outgoing, frames) = mpsc::channel::<Vec<u8>>();

        // Writer: frames -> stdin.
        thread::Builder::new()
            .name("fergents-lsp-writer".to_string())
            .spawn(move || {
                let mut stdin = stdin;
                for frame in frames {
                    if stdin.write_all(&frame).is_err() {
                        break;
                    }
                }
            })?;

        // Stderr is drained so a chatty server cannot block on a full pipe.
        thread::Builder::new()
            .name("fergents-lsp-stderr".to_string())
            .spawn(move || {
                use std::io::Read;
                let mut stderr = stderr;
                let mut sink = [0u8; 4096];
                while matches!(stderr.read(&mut sink), Ok(size) if size > 0) {}
            })?;

        // Reader: stdout -> responses / notifications.
        let reader_pending = pending.clone();
        let reader_alive = alive.clone();
        thread::Builder::new()
            .name("fergents-lsp-reader".to_string())
            .spawn(move || {
                let mut reader = std::io::BufReader::new(stdout);
                while let Ok(Some(message)) = read_message(&mut reader) {
                    dispatch(message, &reader_pending, &on_notification);
                }
                reader_alive.store(false, Ordering::SeqCst);
                if let Ok(mut pending) = reader_pending.lock() {
                    pending.clear();
                }
            })?;

        Ok(Self {
            child: Mutex::new(child),
            outgoing,
            pending,
            next_id: AtomicI64::new(1),
            alive,
        })
    }

    /// Whether the process is still running.
    pub fn is_alive(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        match self.child.lock() {
            Ok(mut child) => matches!(child.try_wait(), Ok(None)),
            Err(_) => false,
        }
    }

    /// Send a request and wait up to `timeout` for its response.
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        if !self.is_alive() {
            return Err(LspError::Failed("server is not running".to_string()));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| LspError::Protocol("pending table poisoned".to_string()))?
            .insert(id, tx);
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        match rx.recv_timeout(timeout) {
            Ok(message) => {
                if let Some(error) = message.get("error") {
                    return Err(LspError::Server(error.to_string()));
                }
                Ok(message.get("result").cloned().unwrap_or(Value::Null))
            }
            Err(_) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                Err(LspError::Timeout(method.to_string()))
            }
        }
    }

    /// Send a notification (no response).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    /// Run the `initialize` / `initialized` handshake.
    pub fn initialize(&self, root_uri: &str, timeout: Duration) -> Result<Value, LspError> {
        let name = root_uri.rsplit('/').next().unwrap_or("workspace");
        let result = self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "fergents", "version": env!("CARGO_PKG_VERSION") },
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "didSave": true, "dynamicRegistration": false },
                        "completion": {
                            "completionItem": {
                                "resolveSupport": {
                                    "properties": ["documentation", "detail", "additionalTextEdits"]
                                }
                            }
                        },
                        "hover": {},
                        "definition": {},
                        "references": {},
                        "rename": {},
                        "documentSymbol": {},
                        "formatting": {},
                        "codeAction": {},
                        "signatureHelp": {},
                        "publishDiagnostics": {}
                    },
                    "workspace": { "symbol": {} }
                },
                "workspaceFolders": [{ "uri": root_uri, "name": name }]
            }),
            timeout,
        )?;
        self.notify("initialized", json!({}))?;
        Ok(result)
    }

    /// Graceful `shutdown` + `exit`, then kill if the process lingers.
    pub fn shutdown(&self) {
        let _ = self.request("shutdown", Value::Null, Duration::from_secs(2));
        let _ = self.notify("exit", Value::Null);
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(50));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    }

    fn send(&self, message: &Value) -> Result<(), LspError> {
        let mut frame = Vec::new();
        write_message(&mut frame, message)?;
        self.outgoing
            .send(frame)
            .map_err(|_| LspError::Failed("server stdin is closed".to_string()))
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
}

fn dispatch(
    message: Value,
    pending: &Mutex<HashMap<i64, mpsc::Sender<Value>>>,
    on_notification: &NotificationHandler,
) {
    if let Some(id) = message.get("id").and_then(Value::as_i64) {
        if let Ok(mut pending) = pending.lock() {
            if let Some(sender) = pending.remove(&id) {
                let _ = sender.send(message);
                return;
            }
        }
    }
    if message.get("method").is_some() {
        on_notification(message);
    }
}
