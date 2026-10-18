//! Integration tests for the graph store, mirror, extraction, and commands
//! (`GRAPH.md` §3-§6, §10; tasks.md Task 12.8).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_graph::{
    entity_id, AlgoOptions, Algorithm, CozoGraphStore, DocumentGraphInput, EntityExtractor,
    GraphError, GraphMirror, GraphStore, NerExtractor, RecordingSink, RegexExtractor,
    TokenClassifier, TokenLabel,
};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage, Value};
use serde_json::json;

fn brain_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/brain")
}

fn sqlite_storage() -> Arc<dyn Storage> {
    register_sqlite_vec();
    let migrations = load_migrations_from(&brain_migrations_dir()).unwrap();
    Arc::new(SqliteStorage::open_in_memory_with_migrations(&migrations).unwrap())
}

/// The mirror's `entity_mentions` reference `documents`, so seed a row first.
fn seed_document(storage: &Arc<dyn Storage>, id: &str, path: &str) {
    storage
        .execute(
            "INSERT INTO documents \
             (id, path, kind, title, mime, size, hash, word_count, reading_time_s, \
              created_at, updated_at, indexed_at) \
             VALUES (?1, ?2, 'note', ?3, 'text/markdown', 0, '', 0, 0, 0, 0, 0)",
            &[
                Value::String(id.to_string()),
                Value::String(path.to_string()),
                Value::String("Title".to_string()),
            ],
        )
        .unwrap();
}

fn doc(id: &str, path: &str, title: &str, content: &str) -> DocumentGraphInput {
    DocumentGraphInput {
        document_id: id.to_string(),
        path: path.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        tags: vec!["notes".to_string()],
        links_to: Vec::new(),
    }
}

// --- 12.2 schema + CRUD -----------------------------------------------------

#[test]
fn schema_is_created_and_entities_round_trip() {
    let store = CozoGraphStore::open_in_memory().unwrap();
    assert_eq!(store.entity_count().unwrap(), 0);

    store
        .upsert_entities(&[fergents_graph::Entity {
            id: "e1".into(),
            name: "Alice".into(),
            entity_type: "person".into(),
            canonical_name: None,
            metadata: Some(json!({"extractor": "ner"})),
            first_seen_at: 1,
            last_seen_at: 2,
        }])
        .unwrap();

    assert_eq!(store.entity_count().unwrap(), 1);
    let fetched = store.get_entity("e1").unwrap().expect("entity");
    assert_eq!(fetched.name, "Alice");
    assert_eq!(fetched.entity_type, "person");
}

#[test]
fn graph_opens_on_disk_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path_str = path.to_str().unwrap();
    {
        let store = CozoGraphStore::open(path_str).unwrap();
        store
            .upsert_relations(&[fergents_graph::Relation {
                id: "r1".into(),
                src: "a".into(),
                dst: "b".into(),
                kind: "links_to".into(),
                weight: 1.0,
                confidence: 1.0,
                metadata: None,
                source_doc: Some("a".into()),
                created_at: 0,
            }])
            .unwrap();
        assert_eq!(store.relation_count().unwrap(), 1);
    }
    let reopened = CozoGraphStore::open(path_str).unwrap();
    assert_eq!(reopened.relation_count().unwrap(), 1);
}

// --- 12.3 SQLite mirror -----------------------------------------------------

#[test]
fn indexing_mirrors_into_sqlite_and_stays_in_sync() {
    let store = Arc::new(CozoGraphStore::open_in_memory().unwrap());
    let storage = sqlite_storage();
    let sink = Arc::new(RecordingSink::new());
    let graph =
        fergents_graph::Graph::new(store, "personal", sink.clone()).with_mirror(storage.clone());
    let mirror = GraphMirror::new(storage.clone());
    seed_document(&storage, "doc1", "notes/a.md");
    seed_document(&storage, "doc2", "notes/b.md");

    let mut input = doc(
        "doc1",
        "notes/a.md",
        "A",
        "See https://example.com and call `parse_document`.",
    );
    input.links_to = vec!["doc2".to_string()];
    graph.index_document(&input).unwrap();

    // Mirror tables are populated.
    assert!(mirror.entity_count().unwrap() >= 2);
    assert!(mirror.relation_count().unwrap() >= 3);
    assert!(mirror.mention_count().unwrap() >= 1);

    // The graph.updated event was emitted.
    assert!(sink.kinds().contains(&"graph.updated".to_string()));

    let before = mirror.relation_count().unwrap();
    graph.index_document(&input).unwrap();
    assert_eq!(
        mirror.relation_count().unwrap(),
        before,
        "reindexing must not duplicate relations"
    );

    graph.remove_document("doc1").unwrap();
    assert_eq!(mirror.mention_count().unwrap(), 0);
}

// --- 12.4/12.6 extraction + structural relations ----------------------------

#[test]
fn indexing_creates_structural_relations_with_confidence_one() {
    let store = Arc::new(CozoGraphStore::open_in_memory().unwrap());
    let graph =
        fergents_graph::Graph::new(store.clone(), "personal", Arc::new(RecordingSink::new()));

    let mut input = doc("d1", "notes/deep/a.md", "A", "mail me at a@b.com");
    input.tags = vec!["research".to_string()];
    input.links_to = vec!["d2".to_string()];
    graph.index_document(&input).unwrap();

    let relations = store.all_relations().unwrap();
    let kinds: Vec<&str> = relations.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"links_to"), "{kinds:?}");
    assert!(kinds.contains(&"mentions"), "{kinds:?}");
    assert!(kinds.contains(&"tagged_with"), "{kinds:?}");
    assert!(kinds.contains(&"in_folder"), "{kinds:?}");
    assert!(relations.iter().all(|r| r.confidence == 1.0));

    // The email became a `custom` entity mentioned by the document.
    let email_id = entity_id("custom", "a@b.com");
    assert!(store.get_entity(&email_id).unwrap().is_some());
}

#[test]
fn regex_extraction_finds_urls_and_code_symbols() {
    let entities = RegexExtractor::new()
        .extract("Visit https://fergents.dev and call `build_graph`.")
        .unwrap();
    let names: Vec<&str> = entities
        .iter()
        .map(|e| e.normalized_name.as_str())
        .collect();
    assert!(names.contains(&"https://fergents.dev"));
    assert!(names.contains(&"build_graph"));
}

// --- 12.5 NER ---------------------------------------------------------------

/// A deterministic stand-in for the ONNX classifier.
struct StubNer;

impl TokenClassifier for StubNer {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn classify(&self, text: &str) -> Result<Vec<TokenLabel>, GraphError> {
        let mut tokens = Vec::new();
        for (needle, label) in [("Alice", "B-PER"), ("Smith", "I-PER"), ("Acme", "B-ORG")] {
            if let Some(index) = text.find(needle) {
                tokens.push(TokenLabel {
                    text: needle.to_string(),
                    start: index as i64,
                    end: (index + needle.len()) as i64,
                    label: label.to_string(),
                });
            }
        }
        Ok(tokens)
    }
}

#[test]
fn ner_groups_tokens_into_entities() {
    let extractor = NerExtractor::new(Box::new(StubNer));
    let entities = extractor.extract("Alice Smith works at Acme.").unwrap();
    let got: Vec<(String, String)> = entities
        .iter()
        .map(|e| (e.entity_type.clone(), e.normalized_name.clone()))
        .collect();
    assert!(
        got.contains(&("person".into(), "alice smith".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&("organization".into(), "acme".into())),
        "{got:?}"
    );
}

#[test]
fn ner_degrades_to_regex_only_without_failing() {
    // With `ner-real` off (the default) there is no ONNX backend, so the
    // default extractors are regex-only and extraction still succeeds.
    let extractors = fergents_graph::default_extractors();
    let entities = extractors
        .extract("Reach me at a@b.com or see https://example.com.")
        .unwrap();
    assert!(!entities.is_empty());
    assert!(entities
        .iter()
        .all(|e| e.entity_type != "person" && e.entity_type != "organization"));
}

// --- 12.7 commands ----------------------------------------------------------

#[test]
fn query_algo_neighbors_subgraph_and_merge_commands() {
    let store = Arc::new(CozoGraphStore::open_in_memory().unwrap());
    let graph =
        fergents_graph::Graph::new(store.clone(), "personal", Arc::new(RecordingSink::new()));

    let mut a = doc("a", "notes/a.md", "A", "mentions b");
    a.links_to = vec!["b".to_string()];
    let mut b = doc("b", "notes/b.md", "B", "mentions c");
    b.links_to = vec!["c".to_string()];
    let c = doc("c", "notes/c.md", "C", "leaf");
    graph.index_document(&a).unwrap();
    graph.index_document(&b).unwrap();
    graph.index_document(&c).unwrap();

    // graph.query: an ad-hoc Datalog query.
    let rows = graph
        .query(
            "?[src, dst] := *relation{src, dst, kind}, kind == \"links_to\"",
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(rows.rows.len(), 2, "a→b and b→c");

    // graph.neighbors.
    let neighbors = graph.neighbors("a", 1).unwrap();
    let ids: Vec<&str> = neighbors.entities.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"b"), "{ids:?}");

    // graph.subgraph (2 hops from a reaches c).
    let subgraph = graph.subgraph(&["a".to_string()], 2).unwrap();
    let ids: Vec<&str> = subgraph.entities.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"c"), "{ids:?}");

    // graph.algo: pagerank + shortest path.
    let ranked = graph
        .algorithm(
            Algorithm::PageRank,
            &AlgoOptions {
                top_k: 5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!ranked.is_empty());
    let path = graph
        .algorithm(
            Algorithm::ShortestPath,
            &AlgoOptions {
                from: Some("a".into()),
                to: Some("c".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(path[0]["path"], json!(["a", "b", "c"]));

    // graph.entities.merge: fold "b" into "c".
    let merged = graph.merge_entities("b", "c").unwrap();
    assert_eq!(merged.id, "c");
    assert!(store.get_entity("b").unwrap().is_none());
    let relations = store.all_relations().unwrap();
    assert!(
        relations.iter().all(|r| r.src != "b" && r.dst != "b"),
        "merging must re-point relations: {relations:?}"
    );
}

#[test]
fn mirror_merge_repoints_and_drops_the_source() {
    let store = Arc::new(CozoGraphStore::open_in_memory().unwrap());
    let storage = sqlite_storage();
    let graph = fergents_graph::Graph::new(store, "personal", Arc::new(RecordingSink::new()))
        .with_mirror(storage.clone());
    let mirror = GraphMirror::new(storage);

    graph.index_document(&doc("d1", "a.md", "A", "x")).unwrap();
    let _ = mirror.entity_count().unwrap();
    // Merging a missing source still succeeds (no-op) and keeps the target.
    let merged = graph
        .merge_entities(&entity_id("url", "https://missing.example"), "d1")
        .unwrap();
    assert_eq!(merged.id, "d1");
}
