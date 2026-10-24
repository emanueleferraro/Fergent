//! The LSP registry and lifecycle over the wire, plus the `capability.lsp`
//! dispatch guard (task 3.3).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("lsp.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-lsp-{}-{index}",
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
    (dir, socket, handle, kernel)
}

async fn open_project(client: &KernelClient, dir: &Path, trusted: bool) -> String {
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(workspace.join("src/main.rs"), "fn main() {}\n").unwrap();
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
async fn lsp_registry_and_lifecycle_over_ipc() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let id = open_project(&client, dir.path(), true).await;

    let listed = client.call("lsp.server.list", json!({})).await.unwrap();
    assert!(listed["servers"].as_array().unwrap().is_empty());

    // A missing executable is a valid registration, reported as not installed.
    let added = client
        .call(
            "lsp.server.add",
            json!({
                "language_id": "rust",
                "command": "/nonexistent/definitely-not-a-language-server",
                "args": ["--stdio"],
                "env": { "RUST_LOG": "error" },
                "source": "user",
            }),
        )
        .await
        .unwrap();
    assert_eq!(added["server"]["language_id"], json!("rust"));
    assert_eq!(added["server"]["installed"], json!(false));
    assert_eq!(added["server"]["status"], json!("not_installed"));
    let server_id = added["server"]["id"].as_str().unwrap().to_string();

    // Re-adding the same `(language_id, command)` upserts.
    client
        .call(
            "lsp.server.add",
            json!({
                "language_id": "rust",
                "command": "/nonexistent/definitely-not-a-language-server",
                "args": ["--stdio"],
            }),
        )
        .await
        .unwrap();
    let listed = client.call("lsp.server.list", json!({})).await.unwrap();
    assert_eq!(listed["servers"].as_array().unwrap().len(), 1);

    let status = client
        .call(
            "lsp.server.status",
            json!({ "project_id": id, "language_id": "rust" }),
        )
        .await
        .unwrap();
    assert_eq!(status["server"]["status"], json!("not_installed"));

    let started = client
        .call(
            "lsp.server.start",
            json!({ "project_id": id, "language_id": "rust" }),
        )
        .await
        .unwrap();
    assert_eq!(started["server"]["status"], json!("not_installed"));

    let stopped = client
        .call(
            "lsp.server.stop",
            json!({ "project_id": id, "language_id": "rust" }),
        )
        .await
        .unwrap();
    assert_eq!(stopped["stopped"], json!(false));

    let diagnostics = client
        .call("lsp.diagnostics.list", json!({ "project_id": id }))
        .await
        .unwrap();
    assert!(diagnostics["diagnostics"].as_array().unwrap().is_empty());

    // A language with no server mapping is `invalid_params`.
    let err = client
        .call(
            "lsp.hover",
            json!({ "project_id": id, "path": "README.md", "line": 0, "character": 3 }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let removed = client
        .call("lsp.server.remove", json!({ "id": server_id }))
        .await
        .unwrap();
    assert_eq!(removed["removed"], json!(true));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_respects_workspace_trust_and_the_capability_guard() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let id = open_project(&client, dir.path(), false).await;

    client
        .call(
            "lsp.server.add",
            json!({ "language_id": "rust", "command": "/nonexistent/server" }),
        )
        .await
        .unwrap();

    // Workspace trust gates the server lifecycle.
    let err = client
        .call(
            "lsp.server.start",
            json!({ "project_id": id, "language_id": "rust" }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::ProjectUntrusted.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // A disabled capability refuses every `lsp.*` command.
    client
        .call(
            "capability.set",
            json!({ "id": "capability.lsp", "enabled": false }),
        )
        .await
        .unwrap();
    let err = client.call("lsp.server.list", json!({})).await.unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
