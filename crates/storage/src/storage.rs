//! The generic `Storage` surface.

use crate::audit::{entry_hash, AuditEntry, GENESIS_PREV_HASH};
use crate::error::StorageError;

pub use serde_json::Value;

/// A logical key in the generic key/value surface.
///
/// `get`/`put`/`delete` operate on the `settings` table: each key maps to a
/// single JSON value stored in `value_json`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(pub String);

/// A single result row, decoded as a JSON object keyed by column name.
pub type Row = serde_json::Map<String, Value>;

/// A minimal, object-safe storage abstraction.
///
/// All methods take `&self` so the trait is object-safe (`Box<dyn Storage>`).
/// The five methods are intentionally blocking; an async wrapper can be layered
/// on top later without changing this contract.
pub trait Storage: Send + Sync {
    /// Fetch the JSON value stored under `key`, if any.
    ///
    /// Backed by the `settings` table (`key` -> `value_json`).
    fn get(&self, key: &Key) -> Result<Option<Value>, StorageError>;

    /// Insert or update the JSON value stored under `key` (upsert).
    ///
    /// Backed by the `settings` table (`key` -> `value_json`).
    fn put(&self, key: Key, value: Value) -> Result<(), StorageError>;

    /// Remove `key` from the store. A missing key is not an error.
    ///
    /// Backed by the `settings` table (`key` -> `value_json`).
    fn delete(&self, key: &Key) -> Result<(), StorageError>;

    /// Run a read query and return the decoded rows.
    ///
    /// `params` are positional (`?1`, `?2`, ...) bound as JSON values.
    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, StorageError>;

    /// Run a write statement and return the number of affected rows.
    fn execute(&self, sql: &str, params: &[Value]) -> Result<usize, StorageError>;

    /// Append a tamper-evident entry to the `audit_log` hash chain.
    ///
    /// The default implementation reads the last entry's hash, computes the new
    /// entry's hash with [`entry_hash`], and inserts the row. Returns the new
    /// entry's hash.
    fn audit(&self, entry: &AuditEntry<'_>) -> Result<String, StorageError> {
        let rows = self.query(
            "SELECT hash FROM audit_log ORDER BY rowid DESC LIMIT 1",
            &[],
        )?;
        let prev_hash = rows
            .first()
            .and_then(|row| row.get("hash"))
            .and_then(Value::as_str)
            .unwrap_or(GENESIS_PREV_HASH)
            .to_string();
        let hash = entry_hash(entry, &prev_hash);
        self.execute(
            "INSERT INTO audit_log (id, ts, actor_type, actor_id, session_id, action, target, \
             args_json, result, reason, correlation_id, reversible, revert_ref, prev_hash, hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            &[
                Value::String(entry.id.to_string()),
                Value::from(entry.ts),
                Value::String(entry.actor_type.to_string()),
                Value::String(entry.actor_id.to_string()),
                opt(entry.session_id),
                Value::String(entry.action.to_string()),
                opt(entry.target),
                Value::String(entry.args_json.to_string()),
                Value::String(entry.result.to_string()),
                opt(entry.reason),
                opt(entry.correlation_id),
                Value::from(i64::from(entry.reversible)),
                opt(entry.revert_ref),
                Value::String(prev_hash),
                Value::String(hash.clone()),
            ],
        )?;
        Ok(hash)
    }
}

fn opt(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.to_string()))
}
