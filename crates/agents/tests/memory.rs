//! Agent memory: scopes, decay, consolidation, agent files, and compaction.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_agents::{
    memory_tools, AgentService, AllowAllGate, MemoryKind, MemoryScope, MemoryStore,
    RecordingObserver, ToolRegistry,
};
use fergents_events::EventBus;
use fergents_llm::MockProvider;
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn store() -> (tempfile::TempDir, Arc<MemoryStore>) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let storage_dyn: Arc<dyn Storage> = storage;
    (dir, Arc::new(MemoryStore::new(storage_dyn)))
}

fn agent(name: &str) -> MemoryScope {
    MemoryScope::Agent(name.to_string())
}

#[test]
fn remember_recall_list_and_forget() {
    let (_dir, memory) = store();
    let a = agent("a");

    let fact = memory
        .remember(
            &a,
            MemoryKind::Fact,
            "The user prefers concise answers",
            0.9,
        )
        .unwrap();
    memory
        .remember(
            &MemoryScope::Global,
            MemoryKind::Fact,
            "Shared kernel note",
            0.4,
        )
        .unwrap();
    memory
        .remember(&agent("b"), MemoryKind::Fact, "Another agent's secret", 0.9)
        .unwrap();

    let recalled = memory.recall(&a, "concise", 5).unwrap();
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].id, fact.id);

    // A query matching the global entry is visible from agent scope.
    let shared = memory.recall(&a, "Shared", 5).unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].content, "Shared kernel note");

    // Scope isolation: agent `b`'s entries are not visible from `a`.
    assert!(memory.recall(&a, "secret", 5).unwrap().is_empty());

    // Recall reinforces importance (capped at 1.0).
    let bumped = memory.list(&a, Some(MemoryKind::Fact)).unwrap();
    assert!(bumped[0].importance >= 0.9);
    assert!(bumped[0].last_used_at.is_some() || bumped[0].importance <= 1.0);

    assert!(memory.forget(&fact.id).unwrap());
    assert!(memory.list(&a, None).unwrap().is_empty());
    assert!(!memory.forget(&fact.id).unwrap());
}

#[test]
fn decay_removes_low_importance_and_consolidate_merges_duplicates() {
    let (_dir, memory) = store();
    let a = agent("a");
    memory
        .remember(&a, MemoryKind::Fact, "low value", 0.2)
        .unwrap();
    memory
        .remember(&a, MemoryKind::Fact, "keep me", 0.6)
        .unwrap();

    let removed = memory.decay(30.0, 0.5).unwrap();
    assert_eq!(removed, 1);
    let remaining = memory.list(&a, None).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].content, "keep me");

    memory
        .remember(&a, MemoryKind::Fact, "duplicate", 0.3)
        .unwrap();
    memory
        .remember(&a, MemoryKind::Fact, "duplicate", 0.8)
        .unwrap();
    let merged = memory.consolidate(&a).unwrap();
    assert_eq!(merged, 1);
    let after = memory.list(&a, None).unwrap();
    let dup = after.iter().find(|e| e.content == "duplicate").unwrap();
    assert!((dup.importance - 0.8).abs() < f32::EPSILON);
}

#[tokio::test]
async fn compaction_writes_session_md_and_summary() {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    let storage_dyn: Arc<dyn Storage> = storage.clone();
    let memory = Arc::new(MemoryStore::new(storage_dyn.clone()));
    let tools = Arc::new(ToolRegistry::new());
    for tool in memory_tools(memory) {
        tools.register(tool);
    }
    let agents_dir = dir.path().join("agents");
    let service = AgentService::new(
        agents_dir.clone(),
        storage_dyn,
        Arc::new(MockProvider::new("mock")),
        tools,
        Arc::new(AllowAllGate),
        bus,
    );

    service
        .create("---\nid: helper\nname: Helper\nmodel: mock-1\n---\nHelp.\n")
        .unwrap();
    let outcome = service
        .run(
            "helper",
            "a fairly long question about things",
            &RecordingObserver::new(),
            None,
        )
        .await
        .unwrap();

    // Below the threshold: nothing compacted.
    assert!(service
        .compact_session("helper", &outcome.session_id, u64::MAX)
        .await
        .unwrap()
        .is_none());

    // Above the threshold: a summary is written to session.md and the row.
    let summary = service
        .compact_session("helper", &outcome.session_id, 1)
        .await
        .unwrap()
        .expect("compacted");
    assert!(!summary.is_empty());

    let session_md = agents_dir.join("helper").join("session.md");
    assert!(session_md.exists());
    let stored = service.session(&outcome.session_id).unwrap().unwrap();
    assert_eq!(stored.summary.as_deref(), Some(summary.as_str()));

    // brain.md round-trips.
    assert!(fergents_agents::read_brain(&agents_dir, "helper")
        .unwrap()
        .is_none());
    fergents_agents::write_brain(&agents_dir, "helper", "Remember: be terse.\n").unwrap();
    assert_eq!(
        fergents_agents::read_brain(&agents_dir, "helper")
            .unwrap()
            .as_deref(),
        Some("Remember: be terse.\n")
    );
    assert!(fergents_agents::brain_path(&agents_dir, "helper").ends_with("brain.md"));
}
