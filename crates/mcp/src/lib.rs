//! Fergents MCP layer.
//!
//! The kernel acts as an MCP **host** (`MCP.md` §2): it connects to MCP servers,
//! discovers their tools/resources/prompts, and exposes them to agents. It also
//! exposes a minimal MCP **server** (`MCP.md` §3) so external clients can use
//! brain tools.
//!
//! The JSON-RPC 2.0 codec, the [`Transport`] abstraction, and the client/host
//! are all transport-agnostic; `stdio` and an in-memory transport (for tests)
//! ship here, and the trait is the seam for SSE/HTTP.

mod client;
mod error;
mod host;
mod jsonrpc;
mod server;
mod transport;
mod types;

pub use client::McpClient;
pub use error::McpError;
pub use host::{McpEvent, McpEventSink, McpHost, McpServerConfig, NoopMcpSink, RecordingMcpSink};
pub use jsonrpc::{Message, Notification, Request, RequestId, Response, RpcError};
pub use server::{McpHandler, McpServer};
pub use transport::{in_memory_pair, InMemoryTransport, StdioTransport, Transport};
pub use types::{PromptInfo, ResourceInfo, ServerInfo, ToolInfo};
