//! Error type for the brain crate.

use std::path::PathBuf;

/// Errors produced by the brain crate.
///
/// Every fallible operation returns a [`BrainError`]; the crate never panics
/// on invalid input (bad `brain.toml`, unreadable files, failed validation).
#[derive(Debug, thiserror::Error)]
pub enum BrainError {
    /// An I/O error while reading or writing a file.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory that could not be accessed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// `brain.toml` could not be parsed.
    #[error("invalid brain.toml: {source}")]
    Parse {
        /// The underlying TOML deserialization error.
        #[source]
        source: toml::de::Error,
    },

    /// The configuration could not be serialized to TOML.
    #[error("could not serialize brain.toml: {source}")]
    Serialize {
        /// The underlying TOML serialization error.
        #[source]
        source: toml::ser::Error,
    },

    /// The configuration parsed but failed semantic validation.
    #[error("invalid brain configuration: {0}")]
    Invalid(String),
}

impl BrainError {
    /// Wrap an I/O error with the path it occurred on.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Wrap a TOML parse error.
    pub fn parse(source: toml::de::Error) -> Self {
        Self::Parse { source }
    }

    /// Wrap a TOML serialization error.
    pub fn serialize(source: toml::ser::Error) -> Self {
        Self::Serialize { source }
    }

    /// Create a semantic validation error.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
