//! Integration tests for the brain lifecycle and multi-brain operations
//! (`BRAIN.md` §7–§9, §14; tasks.md Task 14.9).
//!
//! Indexing runs through the real `VaultIndexer` over stub embedders, so each
//! brain gets its own `index.sqlite` and duplicate independence is verified for
//! real.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_brain::{
    BrainIndexer, BrainManager, BrainRole, BrainTemplate, CreateBrainSpec, VaultIndexer,
    VaultLayout,
};
use fergents_index::{RecordingSink, SqliteVecStore, StubEmbedder, DEFAULT_EMBED_DIM};
use fergents_storage::{load_migrations_from, register_sqlite_vec, SqliteStorage, Storage, Value};

fn migrations_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn brain_migrations_dir() -> PathBuf {
    migrations_root().join("brain")
}

struct Harness {
    manager: BrainManager,
    sink: Arc<RecordingSink>,
    indexer: Box<dyn BrainIndexer>,
    dir: tempfile::TempDir,
}

fn harness() -> Harness {
    register_sqlite_vec();
    let global = SqliteStorage::open_in_memory(&migrations_root()).unwrap();
    let global: Arc<dyn Storage> = Arc::new(global);
    let sink = Arc::new(RecordingSink::new());
    let dir = tempfile::tempdir().unwrap();
    let manager = BrainManager::new(
        global,
        sink.clone(),
        brain_migrations_dir(),
        dir.path().join("templates/brains"),
    );
    let indexer: Box<dyn BrainIndexer> = Box::new(VaultIndexer::new(
        brain_migrations_dir(),
        sink.clone(),
        DEFAULT_EMBED_DIM,
        StubEmbedder::default,
        |storage| SqliteVecStore::new(storage, DEFAULT_EMBED_DIM),
    ));
    Harness {
        manager,
        sink,
        indexer,
        dir,
    }
}

impl Harness {
    fn brain_path(&self, name: &str) -> PathBuf {
        self.dir.path().join("brains").join(name)
    }

    fn create(&self, name: &str, template: BrainTemplate) -> fergents_brain::BrainRecord {
        let spec = CreateBrainSpec::new(name, self.brain_path(name)).with_template(template);
        self.manager.create(self.indexer.as_ref(), &spec).unwrap()
    }
}

/// The number of documents in a brain's own `index.sqlite`.
fn doc_count(brain_dir: &Path) -> i64 {
    let migrations = load_migrations_from(&brain_migrations_dir()).unwrap();
    let storage =
        SqliteStorage::open_with_migrations(&brain_dir.join("index.sqlite"), &migrations).unwrap();
    storage
        .query("SELECT count(*) AS n FROM documents", &[])
        .unwrap()
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_i64)
        .unwrap_or(-1)
}

#[test]
fn list_is_empty_on_a_fresh_install() {
    let h = harness();
    assert!(h.manager.list(false).unwrap().is_empty());
    assert!(h.manager.list(true).unwrap().is_empty());
}

#[test]
fn create_writes_the_folder_registers_and_runs_the_first_index() {
    let h = harness();
    let record = h.create("Personal", BrainTemplate::Personal);

    let dir = h.brain_path("Personal");
    assert!(dir.join("brain.toml").is_file());
    assert!(dir.join("vault/notes/index.md").is_file());
    assert!(dir.join("vault/templates/daily.md").is_file());

    assert_eq!(record.name, "Personal");
    assert_eq!(record.schema_version, 2);
    assert_eq!(h.manager.list(false).unwrap().len(), 1);
    assert_eq!(h.sink.count("brain.created"), 1);

    // The first index ran and the seeded notes are in the brain's own store.
    assert!(doc_count(&dir) >= 2);
}

#[test]
fn open_and_close_track_state_and_emit_events() {
    let h = harness();
    let record = h.create("Research", BrainTemplate::Research);
    assert!(!h.manager.is_open(&record.brain_id));

    let opened = h
        .manager
        .open(h.indexer.as_ref(), &record.brain_id)
        .unwrap();
    assert!(h.manager.is_open(&record.brain_id));
    assert!(opened.last_opened_at.is_some());
    assert_eq!(h.sink.count("brain.opened"), 1);

    h.manager
        .close(h.indexer.as_ref(), &record.brain_id)
        .unwrap();
    assert!(!h.manager.is_open(&record.brain_id));
    assert_eq!(h.sink.count("brain.closed"), 1);
}

#[test]
fn archive_hides_from_the_active_list_but_keeps_files() {
    let h = harness();
    let record = h.create("Coding", BrainTemplate::Coding);

    h.manager.archive(&record.brain_id).unwrap();
    assert!(h.manager.list(false).unwrap().is_empty());
    assert_eq!(h.manager.list(true).unwrap().len(), 1);
    assert!(h.brain_path("Coding").join("brain.toml").is_file());
    assert_eq!(h.sink.count("brain.archived"), 1);

    h.manager.unarchive(&record.brain_id).unwrap();
    assert_eq!(h.manager.list(false).unwrap().len(), 1);
}

#[test]
fn update_rewrites_brain_toml_and_the_registry() {
    let h = harness();
    let record = h.create("Team", BrainTemplate::Team);
    let patch = fergents_brain::BrainPatch {
        name: Some("Team Brain".into()),
        icon: Some("star".into()),
        ..Default::default()
    };
    let updated = h.manager.update(&record.brain_id, &patch).unwrap();
    assert_eq!(updated.name, "Team Brain");
    assert_eq!(h.manager.get(&record.brain_id).unwrap().name, "Team Brain");
    let config = fergents_brain::BrainConfig::load(&h.brain_path("Team")).unwrap();
    assert_eq!(config.brain.icon.as_deref(), Some("star"));
    assert_eq!(h.sink.count("brain.updated"), 1);
}

#[test]
fn delete_requires_the_exact_name_and_removes_files() {
    let h = harness();
    let record = h.create("Scratch", BrainTemplate::Blank);

    let err = h
        .manager
        .delete(h.indexer.as_ref(), &record.brain_id, "wrong", None)
        .unwrap_err();
    assert!(err.to_string().contains("does not match"));
    assert!(h.brain_path("Scratch").is_dir());

    h.manager
        .delete(h.indexer.as_ref(), &record.brain_id, "Scratch", None)
        .unwrap();
    assert!(h.manager.get(&record.brain_id).is_err());
    assert!(!h.brain_path("Scratch").exists());
    assert_eq!(h.sink.count("brain.deleted"), 1);
}

#[test]
fn duplicate_produces_an_independent_brain() {
    let h = harness();
    let source = h.create("Personal", BrainTemplate::Personal);
    let source_dir = h.brain_path("Personal");
    let target = h.brain_path("Personal Copy");

    let copy = h
        .manager
        .duplicate(
            h.indexer.as_ref(),
            &source.brain_id,
            &target,
            Some("Personal 2"),
        )
        .unwrap();

    // New id, new folder, its own index.
    assert_ne!(copy.brain_id, source.brain_id);
    assert_eq!(copy.name, "Personal 2");
    assert!(target.join("brain.toml").is_file());
    assert!(target.join("index.sqlite").is_file());
    let copy_config = fergents_brain::BrainConfig::load(&target).unwrap();
    assert_eq!(copy_config.brain.id, copy.brain_id);

    // Independent: writing to the source does not change the copy's index.
    let before = doc_count(&target);
    h.manager
        .write_note(
            h.indexer.as_ref(),
            None,
            &source.brain_id,
            "notes/only-source.md",
            "# Only source\n",
        )
        .unwrap();
    assert!(source_dir.join("vault/notes/only-source.md").is_file());
    assert!(!target.join("vault/notes/only-source.md").exists());
    assert_eq!(doc_count(&target), before);
}

#[test]
fn move_keeps_the_brain_id() {
    let h = harness();
    let record = h.create("Research", BrainTemplate::Research);
    let target = h.dir.path().join("moved/research");

    let moved = h.manager.mv(&record.brain_id, &target).unwrap();
    assert_eq!(moved.brain_id, record.brain_id);
    assert_eq!(moved.path, target);
    assert!(target.join("brain.toml").is_file());
    assert!(!h.brain_path("Research").exists());
    assert_eq!(h.manager.get(&record.brain_id).unwrap().path, target);
}

#[test]
fn export_excludes_regenerable_files_and_import_registers() {
    let h1 = harness();
    let record = h1.create("Personal", BrainTemplate::Personal);
    let archive = h1.dir.path().join("export/personal.fergbrain");

    let manifest = h1.manager.export(&record.brain_id, &archive).unwrap();
    let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"brain.toml"));
    assert!(!paths.iter().any(|p| p.ends_with("index.sqlite")));
    assert!(!paths.iter().any(|p| p.starts_with(".fergents/")));
    assert_eq!(h1.sink.count("brain.exported"), 1);

    // Import into a fresh install (models importing on another machine).
    let h2 = harness();
    let target = h2.dir.path().join("imported/personal");
    let imported = h2
        .manager
        .import(h2.indexer.as_ref(), &archive, &target)
        .unwrap();

    assert_eq!(imported.brain_id, record.brain_id);
    assert!(target.join("brain.toml").is_file());
    assert!(target.join("vault/notes/index.md").is_file());
    assert_eq!(h2.sink.count("brain.imported"), 1);
    assert_eq!(h2.manager.list(false).unwrap().len(), 1);
}

#[test]
fn read_only_role_rejects_writes() {
    let h = harness();
    let record = h.create("Research", BrainTemplate::Blank);
    let project = "project-1";

    h.manager
        .attach(project, &record.brain_id, BrainRole::ReadOnly)
        .unwrap();
    assert_eq!(
        h.manager.role_for(project, &record.brain_id).unwrap(),
        Some(BrainRole::ReadOnly)
    );
    let err = h
        .manager
        .write_note(
            h.indexer.as_ref(),
            Some(project),
            &record.brain_id,
            "notes/x.md",
            "hi",
        )
        .unwrap_err();
    assert!(err.to_string().contains("read-only"));
    assert!(!h.brain_path("Research").join("vault/notes/x.md").exists());

    // Re-attaching as primary allows the write.
    h.manager
        .attach(project, &record.brain_id, BrainRole::Primary)
        .unwrap();
    h.manager
        .write_note(
            h.indexer.as_ref(),
            Some(project),
            &record.brain_id,
            "notes/x.md",
            "hi",
        )
        .unwrap();
    assert!(h.brain_path("Research").join("vault/notes/x.md").is_file());
}

#[test]
fn detach_clears_the_role() {
    let h = harness();
    let record = h.create("Team", BrainTemplate::Blank);
    h.manager
        .attach("p", &record.brain_id, BrainRole::Scratch)
        .unwrap();
    h.manager.detach("p", &record.brain_id).unwrap();
    assert_eq!(h.manager.role_for("p", &record.brain_id).unwrap(), None);
}

#[test]
fn reindex_emits_brain_indexed() {
    let h = harness();
    let record = h.create("Coding", BrainTemplate::Coding);
    // Add a note directly on disk so the reindex has something new to process.
    std::fs::write(h.brain_path("Coding").join("vault/notes/new.md"), "# New\n").unwrap();
    let report = h
        .manager
        .reindex(h.indexer.as_ref(), &record.brain_id)
        .unwrap();
    assert!(report.documents > 0);
    assert_eq!(h.sink.count("brain.indexed"), 1);
}

#[test]
fn save_template_copies_the_brain_without_regenerable_files() {
    let h = harness();
    let record = h.create("Personal", BrainTemplate::Personal);
    let template_dir = h
        .manager
        .save_template(&record.brain_id, "my-personal")
        .unwrap();
    assert!(template_dir.join("brain.toml").is_file());
    assert!(template_dir.join("vault/notes/index.md").is_file());
    assert!(!template_dir.join("index.sqlite").exists());

    assert!(h
        .manager
        .save_template(&record.brain_id, "my-personal")
        .is_err());
}

/// `VaultLayout` is used by the manager internally; assert it resolves the
/// brain's expected folders so the layout contract stays covered.
#[test]
fn vault_layout_matches_the_created_brain() {
    let h = harness();
    let record = h.create("Blank", BrainTemplate::Blank);
    let config = fergents_brain::BrainConfig::load(&record.path).unwrap();
    let layout = VaultLayout::from_config(record.path.clone(), &config);
    assert_eq!(layout.vault_root, record.path.join("vault"));
    assert!(layout.daily_dir.is_dir());
}
