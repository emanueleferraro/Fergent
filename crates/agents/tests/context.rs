//! The context builder: token accounting, toggling, and snapshot persistence.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_agents::{ContextBuilder, ContextSettings, SectionInput, SectionKind, DEFAULT_BUDGET};
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn builder() -> (tempfile::TempDir, ContextBuilder) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let storage_dyn: Arc<dyn Storage> = storage;
    (dir, ContextBuilder::new(storage_dyn))
}

#[test]
fn counts_tokens_and_sums_enabled_sections() {
    let (_dir, builder) = builder();
    let settings = ContextSettings::default();
    let parts = vec![
        SectionInput::new(SectionKind::System, "You are a tester."),
        SectionInput::new(SectionKind::Input, "hello world, this is a question"),
    ];

    let snapshot = builder.build("s1", 0, parts, &settings);
    let system = snapshot.section("system").unwrap();
    let input = snapshot.section("input").unwrap();
    assert!(system.tokens > 0);
    assert!(input.tokens > 0);
    assert_eq!(snapshot.total_tokens, system.tokens + input.tokens);
    assert_eq!(snapshot.budget, DEFAULT_BUDGET);
    assert!(snapshot.within_budget());
    // Sections with no input are absent.
    assert!(snapshot.section("history").is_none());
}

#[test]
fn toggling_drops_a_section_from_the_payload() {
    let (_dir, builder) = builder();
    builder.set_section("rag", false).unwrap();
    let settings = builder.settings().unwrap();
    assert!(!settings.is_enabled("rag"));

    let parts = vec![
        SectionInput::new(SectionKind::Rag, "retrieved chunk text"),
        SectionInput::new(SectionKind::Input, "hi there"),
    ];
    let snapshot = builder.build("s1", 0, parts, &settings);

    let rag = snapshot.section("rag").unwrap();
    assert!(!rag.enabled);
    assert_eq!(rag.tokens, 0);
    assert!(!snapshot.payload().contains("retrieved chunk"));
    assert!(snapshot.payload().contains("hi there"));
    assert_eq!(
        snapshot.total_tokens,
        snapshot.section("input").unwrap().tokens
    );

    // Unknown sections are rejected.
    assert!(builder.set_section("nope", false).is_err());
}

#[test]
fn persists_and_reloads_settings_and_snapshots() {
    let (_dir, builder) = builder();
    builder.set_budget(4096).unwrap();
    let settings = builder.settings().unwrap();
    assert_eq!(settings.budget, 4096);

    let snapshot = builder.build(
        "s1",
        0,
        vec![SectionInput::new(SectionKind::Input, "xyz")],
        &settings,
    );
    assert_eq!(snapshot.budget, 4096);
    builder.persist(&snapshot).unwrap();

    let loaded = builder.snapshots("s1").unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, snapshot.id);
    assert_eq!(loaded[0].total_tokens, snapshot.total_tokens);
    assert_eq!(loaded[0].section("input").unwrap().content, "xyz");
}
