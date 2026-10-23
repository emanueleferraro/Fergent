//! The chat backend over the wire: sessions, search, fork, continue, prompts.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("chat.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-chat-{}-{index}",
            std::process::id()
        ))
    }
}

async fn start_stack() -> (tempfile::TempDir, SocketPath, ServerHandle) {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let kernel = Kernel::start(&KernelConfig {
        data_dir: dir.path().to_path_buf(),
        migrations_dir,
        event_capacity: 256,
    })
    .unwrap();
    let socket = test_socket(dir.path());
    let (tx, rx) = watch::channel(false);
    let handle = ServerHandle::spawn(kernel.clone(), socket.clone(), tx, rx);
    for _ in 0..100 {
        if connect_stream(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (dir, socket, handle)
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_round_trip() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    client
        .call(
            "agent.create",
            json!({
                "definition": "---\nid: assistant\nname: Assistant\nmodel: mock-1\ntools: []\n---\n\nYou are a helpful assistant.\n"
            }),
        )
        .await
        .unwrap();

    let session = client
        .call(
            "chat.session.create",
            json!({ "agent_id": "assistant", "title": "New chat", "model": "mock-1" }),
        )
        .await
        .unwrap();
    let session_id = session["id"].as_str().unwrap().to_string();
    assert_eq!(session["agent_id"], json!("assistant"));

    let listed = client.call("chat.session.list", json!({})).await.unwrap();
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 1);

    // Sending appends the user turn and the agent's reply to the transcript.
    let first = client
        .call(
            "chat.message.send",
            json!({ "session_id": session_id, "text": "Hello MCP" }),
        )
        .await
        .unwrap();
    assert_eq!(first["messages"].as_array().unwrap().len(), 2);
    assert_eq!(first["messages"][0]["role"], json!("user"));
    assert_eq!(first["messages"][1]["role"], json!("assistant"));

    let second = client
        .call(
            "chat.message.send",
            json!({ "session_id": session_id, "text": "and JSON-RPC" }),
        )
        .await
        .unwrap();
    assert_eq!(second["messages"].as_array().unwrap().len(), 4);

    let messages = client
        .call("chat.message.list", json!({ "session_id": session_id }))
        .await
        .unwrap();
    assert_eq!(messages["messages"].as_array().unwrap().len(), 4);

    // Retry re-answers the last user turn without duplicating it.
    let retried = client
        .call("chat.message.retry", json!({ "session_id": session_id }))
        .await
        .unwrap();
    let retried_messages = retried["messages"].as_array().unwrap();
    assert_eq!(retried_messages.len(), 4);
    assert_eq!(retried_messages.last().unwrap()["role"], json!("assistant"));

    // FTS search.
    let hits = client
        .call("chat.session.search", json!({ "query": "MCP" }))
        .await
        .unwrap();
    assert!(!hits["hits"].as_array().unwrap().is_empty());

    // Fork copies history.
    let fork = client
        .call("chat.message.fork", json!({ "session_id": session_id }))
        .await
        .unwrap();
    let fork_id = fork["id"].as_str().unwrap().to_string();
    assert_eq!(fork["parent_session_id"], json!(session_id));
    let forked = client
        .call("chat.message.list", json!({ "session_id": fork_id }))
        .await
        .unwrap();
    assert_eq!(forked["messages"].as_array().unwrap().len(), 4);

    // Continue is capped.
    let turns = client
        .call(
            "chat.continue",
            json!({ "session_id": fork_id, "requested": 100 }),
        )
        .await
        .unwrap();
    assert_eq!(turns["turns"], json!(3));

    // Regenerate drops the last assistant answer and returns the prompt.
    client
        .call(
            "chat.message.send",
            json!({ "session_id": session_id, "text": "again" }),
        )
        .await
        .unwrap();

    // Attachments degrade gracefully.
    let routing = client
        .call(
            "chat.attach",
            json!({ "session_id": session_id, "name": "a.pdf", "media_type": "application/pdf", "size": 10 }),
        )
        .await
        .unwrap();
    assert_eq!(routing["routed"], json!("metadata_only"));

    // Prompt library.
    let prompt = client
        .call(
            "prompt.create",
            json!({ "title": "T", "body": "B", "tags": ["x"] }),
        )
        .await
        .unwrap();
    let prompts = client.call("prompt.list", json!({})).await.unwrap();
    assert_eq!(prompts["prompts"].as_array().unwrap().len(), 1);
    client
        .call("prompt.delete", json!({ "id": prompt["id"] }))
        .await
        .unwrap();
    assert!(
        client.call("prompt.list", json!({})).await.unwrap()["prompts"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Model pinning.
    let pinned = client
        .call(
            "chat.session.model",
            json!({ "session_id": session_id, "model": "mock:mock-model" }),
        )
        .await
        .unwrap();
    assert_eq!(pinned["model"], json!("mock:mock-model"));
    let cleared = client
        .call(
            "chat.session.model",
            json!({ "session_id": session_id, "model": null }),
        )
        .await
        .unwrap();
    assert!(cleared["model"].is_null());

    // Rename + delete.
    let renamed = client
        .call(
            "chat.session.rename",
            json!({ "session_id": session_id, "title": "Renamed" }),
        )
        .await
        .unwrap();
    assert_eq!(renamed["title"], json!("Renamed"));
    client
        .call("chat.session.delete", json!({ "session_id": session_id }))
        .await
        .unwrap();
    assert!(client
        .call("chat.session.get", json!({ "session_id": session_id }))
        .await
        .is_err());

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
