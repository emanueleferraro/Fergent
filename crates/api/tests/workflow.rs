//! The spec workflow over the wire: start → pause → approve → complete.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("spec.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-spec-{}-{index}",
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
async fn spec_workflow_round_trip() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let run = client
        .call(
            "spec.start",
            json!({ "slug": "wire", "task": "Add a widget", "mode": "supervised" }),
        )
        .await
        .unwrap();
    assert_eq!(run["state"], json!("awaiting_spec_approval"));
    let run_id = run["id"].as_str().unwrap().to_string();

    // The spec is on disk and readable through spec.get.
    let fetched = client
        .call("spec.get", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert!(fetched["artifacts"]["spec"]
        .as_str()
        .unwrap()
        .contains("Add a widget"));

    // Approve through run.approve → plan gate.
    let run = client
        .call("run.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(run["state"], json!("awaiting_plan_approval"));

    // Then through spec.approve → tasks gate, then complete.
    let run = client
        .call("spec.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(run["state"], json!("awaiting_task_approval"));
    let run = client
        .call("run.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(run["state"], json!("completed"));

    let listed = client.call("run.list", json!({})).await.unwrap();
    assert_eq!(listed["runs"].as_array().unwrap().len(), 1);

    let got = client
        .call("run.get", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(got["state"], json!("completed"));

    // Rejecting stops a run; unknown runs are rejected.
    let second = client
        .call(
            "spec.start",
            json!({ "slug": "wire-2", "task": "x", "mode": "auto" }),
        )
        .await
        .unwrap();
    assert_eq!(second["state"], json!("completed"));
    let err = client
        .call("run.reject", json!({ "run_id": "nope" }))
        .await
        .unwrap_err();
    assert!(matches!(err, fergents_api::ApiError::Rpc { .. }));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
