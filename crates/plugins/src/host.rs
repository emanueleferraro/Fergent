//! The gated host bridge (`PLUGINS.md` §6, spec §2).
//!
//! Every operation requires a stored grant. Paths are canonicalized and
//! matched against the declared globs; URLs, models, and MCP servers are
//! checked against their scopes; risky calls are also checked against the
//! kernel permission policy (denials only — install consent is the prompt).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use fergents_events::EventBus;
use fergents_permissions::{Decision, PermissionRequest, PermissionService};
use fergents_storage::{Key, Storage, Value};
use fergents_wasm::{HostBridge, HostError};
use serde_json::json;

/// The grant map: category -> `None` (unscoped) or declared scopes.
pub type Grants = HashMap<String, Option<Vec<String>>>;

/// Kernel services a plugin may reach through host calls.
///
/// Implemented by `fergents-core`; tests use [`UnavailableHostServices`].
pub trait HostServices: Send + Sync {
    /// The kernel data directory (`$FERGENTS_HOME`).
    fn data_dir(&self) -> &Path;

    /// The primary brain root, when one is open.
    fn brain_root(&self) -> Option<PathBuf> {
        None
    }

    /// The active project root, when one is open.
    fn project_root(&self) -> Option<PathBuf> {
        None
    }

    /// Perform an HTTP request and return the response body.
    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &serde_json::Value,
        _body: Option<&str>,
    ) -> Result<String, String> {
        Err("http is unavailable".to_string())
    }

    /// Call an LLM and return the raw text.
    fn llm_chat(&self, _model: &str, _messages: &serde_json::Value) -> Result<String, String> {
        Err("llm is unavailable".to_string())
    }

    /// Call an MCP tool.
    fn mcp_call(
        &self,
        _server: &str,
        _tool: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err("mcp is unavailable".to_string())
    }

    /// Search the brain and return JSON results.
    fn brain_search(&self, _query: &str, _top_k: usize) -> Result<String, String> {
        Err("brain is unavailable".to_string())
    }

    /// Read a brain note.
    fn brain_read(&self, _path: &str) -> Result<String, String> {
        Err("brain is unavailable".to_string())
    }

    /// Write a brain note.
    fn brain_write(&self, _path: &str, _content: &str) -> Result<(), String> {
        Err("brain is unavailable".to_string())
    }

    /// Read a secret value by name.
    fn secret(&self, _name: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Show a kernel notification.
    fn notify(&self, _title: &str, _body: &str) -> Result<(), String> {
        Ok(())
    }
}

/// A host-services implementation that reaches nothing (tests, headless).
pub struct UnavailableHostServices {
    data_dir: PathBuf,
}

impl UnavailableHostServices {
    /// Build it for `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

impl HostServices for UnavailableHostServices {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// The per-plugin host bridge.
pub struct PluginHostBridge {
    plugin_id: String,
    grants: Grants,
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    host: Arc<dyn HostServices>,
    policy: Option<Arc<PermissionService>>,
}

impl PluginHostBridge {
    /// Build a bridge for one plugin.
    pub fn new(
        plugin_id: impl Into<String>,
        grants: Grants,
        storage: Arc<dyn Storage>,
        bus: Arc<EventBus>,
        host: Arc<dyn HostServices>,
        policy: Option<Arc<PermissionService>>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            grants,
            storage,
            bus,
            host,
            policy,
        }
    }

    fn check(&self, category: &str) -> Result<Option<&[String]>, HostError> {
        match self.grants.get(category) {
            Some(None) => Ok(None),
            Some(Some(scopes)) => Ok(Some(scopes)),
            None => {
                self.deny(category, "no grant");
                Err(HostError::denied(format!(
                    "plugin {} has no grant for {category}",
                    self.plugin_id
                )))
            }
        }
    }

    fn deny(&self, category: &str, detail: &str) {
        let _ = self.bus.publish(
            "plugin.permission.denied",
            "plugins",
            json!({
                "plugin_id": self.plugin_id,
                "category": category,
                "detail": detail,
            }),
        );
    }

    fn policy_allows(&self, tool: &str, arguments: serde_json::Value) -> bool {
        let Some(policy) = &self.policy else {
            return true;
        };
        let request = PermissionRequest::new(tool, arguments);
        match policy.check(&request) {
            Ok(resolved) => resolved.decision != Decision::Deny,
            Err(_) => true,
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.host.data_dir().to_path_buf()
    }

    fn plugin_data(&self) -> PathBuf {
        self.data_dir()
            .join("plugins")
            .join("data")
            .join(&self.plugin_id)
    }

    fn expand(&self, raw: &str) -> Result<PathBuf, HostError> {
        let (variable, rest) = match raw.split_once('/') {
            Some((variable, rest)) => (variable, format!("/{rest}")),
            None => (raw, String::new()),
        };
        let base = match variable {
            "$FERGENTS_HOME" => self.data_dir(),
            "$CACHE" => self.data_dir().join("cache"),
            "$PLUGIN_DATA" => self.plugin_data(),
            "$BRAIN" => self
                .host
                .brain_root()
                .ok_or_else(|| HostError::denied("no brain is open"))?,
            "$PROJECT" => self
                .host
                .project_root()
                .ok_or_else(|| HostError::denied("no project is open"))?,
            _ => PathBuf::from(raw),
        };
        let joined = format!("{}{}", base.display(), rest);
        normalize(&PathBuf::from(joined))
    }

    fn scoped(&self, category: &str, value: &str) -> Result<(), HostError> {
        let scopes = self.check(category)?;
        if scopes.is_none() {
            return Ok(());
        }
        let scopes = scopes.unwrap_or(&[]);
        if matches_any(scopes, value) {
            Ok(())
        } else {
            self.deny(category, value);
            Err(HostError::denied(format!(
                "{value} is outside the {category} scope"
            )))
        }
    }

    fn store_get(&self, key: &str) -> Result<Option<String>, HostError> {
        let rows = self
            .storage
            .query(
                "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
                &[
                    Value::String(self.plugin_id.clone()),
                    Value::String(key.to_string()),
                ],
            )
            .map_err(|err| HostError::failed(err.to_string()))?;
        Ok(rows
            .first()
            .and_then(|row| row.get("value"))
            .and_then(|value| value.as_str())
            .map(str::to_string))
    }

    fn store_set(&self, key: &str, value: &str) -> Result<(), HostError> {
        if key.len() > 256 || value.len() > 1024 * 1024 {
            return Err(HostError::failed("storage entry is too large"));
        }
        let used: i64 = self
            .storage
            .query(
                "SELECT COALESCE(SUM(LENGTH(value)), 0) AS used FROM plugin_storage WHERE plugin_id = ?1",
                &[Value::String(self.plugin_id.clone())],
            )
            .map_err(|err| HostError::failed(err.to_string()))?
            .first()
            .and_then(|row| row.get("used"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if used + value.len() as i64 > 5 * 1024 * 1024 {
            return Err(HostError::failed("plugin storage quota exceeded (5 MiB)"));
        }
        self.storage
            .execute(
                "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
                &[
                    Value::String(self.plugin_id.clone()),
                    Value::String(key.to_string()),
                    Value::String(value.to_string()),
                    Value::from(fergents_events::now_ms()),
                ],
            )
            .map_err(|err| HostError::failed(err.to_string()))?;
        Ok(())
    }

    fn store_delete(&self, key: &str) -> Result<(), HostError> {
        self.storage
            .execute(
                "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
                &[
                    Value::String(self.plugin_id.clone()),
                    Value::String(key.to_string()),
                ],
            )
            .map_err(|err| HostError::failed(err.to_string()))?;
        Ok(())
    }

    fn store_keys(&self) -> Result<Vec<String>, HostError> {
        self.storage
            .query(
                "SELECT key FROM plugin_storage WHERE plugin_id = ?1 ORDER BY key",
                &[Value::String(self.plugin_id.clone())],
            )
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.get("key").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .map_err(|err| HostError::failed(err.to_string()))
    }
}

impl HostBridge for PluginHostBridge {
    fn call(&self, op: &str, payload: &str) -> Result<String, HostError> {
        let payload: serde_json::Value = if payload.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(payload)
                .map_err(|err| HostError::failed(format!("invalid payload: {err}")))?
        };
        match op {
            "log.debug" | "log.info" | "log.warn" | "log.error" => {
                let message = string_field(&payload, "message")?;
                tracing::info!(plugin = %self.plugin_id, op, message, "plugin log");
                Ok("{}".to_string())
            }
            "fs.read" => {
                let raw = string_field(&payload, "path")?;
                let path = self.expand(raw)?;
                self.scoped("fs.read", &display_path(&path))?;
                if !self.policy_allows("fs.read", json!({ "path": path })) {
                    self.deny("fs.read", raw);
                    return Err(HostError::denied("permission policy denied fs.read"));
                }
                let bytes =
                    std::fs::read(&path).map_err(|err| HostError::failed(err.to_string()))?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return Err(HostError::failed("file exceeds 8 MiB"));
                }
                let text = String::from_utf8(bytes)
                    .map_err(|_| HostError::failed("file is not valid UTF-8"))?;
                Ok(json!({ "content": text }).to_string())
            }
            "fs.write" => {
                let raw = string_field(&payload, "path")?;
                let path = self.expand(raw)?;
                self.scoped("fs.write", &display_path(&path))?;
                if !self.policy_allows("fs.write", json!({ "path": path })) {
                    self.deny("fs.write", raw);
                    return Err(HostError::denied("permission policy denied fs.write"));
                }
                let content = string_field(&payload, "content")?;
                if content.len() > 8 * 1024 * 1024 {
                    return Err(HostError::failed("payload exceeds 8 MiB"));
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|err| HostError::failed(err.to_string()))?;
                }
                std::fs::write(&path, content).map_err(|err| HostError::failed(err.to_string()))?;
                Ok("{}".to_string())
            }
            "fs.list" => {
                let raw = string_field(&payload, "path")?;
                let path = self.expand(raw)?;
                self.scoped("fs.read", &display_path(&path))?;
                let mut names = Vec::new();
                for entry in
                    std::fs::read_dir(&path).map_err(|err| HostError::failed(err.to_string()))?
                {
                    let entry = entry.map_err(|err| HostError::failed(err.to_string()))?;
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
                names.sort();
                Ok(json!({ "entries": names }).to_string())
            }
            "fs.delete" => {
                let path = resolve_scoped(self, "fs.write", &payload, "path")?;
                if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                }
                .map_err(|err| HostError::failed(err.to_string()))?;
                Ok("{}".to_string())
            }
            "fs.stat" => {
                let path = resolve_scoped(self, "fs.read", &payload, "path")?;
                let metadata = std::fs::metadata(&path).ok();
                Ok(json!({
                    "exists": metadata.is_some(),
                    "is_dir": metadata.as_ref().is_some_and(|m| m.is_dir()),
                    "len": metadata.map_or(0, |m| m.len()),
                })
                .to_string())
            }
            "http.request" => {
                self.check("net.http")?;
                let url = string_field(&payload, "url")?;
                self.scoped("net.http", url)?;
                if !self.policy_allows("web.fetch", json!({ "url": url })) {
                    self.deny("net.http", url);
                    return Err(HostError::denied("permission policy denied net.http"));
                }
                let method = payload
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("GET");
                let headers = payload.get("headers").cloned().unwrap_or(json!({}));
                let body = payload.get("body").and_then(serde_json::Value::as_str);
                let body = self
                    .host
                    .http_request(method, url, &headers, body)
                    .map_err(HostError::failed)?;
                Ok(json!({ "body": body }).to_string())
            }
            "llm.chat" => {
                self.check("llm.call")?;
                let model = string_field(&payload, "model")?;
                self.scoped("llm.call", model)?;
                if !self.policy_allows("llm.chat", json!({ "model": model })) {
                    self.deny("llm.call", model);
                    return Err(HostError::denied("permission policy denied llm.call"));
                }
                let messages = payload.get("messages").cloned().unwrap_or(json!([]));
                let text = self
                    .host
                    .llm_chat(model, &messages)
                    .map_err(HostError::failed)?;
                Ok(json!({ "text": text }).to_string())
            }
            "mcp.call" => {
                self.check("mcp.connect")?;
                let server = string_field(&payload, "server")?;
                self.scoped("mcp.connect", server)?;
                if !self.policy_allows("mcp.call", json!({ "server": server })) {
                    self.deny("mcp.connect", server);
                    return Err(HostError::denied("permission policy denied mcp.connect"));
                }
                let tool = string_field(&payload, "tool")?;
                let arguments = payload.get("arguments").cloned().unwrap_or(json!({}));
                let result = self
                    .host
                    .mcp_call(server, tool, &arguments)
                    .map_err(HostError::failed)?;
                Ok(json!({ "result": result }).to_string())
            }
            "storage.get" => {
                self.check("storage.local")?;
                let key = string_field(&payload, "key")?;
                Ok(serde_json::to_value(self.store_get(key)?)
                    .unwrap_or(serde_json::Value::Null)
                    .to_string())
            }
            "storage.set" => {
                self.check("storage.local")?;
                let key = string_field(&payload, "key")?;
                let value = payload
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                self.store_set(key, &value.to_string())?;
                Ok("{}".to_string())
            }
            "storage.delete" => {
                self.check("storage.local")?;
                let key = string_field(&payload, "key")?;
                self.store_delete(key)?;
                Ok("{}".to_string())
            }
            "storage.list" => {
                self.check("storage.local")?;
                Ok(json!({ "keys": self.store_keys()? }).to_string())
            }
            "event.emit" => {
                let topic = string_field(&payload, "topic")?;
                self.scoped("events.emit", topic)?;
                let body = payload.get("payload").cloned().unwrap_or(json!({}));
                let namespaced = format!("plugin.{}.{}", self.plugin_id, topic);
                self.bus
                    .publish(namespaced, format!("plugin:{}", self.plugin_id), body)
                    .map_err(|err| HostError::failed(err.to_string()))?;
                Ok("{}".to_string())
            }
            "event.subscribe" | "event.unsubscribe" => Ok("{}".to_string()),
            "secrets.get" => {
                let name = string_field(&payload, "name")?;
                self.scoped("secrets.get", name)?;
                let value = self.host.secret(name).map_err(HostError::failed)?;
                Ok(json!({ "value": value }).to_string())
            }
            "brain.search" => {
                self.check("brain.read")?;
                let query = string_field(&payload, "query")?;
                let top_k = payload
                    .get("top_k")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(8) as usize;
                let results = self
                    .host
                    .brain_search(query, top_k)
                    .map_err(HostError::failed)?;
                Ok(results)
            }
            "brain.read" => {
                self.check("brain.read")?;
                let path = string_field(&payload, "path")?;
                let content = self.host.brain_read(path).map_err(HostError::failed)?;
                Ok(json!({ "content": content }).to_string())
            }
            "brain.write" => {
                self.check("brain.write")?;
                if !self.policy_allows("brain.write", payload.clone()) {
                    self.deny("brain.write", "policy");
                    return Err(HostError::denied("permission policy denied brain.write"));
                }
                let path = string_field(&payload, "path")?;
                let content = string_field(&payload, "content")?;
                self.host
                    .brain_write(path, content)
                    .map_err(HostError::failed)?;
                Ok("{}".to_string())
            }
            "ui.notify" => {
                self.check("ui.notify")?;
                let title = string_field(&payload, "title")?;
                let body = payload
                    .get("body")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                self.host.notify(title, body).map_err(HostError::failed)?;
                Ok("{}".to_string())
            }
            other => Err(HostError::unsupported(other)),
        }
    }
}

fn resolve_scoped(
    bridge: &PluginHostBridge,
    category: &str,
    payload: &serde_json::Value,
    field: &str,
) -> Result<PathBuf, HostError> {
    let raw = string_field(payload, field)?;
    let path = bridge.expand(raw)?;
    bridge.scoped(category, &display_path(&path))?;
    Ok(path)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize(path: &Path) -> Result<PathBuf, HostError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => return Err(HostError::denied("path traversal is not allowed")),
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn matches_any(scopes: &[String], value: &str) -> bool {
    scopes.iter().any(|scope| {
        let Ok(glob) = globset::Glob::new(scope) else {
            return false;
        };
        glob.compile_matcher().is_match(value)
    })
}

fn string_field<'a>(payload: &'a serde_json::Value, key: &str) -> Result<&'a str, HostError> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| HostError::failed(format!("missing {key}")))
}

/// Read a plugin setting from the global settings store.
pub fn setting_key(plugin_id: &str, key: &str) -> Key {
    Key(format!("plugin.{plugin_id}.{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matching_uses_globs() {
        assert!(matches_any(
            &["/home/me/plugins/data/x/**".to_string()],
            "/home/me/plugins/data/x/a/b.txt"
        ));
        assert!(!matches_any(
            &["/home/me/plugins/data/x/**".to_string()],
            "/home/me/plugins/data/y/a.txt"
        ));
    }

    #[test]
    fn normalize_rejects_parent_dirs() {
        assert!(normalize(Path::new("/a/../b")).is_err());
        assert_eq!(
            normalize(Path::new("/a/./b")).unwrap(),
            PathBuf::from("/a/b")
        );
    }
}
