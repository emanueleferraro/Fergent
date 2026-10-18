//! Integration tests for the indexing pipeline: scan, incremental reindex,
//! soft delete/restore, queue recovery, limits, and the watcher.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_index::{
    EmbeddingCache, EmbeddingPipeline, IndexJobOp, IndexQueue, Indexer, IndexerConfig,
    RecordingSink, SqliteVecStore, StubEmbedder, VaultWatcher, DEFAULT_EMBED_DIM,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};
use serde_json::Value;

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn migrations() -> Vec<fergents_storage::Migration> {
    load_migrations_from(&brain_migrations_dir()).unwrap()
}

fn memory_storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations()).unwrap())
}

type TestIndexer = Indexer<StubEmbedder, SqliteVecStore>;

fn build(vault: &Path) -> (TestIndexer, Arc<RecordingSink>, Arc<dyn Storage>) {
    let storage = memory_storage();
    let sink = Arc::new(RecordingSink::new());
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let config = IndexerConfig::new("brain-1", vault);
    let indexer = Indexer::new(storage.clone(), config, pipeline, sink.clone()).unwrap();
    (indexer, sink, storage)
}

fn write(vault: &Path, rel: &str, content: &str) {
    let path = vault.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn scalar(storage: &Arc<dyn Storage>, sql: &str, params: &[Value]) -> Option<Value> {
    storage
        .query(sql, params)
        .unwrap()
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next().map(|(_, value)| value))
}

const NOTE: &str =
    "---\ntitle: Alpha\ntags: [x]\n---\n\n# Heading\n\nSome body words here for chunking.\n";

#[test]
fn scan_indexes_the_vault_and_is_idempotent() {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "alpha.md", NOTE);
    write(vault.path(), "notes/beta.md", "# Beta\n\nMore words.\n");

    let (indexer, sink, storage) = build(vault.path());
    let scan = indexer.scan_and_enqueue().unwrap();
    assert_eq!(scan.scanned, 2);
    assert_eq!(scan.enqueued, 2);

    let processed = indexer.process_pending().unwrap();
    assert_eq!(processed.processed, 2);
    assert_eq!(processed.failed, 0);

    assert_eq!(
        scalar(&storage, "SELECT count(*) AS n FROM documents", &[]),
        Some(Value::from(2))
    );
    assert_eq!(
        scalar(
            &storage,
            "SELECT title FROM documents WHERE path = 'alpha.md'",
            &[]
        ),
        Some(Value::String("Alpha".to_string()))
    );
    assert_eq!(
        scalar(
            &storage,
            "SELECT kind FROM documents WHERE path = 'notes/beta.md'",
            &[]
        ),
        Some(Value::String("note".to_string()))
    );

    // Every chunk got a vector.
    let chunks = scalar(&storage, "SELECT count(*) AS n FROM chunks", &[])
        .and_then(|v| v.as_u64())
        .unwrap();
    let vectors = scalar(&storage, "SELECT count(*) AS n FROM vec_chunks", &[])
        .and_then(|v| v.as_u64())
        .unwrap();
    assert!(chunks > 0);
    assert_eq!(chunks, vectors);

    // Events fired.
    assert!(sink.count("index.queued") >= 2);
    assert_eq!(sink.count("index.completed"), 2);
    assert_eq!(sink.count("note.created"), 2);

    // A second scan finds nothing to do.
    let scan = indexer.scan_and_enqueue().unwrap();
    assert_eq!(scan.unchanged, 2);
    assert_eq!(scan.enqueued, 0);
    assert!(indexer.queue().is_empty().unwrap());
}

#[test]
fn incremental_reindex_of_one_file_is_fast() {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "alpha.md", NOTE);
    let (indexer, _sink, storage) = build(vault.path());
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    let first_hash = scalar(
        &storage,
        "SELECT hash FROM documents WHERE path = 'alpha.md'",
        &[],
    )
    .unwrap();

    // Change one file.
    write(
        vault.path(),
        "alpha.md",
        "# Alpha\n\nChanged body text entirely.\n",
    );

    let started = Instant::now();
    let scan = indexer.scan_and_enqueue().unwrap();
    assert_eq!(scan.enqueued, 1);
    let processed = indexer.process_pending().unwrap();
    let elapsed = started.elapsed();
    assert_eq!(processed.processed, 1);

    assert!(
        elapsed < Duration::from_millis(200),
        "incremental reindex took {elapsed:?}"
    );

    let second_hash = scalar(
        &storage,
        "SELECT hash FROM documents WHERE path = 'alpha.md'",
        &[],
    )
    .unwrap();
    assert_ne!(first_hash, second_hash);
    assert_eq!(
        scalar(
            &storage,
            "SELECT title FROM documents WHERE path = 'alpha.md'",
            &[]
        ),
        Some(Value::String("Alpha".to_string()))
    );
}

#[test]
fn deletion_is_soft_and_restorable() {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "alpha.md", NOTE);
    let (indexer, _sink, storage) = build(vault.path());
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();

    // Remove the file from disk; the scan enqueues a soft delete.
    std::fs::remove_file(vault.path().join("alpha.md")).unwrap();
    let scan = indexer.scan_and_enqueue().unwrap();
    assert_eq!(scan.enqueued, 1);
    indexer.process_pending().unwrap();

    let deleted = scalar(
        &storage,
        "SELECT deleted_at FROM documents WHERE path = 'alpha.md'",
        &[],
    )
    .unwrap();
    assert!(!deleted.is_null(), "document should be soft-deleted");
    assert_eq!(
        scalar(&storage, "SELECT count(*) AS n FROM chunks", &[]),
        Some(Value::from(0))
    );

    // Restore: the file comes back and is re-indexed.
    write(vault.path(), "alpha.md", NOTE);
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    let restored = scalar(
        &storage,
        "SELECT deleted_at FROM documents WHERE path = 'alpha.md'",
        &[],
    )
    .unwrap();
    assert!(restored.is_null(), "document should be restored");
}

#[test]
fn interrupted_queue_resumes_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    register_sqlite_vec();

    {
        let storage: Arc<dyn Storage> =
            Arc::new(SqliteStorage::open_with_migrations(&db_path, &migrations()).unwrap());
        let queue = IndexQueue::new(storage);
        queue.enqueue("a.md", IndexJobOp::Upsert, 0).unwrap();
        queue.enqueue("b.md", IndexJobOp::Delete, 0).unwrap();
        assert_eq!(queue.stats().unwrap().queued, 2);
    }

    // "Restart": reopen the same file and assert pending work survives.
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open_with_migrations(&db_path, &migrations()).unwrap());
    let queue = IndexQueue::new(storage);
    let pending = queue.pending().unwrap();
    assert_eq!(pending.len(), 2, "queued jobs must resume after restart");
    assert!(pending.iter().any(|j| j.path == "a.md"));
    assert!(pending.iter().any(|j| j.path == "b.md"));
}

#[test]
fn oversized_file_is_skipped() {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "big.md", &"word ".repeat(100));

    let storage = memory_storage();
    let sink = Arc::new(RecordingSink::new());
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let mut config = IndexerConfig::new("brain-1", vault.path());
    config.max_file_bytes = 16; // smaller than the file
    let indexer = Indexer::new(storage.clone(), config, pipeline, sink).unwrap();

    let scan = indexer.scan_and_enqueue().unwrap();
    assert_eq!(scan.skipped_oversized, 1);
    assert_eq!(scan.enqueued, 0);
    assert_eq!(
        scalar(&storage, "SELECT count(*) AS n FROM documents", &[]),
        Some(Value::from(0)),
        "an oversized file must not create a document"
    );
}

#[test]
fn watcher_enqueues_on_change() {
    let vault = tempfile::tempdir().unwrap();
    let storage = memory_storage();
    let sink = Arc::new(RecordingSink::new());

    let watcher = VaultWatcher::start(
        vault.path().to_path_buf(),
        "brain-1".to_string(),
        IndexQueue::new(storage),
        sink.clone(),
    )
    .unwrap();

    write(vault.path(), "new.md", "# New\n\nhello\n");

    // Poll for the enqueued job (filesystem notifications are asynchronous).
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if sink
            .kinds()
            .iter()
            .any(|k| k == "file.created" || k == "file.changed")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    watcher.stop();

    assert!(
        sink.kinds()
            .iter()
            .any(|k| k == "file.created" || k == "file.changed"),
        "watcher should observe the new file: {:?}",
        sink.kinds()
    );
    assert!(sink.count("index.queued") >= 1);
}
