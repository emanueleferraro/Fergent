//! The secret vault over the wire: values never cross the boundary.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_secrets::MemoryKeychain;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("secrets.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-secrets-{}-{index}",
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

#[tokio::test(flavor = "multi_thread")]
async fn secret_round_trip_never_returns_values() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    client
        .call(
            "secret.set",
            json!({ "name": "anthropic.api_key", "value": "sk-ant-secret", "provider": "anthropic" }),
        )
        .await
        .unwrap();

    let listed = client.call("secret.list", json!({})).await.unwrap();
    let secrets = listed["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["name"], "anthropic.api_key");
    assert_eq!(secrets[0]["provider"], "anthropic");
    assert!(secrets[0].get("value").is_none());
    assert!(!listed.to_string().contains("sk-ant-secret"));

    let tested = client
        .call("secret.test", json!({ "name": "anthropic.api_key" }))
        .await
        .unwrap();
    assert_eq!(tested["present"], true);
    assert!(!tested.to_string().contains("sk-ant-secret"));

    let missing = client
        .call("secret.test", json!({ "name": "openai.api_key" }))
        .await
        .unwrap();
    assert_eq!(missing["present"], false);

    let deleted = client
        .call("secret.delete", json!({ "name": "anthropic.api_key" }))
        .await
        .unwrap();
    assert_eq!(deleted["deleted"], true);

    let empty = client.call("secret.list", json!({})).await.unwrap();
    assert_eq!(empty["secrets"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_secret_names_are_rejected() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let err = client
        .call("secret.set", json!({ "name": "bad name", "value": "v" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid secret name"));
}
