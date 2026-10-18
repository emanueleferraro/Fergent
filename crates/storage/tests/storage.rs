//! Integration tests for `SqliteStorage` and the migration runner.

use std::collections::HashSet;
use std::path::PathBuf;

use fergents_storage::{Key, SqliteStorage, Storage, StorageError};

/// Walk up from the crate manifest dir until a `migrations/` directory is found.
fn repo_migrations_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("migrations");
        if candidate.is_dir() {
            return candidate;
        }
        assert!(dir.pop(), "could not locate migrations/ directory");
    }
}

const TABLES: [&str; 9] = [
    "schema_version",
    "events",
    "audit_log",
    "sessions",
    "brains",
    "projects",
    "project_brains",
    "llm_requests",
    "settings",
];

/// The number of global migration files currently shipped (`migrations/global/*.sql`).
fn global_migration_count() -> i64 {
    std::fs::read_dir(repo_migrations_dir().join("global"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
        .count() as i64
}

#[test]
fn migrations_apply_on_fresh_db() {
    let storage = SqliteStorage::open_in_memory(&repo_migrations_dir()).unwrap();

    let rows = storage
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            &[],
        )
        .unwrap();
    let names: HashSet<String> = rows
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect();
    for table in TABLES {
        assert!(names.contains(table), "missing table {table}");
    }

    let count = storage
        .query("SELECT COUNT(*) AS n FROM schema_version", &[])
        .unwrap();
    assert_eq!(count[0]["n"].as_i64().unwrap(), global_migration_count());
}

#[test]
fn reopening_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("fergents.sqlite");
    let migrations = repo_migrations_dir();

    drop(SqliteStorage::open(&db, &migrations).unwrap());
    let storage = SqliteStorage::open(&db, &migrations).unwrap();

    let count = storage
        .query("SELECT COUNT(*) AS n FROM schema_version", &[])
        .unwrap();
    assert_eq!(count[0]["n"].as_i64().unwrap(), global_migration_count());
}

#[test]
fn refuses_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("fergents.sqlite");
    drop(SqliteStorage::open(&db, &repo_migrations_dir()).unwrap());

    // A migrations directory with no migrations implies an older schema.
    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(empty.path().join("global")).unwrap();

    match SqliteStorage::open(&db, empty.path()) {
        Ok(_) => panic!("expected NewerSchema error"),
        Err(StorageError::NewerSchema { found, available }) => {
            assert_eq!(found, global_migration_count());
            assert_eq!(available, 0);
        }
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn kv_put_get_delete_roundtrip() {
    let storage = SqliteStorage::open_in_memory(&repo_migrations_dir()).unwrap();
    let key = Key("theme".to_string());

    assert_eq!(storage.get(&key).unwrap(), None);

    storage
        .put(key.clone(), serde_json::json!({ "mode": "dark" }))
        .unwrap();
    assert_eq!(
        storage.get(&key).unwrap(),
        Some(serde_json::json!({ "mode": "dark" }))
    );

    // Upsert overwrites.
    storage.put(key.clone(), serde_json::json!(42)).unwrap();
    assert_eq!(storage.get(&key).unwrap(), Some(serde_json::json!(42)));

    storage.delete(&key).unwrap();
    assert_eq!(storage.get(&key).unwrap(), None);

    // Deleting a missing key is not an error.
    storage.delete(&key).unwrap();
}

#[test]
fn transaction_rollback_leaves_no_partial_writes() {
    let storage = SqliteStorage::open_in_memory(&repo_migrations_dir()).unwrap();

    let result: Result<(), StorageError> = storage.transaction(|tx| {
        tx.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES ('tx_key', '\"1\"', 0)",
            [],
        )?;
        Err(StorageError::Migration("intentional rollback".to_string()))
    });
    assert!(result.is_err());

    let rows = storage
        .query("SELECT key FROM settings WHERE key = 'tx_key'", &[])
        .unwrap();
    assert!(rows.is_empty(), "rolled-back insert must not persist");
}

#[test]
fn transaction_commit_persists() {
    let storage = SqliteStorage::open_in_memory(&repo_migrations_dir()).unwrap();

    storage
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES ('ok_key', '\"1\"', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    let rows = storage
        .query("SELECT key FROM settings WHERE key = 'ok_key'", &[])
        .unwrap();
    assert_eq!(rows.len(), 1);
}
