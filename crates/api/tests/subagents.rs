//! Sub-agents over the wire: spawn, collect, cancel, message, and handoff.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use fergents_secrets::MemoryKeychain;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("subagents.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-subagents-{}-{index}",
            std::process::id()
        ))
    }
}

async fn start_stack() -> (tempfile::TempDir, SocketPath, ServerHandle) {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let kernel = Kernel::start_with_keychain(
        &KernelConfig {
            data_dir: dir.path().to_path_buf(),
            migrations_dir,
            event_capacity: 256,
        },
        Arc::new(MemoryKeychain::new()),
    )
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

const PARENT: &str = "---\nid: parent\nname: Parent\nmodel: mock-1\nsub_agents:\n  allow: true\nlimits:\n  max_sub_agents: 2\n---\nCoordinate.\n";
const WORKER: &str = "---\nid: worker\nname: Worker\nmodel: mock-1\n---\nWork.\n";

async fn enable(client: &KernelClient) {
    for id in ["capability.sub-agents", "capability.agent-sync"] {
        client
            .call(
                "capability.set",
                json!({ "id": id, "enabled": true, "auto_enable_deps": true }),
            )
            .await
            .unwrap();
    }
    client
        .call("agent.create", json!({ "definition": PARENT }))
        .await
        .unwrap();
    client
        .call("agent.create", json!({ "definition": WORKER }))
        .await
        .unwrap();
}

async fn parent_session(client: &KernelClient) -> String {
    let run = client
        .call("agent.run", json!({ "id": "parent", "task": "plan" }))
        .await
        .unwrap();
    run["session_id"].as_str().unwrap().to_string()
}

async fn wait_done(client: &KernelClient, session_id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let child = client
            .call("agent.collect", json!({ "session_id": session_id }))
            .await
            .unwrap()["child"]
            .clone();
        if child["state"] != json!("running") {
            return child;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("sub-agent did not settle");
}

#[tokio::test(flavor = "multi_thread")]
async fn spawns_children_and_supports_message_handoff_and_cancel() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    enable(&client).await;
    let parent = parent_session(&client).await;

    // --- spawn runs the child in its own session ---------------------------
    let spawned = client
        .call(
            "agent.spawn",
            json!({ "agent_id": "worker", "task": "hi", "parent_session_id": parent }),
        )
        .await
        .unwrap();
    let child_session = spawned["child"]["session_id"].as_str().unwrap().to_string();
    let child = wait_done(&client, &child_session).await;
    assert_eq!(child["state"], json!("done"));
    assert_eq!(child["result"], json!("echo: hi"));
    assert_eq!(child["parent_session_id"], json!(parent));

    // --- monitoring --------------------------------------------------------
    let children = client
        .call("agent.children", json!({ "parent_session_id": parent }))
        .await
        .unwrap();
    assert_eq!(children["children"].as_array().unwrap().len(), 1);

    // --- messaging appends to the child transcript -------------------------
    client
        .call(
            "agent.message",
            json!({ "session_id": child_session, "content": "keep going" }),
        )
        .await
        .unwrap();
    let messages = client
        .call(
            "agent.session.messages",
            json!({ "session_id": child_session }),
        )
        .await
        .unwrap();
    let contents: Vec<&str> = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| message["content"].as_str().unwrap())
        .collect();
    assert!(contents.contains(&"keep going"));

    // --- handoff carries the source transcript ------------------------------
    let handed = client
        .call(
            "agent.handoff",
            json!({ "from_session_id": parent, "to_agent_id": "worker", "task": "take over" }),
        )
        .await
        .unwrap();
    assert_eq!(handed["child"]["state"], json!("done"));
    assert!(handed["child"]["result"]
        .as_str()
        .unwrap()
        .contains("Handoff context"));

    // --- cancel ------------------------------------------------------------
    let cancelled = client
        .call("agent.cancel", json!({ "session_id": child_session }))
        .await
        .unwrap();
    assert_eq!(cancelled["cancelled"], json!(true));
    let err = client
        .call("agent.cancel", json!({ "session_id": "nope" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_is_gated_and_rejects_disabled_or_unpermitted_agents() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- capability first ---------------------------------------------------
    client
        .call(
            "capability.set",
            json!({ "id": "capability.sub-agents", "enabled": false }),
        )
        .await
        .unwrap();
    let err = client
        .call("agent.spawn", json!({ "agent_id": "worker", "task": "hi" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    enable(&client).await;
    client
        .call("agent.disable", json!({ "id": "worker" }))
        .await
        .unwrap();
    let disabled = client
        .call("agent.spawn", json!({ "agent_id": "worker", "task": "hi" }))
        .await
        .unwrap_err();
    match disabled {
        fergents_api::ApiError::Rpc { code, message } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32());
            assert!(message.contains("agent_disabled"), "{message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- a parent that does not allow sub-agents ----------------------------
    client
        .call("agent.enable", json!({ "id": "worker" }))
        .await
        .unwrap();
    let worker_session = client
        .call("agent.run", json!({ "id": "worker", "task": "plan" }))
        .await
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let err = client
        .call(
            "agent.spawn",
            json!({ "agent_id": "worker", "task": "hi", "parent_session_id": worker_session }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, message } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32());
            assert!(message.contains("does not allow sub-agents"), "{message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
