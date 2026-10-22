//! A minimal MCP server over a [`Transport`], driven by a [`McpHandler`].

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::McpError;
use crate::jsonrpc::{Message, Request, Response, RpcError};
use crate::transport::Transport;
use crate::types::{PromptInfo, ResourceInfo, ToolInfo};

/// The behaviour a [`McpServer`] exposes.
pub trait McpHandler: Send + Sync {
    /// The tools this server exposes.
    fn list_tools(&self) -> Vec<ToolInfo>;

    /// Execute a tool call and return its result as JSON.
    fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpError>;

    /// The resources this server exposes.
    fn list_resources(&self) -> Vec<ResourceInfo> {
        Vec::new()
    }

    /// Read a resource by URI.
    fn read_resource(&self, uri: &str) -> Result<Value, McpError> {
        Err(McpError::Protocol(format!("unknown resource: {uri}")))
    }

    /// The prompts this server exposes.
    fn list_prompts(&self) -> Vec<PromptInfo> {
        Vec::new()
    }
}

/// A minimal MCP server: it answers `initialize`, `ping`, `tools/*`,
/// `resources/*`, and `prompts/*` from a handler.
pub struct McpServer {
    handler: Arc<dyn McpHandler>,
    transport: Arc<dyn Transport>,
    name: String,
    version: String,
}

impl McpServer {
    /// Create a server named `name` at version `version`.
    pub fn new(
        handler: Arc<dyn McpHandler>,
        transport: Arc<dyn Transport>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            handler,
            transport,
            name: name.into(),
            version: version.into(),
        }
    }

    /// The underlying transport.
    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.transport
    }

    /// Reply to one inbound message, returning the response to send (if any).
    pub fn handle(&self, message: Message) -> Option<Message> {
        match message {
            Message::Request(request) => Some(Message::Response(self.dispatch(request))),
            Message::Notification(_) | Message::Response(_) => None,
        }
    }

    fn dispatch(&self, request: Request) -> Response {
        let id = request.id.clone();
        let params = request.params.clone().unwrap_or(Value::Null);
        match request.method.as_str() {
            "initialize" => Response::ok(
                id,
                json!({
                    "protocolVersion": crate::client::MCP_PROTOCOL_VERSION,
                    "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
                    "serverInfo": { "name": self.name, "version": self.version },
                }),
            ),
            "ping" => Response::ok(id, json!({})),
            "tools/list" => Response::ok(id, json!({ "tools": self.handler.list_tools() })),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Response::error(id, RpcError::invalid_params("missing name"));
                };
                let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                match self.handler.call_tool(name, arguments) {
                    Ok(value) => Response::ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
                    ),
                    Err(err) => Response::error(id, internal_error(err)),
                }
            }
            "resources/list" => {
                Response::ok(id, json!({ "resources": self.handler.list_resources() }))
            }
            "resources/read" => {
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    return Response::error(id, RpcError::invalid_params("missing uri"));
                };
                match self.handler.read_resource(uri) {
                    Ok(value) => Response::ok(id, json!({ "contents": value })),
                    Err(err) => Response::error(id, internal_error(err)),
                }
            }
            "prompts/list" => Response::ok(id, json!({ "prompts": self.handler.list_prompts() })),
            other => Response::error(id, RpcError::method_not_found(other)),
        }
    }

    /// Read one message, reply if needed, and return `true`. `Closed` on EOF.
    pub async fn run_once(&self) -> Result<(), McpError> {
        let message = self.transport.recv().await?;
        if let Some(response) = self.handle(message) {
            self.transport.send(response).await?;
        }
        Ok(())
    }

    /// Serve until the transport closes.
    pub async fn run(&self) -> Result<(), McpError> {
        loop {
            match self.run_once().await {
                Ok(()) => {}
                Err(McpError::Closed) => return Ok(()),
                Err(err) => return Err(err),
            }
        }
    }
}

fn internal_error(err: McpError) -> RpcError {
    RpcError {
        code: -32603,
        message: err.to_string(),
        data: None,
    }
}
