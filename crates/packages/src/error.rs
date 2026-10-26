//! Package errors with stable codes for the wire.

use std::path::PathBuf;

/// Errors raised while validating, extracting, or verifying a package.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// A filesystem or archive I/O failure.
    #[error("package io: {0}")]
    Io(#[from] std::io::Error),
    /// A malformed archive.
    #[error("invalid archive: {0}")]
    InvalidArchive(String),
    /// An entry escapes the destination directory.
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),
    /// An archive exceeds the configured limits.
    #[error("package too large: {0}")]
    TooLarge(String),
    /// Two entries map to the same path.
    #[error("duplicate archive entry: {0}")]
    DuplicateEntry(String),
    /// The digest of the package does not match the expected value.
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    /// The signature is not valid for the package.
    #[error("signature invalid")]
    SignatureInvalid,
    /// A public key could not be decoded.
    #[error("invalid public key")]
    InvalidKey,
    /// A semantic version could not be parsed.
    #[error("invalid version {value:?}")]
    InvalidVersion { value: String },
    /// The package targets an API version the kernel cannot run.
    #[error("api version {required} incompatible with kernel {current}")]
    ApiIncompatible { required: u32, current: u32 },
    /// A manifest or index document is malformed.
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    /// A path contained a symlink or other unsupported entry type.
    #[error("unsupported file type: {0}")]
    UnsupportedEntry(PathBuf),
}

impl PackageError {
    /// A stable error code (`PLUGINS.md` §15, spec Appendix A).
    pub fn code(&self) -> &'static str {
        match self {
            PackageError::Io(_) => "package_io",
            PackageError::InvalidArchive(_) => "package_invalid_archive",
            PackageError::UnsafePath(_) => "package_unsafe_path",
            PackageError::TooLarge(_) => "package_too_large",
            PackageError::DuplicateEntry(_) => "package_duplicate_entry",
            PackageError::HashMismatch { .. } => "package_hash_mismatch",
            PackageError::SignatureInvalid => "package_signature_invalid",
            PackageError::InvalidKey => "package_invalid_key",
            PackageError::InvalidVersion { .. } => "package_invalid_version",
            PackageError::ApiIncompatible { .. } => "package_api_incompatible",
            PackageError::InvalidDocument(_) => "package_invalid_document",
            PackageError::UnsupportedEntry(_) => "package_unsupported_entry",
        }
    }
}
