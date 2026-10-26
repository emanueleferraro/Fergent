//! Packaging primitives shared by plugins and skills.
//!
//! A package is a directory (or a `.fergplugin`/`.fergskill` zip) containing a
//! manifest and its assets. This crate owns the parts that must be identical
//! for both: path-safe extraction, deterministic digests, Ed25519 signatures,
//! semver parsing, and kernel API-version compatibility (`PLUGINS.md` §11–13).

pub mod archive;
pub mod digest;
pub mod error;
pub mod signature;
pub mod version;

pub use archive::{create_archive, extract_archive, ArchiveLimits};
pub use digest::{compute_digest, digest_bytes, digest_hex};
pub use error::PackageError;
pub use signature::{sign_digest, verify_signature, SignatureStatus};
pub use version::{api_compatible, parse_version, KERNEL_API_VERSION};
