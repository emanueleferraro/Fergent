//! Terminal sessions over IPC (task 3.3/3.4): open, write, resize, read, exec,
//! close, plus the workspace-trust and capability gates.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("terminal.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-terminal-{}-{index}",
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
    (dir, socket, handle, kernel)
}

async fn open_project(client: &KernelClient, dir: &Path, trusted: bool) -> String {
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let opened = client
        .call(
            "project.open",
            json!({ "path": workspace.to_string_lossy() }),
        )
        .await
        .unwrap();
    let id = opened["project"]["id"].as_str().unwrap().to_string();
    if trusted {
        client
            .call(
                "project.trust",
                json!({ "project_id": id, "trusted": true }),
            )
            .await
            .unwrap();
    }
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_lifecycle_over_ipc() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let id = open_project(&client, dir.path(), true).await;

    let opened = match client
        .call(
            "term.open",
            json!({ "project_id": id, "cols": 100, "rows": 30 }),
        )
        .await
    {
        Ok(value) => value,
        Err(err) => {
            // No usable shell on this host: the PTY layer is covered by the
            // `fergents-terminal` tests.
            eprintln!("skipping terminal lifecycle: {err:?}");
            let _ = client.call("kernel.shutdown", json!({})).await;
            handle.join().await.unwrap();
            return;
        }
    };
    let session_id = opened["session"]["id"].as_str().unwrap().to_string();
    assert_eq!(opened["session"]["state"], json!("running"));

    client
        .call(
            "term.resize",
            json!({ "session_id": session_id, "cols": 120, "rows": 40 }),
        )
        .await
        .unwrap();

    client
        .call(
            "term.write",
            json!({ "session_id": opened["session"]["id"], "data": "echo fergents-ipc-marker\r" }),
        )
        .await
        .unwrap();

    let mut seen = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let read = client
            .call("term.read", json!({ "session_id": session_id }))
            .await
            .unwrap();
        if read["output"]
            .as_str()
            .unwrap_or_default()
            .contains("fergents-ipc-marker")
        {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(seen, "the shell never echoed the marker");

    let listed = client
        .call("term.list", json!({ "project_id": id }))
        .await
        .unwrap();
    let sessions = listed["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["cols"], json!(120));

    let executed = client
        .call(
            "term.exec",
            json!({ "project_id": id, "command": "echo fergents-exec", "actor": "agent:test" }),
        )
        .await
        .unwrap();
    assert_eq!(executed["session"]["project_id"], json!(id));

    let closed = client
        .call("term.close", json!({ "session_id": session_id }))
        .await
        .unwrap();
    assert_eq!(closed["ok"], json!(true));

    let listed = client
        .call("term.list", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(listed["sessions"][0]["state"], json!("closed"));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_respects_trust_and_the_capability_guard() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let id = open_project(&client, dir.path(), false).await;

    let err = client
        .call("term.open", json!({ "project_id": id }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::ProjectUntrusted.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let err = client
        .call("term.read", json!({ "session_id": "nope" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    client
        .call(
            "capability.set",
            json!({ "id": "capability.terminal", "enabled": false }),
        )
        .await
        .unwrap();
    let err = client.call("term.list", json!({})).await.unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
