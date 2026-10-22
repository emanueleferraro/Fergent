//! Errors raised by the MCP layer.

/// Errors raised by transports, clients, hosts, and servers.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// An I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The transport was closed.
    #[error("transport closed")]
    Closed,
    /// The peer returned a JSON-RPC error.
    #[error("rpc error {code}: {message}")]
    Rpc {
        /// The numeric error code.
        code: i64,
        /// The error message.
        message: String,
    },
    /// The peer sent something that is not valid MCP.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// No server is registered under this id.
    #[error("unknown mcp server: {0}")]
    UnknownServer(String),
    /// The request timed out.
    #[error("request timed out")]
    Timeout,
}
