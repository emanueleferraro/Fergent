//! Errors raised by the reversibility service.

/// A reversibility-subsystem failure.
#[derive(Debug, thiserror::Error)]
pub enum ReversibilityError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] fergents_storage::StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The backup reference does not exist.
    #[error("unknown backup: {0}")]
    UnknownBackup(String),
}

impl ReversibilityError {
    /// A stable, machine-readable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            ReversibilityError::Storage(_) => "storage_error",
            ReversibilityError::Event(_) => "event_error",
            ReversibilityError::Io(_) => "io_error",
            ReversibilityError::Serde(_) => "serde_error",
            ReversibilityError::UnknownBackup(_) => "backup_unknown",
        }
    }
}
