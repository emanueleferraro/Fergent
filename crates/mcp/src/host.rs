//! The MCP host: a registry of connected servers, health, and events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::client::McpClient;
use crate::error::McpError;
use crate::transport::Transport;
use crate::types::ToolInfo;

/// An event emitted by the host (`MCP.md` §13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpEvent {
    /// A server connected.
    Connected {
        /// The server id.
        server: String,
    },
    /// A server disconnected.
    Disconnected {
        /// The server id.
        server: String,
    },
    /// A server errored.
    Error {
        /// The server id.
        server: String,
        /// The error message.
        message: String,
    },
    /// A tool was discovered and registered.
    ToolRegistered {
        /// The server id.
        server: String,
        /// The tool name.
        tool: String,
    },
}

/// A sink for [`McpEvent`]s. The kernel adapts the event bus to this trait.
pub trait McpEventSink: Send + Sync {
    /// Emit an event.
    fn emit(&self, event: McpEvent);
}

/// A sink that drops every event.
#[derive(Debug, Default)]
pub struct NoopMcpSink;

impl McpEventSink for NoopMcpSink {
    fn emit(&self, _event: McpEvent) {}
}

/// A test sink that records events in memory.
#[derive(Debug, Default)]
pub struct RecordingMcpSink {
    events: Mutex<Vec<McpEvent>>,
}

impl RecordingMcpSink {
    /// Create an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// The events recorded so far.
    pub fn events(&self) -> Vec<McpEvent> {
        self.events
            .lock()
            .expect("recording mcp sink poisoned")
            .clone()
    }
}

impl McpEventSink for RecordingMcpSink {
    fn emit(&self, event: McpEvent) {
        self.events
            .lock()
            .expect("recording mcp sink poisoned")
            .push(event);
    }
}

/// Configuration for one MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    /// The server id.
    pub id: String,
    /// The command to spawn (for stdio transports), if any.
    pub command: Option<String>,
    /// The command arguments.
    pub args: Vec<String>,
    /// Whether `mcp.connect` policy allows this server.
    pub allow: bool,
}

impl McpServerConfig {
    /// A config with no command and policy allowed.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            command: None,
            args: Vec::new(),
            allow: true,
        }
    }

    /// A stdio config.
    pub fn stdio(id: impl Into<String>, command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            id: id.into(),
            command: Some(command.into()),
            args,
            allow: true,
        }
    }
}

/// The host: connects servers, discovers their tools, and enforces policy.
pub struct McpHost {
    sink: Arc<dyn McpEventSink>,
    servers: RwLock<HashMap<String, Arc<McpClient>>>,
    tools: RwLock<HashMap<String, Vec<ToolInfo>>>,
}

impl McpHost {
    /// Create a host emitting events to `sink`.
    pub fn new(sink: Arc<dyn McpEventSink>) -> Self {
        Self {
            sink,
            servers: RwLock::new(HashMap::new()),
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Connect a server over `transport`, run `initialize`, and discover tools.
    pub async fn connect(
        &self,
        config: &McpServerConfig,
        transport: Arc<dyn Transport>,
    ) -> Result<Vec<ToolInfo>, McpError> {
        if !config.allow {
            let message = format!("server {} denied by policy", config.id);
            self.emit(McpEvent::Error {
                server: config.id.clone(),
                message: message.clone(),
            });
            return Err(McpError::Protocol(message));
        }
        let client = Arc::new(McpClient::new(transport));
        client
            .initialize("fergents", env!("CARGO_PKG_VERSION"))
            .await?;
        let _ = client.initialized().await;
        let tools = client.list_tools().await.unwrap_or_default();

        self.servers
            .write()
            .expect("mcp servers poisoned")
            .insert(config.id.clone(), client);
        self.tools
            .write()
            .expect("mcp tools poisoned")
            .insert(config.id.clone(), tools.clone());

        self.emit(McpEvent::Connected {
            server: config.id.clone(),
        });
        for tool in &tools {
            self.emit(McpEvent::ToolRegistered {
                server: config.id.clone(),
                tool: tool.name.clone(),
            });
        }
        Ok(tools)
    }

    /// Disconnect a server.
    pub async fn disconnect(&self, id: &str) -> Result<(), McpError> {
        let client = self
            .servers
            .write()
            .expect("mcp servers poisoned")
            .remove(id)
            .ok_or_else(|| McpError::UnknownServer(id.to_string()))?;
        self.tools.write().expect("mcp tools poisoned").remove(id);
        client.transport().close().await?;
        self.emit(McpEvent::Disconnected {
            server: id.to_string(),
        });
        Ok(())
    }

    /// Ping a server.
    pub async fn health(&self, id: &str) -> Result<bool, McpError> {
        let client = self
            .client(id)
            .ok_or_else(|| McpError::UnknownServer(id.to_string()))?;
        match client.ping().await {
            Ok(()) => Ok(true),
            Err(err) => {
                self.emit(McpEvent::Error {
                    server: id.to_string(),
                    message: err.to_string(),
                });
                Ok(false)
            }
        }
    }

    /// The connected server ids, sorted.
    pub fn servers(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .servers
            .read()
            .expect("mcp servers poisoned")
            .keys()
            .cloned()
            .collect();
        ids.sort();
        ids
    }

    /// The client for `id`, if connected.
    pub fn client(&self, id: &str) -> Option<Arc<McpClient>> {
        self.servers
            .read()
            .expect("mcp servers poisoned")
            .get(id)
            .cloned()
    }

    /// The tools discovered from `id`.
    pub fn tools(&self, id: &str) -> Vec<ToolInfo> {
        self.tools
            .read()
            .expect("mcp tools poisoned")
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Every discovered tool as `(server, tool)`, sorted by server then tool.
    pub fn all_tools(&self) -> Vec<(String, ToolInfo)> {
        let tools = self.tools.read().expect("mcp tools poisoned");
        let mut out: Vec<(String, ToolInfo)> = tools
            .iter()
            .flat_map(|(server, list)| {
                list.iter()
                    .map(|tool| (server.clone(), tool.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        out
    }

    fn emit(&self, event: McpEvent) {
        self.sink.emit(event);
    }
}
