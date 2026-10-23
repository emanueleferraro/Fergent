//! Fergents secret vault (spec decision D10, `SECURITY.md` §14).
//!
//! Secret **values** live only in the OS keychain. The global database stores
//! references (`secret_refs`), never values, and `list` never returns them.

use std::sync::Arc;

use fergents_storage::{Storage, StorageError, Value};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors surfaced by the vault.
#[derive(Debug, Error)]
pub enum SecretError {
    /// The name is empty, too long, or contains whitespace.
    #[error("invalid secret name: {0}")]
    InvalidName(String),
    /// The stored value could not be resolved.
    #[error("unknown secret: {0}")]
    Unknown(String),
    /// The OS keychain rejected the operation.
    #[error("keychain error: {0}")]
    Keychain(String),
    /// The reference row could not be read or written.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// A keychain backend: stores values under a stable name.
pub trait Keychain: Send + Sync + std::fmt::Debug {
    /// Store (or replace) the value under `name`.
    fn set(&self, name: &str, value: &str) -> Result<(), SecretError>;
    /// Resolve the value under `name`.
    fn get(&self, name: &str) -> Result<Option<String>, SecretError>;
    /// Remove `name`; a missing entry is not an error.
    fn delete(&self, name: &str) -> Result<(), SecretError>;
}

/// The production backend: the OS keychain via the `keyring` crate.
#[derive(Debug, Clone)]
pub struct KeyringKeychain {
    service: String,
}

impl KeyringKeychain {
    /// A keychain rooted at `service` (e.g. `fergents`).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, name)
            .map_err(|err| SecretError::Keychain(err.to_string()))
    }
}

impl Keychain for KeyringKeychain {
    fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        self.entry(name)?
            .set_password(value)
            .map_err(|err| SecretError::Keychain(err.to_string()))
    }

    fn get(&self, name: &str) -> Result<Option<String>, SecretError> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(SecretError::Keychain(err.to_string())),
        }
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(SecretError::Keychain(err.to_string())),
        }
    }
}

/// An in-memory backend, used by tests and headless tooling.
#[derive(Debug, Default)]
pub struct MemoryKeychain {
    values: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl MemoryKeychain {
    /// An empty in-memory keychain.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Keychain for MemoryKeychain {
    fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .expect("keychain poisoned")
            .insert(name.to_string(), value.to_string());
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .values
            .lock()
            .expect("keychain poisoned")
            .get(name)
            .cloned())
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        self.values.lock().expect("keychain poisoned").remove(name);
        Ok(())
    }
}

/// A reference row (never includes the value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    /// The logical name, e.g. `anthropic.api_key`.
    pub name: String,
    /// The provider this secret belongs to, if any.
    pub provider: Option<String>,
    /// When the reference was created (ms since the epoch).
    pub created_at: i64,
}

/// Resolve result of `secret.test` (never includes the value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretTest {
    /// The tested name.
    pub name: String,
    /// Whether the keychain holds a value for it.
    pub present: bool,
}

/// The vault facade.
pub struct SecretVault {
    storage: Arc<dyn Storage>,
    keychain: Arc<dyn Keychain>,
}

fn validate_name(name: &str) -> Result<(), SecretError> {
    if name.is_empty() || name.len() > 120 || name.chars().any(char::is_whitespace) {
        return Err(SecretError::InvalidName(name.to_string()));
    }
    Ok(())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl SecretVault {
    /// Build a vault over `storage` and `keychain`.
    pub fn new(storage: Arc<dyn Storage>, keychain: Arc<dyn Keychain>) -> Self {
        Self { storage, keychain }
    }

    /// Every reference, newest first. Values are never returned.
    pub fn list(&self) -> Result<Vec<SecretRef>, SecretError> {
        let rows = self.storage.query(
            "SELECT name, provider, created_at FROM secret_refs ORDER BY created_at DESC",
            &[],
        )?;
        Ok(rows
            .into_iter()
            .map(|row| SecretRef {
                name: row
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                provider: row
                    .get("provider")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            })
            .collect())
    }

    /// Store `value` in the keychain and record its reference.
    pub fn set(
        &self,
        name: &str,
        value: &str,
        provider: Option<&str>,
    ) -> Result<SecretRef, SecretError> {
        validate_name(name)?;
        self.keychain.set(name, value)?;
        let created_at = now_ms();
        self.storage.execute(
            "INSERT INTO secret_refs (name, keychain_ref, provider, created_at) \
             VALUES (?1, ?1, ?2, ?3) \
             ON CONFLICT(name) DO UPDATE SET provider = excluded.provider",
            &[
                Value::String(name.to_string()),
                provider.map_or(Value::Null, |p| Value::String(p.to_string())),
                Value::from(created_at),
            ],
        )?;
        let created_at = self
            .storage
            .query(
                "SELECT created_at FROM secret_refs WHERE name = ?1",
                &[Value::String(name.to_string())],
            )?
            .first()
            .and_then(|row| row.get("created_at"))
            .and_then(Value::as_i64)
            .unwrap_or(created_at);
        Ok(SecretRef {
            name: name.to_string(),
            provider: provider.map(str::to_string),
            created_at,
        })
    }

    /// Remove the value and its reference. Returns whether a reference existed.
    pub fn delete(&self, name: &str) -> Result<bool, SecretError> {
        validate_name(name)?;
        self.keychain.delete(name)?;
        let removed = self.storage.execute(
            "DELETE FROM secret_refs WHERE name = ?1",
            &[Value::String(name.to_string())],
        )?;
        Ok(removed > 0)
    }

    /// Check whether a value is resolvable, without returning it.
    pub fn test(&self, name: &str) -> Result<SecretTest, SecretError> {
        validate_name(name)?;
        let present = self.keychain.get(name)?.is_some();
        Ok(SecretTest {
            name: name.to_string(),
            present,
        })
    }

    /// Resolve a value for internal callers (providers). Never exposed by the
    /// protocol surface.
    pub fn resolve(&self, name: &str) -> Result<String, SecretError> {
        self.keychain
            .get(name)?
            .ok_or_else(|| SecretError::Unknown(name.to_string()))
    }
}
