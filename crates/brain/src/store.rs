//! The per-brain index store: `<brain>/index.sqlite`.
//!
//! The store is opened through the Phase 0 [`SqliteStorage`] and the migration
//! runner, parameterized by the per-brain migrations directory
//! (`migrations/brain`) instead of the global one. It is a *regenerable*
//! index: the markdown vault on disk is the source of truth (`BRAIN.md` §5).
//!
//! Opening refuses a store whose recorded `schema_version` is newer than the
//! migrations available on disk (the runner returns
//! [`fergents_storage::StorageError::NewerSchema`]).

use std::path::{Path, PathBuf};

use fergents_storage::{load_migrations_from, Migration, SqliteStorage, Storage};

use crate::error::BrainError;

/// The index database file name inside a brain folder.
pub const INDEX_DB: &str = "index.sqlite";

/// The per-brain schema directory, relative to the migrations root.
pub const BRAIN_MIGRATIONS_SUBDIR: &str = "brain";

/// Register the statically linked `sqlite-vec` extension so the `vec0` module
/// is available on every connection opened afterwards.
///
/// Delegates to [`fergents_storage::register_sqlite_vec`]; kept here for
/// backward compatibility with callers of the brain crate.
pub use fergents_storage::register_sqlite_vec;

/// The newest migration version available in `brain_migrations_dir`.
pub fn available_schema_version(brain_migrations_dir: &Path) -> Result<i64, BrainError> {
    let migrations = load_migrations_from(brain_migrations_dir)?;
    Ok(migrations.last().map(|m| m.version).unwrap_or(0))
}

/// An open per-brain index store.
pub struct BrainStore {
    storage: SqliteStorage,
    path: PathBuf,
}

impl BrainStore {
    /// Open (creating if needed) `<brain_dir>/index.sqlite` and apply the
    /// per-brain migrations from `brain_migrations_dir`.
    pub fn open(brain_dir: &Path, brain_migrations_dir: &Path) -> Result<Self, BrainError> {
        let migrations = load_migrations_from(brain_migrations_dir)?;
        Self::open_with_migrations(brain_dir, &migrations)
    }

    /// Like [`BrainStore::open`], but with preloaded migrations.
    pub fn open_with_migrations(
        brain_dir: &Path,
        migrations: &[Migration],
    ) -> Result<Self, BrainError> {
        register_sqlite_vec();
        if !brain_dir.as_os_str().is_empty() {
            std::fs::create_dir_all(brain_dir).map_err(|e| BrainError::io(brain_dir, e))?;
        }
        let path = brain_dir.join(INDEX_DB);
        let storage = SqliteStorage::open_with_migrations(&path, migrations)?;
        Ok(Self { storage, path })
    }

    /// The index database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The underlying storage handle.
    pub fn storage(&self) -> &SqliteStorage {
        &self.storage
    }

    /// The highest `schema_version` recorded in the store (0 if none).
    pub fn schema_version(&self) -> Result<i64, BrainError> {
        let rows = self.storage.query(
            "SELECT COALESCE(MAX(version), 0) AS version FROM schema_version",
            &[],
        )?;
        let version = rows
            .first()
            .and_then(|row| row.get("version"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        Ok(version)
    }
}
