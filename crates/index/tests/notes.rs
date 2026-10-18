//! Integration tests for links, tags, backlinks, outline, and graph queries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_index::{
    backlinks, broken_links, dead_ends, delete_tag, list_tags, orphans, outline, rename_tag,
    unlinked_mentions, EmbeddingCache, EmbeddingPipeline, Indexer, IndexerConfig, SqliteVecStore,
    StubEmbedder, DEFAULT_EMBED_DIM,
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

type TestIndexer = Indexer<StubEmbedder, SqliteVecStore>;

fn build(vault: &Path) -> (TestIndexer, Arc<dyn Storage>) {
    let storage = memory_storage();
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let config = IndexerConfig::new("brain-1", vault);
    let indexer = Indexer::new(storage.clone(), config, pipeline, Arc::new(NullSinkStub)).unwrap();
    (indexer, storage)
}

/// A no-op sink (keeps the tests focused on the data model).
struct NullSinkStub;
impl fergents_index::EventSink for NullSinkStub {
    fn emit(&self, _kind: &str, _payload: Value) {}
}

fn write(vault: &Path, rel: &str, content: &str) {
    std::fs::write(vault.join(rel), content).unwrap();
}

fn id_of(storage: &Arc<dyn Storage>, path: &str) -> String {
    storage
        .query(
            "SELECT id FROM documents WHERE path = ?1",
            &[Value::String(path.to_string())],
        )
        .unwrap()
        .first()
        .and_then(|row| row.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no document at {path}"))
        .to_string()
}

const ALPHA: &str = "# Alpha\n\nAlpha body text.\n\n#tag\n";
const BETA: &str = "# Beta\n\nSee [[alpha]] and [[missing]] for details.\n";
const GAMMA: &str = "# Gamma\n\nMentions Alpha as plain text.\n";

/// Index three notes: alpha (tagged, no links), beta (links to alpha + a
/// missing target), gamma (mentions alpha as plain text).
fn setup() -> (tempfile::TempDir, Arc<dyn Storage>) {
    let vault = tempfile::tempdir().unwrap();
    write(vault.path(), "alpha.md", ALPHA);
    write(vault.path(), "beta.md", BETA);
    write(vault.path(), "gamma.md", GAMMA);

    let (indexer, storage) = build(vault.path());
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    (vault, storage)
}

#[test]
fn backlinks_resolve_and_unresolved_links_are_reported() {
    let (_vault, storage) = setup();
    let alpha = id_of(&storage, "alpha.md");

    let inbound = backlinks(&storage, &alpha).unwrap();
    assert_eq!(inbound.len(), 1, "backlinks: {inbound:?}");
    let link = &inbound[0];
    assert_eq!(link.kind, "wiki");
    assert_eq!(link.dst_document_id.as_deref(), Some(alpha.as_str()));
    assert!(!link.unresolved);
    assert!(
        link.anchor
            .as_deref()
            .unwrap_or_default()
            .contains("[[alpha]]"),
        "anchor: {:?}",
        link.anchor
    );

    let broken = broken_links(&storage).unwrap();
    assert_eq!(broken.len(), 1, "broken: {broken:?}");
    assert_eq!(broken[0].dst_path.as_deref(), Some("missing"));
    assert!(broken[0].unresolved);
}

#[test]
fn link_to_a_file_by_name_resolves_when_the_target_is_indexed_later() {
    let vault = tempfile::tempdir().unwrap();
    // The note links to an asset that does not exist yet.
    write(
        vault.path(),
        "design.md",
        "# Design\n\nSee [[diagram.png]]\n",
    );
    let (indexer, storage) = build(vault.path());
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    assert_eq!(broken_links(&storage).unwrap().len(), 1);

    // The asset appears later, in a subfolder.
    std::fs::create_dir_all(vault.path().join("assets")).unwrap();
    std::fs::write(vault.path().join("assets/diagram.png"), "PNGBYTES").unwrap();
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();

    let png = id_of(&storage, "assets/diagram.png");
    let inbound = backlinks(&storage, &png).unwrap();
    assert_eq!(inbound.len(), 1, "backlinks: {inbound:?}");
    assert_eq!(inbound[0].dst_document_id.as_deref(), Some(png.as_str()));
    assert!(!inbound[0].unresolved);
    assert!(broken_links(&storage).unwrap().is_empty());
}

#[test]
fn unlinked_mentions_are_suggested_but_linked_notes_are_not() {
    let (_vault, storage) = setup();
    let alpha = id_of(&storage, "alpha.md");
    let beta = id_of(&storage, "beta.md");
    let gamma = id_of(&storage, "gamma.md");

    let mentions = unlinked_mentions(&storage, &alpha).unwrap();
    let ids: Vec<&str> = mentions.iter().map(|m| m.document_id.as_str()).collect();
    assert!(ids.contains(&gamma.as_str()), "mentions: {mentions:?}");
    assert!(
        !ids.contains(&beta.as_str()),
        "a note that links to the target must not be suggested"
    );
    assert!(
        !ids.contains(&alpha.as_str()),
        "the note must not mention itself"
    );
}

#[test]
fn outline_is_in_document_order() {
    let vault = tempfile::tempdir().unwrap();
    write(
        vault.path(),
        "doc.md",
        "# Title\n\nintro\n\n## Section\n\ntext\n\n### Sub\n\nmore\n",
    );
    let (indexer, storage) = build(vault.path());
    indexer.scan_and_enqueue().unwrap();
    indexer.process_pending().unwrap();
    let id = id_of(&storage, "doc.md");

    let entries = outline(&storage, vault.path(), &id).unwrap();
    let levels: Vec<u8> = entries.iter().map(|e| e.level).collect();
    let texts: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(levels, vec![1, 2, 3]);
    assert_eq!(texts, vec!["Title", "Section", "Sub"]);
}

#[test]
fn orphans_and_dead_ends_are_detected() {
    let (_vault, storage) = setup();
    let alpha = id_of(&storage, "alpha.md");
    let beta = id_of(&storage, "beta.md");
    let gamma = id_of(&storage, "gamma.md");

    let orphan_ids: Vec<String> = orphans(&storage)
        .unwrap()
        .into_iter()
        .map(|n| n.document_id)
        .collect();
    // beta has no incoming links; alpha has one (from beta).
    assert!(orphan_ids.contains(&beta));
    assert!(orphan_ids.contains(&gamma));
    assert!(!orphan_ids.contains(&alpha));

    let dead_end_ids: Vec<String> = dead_ends(&storage)
        .unwrap()
        .into_iter()
        .map(|n| n.document_id)
        .collect();
    // alpha has no outgoing links; beta has outgoing links.
    assert!(dead_end_ids.contains(&alpha));
    assert!(!dead_end_ids.contains(&beta));
}

#[test]
fn tag_rename_updates_document_tags_and_delete_removes_them() {
    let (_vault, storage) = setup();

    let tags = list_tags(&storage).unwrap();
    let tag = tags
        .iter()
        .find(|t| t.name == "tag")
        .expect("the inline #tag should be indexed");
    assert_eq!(tag.parent, None);

    let renamed = rename_tag(&storage, &tag.id, "category/renamed").unwrap();
    assert_eq!(renamed.name, "category/renamed");
    assert_eq!(renamed.parent.as_deref(), Some("category"));

    // The association survives a rename: same tag id, still attached.
    let linked = storage
        .query(
            "SELECT count(*) AS n FROM document_tags WHERE tag_id = ?1",
            &[Value::String(tag.id.clone())],
        )
        .unwrap()
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_u64)
        .unwrap();
    assert_eq!(linked, 1);

    delete_tag(&storage, &tag.id).unwrap();
    let remaining = storage
        .query(
            "SELECT count(*) AS n FROM document_tags WHERE tag_id = ?1",
            &[Value::String(tag.id.clone())],
        )
        .unwrap()
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_u64)
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(list_tags(&storage).unwrap().iter().all(|t| t.id != tag.id));
}
