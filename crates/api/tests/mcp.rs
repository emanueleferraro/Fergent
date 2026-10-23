//! The MCP server registry over the wire: configuration, test, health, tools.

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
        SocketPath::from_path(dir.join("mcp.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-mcp-{}-{index}",
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
async fn mcp_server_round_trip() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let added = client
        .call(
            "mcp.server.add",
            json!({ "id": "filesystem", "command": "npx", "args": ["-y", "server"] }),
        )
        .await
        .unwrap();
    assert_eq!(added["id"], "filesystem");
    assert_eq!(added["command"], "npx");
    assert_eq!(added["args"], json!(["-y", "server"]));
    assert_eq!(added["allow"], true);
    assert_eq!(added["connected"], false);

    let listed = client.call("mcp.server.list", json!({})).await.unwrap();
    let servers = listed["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["id"], "filesystem");

    // Health of a configured but not-connected server.
    let health = client
        .call("mcp.server.health", json!({ "id": "filesystem" }))
        .await
        .unwrap();
    assert_eq!(health["connected"], false);
    assert!(health["healthy"].is_null());

    // A command-less server cannot be tested.
    client
        .call("mcp.server.add", json!({ "id": "local" }))
        .await
        .unwrap();
    let tested = client
        .call("mcp.server.test", json!({ "id": "local" }))
        .await
        .unwrap();
    assert_eq!(tested["ok"], false);
    assert_eq!(tested["detail"], "no command configured");

    // Nothing is connected, so no tools are discoverable.
    let tools = client.call("mcp.tool.list", json!({})).await.unwrap();
    assert_eq!(tools["tools"].as_array().unwrap().len(), 0);

    let removed = client
        .call("mcp.server.remove", json!({ "id": "filesystem" }))
        .await
        .unwrap();
    assert_eq!(removed["deleted"], true);
    let listed = client.call("mcp.server.list", json!({})).await.unwrap();
    assert_eq!(listed["servers"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_mcp_server_is_rejected() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let err = client
        .call("mcp.server.health", json!({ "id": "missing" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown mcp server"));
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_mcp_server_ids_are_rejected() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let err = client
        .call("mcp.server.add", json!({ "id": "bad id" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid server id"));
}
