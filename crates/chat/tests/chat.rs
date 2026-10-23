//! Chat sessions, FTS search, forks, continuation, attachments, and prompts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_chat::{Attachment, ChatService};
use fergents_events::EventBus;
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn service() -> (tempfile::TempDir, ChatService) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 256));
    let storage_dyn: Arc<dyn Storage> = storage;
    (dir, ChatService::new(storage_dyn, bus))
}

#[test]
fn sessions_can_be_created_listed_renamed_and_deleted() {
    let (_dir, chat) = service();
    let session = chat
        .create_session("assistant", Some("First"), Some("mock-1"))
        .unwrap();
    assert_eq!(session.agent_id, "assistant");
    assert_eq!(session.title.as_deref(), Some("First"));
    assert_eq!(session.state, "created");

    let second = chat.create_session("assistant", None, None).unwrap();
    assert_eq!(chat.list(10).unwrap().len(), 2);

    let renamed = chat.rename(&session.id, "Renamed").unwrap();
    assert_eq!(renamed.title.as_deref(), Some("Renamed"));

    chat.send(&session.id, "hello").unwrap();
    chat.append(&session.id, "assistant", "hi there", None)
        .unwrap();
    let messages = chat.messages(&session.id).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].seq, 1);
    assert_eq!(messages[1].seq, 2);

    chat.delete(&session.id).unwrap();
    assert!(chat.get(&session.id).unwrap().is_none());
    assert!(chat.messages(&session.id).unwrap().is_empty());
    assert_eq!(chat.list(10).unwrap().len(), 1);
    assert!(chat.delete(&session.id).is_err());
    assert!(chat.rename(&second.id, "").is_ok());
    assert!(chat.send(&second.id, "   ").is_err());
}

#[test]
fn full_text_search_finds_messages() {
    let (_dir, chat) = service();
    let session = chat.create_session("assistant", None, None).unwrap();
    chat.send(&session.id, "How does the MCP host work?")
        .unwrap();
    chat.append(&session.id, "assistant", "It speaks JSON-RPC.", None)
        .unwrap();

    let hits = chat.search("JSON-RPC", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, session.id);
    assert_eq!(hits[0].role, "assistant");

    // Quotes in the query cannot break FTS syntax.
    assert!(chat.search("\"unbalanced", 10).unwrap().is_empty());
    assert!(chat.search("   ", 10).unwrap().is_empty());
}

#[test]
fn fork_copies_history_into_a_child_session() {
    let (_dir, chat) = service();
    let parent = chat
        .create_session("assistant", Some("Parent"), None)
        .unwrap();
    chat.send(&parent.id, "one").unwrap();
    chat.append(&parent.id, "assistant", "two", None).unwrap();

    let child = chat.fork(&parent.id, None).unwrap();
    assert_eq!(child.parent_session_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(child.title.as_deref(), Some("Parent (fork)"));

    let copied = chat.messages(&child.id).unwrap();
    assert_eq!(copied.len(), 2);
    assert_eq!(copied[0].content, "one");
    assert_eq!(copied[1].content, "two");
    assert_ne!(copied[0].id, chat.messages(&parent.id).unwrap()[0].id);

    // Appending to the fork does not touch the parent.
    chat.send(&child.id, "three").unwrap();
    assert_eq!(chat.messages(&parent.id).unwrap().len(), 2);
}

#[test]
fn continue_is_capped_and_regenerate_drops_the_last_answer() {
    let (_dir, chat) = service();
    let session = chat.create_session("assistant", None, None).unwrap();
    chat.send(&session.id, "question").unwrap();
    chat.append(&session.id, "assistant", "answer", None)
        .unwrap();

    let turns = chat.auto_continue(&session.id, 100).unwrap();
    assert_eq!(turns, chat.max_auto_continues());
    assert_eq!(
        chat.messages(&session.id).unwrap().len(),
        2 + turns as usize
    );

    let prompt = chat.regenerate(&session.id).unwrap().unwrap();
    assert_eq!(prompt.role, "user");
    assert!(chat
        .messages(&session.id)
        .unwrap()
        .iter()
        .all(|message| message.role != "assistant"));
}

#[test]
fn attachments_degrade_gracefully_and_prompts_round_trip() {
    let (_dir, chat) = service();
    let session = chat.create_session("assistant", None, None).unwrap();

    let text = chat
        .attach(
            &session.id,
            Attachment::new("notes.md", "text/markdown", 12),
        )
        .unwrap();
    assert_eq!(text.routed, "inline");

    let image = chat
        .attach(&session.id, Attachment::new("chart.png", "image/png", 100))
        .unwrap();
    assert_eq!(image.routed, "vision");
    assert!(image.note.is_some());

    let pdf = chat
        .attach(
            &session.id,
            Attachment::new("paper.pdf", "application/pdf", 900),
        )
        .unwrap();
    assert_eq!(pdf.routed, "metadata_only");

    let other = chat
        .attach(
            &session.id,
            Attachment::new("data.bin", "application/octet-stream", 5),
        )
        .unwrap();
    assert_eq!(other.routed, "metadata_only");
    assert_eq!(chat.messages(&session.id).unwrap().len(), 4);

    let prompt = chat
        .create_prompt(
            "Summarize",
            "Summarize this: {{input}}",
            &["writing".to_string()],
        )
        .unwrap();
    let listed = chat.list_prompts().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].tags, vec!["writing".to_string()]);
    assert!(chat.delete_prompt(&prompt.id).unwrap());
    assert!(!chat.delete_prompt(&prompt.id).unwrap());
    assert!(chat.create_prompt("", "body", &[]).is_err());
}
