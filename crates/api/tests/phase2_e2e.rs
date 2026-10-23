//! Phase 2 end-to-end: create an agent, run it, resolve a tool permission,
//! toggle a capability, and round-trip the agent's `brain.md`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("e2e.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-e2e-{}-{index}",
            std::process::id()
        ))
    }
}

async fn start_stack() -> (tempfile::TempDir, SocketPath, ServerHandle, Arc<Kernel>) {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let kernel = Kernel::start(&KernelConfig {
        data_dir: dir.path().to_path_buf(),
        migrations_dir,
        event_capacity: 128,
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
    (dir, socket, handle, kernel)
}

const DEFINITION: &str = "---\nid: helper\nname: Helper\nmodel: mock-1\n---\nHelp the user.\n";

fn is_capability_disabled(err: fergents_api::ApiError) -> bool {
    matches!(err, fergents_api::ApiError::Rpc { code, .. } if code == ErrorCode::CapabilityDisabled.as_i32())
}

#[tokio::test(flavor = "multi_thread")]
async fn phase2_end_to_end() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let agents_dir = dir.path().join("agents");

    // --- 1. create an agent -------------------------------------------------
    let created = client
        .call("agent.create", json!({ "definition": DEFINITION }))
        .await
        .unwrap();
    assert_eq!(created["id"], json!("helper"));

    // --- 2. run it ----------------------------------------------------------
    let outcome = client
        .call("agent.run", json!({ "agent_id": "helper", "task": "hi" }))
        .await
        .unwrap();
    assert_eq!(outcome["stop"], json!("completed"));
    let session_id = outcome["session_id"].as_str().unwrap().to_string();

    // --- 3. tool permission: manual prompts, full allows ---------------------
    client
        .call(
            "permission.mode.set",
            json!({ "mode": "manual", "session_id": session_id }),
        )
        .await
        .unwrap();
    let prompted = client
        .call(
            "permission.check",
            json!({ "tool": "fs.write", "target": "inbox/note.md", "session_id": session_id }),
        )
        .await
        .unwrap();
    assert_eq!(prompted["decision"], json!("prompt"));

    client
        .call(
            "permission.mode.set",
            json!({ "mode": "full", "session_id": session_id }),
        )
        .await
        .unwrap();
    let allowed = client
        .call(
            "permission.check",
            json!({ "tool": "fs.write", "target": "inbox/note.md", "session_id": session_id }),
        )
        .await
        .unwrap();
    assert_eq!(allowed["decision"], json!("allow"));

    // --- 4. toggle a capability --------------------------------------------
    client
        .call(
            "capability.set",
            json!({ "id": "capability.chat-search", "enabled": false }),
        )
        .await
        .unwrap();
    assert!(is_capability_disabled(
        client
            .call("chat.session.search", json!({ "query": "hi" }))
            .await
            .unwrap_err()
    ));

    client
        .call(
            "capability.set",
            json!({ "id": "capability.chat-search", "enabled": true }),
        )
        .await
        .unwrap();
    let hits = client
        .call("chat.session.search", json!({ "query": "hi" }))
        .await
        .unwrap();
    assert!(hits["hits"].is_array());

    // --- 5. brain.md round-trip --------------------------------------------
    assert!(fergents_agents::read_brain(&agents_dir, "helper")
        .unwrap()
        .is_none());
    fergents_agents::write_brain(&agents_dir, "helper", "# Facts\n- Be terse.\n").unwrap();
    assert_eq!(
        fergents_agents::read_brain(&agents_dir, "helper")
            .unwrap()
            .as_deref(),
        Some("# Facts\n- Be terse.\n")
    );
    assert!(fergents_agents::brain_path(&agents_dir, "helper").ends_with("brain.md"));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
