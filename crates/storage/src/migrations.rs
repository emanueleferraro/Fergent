//! Loading and applying numbered SQL migrations.
//!
//! The global schema lives in `migrations/global/`; other schema directories
//! (e.g. the per-brain `migrations/brain/`) are loaded with
//! [`load_migrations_from`].

use std::path::Path;

use crate::error::StorageError;

/// A single migration loaded from disk.
#[derive(Debug, Clone)]
pub struct Migration {
    /// The numeric version parsed from the filename prefix.
    pub version: i64,
    /// The migration filename (used as the recorded description).
    pub description: String,
    /// The raw SQL.
    pub sql: String,
    /// Hex-encoded SHA-256 of the raw SQL bytes.
    pub checksum: String,
}

/// Load every `*.sql` migration in `<dir>/global`, sorted ascending by version.
///
/// This is the global-schema loader (Phase 0). Per-brain schemas use
/// [`load_migrations_from`] with `migrations/brain`.
pub fn load_migrations(dir: &Path) -> Result<Vec<Migration>, StorageError> {
    load_migrations_from(&dir.join("global"))
}

/// Load every `*.sql` migration directly in `dir`, sorted ascending by version.
///
/// Used for non-global schema directories (e.g. `migrations/brain`).
pub fn load_migrations_from(dir: &Path) -> Result<Vec<Migration>, StorageError> {
    let mut migrations = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let description = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let version = parse_version(&description).ok_or_else(|| {
            StorageError::Migration(format!(
                "migration filename has no numeric prefix: {description}"
            ))
        })?;
        let sql = std::fs::read_to_string(&path)?;
        let checksum = sha256_hex(sql.as_bytes());
        migrations.push(Migration {
            version,
            description,
            sql,
            checksum,
        });
    }
    migrations.sort_by_key(|m| m.version);
    Ok(migrations)
}

/// Apply any migrations not yet recorded in `schema_version`.
///
/// Each migration and its `schema_version` bookkeeping row are applied in a
/// single transaction. Already-applied versions are skipped, making this
/// idempotent. Refuses with [`StorageError::NewerSchema`] if the database
/// records a higher version than any migration on disk.
pub fn apply_migrations(
    conn: &mut rusqlite::Connection,
    migrations: &[Migration],
) -> Result<(), StorageError> {
    let available_max = migrations.last().map(|m| m.version).unwrap_or(0);
    let applied = applied_versions(conn)?;
    let applied_max = applied.iter().copied().max().unwrap_or(0);
    if applied_max > available_max {
        return Err(StorageError::NewerSchema {
            found: applied_max,
            available: available_max,
        });
    }

    for migration in migrations {
        if applied.contains(&migration.version) {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(&migration.sql)?;
        tx.execute(
            "INSERT INTO schema_version (version, description, checksum, applied_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                migration.version,
                migration.description,
                migration.checksum,
                crate::now_ms()
            ],
        )?;
        tx.commit()?;
        tracing::debug!(
            version = migration.version,
            description = %migration.description,
            "applied migration"
        );
    }
    Ok(())
}

/// The set of versions already recorded in `schema_version`.
///
/// Returns an empty vector if the table does not exist yet (fresh database).
fn applied_versions(conn: &rusqlite::Connection) -> Result<Vec<i64>, StorageError> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
        [],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare("SELECT version FROM schema_version")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(row.get(0)?);
    }
    Ok(out)
}

/// Parse the leading run of ASCII digits from a filename.
fn parse_version(name: &str) -> Option<i64> {
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Hex-encoded SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
