//! An MCP client: request/response over a [`Transport`].

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::McpError;
use crate::jsonrpc::{Message, Request, RequestId};
use crate::transport::Transport;
use crate::types::{PromptInfo, ResourceInfo, ServerInfo, ToolInfo};

/// The MCP protocol version this client speaks.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// A client bound to one MCP server over a transport.
pub struct McpClient {
    transport: Arc<dyn Transport>,
    next_id: AtomicI64,
}

impl McpClient {
    /// Wrap `transport`.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            next_id: AtomicI64::new(1),
        }
    }

    /// The underlying transport.
    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.transport
    }

    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.transport
            .send(Message::Request(Request::new(id, method, params)))
            .await?;
        loop {
            match self.transport.recv().await? {
                Message::Response(response) => {
                    if response.id != RequestId::Number(id) {
                        continue;
                    }
                    if let Some(error) = response.error {
                        return Err(McpError::Rpc {
                            code: error.code,
                            message: error.message,
                        });
                    }
                    return Ok(response.result.unwrap_or(Value::Null));
                }
                // Requests/notifications from the server are ignored for now.
                Message::Notification(_) | Message::Request(_) => continue,
            }
        }
    }

    /// Perform the MCP `initialize` handshake.
    pub async fn initialize(
        &self,
        client_name: &str,
        client_version: &str,
    ) -> Result<ServerInfo, McpError> {
        let result = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": client_name, "version": client_version },
                })),
            )
            .await?;
        Ok(serde_json::from_value(
            result.get("serverInfo").cloned().unwrap_or(Value::Null),
        )?)
    }

    /// Send a `notifications/initialized` notification.
    pub async fn initialized(&self) -> Result<(), McpError> {
        self.transport
            .send(Message::Notification(crate::jsonrpc::Notification {
                jsonrpc: crate::jsonrpc::JSONRPC_VERSION.to_string(),
                method: "notifications/initialized".to_string(),
                params: None,
            }))
            .await
    }

    /// Ping the server (`ping`).
    pub async fn ping(&self) -> Result<(), McpError> {
        self.request("ping", None).await.map(|_| ())
    }

    /// List the server's tools.
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, McpError> {
        let result = self.request("tools/list", None).await?;
        let tools = result
            .get("tools")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(tools)?)
    }

    /// Call a tool by name.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpError> {
        self.request(
            "tools/call",
            Some(json!({ "name": name, "arguments": arguments })),
        )
        .await
    }

    /// List the server's resources.
    pub async fn list_resources(&self) -> Result<Vec<ResourceInfo>, McpError> {
        let result = self.request("resources/list", None).await?;
        let resources = result
            .get("resources")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(resources)?)
    }

    /// Read a resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<Value, McpError> {
        self.request("resources/read", Some(json!({ "uri": uri })))
            .await
    }

    /// List the server's prompts.
    pub async fn list_prompts(&self) -> Result<Vec<PromptInfo>, McpError> {
        let result = self.request("prompts/list", None).await?;
        let prompts = result
            .get("prompts")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(serde_json::from_value(prompts)?)
    }
}
