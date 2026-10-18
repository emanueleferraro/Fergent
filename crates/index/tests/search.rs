//! Integration tests for query analysis and structured filters (BM25 leg,
//! `SEARCH.md` §5, §9, §10). The hybrid pipeline is covered in `hybrid.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_index::{
    analyze_query, EmbeddingCache, EmbeddingPipeline, Indexer, IndexerConfig, NullSink,
    SearchEngine, SearchMode, SearchRequest, SqliteVecStore, StubEmbedder, DEFAULT_EMBED_DIM,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage};
use serde_json::Value;

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn memory_storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    let migrations = load_migrations_from(&brain_migrations_dir()).unwrap();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations).unwrap())
}

fn write(vault: &Path, rel: &str, content: &str) {
    let path = vault.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// Set a file's modification time (the pipeline stores it in `updated_at`).
fn set_modified(vault: &Path, rel: &str, unix_secs: u64) {
    let file = std::fs::File::options()
        .write(true)
        .open(vault.join(rel))
        .unwrap();
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs);
    file.set_modified(time).unwrap();
}

/// 2024-06-01T00:00:00Z.
const APPLE_MTIME: u64 = 1_717_200_000;
/// 2025-06-01T00:00:00Z.
const BETA_MTIME: u64 = 1_748_736_000;

struct NullSinkStub;
impl fergents_index::EventSink for NullSinkStub {
    fn emit(&self, _kind: &str, _payload: Value) {}
}

/// Index the fixture vault into a fresh in-memory brain.
fn setup() -> (tempfile::TempDir, Arc<dyn Storage>) {
    let vault = tempfile::tempdir().unwrap();
    write(
        vault.path(),
        "notes/apple.md",
        "---\nstatus: draft\ntags: [fruit]\n---\n# Apple\n\napple apple apple fruit\n",
    );
    write(
        vault.path(),
        "notes/beta.md",
        "---\nstatus: published\ntags: [fruit]\n---\n# Beta\n\napple beta fruit. See [[apple]].\n",
    );
    write(
        vault.path(),
        "archive/cherry.md",
        "---\nstatus: draft\ntags: [archived]\n---\n# Cherry\n\napple cherry cherry archive\n",
    );
    write(
        vault.path(),
        "code/sample.rs",
        "fn apple() { let apple = 1; }\n",
    );
    // A note that embeds an attachment (so `has:attachment` has a subject).
    write(
        vault.path(),
        "notes/design.md",
        "# Design\n\nSee ![[diagram.png]] apple diagram\n",
    );
    write(vault.path(), "assets/diagram.png", "PNGBYTES");

    // Distinct file modification times so the date filters have real subjects.
    set_modified(vault.path(), "notes/apple.md", APPLE_MTIME);
    set_modified(vault.path(), "notes/beta.md", BETA_MTIME);

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

fn engine(storage: &Arc<dyn Storage>) -> SearchEngine {
    SearchEngine::new(
        storage.clone(),
        "personal",
        Arc::new(StubEmbedder::default()),
        Arc::new(SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM)),
        None,
        Arc::new(NullSink),
    )
}

fn paths(response: &fergents_index::SearchResponse) -> Vec<String> {
    response.results.iter().map(|h| h.path.clone()).collect()
}

/// Run a BM25-only query (the Task 10 leg).
fn run(storage: &Arc<dyn Storage>, query: &str) -> Vec<String> {
    let mut request = SearchRequest::new(query);
    request.mode = SearchMode::Bm25;
    paths(&engine(storage).search(&request).unwrap())
}

// --- Query analysis --------------------------------------------------------

#[test]
fn query_analysis_extracts_terms_phrases_negation_and_filters() {
    let analyzed = analyze_query("foo \"bar baz\" -qux -tag:archive path:notes/ type:note");
    assert_eq!(analyzed.terms, vec!["foo"]);
    assert_eq!(analyzed.phrases, vec!["bar baz"]);
    assert_eq!(analyzed.excluded, vec!["qux"]);
    assert_eq!(analyzed.filters.path.as_deref(), Some("notes/"));
    assert_eq!(analyzed.filters.doc_type.as_deref(), Some("note"));
    assert_eq!(analyzed.filters.exclude_tags, vec!["archive"]);
}

#[test]
fn query_analysis_supports_frontmatter_and_brain_filters() {
    let analyzed = analyze_query("frontmatter.status:draft brain:work");
    assert_eq!(
        analyzed.filters.frontmatter,
        vec![("status".to_string(), "draft".to_string())]
    );
    assert_eq!(analyzed.brains, vec!["work"]);
    assert!(analyzed.terms.is_empty());
}

#[test]
fn query_analysis_handles_negated_phrases() {
    let analyzed = analyze_query("keep -\"drop this\"");
    assert_eq!(analyzed.terms, vec!["keep"]);
    assert_eq!(analyzed.excluded, vec!["drop this"]);
    assert!(analyzed.phrases.is_empty());
}

// --- BM25 ranking ----------------------------------------------------------

#[test]
fn bm25_ranks_more_relevant_documents_first() {
    let (_vault, storage) = setup();
    let results = run(&storage, "apple");
    assert!(!results.is_empty());
    assert_eq!(
        results.first().map(String::as_str),
        Some("notes/apple.md"),
        "the densest match should rank first: {results:?}"
    );
    assert!(results.iter().any(|p| p == "notes/beta.md"));
}

#[test]
fn phrase_query_matches_only_the_exact_sequence() {
    let (_vault, storage) = setup();
    let results = run(&storage, "\"apple apple\"");
    assert_eq!(results, vec!["notes/apple.md".to_string()]);
}

#[test]
fn negation_excludes_documents_containing_the_term() {
    let (_vault, storage) = setup();
    let results = run(&storage, "apple -fruit");
    assert!(!results.contains(&"notes/apple.md".to_string()));
    assert!(!results.contains(&"notes/beta.md".to_string()));
    assert!(results.contains(&"notes/design.md".to_string()));
    assert!(results.contains(&"code/sample.rs".to_string()));
}

// --- Filters ---------------------------------------------------------------

#[test]
fn tag_filter_restricts_to_tagged_documents() {
    let (_vault, storage) = setup();
    let results = run(&storage, "apple tag:fruit");
    assert!(results.contains(&"notes/apple.md".to_string()));
    assert!(results.contains(&"notes/beta.md".to_string()));
    assert!(!results.contains(&"notes/design.md".to_string()));

    let archived = run(&storage, "apple tag:archived");
    assert_eq!(archived, vec!["archive/cherry.md".to_string()]);
}

#[test]
fn exclude_tag_filter_removes_tagged_documents() {
    let (_vault, storage) = setup();
    let results = run(&storage, "apple -tag:archived");
    assert!(!results.contains(&"archive/cherry.md".to_string()));
    assert!(results.contains(&"notes/apple.md".to_string()));
}

#[test]
fn path_filter_is_a_prefix_match() {
    let (_vault, storage) = setup();
    let notes = run(&storage, "apple path:notes/");
    assert!(notes.iter().all(|p| p.starts_with("notes/")));
    assert!(notes.contains(&"notes/design.md".to_string()));

    let others = run(&storage, "apple -path:notes/");
    assert!(!others.iter().any(|p| p.starts_with("notes/")));
    assert!(others.contains(&"code/sample.rs".to_string()));
    assert!(others.contains(&"archive/cherry.md".to_string()));
}

#[test]
fn type_filter_restricts_to_kind() {
    let (_vault, storage) = setup();
    let results = run(&storage, "apple type:code");
    assert_eq!(results, vec!["code/sample.rs".to_string()]);
}

#[test]
fn language_filter_restricts_code_chunks() {
    let (_vault, storage) = setup();
    // Code language tagging is a Phase 3 concern; set it directly to exercise
    // the filter.
    storage
        .execute(
            "UPDATE chunks SET language = 'rust' WHERE document_id = \
             (SELECT id FROM documents WHERE path = 'code/sample.rs')",
            &[],
        )
        .unwrap();
    let rust = run(&storage, "apple language:rust");
    assert_eq!(rust, vec!["code/sample.rs".to_string()]);

    let python = run(&storage, "apple language:python");
    assert!(python.is_empty());
}

#[test]
fn after_and_before_filters_bound_the_file_mtime() {
    let (_vault, storage) = setup();

    let after = run(&storage, "apple after:2025-01-01");
    assert!(!after.contains(&"notes/apple.md".to_string()));
    assert!(after.contains(&"notes/beta.md".to_string()));

    let before = run(&storage, "apple before:2025-01-01");
    assert!(before.contains(&"notes/apple.md".to_string()));
    assert!(!before.contains(&"notes/beta.md".to_string()));
}

#[test]
fn has_filter_selects_links_backlinks_and_attachments() {
    let (_vault, storage) = setup();

    let with_link = run(&storage, "apple has:link");
    assert!(with_link.contains(&"notes/beta.md".to_string()));
    assert!(!with_link.contains(&"notes/apple.md".to_string()));

    let with_backlink = run(&storage, "apple has:backlink");
    assert_eq!(with_backlink, vec!["notes/apple.md".to_string()]);

    let with_attachment = run(&storage, "apple has:attachment");
    assert_eq!(with_attachment, vec!["notes/design.md".to_string()]);
}

#[test]
fn frontmatter_filter_matches_a_field_value() {
    let (_vault, storage) = setup();
    let drafts = run(&storage, "apple frontmatter.status:draft");
    assert!(drafts.contains(&"notes/apple.md".to_string()));
    assert!(drafts.contains(&"archive/cherry.md".to_string()));
    assert!(!drafts.contains(&"notes/beta.md".to_string()));
}

#[test]
fn invalid_filters_are_reported() {
    let (_vault, storage) = setup();
    let bad_date = SearchRequest::new("apple after:not-a-date");
    assert!(engine(&storage).search(&bad_date).is_err());

    let bad_has = SearchRequest::new("apple has:nonsense");
    assert!(engine(&storage).search(&bad_has).is_err());
}

// --- Cross-brain filter ----------------------------------------------------

#[test]
fn brain_filter_only_searches_the_requested_brain() {
    let (_vault, storage) = setup();

    // This storage is the "personal" brain.
    let mut request = SearchRequest::new("apple");
    request.brains = vec!["personal".to_string()];
    let hit = engine(&storage).search(&request).unwrap();
    assert!(!hit.results.is_empty());

    // A request for another brain finds nothing here.
    let mut other = SearchRequest::new("apple");
    other.brains = vec!["work".to_string()];
    let empty = engine(&storage).search(&other).unwrap();
    assert!(empty.results.is_empty());
    assert_eq!(empty.candidates.bm25, 0);

    // The inline `brain:` token behaves the same way.
    let inline_other = SearchRequest::new("apple brain:work");
    assert!(engine(&storage)
        .search(&inline_other)
        .unwrap()
        .results
        .is_empty());
    let inline_self = SearchRequest::new("apple brain:personal");
    assert!(!engine(&storage)
        .search(&inline_self)
        .unwrap()
        .results
        .is_empty());
}

#[test]
fn result_carries_snippet_and_metadata() {
    let (_vault, storage) = setup();
    let mut request = SearchRequest::new("apple apple");
    request.mode = SearchMode::Bm25;
    let response = engine(&storage).search(&request).unwrap();
    let top = response.results.first().expect("a result");
    assert_eq!(top.path, "notes/apple.md");
    assert_eq!(top.title, "Apple");
    assert!(!top.snippet.is_empty());
    assert!(top.score > 0.0, "score should be positive: {}", top.score);
    assert_eq!(top.scores.bm25, Some(top.score));
}
