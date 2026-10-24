//! Terminal errors.

use fergents_events::EventError;
use fergents_ide::ProjectError;
use fergents_storage::StorageError;

/// Errors raised by the terminal service.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// Event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// Project-layer failure.
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// PTY failure.
    #[error("terminal io error: {0}")]
    Io(#[from] std::io::Error),
    /// The native PTY rejected the request.
    #[error("pty error: {0}")]
    Pty(String),
    /// The session does not exist (any more).
    #[error("unknown terminal session: {0}")]
    UnknownSession(String),
    /// The session is not running.
    #[error("terminal session is not running: {0}")]
    NotRunning(String),
    /// The requested shell is not allowed.
    #[error("shell is not allowed: {0}")]
    ShellNotAllowed(String),
    /// Invalid parameters.
    #[error("invalid terminal request: {0}")]
    Invalid(String),
}

impl TerminalError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "storage_error",
            Self::Event(_) => "event_error",
            Self::Project(err) => err.code(),
            Self::Io(_) => "term_io_error",
            Self::Pty(_) => "term_pty_error",
            Self::UnknownSession(_) => "term_unknown_session",
            Self::NotRunning(_) => "term_not_running",
            Self::ShellNotAllowed(_) => "term_shell_not_allowed",
            Self::Invalid(_) => "term_invalid",
        }
    }
}
