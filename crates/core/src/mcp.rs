//! The MCP server registry and its live host (task 2.17c).
//!
//! Configuration lives in the global store; connections live in the
//! [`McpHost`]. Health is derived from the host, never persisted.

use std::sync::Arc;

use fergents_mcp::{McpError, McpHost, McpServerConfig, NoopMcpSink, StdioTransport, ToolInfo};
use fergents_storage::{Storage, StorageError, Value};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised by the MCP registry.
#[derive(Debug, Error)]
pub enum McpServiceError {
    /// The id is empty or contains whitespace.
    #[error("invalid server id: {0}")]
    InvalidId(String),
    /// No server is configured under that id.
    #[error("unknown mcp server: {0}")]
    Unknown(String),
    /// The host failed to connect or talk to the server.
    #[error(transparent)]
    Host(#[from] McpError),
    /// The row could not be read or written.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// A configured MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerRecord {
    /// Stable id, e.g. `filesystem`.
    pub id: String,
    /// The stdio command, if any.
    pub command: Option<String>,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Whether the host may connect to it.
    pub allow: bool,
    /// Whether the server is currently connected.
    pub connected: bool,
    /// When the server was configured (ms since the epoch).
    pub created_at: i64,
}

/// The outcome of `mcp.server.health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpHealth {
    /// The server id.
    pub id: String,
    /// Whether the host holds a live connection.
    pub connected: bool,
    /// The ping result, when connected.
    pub healthy: Option<bool>,
}

/// The outcome of `mcp.server.test`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTest {
    /// The tested server id.
    pub id: String,
    /// Whether the server answered `initialize` and `tools/list`.
    pub ok: bool,
    /// Failure detail when `ok` is false.
    pub detail: Option<String>,
}

/// A discovered MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTool {
    /// The owning server id.
    pub server: String,
    /// The tool name as advertised.
    pub name: String,
    /// The tool description, if any.
    pub description: String,
}

fn validate_id(id: &str) -> Result<(), McpServiceError> {
    if id.is_empty() || id.len() > 64 || id.chars().any(char::is_whitespace) {
        return Err(McpServiceError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn encode_args(args: &[String]) -> Result<String, McpServiceError> {
    serde_json::to_string(args).map_err(|err| McpServiceError::InvalidId(format!("args: {err}")))
}

fn decode_args(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The MCP registry facade.
pub struct McpService {
    storage: Arc<dyn Storage>,
    host: McpHost,
}

impl McpService {
    /// Build the registry over `storage`, with a fresh host.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            host: McpHost::new(Arc::new(NoopMcpSink)),
        }
    }

    /// Every configured server, oldest first.
    pub fn list(&self) -> Result<Vec<McpServerRecord>, McpServiceError> {
        let rows = self.storage.query(
            "SELECT id, command, args, allow, created_at \
             FROM mcp_servers ORDER BY created_at ASC",
            &[],
        )?;
        let connected = self.host.servers();
        Ok(rows
            .into_iter()
            .map(|row| {
                let id = row
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                McpServerRecord {
                    command: row
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    args: row
                        .get("args")
                        .and_then(Value::as_str)
                        .map(decode_args)
                        .unwrap_or_default(),
                    allow: row.get("allow").and_then(Value::as_i64).unwrap_or(1) != 0,
                    connected: connected.iter().any(|server| server == &id),
                    created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
                    id,
                }
            })
            .collect())
    }

    /// One configured server by id.
    pub fn get(&self, id: &str) -> Result<McpServerRecord, McpServiceError> {
        self.list()?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(|| McpServiceError::Unknown(id.to_string()))
    }

    /// Add or replace a server.
    pub fn add(
        &self,
        id: &str,
        command: Option<&str>,
        args: &[String],
        allow: bool,
    ) -> Result<McpServerRecord, McpServiceError> {
        validate_id(id)?;
        let created_at = self
            .storage
            .query(
                "SELECT created_at FROM mcp_servers WHERE id = ?1",
                &[Value::String(id.to_string())],
            )?
            .first()
            .and_then(|row| row.get("created_at"))
            .and_then(Value::as_i64)
            .unwrap_or_else(now_ms);
        self.storage.execute(
            "INSERT INTO mcp_servers (id, command, args, allow, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(id) DO UPDATE SET command = excluded.command, \
             args = excluded.args, allow = excluded.allow",
            &[
                Value::String(id.to_string()),
                command.map_or(Value::Null, |cmd| Value::String(cmd.to_string())),
                Value::String(encode_args(args)?),
                Value::from(if allow { 1_i64 } else { 0_i64 }),
                Value::from(created_at),
            ],
        )?;
        self.get(id)
    }

    /// Remove a server, disconnecting it first when live.
    pub async fn remove(&self, id: &str) -> Result<bool, McpServiceError> {
        validate_id(id)?;
        if self.host.servers().iter().any(|server| server == id) {
            self.host.disconnect(id).await?;
        }
        let removed = self.storage.execute(
            "DELETE FROM mcp_servers WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(removed > 0)
    }

    /// Connect, ping, and disconnect, reporting whether the server works.
    pub async fn test(&self, id: &str) -> Result<McpTest, McpServiceError> {
        let server = self.get(id)?;
        let Some(command) = server.command.clone() else {
            return Ok(McpTest {
                id: id.to_string(),
                ok: false,
                detail: Some("no command configured".to_string()),
            });
        };
        let config = McpServerConfig::stdio(id, command.clone(), server.args.clone());
        let transport = match StdioTransport::spawn(&command, &server.args).await {
            Ok(transport) => transport,
            Err(err) => {
                return Ok(McpTest {
                    id: id.to_string(),
                    ok: false,
                    detail: Some(err.to_string()),
                });
            }
        };
        let result = match self.host.connect(&config, Arc::new(transport)).await {
            Ok(tools) => {
                let _ = self.host.disconnect(id).await;
                McpTest {
                    id: id.to_string(),
                    ok: true,
                    detail: Some(format!("{} tool(s) discovered", tools.len())),
                }
            }
            Err(err) => {
                let _ = self.host.disconnect(id).await;
                McpTest {
                    id: id.to_string(),
                    ok: false,
                    detail: Some(err.to_string()),
                }
            }
        };
        Ok(result)
    }

    /// Whether a connected server answers pings.
    pub async fn health(&self, id: &str) -> Result<McpHealth, McpServiceError> {
        self.get(id)?;
        let connected = self.host.servers().iter().any(|server| server == id);
        let healthy = if connected {
            Some(self.host.health(id).await?)
        } else {
            None
        };
        Ok(McpHealth {
            id: id.to_string(),
            connected,
            healthy,
        })
    }

    /// Every tool discovered from connected servers.
    pub fn tools(&self) -> Vec<McpTool> {
        self.host
            .all_tools()
            .into_iter()
            .map(
                |(
                    server,
                    ToolInfo {
                        name, description, ..
                    },
                )| McpTool {
                    server,
                    name,
                    description,
                },
            )
            .collect()
    }
}
