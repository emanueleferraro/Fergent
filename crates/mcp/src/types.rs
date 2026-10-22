//! MCP discovery types: tools, resources, prompts, and server info.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool advertised by an MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    /// The tool name.
    pub name: String,
    /// A description for the model.
    #[serde(default)]
    pub description: String,
    /// A JSON Schema for the arguments.
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

/// A resource advertised by an MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceInfo {
    /// The resource URI.
    pub uri: String,
    /// A human-readable name.
    #[serde(default)]
    pub name: String,
    /// The MIME type, if known.
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// A prompt advertised by an MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptInfo {
    /// The prompt name.
    pub name: String,
    /// A description.
    #[serde(default)]
    pub description: String,
}

/// Server identity returned by `initialize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// The server name.
    pub name: String,
    /// The server version.
    pub version: String,
}
