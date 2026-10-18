//! Fergents storage layer: the [`Storage`] trait, [`SqliteStorage`], SQL
//! migrations, and the tamper-evident audit log.

mod audit;
mod error;
mod migrations;
mod sqlite;
mod storage;
mod vec;

pub use audit::{append, canonical_bytes, entry_hash, verify_chain, AuditEntry, GENESIS_PREV_HASH};
pub use error::StorageError;
pub use migrations::{load_migrations, load_migrations_from, Migration};
pub use sqlite::SqliteStorage;
pub use storage::{Key, Row, Storage, Value};
pub use vec::register_sqlite_vec;

/// Current wall-clock time in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
