//! Errors raised by the API layer.

/// Errors raised by the local IPC transport and server.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// An I/O failure on the socket.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON serialization/deserialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// A kernel core failure.
    #[error("core error: {0}")]
    Core(#[from] fergents_core::CoreError),
    /// The client has not completed the `hello` handshake.
    #[error("handshake required")]
    HandshakeRequired,
    /// The client's protocol version is unsupported.
    #[error("protocol version unsupported: client={client} kernel={kernel}")]
    ProtocolVersionUnsupported {
        /// The client's requested protocol version.
        client: u32,
        /// The kernel's protocol version.
        kernel: u32,
    },
    /// The connection was closed before a reply arrived.
    #[error("connection closed")]
    ConnectionClosed,
    /// The kernel returned a JSON-RPC error.
    #[error("rpc error {code}: {message}")]
    Rpc {
        /// The numeric JSON-RPC error code.
        code: i32,
        /// The error message.
        message: String,
    },
}
