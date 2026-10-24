//! LSP errors.

use fergents_events::EventError;
use fergents_ide::ProjectError;
use fergents_storage::StorageError;

/// Errors raised by the LSP client and service.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// Event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// Filesystem or process failure.
    #[error("lsp io error: {0}")]
    Io(#[from] std::io::Error),
    /// Project-layer failure (unknown/untrusted project, path escape).
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// JSON failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// A malformed frame or JSON-RPC message.
    #[error("lsp protocol error: {0}")]
    Protocol(String),
    /// A request did not receive a response in time.
    #[error("lsp request timed out: {0}")]
    Timeout(String),
    /// The server returned a JSON-RPC error.
    #[error("lsp server error: {0}")]
    Server(String),
    /// No server is configured or running for this language.
    #[error("lsp server is not configured: {0}")]
    UnknownServer(String),
    /// The server executable is not on `PATH`.
    #[error("lsp server is not installed: {0}")]
    NotInstalled(String),
    /// The server failed and will not be restarted.
    #[error("lsp server failed: {0}")]
    Failed(String),
    /// A document was not opened before it was used.
    #[error("lsp document is not open: {0}")]
    DocumentNotOpen(String),
}

impl LspError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "storage_error",
            Self::Event(_) => "event_error",
            Self::Io(_) => "lsp_io_error",
            Self::Project(err) => err.code(),
            Self::Serde(_) => "serde_error",
            Self::Protocol(_) => "lsp_protocol_error",
            Self::Timeout(_) => "lsp_timeout",
            Self::Server(_) => "lsp_server_error",
            Self::UnknownServer(_) => "lsp_unknown_server",
            Self::NotInstalled(_) => "lsp_not_installed",
            Self::Failed(_) => "lsp_failed",
            Self::DocumentNotOpen(_) => "lsp_document_not_open",
        }
    }
}
