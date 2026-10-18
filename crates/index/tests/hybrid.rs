//! Integration tests for the hybrid pipeline: RRF fusion, the result shape,
//! reranking, the per-mode dispatch, and the `search.executed` event
//! (`SEARCH.md` §2, §7, §8, §11, §20).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_index::{
    EmbeddingCache, EmbeddingPipeline, Indexer, IndexerConfig, RecordingSink, Reranker,
    SearchEngine, SearchMode, SearchRequest, SqliteVecStore, StubEmbedder, DEFAULT_EMBED_DIM,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn memory_storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    let migrations = load_migrations_from(&brain_migrations_dir()).unwrap();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations).unwrap())
}

struct NullSinkStub;
impl fergents_index::EventSink for NullSinkStub {
    fn emit(&self, _kind: &str, _payload: serde_json::Value) {}
}

/// A reranker that scores documents by their position, lowest first, so it
/// always reverses the fused order — an unambiguous, content-independent signal
/// that reranking ran.
struct ReverseReranker;

impl Reranker for ReverseReranker {
    fn model(&self) -> &str {
        "reverse-stub"
    }

    fn rerank(
        &self,
        _query: &str,
        documents: &[String],
    ) -> Result<Vec<f32>, fergents_index::IndexError> {
        Ok((0..documents.len()).map(|i| i as f32).collect())
    }
}

fn write(vault: &Path, rel: &str, content: &str) {
    std::fs::write(vault.join(rel), content).unwrap();
}

/// Index a small vault into a fresh in-memory brain.
fn setup() -> (tempfile::TempDir, Arc<dyn Storage>) {
    let vault = tempfile::tempdir().unwrap();
    write(
        vault.path(),
        "one.md",
        "# One\n\nrust ownership borrow checker memory safety\n",
    );
    write(
        vault.path(),
        "two.md",
        "---\ntags: [systems]\n---\n# Two\n\nrust ownership memory safety crash failure\n",
    );
    write(
        vault.path(),
        "three.md",
        "# Three\n\nunrelated cooking recipe pasta tomato basil\n",
    );

    let storage = memory_storage();
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let config = IndexerConfig::new("personal", vault.path());
    let indexer = Indexer::new(storage.clone(), config, pipeline, Arc::new(NullSinkStub)).unwrap();
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    (vault, storage)
}

fn engine(storage: &Arc<dyn Storage>, reranker: Option<Arc<dyn Reranker>>) -> SearchEngine {
    SearchEngine::new(
        storage.clone(),
        "personal",
        Arc::new(StubEmbedder::default()),
        Arc::new(SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM)),
        reranker,
        Arc::new(RecordingSink::new()),
    )
}

fn paths(response: &fergents_index::SearchResponse) -> Vec<String> {
    response.results.iter().map(|h| h.path.clone()).collect()
}

// --- Fusion ----------------------------------------------------------------

#[test]
fn hybrid_includes_documents_the_and_based_bm25_misses() {
    let (_vault, storage) = setup();
    // "two.md" lacks "borrow"/"checker", so BM25 (implicit AND) misses it.
    let query = "rust ownership borrow checker";

    let mut bm25 = SearchRequest::new(query);
    bm25.mode = SearchMode::Bm25;
    let bm25_paths = paths(&engine(&storage, None).search(&bm25).unwrap());
    assert!(!bm25_paths.contains(&"two.md".to_string()));
    assert!(bm25_paths.contains(&"one.md".to_string()));

    let mut hybrid = SearchRequest::new(query);
    hybrid.mode = SearchMode::Hybrid;
    let hybrid_paths = paths(&engine(&storage, None).search(&hybrid).unwrap());
    assert!(
        hybrid_paths.contains(&"two.md".to_string()),
        "hybrid should surface the vector neighbour: {hybrid_paths:?}"
    );
}

#[test]
fn hybrid_result_shape_carries_per_stage_scores_and_citation() {
    let (_vault, storage) = setup();
    let mut request = SearchRequest::new("rust ownership borrow checker");
    request.mode = SearchMode::Hybrid;
    let response = engine(&storage, None).search(&request).unwrap();
    assert!(response.candidates.bm25 > 0);
    assert!(response.candidates.vector > 0);

    let top = response.results.first().expect("a result");
    assert_eq!(top.path, "one.md");
    assert!(top.scores.bm25.is_some());
    assert!(top.scores.fusion.is_some());
    assert!(top.scores.rerank.is_none(), "no reranker was configured");
    assert!(top.score > 0.0);
    assert!(
        top.citation.contains("one.md"),
        "citation: {}",
        top.citation
    );
    assert_eq!(top.metadata.brain, "personal");
    assert!(top.metadata.created.is_some());
}

#[test]
fn sparse_mode_is_unavailable_until_sparse_vectors_exist() {
    let (_vault, storage) = setup();
    let mut request = SearchRequest::new("rust ownership");
    request.mode = SearchMode::Sparse;
    assert!(engine(&storage, None).search(&request).is_err());
}

#[test]
fn filters_apply_to_the_vector_leg_too() {
    let (_vault, storage) = setup();
    let mut request = SearchRequest::new("rust ownership memory");
    request.mode = SearchMode::Vector;
    let unfiltered = paths(&engine(&storage, None).search(&request).unwrap());
    assert!(unfiltered.contains(&"two.md".to_string()));

    let mut filtered = SearchRequest::new("rust ownership memory tag:systems");
    filtered.mode = SearchMode::Vector;
    let filtered_paths = paths(&engine(&storage, None).search(&filtered).unwrap());
    assert_eq!(filtered_paths, vec!["two.md".to_string()]);
}

// --- Reranking -------------------------------------------------------------

#[test]
fn rerank_reorders_the_candidates() {
    let (_vault, storage) = setup();
    // Three tokens so reranking is not skipped as trivial.
    let query = "rust ownership memory";

    let mut request = SearchRequest::new(query);
    request.mode = SearchMode::Bm25;
    let before = paths(&engine(&storage, None).search(&request).unwrap());
    assert!(
        before.len() >= 2,
        "need at least two candidates: {before:?}"
    );

    let reranked_response = engine(&storage, Some(Arc::new(ReverseReranker)))
        .search(&request)
        .unwrap();
    let after = paths(&reranked_response);

    let mut expected = before.clone();
    expected.reverse();
    assert_eq!(after, expected, "the reranker should reverse the order");

    let top = reranked_response.results.first().unwrap();
    assert!(top.scores.rerank.is_some());
    assert_eq!(top.score, top.scores.rerank.unwrap());
}

#[test]
fn trivial_queries_skip_reranking() {
    let (_vault, storage) = setup();
    // Two tokens → below `RERANK_MIN_TOKENS`.
    let mut request = SearchRequest::new("rust ownership");
    request.mode = SearchMode::Bm25;

    let without = paths(&engine(&storage, None).search(&request).unwrap());
    let with = engine(&storage, Some(Arc::new(ReverseReranker)))
        .search(&request)
        .unwrap();
    assert_eq!(
        paths(&with),
        without,
        "a trivial query must not be reranked"
    );
    assert!(with.results[0].scores.rerank.is_none());
}

#[test]
fn rerank_can_be_disabled_per_request() {
    let (_vault, storage) = setup();
    let mut request = SearchRequest::new("rust ownership memory");
    request.mode = SearchMode::Bm25;
    request.rerank = false;

    let without = paths(&engine(&storage, None).search(&request).unwrap());
    let with = engine(&storage, Some(Arc::new(ReverseReranker)))
        .search(&request)
        .unwrap();
    assert_eq!(paths(&with), without);
    assert!(with.results[0].scores.rerank.is_none());
}

// --- Recall sanity (the >=5 pp gate lives in the ignored nightly bench) -----

#[test]
fn hybrid_recall_is_not_below_bm25_on_the_fixture() {
    let (_vault, storage) = setup();
    let engine = engine(&storage, None);
    // (query, judged-relevant path). The first query omits a term from its note,
    // which the AND-based BM25 leg cannot match.
    let labeled = [
        ("rust ownership borrow failure", "two.md"),
        ("rust ownership borrow checker", "one.md"),
    ];

    let mut bm25_hits = 0;
    let mut hybrid_hits = 0;
    for (query, relevant) in labeled {
        let mut request = SearchRequest::new(query);
        request.top_k = 8;
        request.mode = SearchMode::Bm25;
        if engine
            .search(&request)
            .unwrap()
            .results
            .iter()
            .any(|h| h.path == relevant)
        {
            bm25_hits += 1;
        }
        request.mode = SearchMode::Hybrid;
        if engine
            .search(&request)
            .unwrap()
            .results
            .iter()
            .any(|h| h.path == relevant)
        {
            hybrid_hits += 1;
        }
    }

    assert!(
        bm25_hits < labeled.len(),
        "the fixture must expose a BM25 miss"
    );
    assert!(
        hybrid_hits > bm25_hits,
        "hybrid should recover the document BM25 missed (bm25={bm25_hits}, hybrid={hybrid_hits})"
    );
}

// --- Events ----------------------------------------------------------------

#[test]
fn search_emits_search_executed() {
    let (_vault, storage) = setup();
    let sink = Arc::new(RecordingSink::new());
    let engine = SearchEngine::new(
        storage.clone(),
        "personal",
        Arc::new(StubEmbedder::default()),
        Arc::new(SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM)),
        None,
        sink.clone(),
    );

    let response = engine
        .search(&SearchRequest::new("rust ownership"))
        .unwrap();
    assert!(!response.results.is_empty());

    let kinds = sink.kinds();
    assert_eq!(kinds, vec!["search.executed".to_string()]);
    let payload = sink.first("search.executed").expect("payload");
    assert_eq!(payload["mode"], "hybrid");
    assert_eq!(payload["brain"], "personal");
    assert!(payload["candidates"]["bm25"].as_u64().unwrap() > 0);
}
