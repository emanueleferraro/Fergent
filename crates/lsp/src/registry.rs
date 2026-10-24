//! The global `lsp_servers` registry and language defaults (migration 0013).

use std::collections::BTreeMap;
use std::path::Path;

use fergents_storage::{Row, Storage, Value};

use crate::error::LspError;
use crate::model::{ServerRecord, ServerStatus};

/// A default server for a language: `(command, args)`.
pub fn default_command(language_id: &str) -> Option<(&'static str, &'static [&'static str])> {
    match language_id {
        "rust" => Some(("rust-analyzer", &[])),
        "typescript" | "javascript" => Some(("typescript-language-server", &["--stdio"])),
        "python" => Some(("pyright-langserver", &["--stdio"])),
        "go" => Some(("gopls", &[])),
        "c" | "cpp" => Some(("clangd", &[])),
        _ => None,
    }
}

/// Whether `command` resolves on `PATH`.
pub fn command_on_path(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(env_path) = std::env::var_os("PATH") else {
        return false;
    };
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string())
            .split(';')
            .map(|ext| ext.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&env_path) {
        for extension in &extensions {
            let candidate = if extension.is_empty() {
                dir.join(command)
            } else {
                dir.join(format!("{command}{extension}"))
            };
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

/// List every registered server.
pub fn list_servers(storage: &dyn Storage) -> Result<Vec<ServerRecord>, LspError> {
    let rows = storage.query(
        "SELECT * FROM lsp_servers ORDER BY language_id, command",
        &[],
    )?;
    Ok(rows.iter().map(row_to_record).collect())
}

/// Insert or update a server (unique per `(language_id, command)`).
#[allow(clippy::too_many_arguments)]
pub fn upsert_server(
    storage: &dyn Storage,
    language_id: &str,
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    source: &str,
    enabled: bool,
    now: i64,
) -> Result<ServerRecord, LspError> {
    let args_json =
        serde_json::to_string(args).map_err(|err| LspError::Protocol(err.to_string()))?;
    let env_json = serde_json::to_string(env).map_err(|err| LspError::Protocol(err.to_string()))?;
    storage.execute(
        "INSERT INTO lsp_servers \
         (id, language_id, command, args, env, root_strategy, enabled, source, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'project', ?6, ?7, ?8, ?8) \
         ON CONFLICT(language_id, command) DO UPDATE SET \
         args = excluded.args, env = excluded.env, enabled = excluded.enabled, \
         source = excluded.source, updated_at = excluded.updated_at",
        &[
            string(&ulid::Ulid::new().to_string()),
            string(language_id),
            string(command),
            Value::String(args_json),
            Value::String(env_json),
            Value::from(i64::from(enabled)),
            string(source),
            Value::from(now),
        ],
    )?;
    let rows = storage.query(
        "SELECT * FROM lsp_servers WHERE language_id = ?1 AND command = ?2",
        &[string(language_id), string(command)],
    )?;
    rows.first()
        .map(row_to_record)
        .ok_or_else(|| LspError::UnknownServer(language_id.to_string()))
}

/// Delete a registered server by id.
pub fn remove_server(storage: &dyn Storage, id: &str) -> Result<bool, LspError> {
    let affected = storage.execute("DELETE FROM lsp_servers WHERE id = ?1", &[string(id)])?;
    Ok(affected > 0)
}

/// Resolve the server to use for a language: the first enabled registered
/// one, else the built-in default (not persisted).
pub fn resolve_server(storage: &dyn Storage, language_id: &str) -> Result<ServerRecord, LspError> {
    let rows = storage.query(
        "SELECT * FROM lsp_servers WHERE language_id = ?1 AND enabled = 1 ORDER BY source, id",
        &[string(language_id)],
    )?;
    if let Some(row) = rows.first() {
        return Ok(row_to_record(row));
    }
    let Some((command, args)) = default_command(language_id) else {
        return Err(LspError::UnknownServer(language_id.to_string()));
    };
    Ok(ServerRecord {
        id: String::new(),
        language_id: language_id.to_string(),
        command: command.to_string(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        env: BTreeMap::new(),
        root_strategy: "project".to_string(),
        source: "builtin".to_string(),
        enabled: true,
        status: ServerStatus::Stopped,
        installed: command_on_path(command),
    })
}

/// The `languageId` the protocol reports for a path, if it maps to a server.
pub fn language_for_path(path: &str) -> Option<&'static str> {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        _ => None,
    }
}

/// A `file://` URI for an absolute path.
pub fn file_uri(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.starts_with('/') {
        text = format!("/{text}");
    }
    format!("file://{text}")
}

fn row_to_record(row: &Row) -> ServerRecord {
    let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
    let command = text("command").unwrap_or_default();
    let args: Vec<String> = text("args")
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();
    let env: BTreeMap<String, String> = text("env")
        .and_then(|raw| serde_json::from_str::<BTreeMap<String, String>>(&raw).ok())
        .unwrap_or_default();
    let installed = command_on_path(&command);
    ServerRecord {
        id: text("id").unwrap_or_default(),
        language_id: text("language_id").unwrap_or_default(),
        installed,
        command,
        args,
        env,
        root_strategy: text("root_strategy").unwrap_or_else(|| "project".to_string()),
        source: text("source").unwrap_or_else(|| "user".to_string()),
        enabled: row.get("enabled").and_then(Value::as_i64).unwrap_or(1) != 0,
        status: if installed {
            ServerStatus::Stopped
        } else {
            ServerStatus::NotInstalled
        },
    }
}

fn string(value: &str) -> Value {
    Value::String(value.to_string())
}
