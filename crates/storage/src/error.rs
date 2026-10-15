//! Error type for the storage layer.

use thiserror::Error;

/// Errors produced by the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    /// An I/O error occurred while reading migrations or the database.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A migration could not be loaded, parsed, or applied.
    #[error("migration error: {0}")]
    Migration(String),

    /// A SQLite query or statement failed.
    #[error("database error: {0}")]
    Query(#[from] rusqlite::Error),

    /// A value could not be (de)serialized as JSON.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// The database was written by a newer version of the schema than the
    /// migrations available on disk can describe.
    #[error(
        "database schema version {found} is newer than the newest available migration {available}"
    )]
    NewerSchema {
        /// The maximum schema version recorded in the database.
        found: i64,
        /// The maximum migration version available on disk.
        available: i64,
    },

    /// The connection lock is poisoned (a previous holder panicked).
    #[error("storage connection lock is poisoned")]
    Lock,

    /// The dedicated writer task is gone (it panicked or was shut down).
    #[error("storage writer task is unavailable")]
    WriterGone,

    /// A stored row has an unexpected shape.
    #[error("corrupt storage value: {0}")]
    Corrupt(String),

    /// The audit hash chain failed verification.
    #[error("audit hash chain is broken at entry {id}")]
    AuditChain {
        /// The id of the first entry that fails verification.
        id: String,
    },
}
