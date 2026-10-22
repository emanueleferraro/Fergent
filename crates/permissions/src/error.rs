//! Errors raised by the permission engine.

/// A permission-subsystem failure.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] fergents_storage::StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// The policy document could not be parsed.
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
}

impl PermissionError {
    /// A stable, machine-readable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            PermissionError::Storage(_) => "storage_error",
            PermissionError::Event(_) => "event_error",
            PermissionError::InvalidPolicy(_) => "permission_invalid_policy",
        }
    }
}
