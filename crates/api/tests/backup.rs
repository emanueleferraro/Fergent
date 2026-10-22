//! Reversibility over the wire: snapshot, list, restore, restore_run.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("backup.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-backup-{}-{index}",
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

#[tokio::test(flavor = "multi_thread")]
async fn backup_and_restore_a_file() {
    let (dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let file = dir.path().join("agent-note.md");
    std::fs::write(&file, "v1").unwrap();
    let path = file.to_string_lossy().into_owned();

    let backup = client
        .call(
            "backup.snapshot",
            json!({ "path": path, "run_id": "run-1", "agent_id": "a1" }),
        )
        .await
        .unwrap();
    let backup_id = backup["id"].as_str().unwrap().to_string();
    assert_eq!(backup["existed"], json!(true));

    std::fs::write(&file, "v2").unwrap();

    let listed = client
        .call("backup.list", json!({ "run_id": "run-1" }))
        .await
        .unwrap();
    assert_eq!(listed["backups"].as_array().unwrap().len(), 1);

    let restored = client
        .call("backup.restore", json!({ "backup_id": backup_id }))
        .await
        .unwrap();
    assert_eq!(restored["restored"], json!(path));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1");

    // Run-level restore: take a second snapshot, mutate, then roll back.
    client
        .call(
            "backup.snapshot",
            json!({ "path": path, "run_id": "run-2" }),
        )
        .await
        .unwrap();
    std::fs::write(&file, "v3").unwrap();
    let restored = client
        .call("backup.restore_run", json!({ "run_id": "run-2" }))
        .await
        .unwrap();
    assert_eq!(restored["restored"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1");

    // Unknown backups are invalid params.
    let err = client
        .call("backup.restore", json!({ "backup_id": "nope" }))
        .await
        .unwrap_err();
    assert!(matches!(err, fergents_api::ApiError::Rpc { .. }));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
