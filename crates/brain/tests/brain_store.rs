//! Integration tests for the per-brain index store (`migrations/brain`,
//! spec Appendix B).

use std::path::{Path, PathBuf};

use fergents_brain::{available_schema_version, BrainError, BrainStore};
use fergents_storage::{Storage, StorageError};
use serde_json::{json, Value};

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn open(dir: &Path) -> BrainStore {
    BrainStore::open(dir, &brain_migrations_dir()).expect("open brain store")
}

#[test]
fn open_applies_migrations_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let migrations = brain_migrations_dir();

    let store = open(dir.path());
    assert_eq!(store.schema_version().unwrap(), 2);
    assert!(store.path().ends_with("index.sqlite"));
    drop(store);

    // Reopen: migrations are already applied, so the version is unchanged.
    let store = open(dir.path());
    assert_eq!(store.schema_version().unwrap(), 2);

    assert_eq!(available_schema_version(&migrations).unwrap(), 2);
}

#[test]
fn all_appendix_b_tables_exist() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let rows = store
        .storage()
        .query(
            "SELECT name FROM sqlite_master WHERE type IN ('table', 'view') ORDER BY name",
            &[],
        )
        .unwrap();
    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    for expected in [
        "chunks",
        "chunks_fts",
        "document_tags",
        "documents",
        "embedding_meta",
        "entities",
        "entity_mentions",
        "index_jobs",
        "index_meta",
        "links",
        "notes_meta",
        "relations",
        "schema_version",
        "tags",
        "vec_chunks",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing table {expected}"
        );
    }
}

#[test]
fn refuses_a_store_whose_schema_is_newer() {
    let dir = tempfile::tempdir().unwrap();
    let migrations = brain_migrations_dir();

    let store = open(dir.path());
    store
        .storage()
        .execute(
            "INSERT INTO schema_version (version, description, checksum, applied_at) \
             VALUES (999, 'future', 'checksum', 0)",
            &[],
        )
        .unwrap();
    drop(store);

    let err = match BrainStore::open(dir.path(), &migrations) {
        Ok(_) => panic!("expected the newer schema to be refused"),
        Err(e) => e,
    };
    match err {
        BrainError::Storage(StorageError::NewerSchema { found, available }) => {
            assert_eq!(found, 999);
            assert_eq!(available, 2);
        }
        other => panic!("expected NewerSchema, got {other:?}"),
    }
}

#[test]
fn fts5_chunks_fts_is_usable() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());

    // `chunks_fts` is an external-content FTS5 table (migration 0002): the text
    // lives in `chunks`, and the FTS row is kept in sync via `content_rowid`.
    store
        .storage()
        .execute(
            "INSERT INTO documents \
             (id, path, kind, title, mime, size, hash, word_count, reading_time_s, \
              created_at, updated_at, indexed_at) \
             VALUES ('d1', 'doc.md', 'note', 'Doc', 'text/markdown', 0, '', 0, 0, 0, 0, 0)",
            &[],
        )
        .unwrap();
    store
        .storage()
        .execute(
            "INSERT INTO chunks \
             (id, document_id, ordinal, content, token_count, start_offset, end_offset, heading_path) \
             VALUES ('c1', 'd1', 0, 'A force directed graph layout for the brain', 8, 0, 44, 'Graph')",
            &[],
        )
        .unwrap();
    store
        .storage()
        .execute(
            "INSERT INTO chunks_fts (rowid, content, heading_path) \
             SELECT rowid, content, heading_path FROM chunks WHERE id = 'c1'",
            &[],
        )
        .unwrap();

    let rows = store
        .storage()
        .query(
            "SELECT content FROM chunks_fts WHERE chunks_fts MATCH ?1",
            &[Value::String("graph".to_string())],
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn sqlite_vec_vec_chunks_is_usable() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());

    // A 1024-dim unit vector, passed as JSON text (sqlite-vec accepts JSON).
    let vector: Vec<Value> = (0..1024)
        .map(|i| json!(if i == 0 { 1.0 } else { 0.0 }))
        .collect();

    store
        .storage()
        .execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            &[
                Value::String("c1".to_string()),
                Value::Array(vector.clone()),
            ],
        )
        .unwrap();

    let rows = store
        .storage()
        .query(
            "SELECT chunk_id FROM vec_chunks WHERE embedding MATCH ?1 AND k = 1",
            &[Value::Array(vector)],
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["chunk_id"], json!("c1"));
}

#[test]
fn wal_mode_and_foreign_keys_are_on() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());

    let mode = store.storage().query("PRAGMA journal_mode", &[]).unwrap();
    assert_eq!(mode[0]["journal_mode"], json!("wal"));

    let fk = store.storage().query("PRAGMA foreign_keys", &[]).unwrap();
    assert_eq!(fk[0]["foreign_keys"], json!(1));
}

#[test]
fn foreign_keys_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());

    // A chunk referencing a missing document must be rejected.
    let result = store.storage().execute(
        "INSERT INTO chunks (id, document_id, ordinal, content, token_count, start_offset, end_offset) \
         VALUES ('ch1', 'missing', 0, 'x', 1, 0, 1)",
        &[],
    );
    assert!(result.is_err(), "foreign key violation should be rejected");
}
