//! Errors for the per-brain index.

/// An error from the per-brain index.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// An underlying storage error.
    #[error(transparent)]
    Storage(#[from] fergents_storage::StorageError),

    /// A vector's dimension did not match the store's configuration.
    #[error("vector dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    /// A stored vector could not be decoded.
    #[error("corrupt stored vector for chunk '{0}'")]
    CorruptVector(String),

    /// The embedding backend failed.
    #[error("embedding backend error: {0}")]
    Embedding(String),

    /// The vector backend failed.
    #[error("vector store error: {0}")]
    Vector(String),

    /// A filesystem operation failed.
    #[error("io error: {0}")]
    Io(String),

    /// A search query or one of its filters was invalid.
    #[error("invalid query: {0}")]
    InvalidQuery(String),
}
