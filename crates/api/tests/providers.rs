//! The provider registry over the wire: configuration, routes, and health.

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
        SocketPath::from_path(dir.join("providers.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-providers-{}-{index}",
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
async fn provider_round_trip() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let added = client
        .call(
            "provider.add",
            json!({
                "id": "local",
                "kind": "mock",
                "models": ["mock-1", "mock-2"]
            }),
        )
        .await
        .unwrap();
    assert_eq!(added["id"], "local");
    assert_eq!(added["kind"], "mock");
    assert!(added.get("secret_name").is_none() || added["secret_name"].is_null());

    let listed = client.call("provider.list", json!({})).await.unwrap();
    let providers = listed["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "local");

    // Re-adding updates in place instead of duplicating.
    client
        .call(
            "provider.add",
            json!({ "id": "local", "kind": "mock", "models": ["mock-3"] }),
        )
        .await
        .unwrap();
    let listed = client.call("provider.list", json!({})).await.unwrap();
    assert_eq!(listed["providers"].as_array().unwrap().len(), 1);

    let models = client
        .call("provider.models", json!({ "id": "local" }))
        .await
        .unwrap();
    assert_eq!(models["models"], json!(["mock-3"]));

    let tested = client
        .call("provider.test", json!({ "id": "local" }))
        .await
        .unwrap();
    assert_eq!(tested["ok"], true);

    let removed = client
        .call("provider.remove", json!({ "id": "local" }))
        .await
        .unwrap();
    assert_eq!(removed["deleted"], true);
    let listed = client.call("provider.list", json!({})).await.unwrap();
    assert_eq!(listed["providers"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_routes_round_trip() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    client
        .call(
            "provider.route.set",
            json!({ "task": "chat", "models": ["mock-1", "mock-2"] }),
        )
        .await
        .unwrap();

    let routes = client.call("provider.route.get", json!({})).await.unwrap();
    let routes = routes["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0]["task"], "chat");
    assert_eq!(routes[0]["models"], json!(["mock-1", "mock-2"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_kind_and_missing_provider_are_rejected() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let err = client
        .call("provider.add", json!({ "id": "x", "kind": "not-a-kind" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown provider kind"));

    let err = client
        .call("provider.test", json!({ "id": "missing" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown provider"));
}
