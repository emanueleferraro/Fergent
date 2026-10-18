//! Integration tests for daily notes and templates (`BRAIN.md` §4.4–§4.5;
//! tasks.md Task 13.3).
//!
//! The daily note is opened through a real `Indexer`, so these tests also prove
//! that `daily.open` hooks into the existing pipeline (Task 8).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use fergents_index::{
    create_template, list_templates, render, DailyNotes, EmbeddingCache, EmbeddingPipeline,
    Indexer, IndexerConfig, RecordingSink, SqliteVecStore, StubEmbedder, TemplateVariables,
    DEFAULT_EMBED_DIM,
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

fn build_indexer(vault: &Path) -> (TestIndexer, Arc<dyn Storage>) {
    let storage = memory_storage();
    let sink = Arc::new(RecordingSink::new());
    let pipeline = EmbeddingPipeline::new(
        StubEmbedder::default(),
        SqliteVecStore::new(storage.clone(), DEFAULT_EMBED_DIM),
        EmbeddingCache::new(storage.clone()),
    );
    let indexer = Indexer::new(
        storage.clone(),
        IndexerConfig::new("brain-1", vault),
        pipeline,
        sink,
    )
    .unwrap();
    (indexer, storage)
}

fn at(year: i32, month: u32, day: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(9, 30, 0)
        .unwrap()
}

fn scalar(storage: &Arc<dyn Storage>, sql: &str) -> Option<Value> {
    storage
        .query(sql, &[])
        .unwrap()
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next().map(|(_, value)| value))
}

#[test]
fn daily_note_is_created_once_and_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let daily = vault.join("daily");
    let templates = vault.join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    create_template(&templates, "daily", "## Journal\n\n{{cursor}}").unwrap();

    let (indexer, storage) = build_indexer(&vault);
    let notes = DailyNotes::new(&vault, &daily, &templates);

    let first = notes.open(&indexer, at(2025, 10, 18)).unwrap();
    assert!(first.created);
    assert_eq!(first.rel_path, "daily/2025-10-18.md");
    assert_eq!(first.cursor, Some(12));
    assert_eq!(first.content, "## Journal\n\n");
    assert!(first.path.is_file());

    // The note was indexed by the pipeline.
    assert_eq!(
        scalar(
            &storage,
            "SELECT count(*) AS n FROM documents WHERE path = 'daily/2025-10-18.md'",
        ),
        Some(Value::from(1))
    );

    // Opening again the same day does not recreate, rewrite, or duplicate.
    let second = notes.open(&indexer, at(2025, 10, 18)).unwrap();
    assert!(!second.created);
    assert_eq!(second.content, first.content);
    assert_eq!(
        scalar(&storage, "SELECT count(*) AS n FROM documents"),
        Some(Value::from(1))
    );

    // A different day gets its own note.
    let other = notes.open(&indexer, at(2025, 10, 19)).unwrap();
    assert!(other.created);
    assert_eq!(other.rel_path, "daily/2025-10-19.md");
    assert_eq!(
        scalar(&storage, "SELECT count(*) AS n FROM documents"),
        Some(Value::from(2))
    );
}

#[test]
fn daily_note_without_a_template_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let daily = vault.join("daily");
    let templates = vault.join("templates");

    let (indexer, _storage) = build_indexer(&vault);
    let notes = DailyNotes::new(&vault, &daily, &templates);

    let note = notes.open(&indexer, at(2025, 10, 18)).unwrap();
    assert!(note.created);
    assert!(note.content.is_empty());
    assert_eq!(note.cursor, None);
}

#[test]
fn template_variables_are_substituted() {
    let template =
        "# {{title}}\n\nDate: {{date}} at {{time}} ({{datetime}})\n\n> {{selection}}\n> {{clipboard}}\n";
    let vars = TemplateVariables {
        date: "2025-10-18".into(),
        time: "09:30".into(),
        datetime: "2025-10-18 09:30".into(),
        title: "Standup".into(),
        selection: "chosen".into(),
        clipboard: "pasted".into(),
    };
    let rendered = render(template, &vars);
    assert_eq!(
        rendered.content,
        "# Standup\n\nDate: 2025-10-18 at 09:30 (2025-10-18 09:30)\n\n> chosen\n> pasted\n"
    );
    assert_eq!(rendered.cursor, None);
}

#[test]
fn cursor_position_is_returned() {
    let rendered = render("Hello {{cursor}}world", &TemplateVariables::default());
    assert_eq!(rendered.content, "Hello world");
    assert_eq!(rendered.cursor, Some(6));
}

#[test]
fn template_create_and_list() {
    let dir = tempfile::tempdir().unwrap();
    create_template(dir.path(), "daily", "{{date}}").unwrap();
    create_template(dir.path(), "meeting", "{{title}}").unwrap();

    let names: Vec<_> = list_templates(dir.path())
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["daily", "meeting"]);
    assert!(dir.path().join("daily.md").is_file());
}
