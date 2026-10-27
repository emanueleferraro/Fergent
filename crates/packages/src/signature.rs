//! Ed25519 package signatures (`PLUGINS.md` §11.2, spec D10).

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::digest::digest_bytes;
use crate::error::PackageError;

/// The verification outcome shown in an install review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    /// Signed by a key in the trust store.
    Verified,
    /// Signed, but the key is not trusted.
    UnknownKey,
    /// The signature does not match the package.
    Invalid,
    /// No signature file.
    None,
    /// A development directory (never published as-is).
    Dev,
}

impl SignatureStatus {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            SignatureStatus::Verified => "verified",
            SignatureStatus::UnknownKey => "unknown_key",
            SignatureStatus::Invalid => "invalid",
            SignatureStatus::None => "none",
            SignatureStatus::Dev => "dev",
        }
    }

    /// Parse a label.
    pub fn parse(value: &str) -> SignatureStatus {
        match value {
            "verified" => SignatureStatus::Verified,
            "unknown_key" => SignatureStatus::UnknownKey,
            "invalid" => SignatureStatus::Invalid,
            "dev" => SignatureStatus::Dev,
            _ => SignatureStatus::None,
        }
    }
}

/// Verify that `signature_hex` is a valid Ed25519 signature over `root`'s
/// digest by `public_key_hex`.
pub fn verify_signature(
    root: &Path,
    public_key_hex: &str,
    signature_hex: &str,
) -> Result<(), PackageError> {
    let key_bytes = decode_array::<32>(public_key_hex).ok_or(PackageError::InvalidKey)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| PackageError::InvalidKey)?;
    let signature_bytes =
        decode_array::<64>(signature_hex).ok_or(PackageError::SignatureInvalid)?;
    let signature = Signature::from_bytes(&signature_bytes);
    let digest = digest_bytes(root)?;
    key.verify_strict(&digest, &signature)
        .map_err(|_| PackageError::SignatureInvalid)
}

/// Verify `signature_hex` over an already-computed 32-byte digest.
///
/// The marketplace uses this for signed index documents.
pub fn verify_digest(
    digest: &[u8; 32],
    public_key_hex: &str,
    signature_hex: &str,
) -> Result<(), PackageError> {
    let key_bytes = decode_array::<32>(public_key_hex).ok_or(PackageError::InvalidKey)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| PackageError::InvalidKey)?;
    let signature_bytes =
        decode_array::<64>(signature_hex).ok_or(PackageError::SignatureInvalid)?;
    let signature = Signature::from_bytes(&signature_bytes);
    key.verify_strict(digest, &signature)
        .map_err(|_| PackageError::SignatureInvalid)
}

/// Sign a digest with a raw 32-byte Ed25519 secret key.
///
/// Used by the SDK, the factory, and tests; production installs only verify.
pub fn sign_digest(digest: &[u8; 32], secret_key: &[u8; 32]) -> String {
    let key = SigningKey::from_bytes(secret_key);
    hex::encode(key.sign(digest).to_bytes())
}

fn decode_array<const N: usize>(value: &str) -> Option<[u8; N]> {
    let bytes = hex::decode(value.trim()).ok()?;
    bytes.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn signs_and_verifies_a_package() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("plugin.toml"), b"id = \"x\"").unwrap();
        let secret = [7u8; 32];
        let key = SigningKey::from_bytes(&secret);
        let public = hex::encode(key.verifying_key().to_bytes());
        let digest = digest_bytes(dir.path()).unwrap();
        let signature = sign_digest(&digest, &secret);
        verify_signature(dir.path(), &public, &signature).unwrap();
        fs::write(dir.path().join("plugin.toml"), b"id = \"y\"").unwrap();
        assert!(verify_signature(dir.path(), &public, &signature).is_err());
    }

    #[test]
    fn rejects_malformed_keys() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            verify_signature(dir.path(), "zz", "00"),
            Err(PackageError::InvalidKey)
        ));
    }
}
