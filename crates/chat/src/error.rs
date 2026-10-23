//! Errors raised by the chat backend.

/// A chat-subsystem failure.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] fergents_storage::StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// An IO failure (session files).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// No session exists with this id.
    #[error("unknown session: {0}")]
    UnknownSession(String),
    /// The requested input is invalid.
    #[error("invalid chat input: {0}")]
    Invalid(String),
}

impl ChatError {
    /// A stable, machine-readable wire code.
    pub fn code(&self) -> &'static str {
        match self {
            ChatError::Storage(_) => "storage_error",
            ChatError::Event(_) => "event_error",
            ChatError::Io(_) => "io_error",
            ChatError::Serde(_) => "serde_error",
            ChatError::UnknownSession(_) => "chat_unknown_session",
            ChatError::Invalid(_) => "chat_invalid",
        }
    }
}
