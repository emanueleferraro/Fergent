//! The agent runtime over the wire: list, create, run, and sessions.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("agents.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-agents-{}-{index}",
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
    (dir, socket, handle)
}

const DEFINITION: &str = "---\nid: helper\nname: Helper\nmodel: mock-1\n---\nHelp the user.\n";

#[tokio::test(flavor = "multi_thread")]
async fn agent_create_run_and_sessions() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- list starts empty ------------------------------------------------
    let listed = client.call("agent.list", json!({})).await.unwrap();
    assert_eq!(listed["agents"].as_array().unwrap().len(), 0);

    // --- create -----------------------------------------------------------
    let created = client
        .call("agent.create", json!({ "definition": DEFINITION }))
        .await
        .unwrap();
    assert_eq!(created["id"], json!("helper"));
    assert_eq!(created["name"], json!("Helper"));

    let listed = client.call("agent.list", json!({})).await.unwrap();
    assert_eq!(listed["agents"].as_array().unwrap().len(), 1);

    let got = client
        .call("agent.get", json!({ "agent_id": "helper" }))
        .await
        .unwrap();
    assert_eq!(got["model"], json!("mock-1"));

    // --- invalid definition is rejected -----------------------------------
    let err = client
        .call(
            "agent.create",
            json!({ "definition": "---\nid: Bad\nname: Bad\nmodel: mock-1\n---\nx\n" }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- run --------------------------------------------------------------
    let outcome = client
        .call("agent.run", json!({ "agent_id": "helper", "task": "hi" }))
        .await
        .unwrap();
    assert_eq!(outcome["stop"], json!("completed"));
    assert_eq!(outcome["final_text"], json!("echo: hi"));
    let session_id = outcome["session_id"].as_str().unwrap().to_string();

    // --- sessions ---------------------------------------------------------
    let sessions = client
        .call("agent.session.list", json!({ "limit": 10 }))
        .await
        .unwrap();
    let sessions = sessions["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["agent_id"], json!("helper"));
    assert_eq!(sessions[0]["state"], json!("done"));

    let messages = client
        .call(
            "agent.session.messages",
            json!({ "session_id": session_id }),
        )
        .await
        .unwrap();
    let messages = messages["messages"].as_array().unwrap();
    assert!(messages.iter().any(|m| m["role"] == json!("assistant")));

    // --- unknown agent ----------------------------------------------------
    let err = client
        .call("agent.run", json!({ "agent_id": "nope", "task": "x" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- delete -----------------------------------------------------------
    client
        .call("agent.delete", json!({ "agent_id": "helper" }))
        .await
        .unwrap();
    let listed = client.call("agent.list", json!({})).await.unwrap();
    assert_eq!(listed["agents"].as_array().unwrap().len(), 0);

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
