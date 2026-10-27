//! Deterministic package digests.
//!
//! The digest covers every regular file except `signature.ed25519`, in sorted
//! relative-path order, so signing and verification are reproducible across
//! platforms (`PLUGINS.md` §11.2).

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::PackageError;

/// The signature file excluded from the digest.
pub const SIGNATURE_FILE: &str = "signature.ed25519";

/// Hash `root` into a 32-byte SHA-256 digest.
pub fn digest_bytes(root: &Path) -> Result<[u8; 32], PackageError> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|err| PackageError::Io(std::io::Error::other(err)))?;
        let path = entry.path();
        if path == root {
            continue;
        }
        let ty = entry.file_type();
        if ty.is_symlink() {
            return Err(PackageError::UnsupportedEntry(path.to_path_buf()));
        }
        if !ty.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| PackageError::UnsafePath(path.display().to_string()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if name == SIGNATURE_FILE {
            continue;
        }
        files.push((name, path.to_path_buf()));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (name, path) in files {
        let bytes = fs::read(&path)?;
        hasher.update(name.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().into())
}

/// SHA-256 of a raw byte slice.
///
/// Used by the marketplace to sign and verify index documents.
pub fn digest_of_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// The digest as lowercase hexadecimal.
pub fn compute_digest(root: &Path) -> Result<String, PackageError> {
    Ok(hex::encode(digest_bytes(root)?))
}

/// The digest as lowercase hexadecimal (alias used by the spec).
pub fn digest_hex(root: &Path) -> Result<String, PackageError> {
    compute_digest(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_order_independent_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), b"two").unwrap();
        fs::write(dir.path().join("a.txt"), b"one").unwrap();
        let first = compute_digest(dir.path()).unwrap();
        let second = compute_digest(dir.path()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);

        // Renaming a file changes the digest.
        fs::rename(dir.path().join("a.txt"), dir.path().join("c.txt")).unwrap();
        assert_ne!(first, compute_digest(dir.path()).unwrap());
    }

    #[test]
    fn signature_file_is_excluded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"one").unwrap();
        let before = compute_digest(dir.path()).unwrap();
        fs::write(dir.path().join(SIGNATURE_FILE), b"deadbeef").unwrap();
        assert_eq!(before, compute_digest(dir.path()).unwrap());
    }
}
