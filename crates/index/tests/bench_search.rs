//! Recall benchmark for hybrid vs BM25 search (spec Appendix D,
//! `bench/search/`).
//!
//! Run with `cargo test -p fergents-index --features bench -- --ignored`. It is
//! `#[ignore]`d so it never runs on every PR; the nightly benchmark job runs it
//! explicitly.
#![cfg(feature = "bench")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_index::{
    EmbeddingCache, EmbeddingPipeline, Indexer, IndexerConfig, NullSink, SearchEngine, SearchMode,
    SearchRequest, SqliteVecStore, StubEmbedder, DEFAULT_EMBED_DIM,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};
use serde_json::Value;

/// The minimum hybrid-over-BM25 recall gain required (spec Appendix D).
const MIN_RECALL_GAIN: f64 = 0.05;
/// Recall is measured at this cutoff.
const RECALL_K: usize = 8;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_migrations() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn memory_storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    let migrations = load_migrations_from(&rust_migrations()).unwrap();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations).unwrap())
}

struct NullSinkStub;
impl fergents_index::EventSink for NullSinkStub {
    fn emit(&self, _kind: &str, _payload: Value) {}
}

fn engine(storage: &Arc<dyn Storage>) -> SearchEngine {
    SearchEngine::new(
        storage.clone(),
        "bench",
        Arc::new(StubEmbedder::default()),
        Arc::new(SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM)),
        None,
        Arc::new(NullSink),
    )
}

/// Index the benchmark vault into a fresh in-memory brain.
fn index_vault() -> (tempfile::TempDir, Arc<dyn Storage>) {
    let vault = tempfile::tempdir().unwrap();
    let source = repo_root().join("bench/search/vault");
    copy_dir(&source, vault.path());

    let storage = memory_storage();
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let config = IndexerConfig::new("bench", vault.path());
    let indexer = Indexer::new(storage.clone(), config, pipeline, Arc::new(NullSinkStub)).unwrap();
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    (vault, storage)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A query and its judged-relevant vault paths.
struct Judged {
    query: String,
    relevant: Vec<String>,
}

fn load_queries() -> Vec<Judged> {
    let text = std::fs::read_to_string(repo_root().join("bench/search/queries.json")).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    value["queries"]
        .as_array()
        .expect("queries array")
        .iter()
        .map(|q| Judged {
            query: q["query"].as_str().unwrap_or_default().to_string(),
            relevant: q["relevant"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

/// Mean recall@k for `mode` over `queries`.
fn recall_at_k(engine: &SearchEngine, queries: &[Judged], mode: SearchMode) -> f64 {
    if queries.is_empty() {
        return 0.0;
    }
    let mut total = 0.0;
    for judged in queries {
        let mut request = SearchRequest::new(judged.query.clone());
        request.mode = mode;
        request.top_k = RECALL_K;
        request.rerank = false;
        let response = engine.search(&request).unwrap();
        let retrieved: Vec<&str> = response.results.iter().map(|h| h.path.as_str()).collect();
        let hits = judged
            .relevant
            .iter()
            .filter(|path| retrieved.contains(&path.as_str()))
            .count();
        total += hits as f64 / judged.relevant.len() as f64;
    }
    total / queries.len() as f64
}

#[test]
#[ignore = "nightly benchmark (spec Appendix D); run with --features bench -- --ignored"]
fn hybrid_recall_beats_bm25_by_five_points() {
    let (_vault, storage) = index_vault();
    let engine = engine(&storage);
    let queries = load_queries();
    assert_eq!(queries.len(), 50, "the benchmark set is 50 queries");

    let bm25 = recall_at_k(&engine, &queries, SearchMode::Bm25);
    let hybrid = recall_at_k(&engine, &queries, SearchMode::Hybrid);
    eprintln!("recall@{RECALL_K}: bm25={bm25:.4} hybrid={hybrid:.4}");

    assert!(
        hybrid - bm25 >= MIN_RECALL_GAIN,
        "hybrid recall ({hybrid:.4}) must beat bm25 recall ({bm25:.4}) by >= {MIN_RECALL_GAIN}"
    );
}
