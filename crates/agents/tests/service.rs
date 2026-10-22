//! The agent service: file-backed definitions and persisted sessions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_agents::{
    AgentService, AllowAllGate, RecordingObserver, RunState, StopReason, ToolRegistry,
};
use fergents_events::EventBus;
use fergents_llm::MockProvider;
use fergents_storage::SqliteStorage;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

#[tokio::test]
async fn create_run_persist_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteStorage::open(&dir.path().join("global.sqlite"), &migrations_dir()).unwrap(),
    );
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    let storage_dyn: Arc<dyn fergents_storage::Storage> = storage.clone();
    let service = AgentService::new(
        dir.path().join("agents"),
        storage_dyn,
        Arc::new(MockProvider::new("mock")),
        Arc::new(ToolRegistry::new()),
        Arc::new(AllowAllGate),
        bus.clone(),
    );

    let definition = service
        .create("---\nid: helper\nname: Helper\nmodel: mock-1\n---\nHelp the user.\n")
        .unwrap();
    assert_eq!(definition.id, "helper");
    assert!(service.dir().join("helper.md").exists());
    assert_eq!(service.list().len(), 1);

    // An invalid definition writes nothing.
    assert!(service
        .create("---\nid: Bad\nname: Bad\nmodel: mock-1\n---\nx\n")
        .is_err());
    assert_eq!(service.list().len(), 1);

    let observer = RecordingObserver::new();
    let outcome = service.run("helper", "hi", &observer, None).await.unwrap();
    assert_eq!(outcome.stop, StopReason::Completed);
    assert_eq!(outcome.final_text, "echo: hi");

    let sessions = service.sessions(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].agent_id, "helper");
    assert_eq!(sessions[0].state, RunState::Done);

    let messages = service.session_messages(&outcome.session_id).unwrap();
    assert!(messages.iter().any(|message| message.role == "assistant"));
    assert!(messages.iter().any(|message| message.role == "user"));

    let events = bus.replay(&[], None, Some(100)).unwrap();
    assert!(events.iter().any(|event| event.kind == "agent.created"));
    assert!(events
        .iter()
        .any(|event| event.kind == "agent.run.finished"));

    assert!(service.delete("helper").is_ok());
    assert!(service.get("helper").is_none());
    assert!(!service.dir().join("helper.md").exists());
    assert!(service.delete("helper").is_err());
}
