//! Language-server sessions: lifecycle, document sync, requests, diagnostics,
//! and the crash policy (task 3.3).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fergents_events::EventBus;
use fergents_ide::ProjectService;
use fergents_storage::Storage;
use serde_json::{json, Value};

use crate::client::LspClient;
use crate::error::LspError;
use crate::model::{Diagnostic, ServerRecord, ServerStatus};
use crate::registry;

/// At most this many restarts are allowed inside [`RESTART_WINDOW`].
const MAX_RESTARTS: usize = 3;
/// The restart budget window.
const RESTART_WINDOW: Duration = Duration::from_secs(300);
/// Default per-request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// One running language server.
struct Session {
    client: Arc<LspClient>,
}

/// The LSP subsystem.
pub struct LspService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    projects: Arc<ProjectService>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    statuses: Mutex<HashMap<String, ServerStatus>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
    versions: Mutex<HashMap<String, i32>>,
    restarts: Mutex<HashMap<String, RestartState>>,
    timeout: Duration,
}

impl LspService {
    /// Create the service.
    pub fn new(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        projects: Arc<ProjectService>,
    ) -> Self {
        Self::with_timeout(storage, bus, projects, DEFAULT_TIMEOUT)
    }

    /// Create the service with an explicit request timeout (tests).
    pub fn with_timeout(
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        projects: Arc<ProjectService>,
        timeout: Duration,
    ) -> Self {
        Self {
            storage,
            bus,
            projects,
            sessions: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
            diagnostics: Arc::new(Mutex::new(HashMap::new())),
            versions: Mutex::new(HashMap::new()),
            restarts: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    // --- registry ---------------------------------------------------------

    /// List every registered server.
    pub fn servers(&self) -> Result<Vec<ServerRecord>, LspError> {
        registry::list_servers(self.storage.as_ref())
    }

    /// Register a server (upsert on `(language_id, command)`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_server(
        &self,
        language_id: &str,
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        source: &str,
        enabled: bool,
    ) -> Result<ServerRecord, LspError> {
        let record = registry::upsert_server(
            self.storage.as_ref(),
            language_id,
            command,
            args,
            env,
            source,
            enabled,
            fergents_events::now_ms(),
        )?;
        self.bus.publish(
            "lsp.server.registered",
            "lsp",
            json!({
                "language_id": record.language_id,
                "command": record.command,
            }),
        )?;
        Ok(record)
    }

    /// Remove a registered server by id.
    pub fn remove_server(&self, id: &str) -> Result<bool, LspError> {
        registry::remove_server(self.storage.as_ref(), id)
    }

    /// The status of the server serving `language_id` in `project_id`.
    pub fn status(&self, project_id: &str, language_id: &str) -> Result<ServerRecord, LspError> {
        self.projects.require(project_id)?;
        let mut record = registry::resolve_server(self.storage.as_ref(), language_id)?;
        record.status = self.current_status(project_id, language_id, &record);
        Ok(record)
    }

    // --- lifecycle --------------------------------------------------------

    /// Start (or return) the server for `language_id`.
    pub fn start(&self, project_id: &str, language_id: &str) -> Result<ServerRecord, LspError> {
        let project = self.projects.require_trusted(project_id)?;
        let key = session_key(project_id, language_id);
        if let Some(session) = self.session(&key) {
            if session.client.is_alive() {
                let mut record = registry::resolve_server(self.storage.as_ref(), language_id)?;
                record.status = ServerStatus::Ready;
                return Ok(record);
            }
            self.sessions.lock().ok().map(|mut map| map.remove(&key));
        }

        let mut record = registry::resolve_server(self.storage.as_ref(), language_id)?;
        if !record.enabled {
            return Err(LspError::Failed(format!(
                "server for {language_id} is disabled"
            )));
        }
        if !record.installed {
            self.set_status(&key, ServerStatus::NotInstalled)?;
            record.status = ServerStatus::NotInstalled;
            return Ok(record);
        }

        self.set_status(&key, ServerStatus::Starting)?;
        self.bus.publish(
            "lsp.server.starting",
            "lsp",
            json!({
                "project_id": project_id,
                "language_id": language_id,
                "command": record.command,
            }),
        )?;

        let root = PathBuf::from(&project.path);
        let root_uri = registry::file_uri(&root);
        let handler = self.notification_handler(project_id, root.clone());
        let client =
            match LspClient::spawn(&record.command, &record.args, &record.env, &root, handler) {
                Ok(client) => Arc::new(client),
                Err(err) => {
                    self.set_status(&key, ServerStatus::Failed)?;
                    self.publish_failure(project_id, language_id, &err);
                    return Err(err);
                }
            };

        if let Err(err) = client.initialize(&root_uri, self.timeout) {
            client.shutdown();
            self.set_status(&key, ServerStatus::Failed)?;
            self.publish_failure(project_id, language_id, &err);
            return Err(err);
        }

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(key.clone(), Arc::new(Session { client }));
        }
        self.set_status(&key, ServerStatus::Ready)?;
        self.bus.publish(
            "lsp.server.ready",
            "lsp",
            json!({ "project_id": project_id, "language_id": language_id }),
        )?;
        record.status = ServerStatus::Ready;
        Ok(record)
    }

    /// Stop the server for `language_id`, if running.
    pub fn stop(&self, project_id: &str, language_id: &str) -> Result<bool, LspError> {
        self.projects.require(project_id)?;
        let key = session_key(project_id, language_id);
        let session = self
            .sessions
            .lock()
            .map_err(|_| LspError::Protocol("session table poisoned".to_string()))?
            .remove(&key);
        let Some(session) = session else {
            return Ok(false);
        };
        session.client.shutdown();
        self.set_status(&key, ServerStatus::Stopped)?;
        self.bus.publish(
            "lsp.server.stopped",
            "lsp",
            json!({ "project_id": project_id, "language_id": language_id }),
        )?;
        Ok(true)
    }

    /// Restart the server for `language_id`.
    pub fn restart(&self, project_id: &str, language_id: &str) -> Result<ServerRecord, LspError> {
        self.stop(project_id, language_id)?;
        if let Ok(mut restarts) = self.restarts.lock() {
            restarts.remove(&session_key(project_id, language_id));
        }
        self.start(project_id, language_id)
    }

    // --- documents --------------------------------------------------------

    /// Open a document (`textDocument/didOpen`, version 1).
    pub fn document_open(
        &self,
        project_id: &str,
        path: &str,
        text: &str,
        language_id: Option<&str>,
    ) -> Result<(), LspError> {
        let language = language_id
            .map(str::to_string)
            .or_else(|| registry::language_for_path(path).map(str::to_string))
            .ok_or_else(|| LspError::UnknownServer(path.to_string()))?;
        let session = self.ensure_session(project_id, &language)?;
        let uri = self.uri(project_id, path)?;
        self.set_version(project_id, path, 1)?;
        session.client.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language,
                    "version": 1,
                    "text": text,
                }
            }),
        )
    }

    /// Apply a change (`textDocument/didChange`) with a bumped version.
    pub fn document_change(
        &self,
        project_id: &str,
        path: &str,
        text: &str,
        range: Option<Value>,
    ) -> Result<i32, LspError> {
        let version = self.next_version(project_id, path)?;
        let session = self.session_for_path(project_id, path)?;
        let uri = self.uri(project_id, path)?;
        let change = match range {
            Some(range) => json!({ "range": range, "text": text }),
            None => json!({ "text": text }),
        };
        session.client.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [change],
            }),
        )?;
        Ok(version)
    }

    /// Save a document (`textDocument/didSave`).
    pub fn document_save(
        &self,
        project_id: &str,
        path: &str,
        text: Option<&str>,
    ) -> Result<(), LspError> {
        let session = self.session_for_path(project_id, path)?;
        let uri = self.uri(project_id, path)?;
        let params = match text {
            Some(text) => json!({ "textDocument": { "uri": uri }, "text": text }),
            None => json!({ "textDocument": { "uri": uri } }),
        };
        session.client.notify("textDocument/didSave", params)
    }

    /// Close a document (`textDocument/didClose`).
    pub fn document_close(&self, project_id: &str, path: &str) -> Result<(), LspError> {
        let session = self.session_for_path(project_id, path)?;
        let uri = self.uri(project_id, path)?;
        if let Ok(mut versions) = self.versions.lock() {
            versions.remove(&document_key(project_id, path));
        }
        session.client.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    /// The version last sent for a document, if it is open.
    pub fn document_version(&self, project_id: &str, path: &str) -> Option<i32> {
        self.versions
            .lock()
            .ok()
            .and_then(|versions| versions.get(&document_key(project_id, path)).copied())
    }

    // --- requests ---------------------------------------------------------

    /// `textDocument/completion`.
    pub fn completion(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        self.position_request(project_id, path, "textDocument/completion", line, character)
    }

    /// `completionItem/resolve`.
    pub fn completion_resolve(
        &self,
        project_id: &str,
        path: &str,
        item: Value,
    ) -> Result<Value, LspError> {
        let session = self.session_for_path(project_id, path)?;
        session
            .client
            .request("completionItem/resolve", item, self.timeout)
    }

    /// `textDocument/hover`.
    pub fn hover(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        self.position_request(project_id, path, "textDocument/hover", line, character)
    }

    /// `textDocument/definition`.
    pub fn definition(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        self.position_request(project_id, path, "textDocument/definition", line, character)
    }

    /// `textDocument/references`.
    pub fn references(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        let extra = json!({
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": true },
        });
        self.request_at(project_id, path, "textDocument/references", extra)
    }

    /// `textDocument/rename`.
    pub fn rename(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<Value, LspError> {
        let extra = json!({
            "position": { "line": line, "character": character },
            "newName": new_name,
        });
        self.request_at(project_id, path, "textDocument/rename", extra)
    }

    /// `textDocument/prepareRename`.
    pub fn prepare_rename(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        self.position_request(
            project_id,
            path,
            "textDocument/prepareRename",
            line,
            character,
        )
    }

    /// `textDocument/documentSymbol`.
    pub fn symbols(&self, project_id: &str, path: &str) -> Result<Value, LspError> {
        self.request_at(project_id, path, "textDocument/documentSymbol", json!({}))
    }

    /// `workspace/symbol`.
    pub fn workspace_symbols(
        &self,
        project_id: &str,
        path: &str,
        query: &str,
    ) -> Result<Value, LspError> {
        let session = self.session_for_path(project_id, path)?;
        session
            .client
            .request("workspace/symbol", json!({ "query": query }), self.timeout)
    }

    /// `textDocument/formatting`.
    pub fn format(
        &self,
        project_id: &str,
        path: &str,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Value, LspError> {
        let extra = json!({
            "options": { "tabSize": tab_size, "insertSpaces": insert_spaces },
        });
        self.request_at(project_id, path, "textDocument/formatting", extra)
    }

    /// `textDocument/codeAction`.
    pub fn code_action(
        &self,
        project_id: &str,
        path: &str,
        range: Value,
        diagnostics: Value,
    ) -> Result<Value, LspError> {
        let extra = json!({ "range": range, "context": { "diagnostics": diagnostics } });
        self.request_at(project_id, path, "textDocument/codeAction", extra)
    }

    /// `textDocument/signatureHelp`.
    pub fn signature_help(
        &self,
        project_id: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        self.position_request(
            project_id,
            path,
            "textDocument/signatureHelp",
            line,
            character,
        )
    }

    // --- diagnostics ------------------------------------------------------

    /// Diagnostics for one file, or every file in the project.
    pub fn diagnostics(&self, project_id: &str, path: Option<&str>) -> Vec<Diagnostic> {
        let Ok(diagnostics) = self.diagnostics.lock() else {
            return Vec::new();
        };
        match path {
            Some(path) => diagnostics
                .get(&document_key(project_id, path))
                .cloned()
                .unwrap_or_default(),
            None => {
                let prefix = format!("{project_id}|");
                let mut out: Vec<Diagnostic> = diagnostics
                    .iter()
                    .filter(|(key, _)| key.starts_with(&prefix))
                    .flat_map(|(_, list)| list.clone())
                    .collect();
                out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
                out
            }
        }
    }

    // --- internals --------------------------------------------------------

    fn session(&self, key: &str) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(key).cloned())
    }

    fn set_status(&self, key: &str, status: ServerStatus) -> Result<(), LspError> {
        self.statuses
            .lock()
            .map_err(|_| LspError::Protocol("status table poisoned".to_string()))?
            .insert(key.to_string(), status);
        Ok(())
    }

    fn current_status(
        &self,
        project_id: &str,
        language_id: &str,
        record: &ServerRecord,
    ) -> ServerStatus {
        let key = session_key(project_id, language_id);
        if let Some(status) = self
            .statuses
            .lock()
            .ok()
            .and_then(|statuses| statuses.get(&key).copied())
        {
            if status == ServerStatus::Ready {
                let alive = self
                    .session(&key)
                    .map(|session| session.client.is_alive())
                    .unwrap_or(false);
                return if alive {
                    ServerStatus::Ready
                } else {
                    ServerStatus::Stopped
                };
            }
            return status;
        }
        if !record.installed {
            ServerStatus::NotInstalled
        } else {
            ServerStatus::Stopped
        }
    }

    fn session_for_path(&self, project_id: &str, path: &str) -> Result<Arc<Session>, LspError> {
        let language = registry::language_for_path(path)
            .ok_or_else(|| LspError::UnknownServer(path.to_string()))?;
        self.ensure_session(project_id, language)
    }

    fn ensure_session(&self, project_id: &str, language: &str) -> Result<Arc<Session>, LspError> {
        let key = session_key(project_id, language);
        if let Some(session) = self.session(&key) {
            if session.client.is_alive() {
                return Ok(session);
            }
            if let Ok(mut sessions) = self.sessions.lock() {
                sessions.remove(&key);
            }
        }
        if !self.restart_allowed(&key) {
            self.set_status(&key, ServerStatus::Failed)?;
            self.publish_failure(
                project_id,
                language,
                &LspError::Failed("restart budget exhausted".to_string()),
            );
            return Err(LspError::Failed(format!(
                "server for {language} failed repeatedly and will not be restarted"
            )));
        }
        self.start(project_id, language)?;
        self.session(&key)
            .ok_or_else(|| LspError::Failed(format!("server for {language} did not start")))
    }

    fn request_at(
        &self,
        project_id: &str,
        path: &str,
        method: &str,
        extra: Value,
    ) -> Result<Value, LspError> {
        let session = self.session_for_path(project_id, path)?;
        let uri = self.uri(project_id, path)?;
        let mut params = json!({ "textDocument": { "uri": uri } });
        if let (Some(target), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                target.insert(key.clone(), value.clone());
            }
        }
        session.client.request(method, params, self.timeout)
    }

    fn position_request(
        &self,
        project_id: &str,
        path: &str,
        method: &str,
        line: u32,
        character: u32,
    ) -> Result<Value, LspError> {
        let extra = json!({ "position": { "line": line, "character": character } });
        self.request_at(project_id, path, method, extra)
    }

    fn uri(&self, project_id: &str, path: &str) -> Result<String, LspError> {
        let project = self.projects.require(project_id)?;
        let absolute = self.projects.resolve(&project, path)?;
        Ok(registry::file_uri(&absolute))
    }

    fn set_version(&self, project_id: &str, path: &str, version: i32) -> Result<(), LspError> {
        self.versions
            .lock()
            .map_err(|_| LspError::Protocol("version table poisoned".to_string()))?
            .insert(document_key(project_id, path), version);
        Ok(())
    }

    fn next_version(&self, project_id: &str, path: &str) -> Result<i32, LspError> {
        let mut versions = self
            .versions
            .lock()
            .map_err(|_| LspError::Protocol("version table poisoned".to_string()))?;
        let version = versions
            .get_mut(&document_key(project_id, path))
            .ok_or_else(|| LspError::DocumentNotOpen(path.to_string()))?;
        *version += 1;
        Ok(*version)
    }

    fn restart_allowed(&self, key: &str) -> bool {
        let Ok(mut restarts) = self.restarts.lock() else {
            return false;
        };
        restarts
            .entry(key.to_string())
            .or_default()
            .allow(Instant::now())
    }

    fn publish_failure(&self, project_id: &str, language_id: &str, err: &LspError) {
        let _ = self.bus.publish(
            "lsp.server.failed",
            "lsp",
            json!({
                "project_id": project_id,
                "language_id": language_id,
                "error": err.to_string(),
            }),
        );
    }

    fn notification_handler(
        &self,
        project_id: &str,
        root: PathBuf,
    ) -> crate::client::NotificationHandler {
        let diagnostics = self.diagnostics.clone();
        let bus = self.bus.clone();
        let project_id = project_id.to_string();
        Arc::new(move |message: Value| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if method == "textDocument/publishDiagnostics" {
                let params = &message["params"];
                let uri = params["uri"].as_str().unwrap_or_default();
                let path = uri_to_relative(uri, &root);
                let list = parse_diagnostics(&path, params);
                if let Ok(mut store) = diagnostics.lock() {
                    store.insert(document_key(&project_id, &path), list.clone());
                }
                let _ = bus.publish(
                    "lsp.diagnostics",
                    "lsp",
                    json!({ "project_id": project_id, "path": path, "diagnostics": list }),
                );
            } else {
                let _ = bus.publish(
                    "lsp.log",
                    "lsp",
                    json!({ "project_id": project_id, "message": message }),
                );
            }
        })
    }
}

/// The restart budget for one session.
#[derive(Debug, Default)]
struct RestartState {
    attempts: Vec<Instant>,
}

impl RestartState {
    /// Record an attempt; `false` when the budget is exhausted.
    fn allow(&mut self, now: Instant) -> bool {
        self.attempts
            .retain(|attempt| now.duration_since(*attempt) < RESTART_WINDOW);
        if self.attempts.len() >= MAX_RESTARTS {
            return false;
        }
        self.attempts.push(now);
        true
    }
}

fn session_key(project_id: &str, language_id: &str) -> String {
    format!("{project_id}|{language_id}")
}

fn document_key(project_id: &str, path: &str) -> String {
    format!("{project_id}|{}", path.replace('\\', "/"))
}

fn uri_to_relative(uri: &str, root: &Path) -> String {
    let decoded = uri.strip_prefix("file://").unwrap_or(uri);
    let root_uri = registry::file_uri(root);
    let root_path = root_uri.trim_start_matches("file://");
    decoded
        .strip_prefix(root_path)
        .unwrap_or(decoded)
        .trim_start_matches('/')
        .replace('\\', "/")
}

fn parse_diagnostics(path: &str, params: &Value) -> Vec<Diagnostic> {
    let Some(items) = params.get("diagnostics").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let range = &item["range"];
            Diagnostic {
                path: path.to_string(),
                line: range["start"]["line"].as_u64().unwrap_or(0) as u32,
                column: range["start"]["character"].as_u64().unwrap_or(0) as u32,
                end_line: range["end"]["line"].as_u64().unwrap_or(0) as u32,
                end_column: range["end"]["character"].as_u64().unwrap_or(0) as u32,
                severity: item["severity"].as_u64().unwrap_or(1) as u8,
                message: item["message"].as_str().unwrap_or_default().to_string(),
                source: item["source"].as_str().map(str::to_string),
                code: match &item["code"] {
                    Value::String(code) => Some(code.clone()),
                    Value::Number(code) => Some(code.to_string()),
                    _ => None,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_budget_allows_three_in_the_window() {
        let now = Instant::now();
        let mut state = RestartState::default();
        assert!(state.allow(now));
        assert!(state.allow(now));
        assert!(state.allow(now));
        assert!(!state.allow(now));
    }

    #[test]
    fn restart_budget_recovers_after_the_window() {
        let start = Instant::now();
        let mut state = RestartState::default();
        assert!(state.allow(start));
        assert!(state.allow(start));
        assert!(state.allow(start));
        assert!(!state.allow(start));
        assert!(state.allow(start + RESTART_WINDOW + Duration::from_secs(1)));
    }

    #[test]
    fn uris_map_back_to_relative_paths() {
        let root = Path::new("/work/repo");
        assert_eq!(
            uri_to_relative("file:///work/repo/src/main.rs", root),
            "src/main.rs"
        );
    }
}
