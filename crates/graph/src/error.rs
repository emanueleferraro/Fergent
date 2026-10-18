//! Graph errors.

use fergents_storage::StorageError;

/// Errors from the graph store, extraction, and graph commands.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    /// The graph backend (CozoDB) returned an error.
    #[error("graph backend error: {0}")]
    Backend(String),
    /// A SQLite mirror operation failed.
    #[error("graph mirror storage error: {0}")]
    Storage(#[from] StorageError),
    /// A query or algorithm request was invalid.
    #[error("invalid graph request: {0}")]
    Invalid(String),
    /// The requested entity does not exist.
    #[error("entity not found: {0}")]
    EntityNotFound(String),
}
