//! The generic `Storage` surface.

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
}
