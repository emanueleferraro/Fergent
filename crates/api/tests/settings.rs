//! The generic settings store over the wire: read, write, overwrite.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("settings.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-settings-{}-{index}",
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
async fn settings_round_trip() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- unset keys read as an empty object -------------------------------
    let missing = client
        .call("settings.get", json!({ "key": "ide.shell" }))
        .await
        .unwrap();
    assert_eq!(missing["value"], json!({}));

    // --- a write is readable back -----------------------------------------
    let written = client
        .call(
            "settings.set",
            json!({ "key": "ide.shell", "value": { "shell": "zsh" } }),
        )
        .await
        .unwrap();
    assert_eq!(written["value"], json!({ "shell": "zsh" }));

    let read = client
        .call("settings.get", json!({ "key": "ide.shell" }))
        .await
        .unwrap();
    assert_eq!(read["value"], json!({ "shell": "zsh" }));

    // --- a second write replaces the value --------------------------------
    client
        .call(
            "settings.set",
            json!({ "key": "ide.shell", "value": { "shell": "pwsh" } }),
        )
        .await
        .unwrap();
    let read = client
        .call("settings.get", json!({ "key": "ide.shell" }))
        .await
        .unwrap();
    assert_eq!(read["value"], json!({ "shell": "pwsh" }));

    // --- keys are independent ---------------------------------------------
    let other = client
        .call("settings.get", json!({ "key": "ide.editor" }))
        .await
        .unwrap();
    assert_eq!(other["value"], json!({}));

    // --- params are validated ---------------------------------------------
    assert!(client
        .call("settings.set", json!({ "key": "ide.shell" }))
        .await
        .is_err());

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
