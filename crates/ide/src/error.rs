//! Project errors.

use fergents_events::EventError;
use fergents_storage::StorageError;

/// Errors raised by the project layer.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// Event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// Backup (reversibility) failure.
    #[error("reversibility error: {0}")]
    Reversibility(#[from] fergents_reversibility::ReversibilityError),
    /// Filesystem failure.
    #[error("project io error: {0}")]
    Io(#[from] std::io::Error),
    /// The project id is unknown.
    #[error("unknown project: {0}")]
    Unknown(String),
    /// The project exists but is not trusted (spec decision D3).
    #[error("project is not trusted: {0}")]
    Untrusted(String),
    /// The path is missing, not a directory, or cannot be canonicalised.
    #[error("invalid project path: {0}")]
    InvalidPath(String),
    /// A relative path would escape the project root.
    #[error("path escapes the project root: {0}")]
    Escape(String),
    /// The file changed underneath the caller (`base_etag` mismatch).
    #[error("file conflict: {0}")]
    Conflict(String),
    /// The filesystem watcher failed to start.
    #[error("file watcher error: {0}")]
    Watch(String),
    /// An argument is invalid.
    #[error("invalid project request: {0}")]
    Invalid(String),
}

impl ProjectError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "storage_error",
            Self::Event(_) => "event_error",
            Self::Reversibility(_) => "reversibility_error",
            Self::Io(_) => "project_io_error",
            Self::Unknown(_) => "project_unknown",
            Self::Untrusted(_) => "project_untrusted",
            Self::InvalidPath(_) => "project_invalid_path",
            Self::Escape(_) => "project_path_escape",
            Self::Conflict(_) => "file_conflict",
            Self::Watch(_) => "file_watch_error",
            Self::Invalid(_) => "project_invalid",
        }
    }
}
