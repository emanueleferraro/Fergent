//! The permission engine over the wire: modes, policy, and checks.

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
        SocketPath::from_path(dir.join("perm.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-perm-{}-{index}",
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
async fn modes_policy_and_checks() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // Default mode is auto: reads allow, writes prompt with a card.
    let mode = client.call("permission.mode.get", json!({})).await.unwrap();
    assert_eq!(mode["mode"], json!("auto"));

    let read = client
        .call("permission.check", json!({ "tool": "fs.read" }))
        .await
        .unwrap();
    assert_eq!(read["decision"], json!("allow"));

    let write = client
        .call("permission.check", json!({ "tool": "fs.write" }))
        .await
        .unwrap();
    assert_eq!(write["decision"], json!("prompt"));
    assert_eq!(write["card"]["category"], json!("fs.write"));

    // Full mode stops prompting.
    let set = client
        .call("permission.mode.set", json!({ "mode": "full" }))
        .await
        .unwrap();
    assert_eq!(set["mode"], json!("full"));
    let write = client
        .call("permission.check", json!({ "tool": "fs.write" }))
        .await
        .unwrap();
    assert_eq!(write["decision"], json!("allow"));

    // A policy denial is final even in full mode.
    let policy = "[[rule]]\ncategory = \"fs.write\"\naction = \"deny\"\nnote = \"locked\"\n";
    client
        .call("permission.policy.set", json!({ "toml": policy }))
        .await
        .unwrap();
    let write = client
        .call("permission.check", json!({ "tool": "fs.write" }))
        .await
        .unwrap();
    assert_eq!(write["decision"], json!("deny"));
    assert_eq!(write["reason"], json!("locked"));

    // The policy is readable back.
    let policy = client
        .call("permission.policy.get", json!({}))
        .await
        .unwrap();
    assert_eq!(policy["rules"].as_array().unwrap().len(), 1);

    // Invalid TOML is rejected as invalid params.
    let err = client
        .call(
            "permission.policy.set",
            json!({ "toml": "[[rule]]\naction = 1" }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
