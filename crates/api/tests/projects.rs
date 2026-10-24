//! The project layer over the wire: open, trust, files, search, update,
//! close, and the `capability.ide` dispatch guard (task 3.1).

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
        SocketPath::from_path(dir.join("projects.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-projects-{}-{index}",
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

/// Disable `capability.ide`, first switching off the enabled dependents that
/// would otherwise block it.
async fn disable_ide(client: &KernelClient) {
    for id in [
        "capability.git-remote",
        "capability.git",
        "capability.terminal",
        "capability.lsp",
        "capability.ide",
    ] {
        client
            .call("capability.set", json!({ "id": id, "enabled": false }))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn file_operations_over_ipc() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let opened = client
        .call(
            "project.open",
            json!({ "path": workspace.to_string_lossy() }),
        )
        .await
        .unwrap();
    let id = opened["project"]["id"].as_str().unwrap().to_string();

    // Writes are refused until the project is trusted.
    let err = client
        .call(
            "file.write",
            json!({ "project_id": id, "path": "a.txt", "content": "one\n" }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::ProjectUntrusted.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    client
        .call(
            "project.trust",
            json!({ "project_id": id, "trusted": true }),
        )
        .await
        .unwrap();

    // --- write / read -----------------------------------------------------
    let written = client
        .call(
            "file.write",
            json!({ "project_id": id, "path": "a.txt", "content": "one\n" }),
        )
        .await
        .unwrap();
    let etag = written["file"]["etag"].as_str().unwrap().to_string();
    assert_eq!(written["file"]["language_id"], json!("plaintext"));

    let read = client
        .call("file.read", json!({ "project_id": id, "path": "a.txt" }))
        .await
        .unwrap();
    assert_eq!(read["file"]["content"], json!("one\n"));
    assert_eq!(read["file"]["etag"], json!(etag));

    // --- stale etag -------------------------------------------------------
    let err = client
        .call(
            "file.write",
            json!({
                "project_id": id,
                "path": "a.txt",
                "content": "two\n",
                "base_etag": "deadbeef"
            }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::FileConflict.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- mkdir / create / list / rename / delete --------------------------
    client
        .call("file.mkdir", json!({ "project_id": id, "path": "src" }))
        .await
        .unwrap();
    client
        .call(
            "file.create",
            json!({ "project_id": id, "path": "src/main.rs", "content": "fn main() {}\n" }),
        )
        .await
        .unwrap();

    let listed = client
        .call("file.list", json!({ "project_id": id, "path": "" }))
        .await
        .unwrap();
    let names: Vec<_> = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["src", "a.txt"]);

    let renamed = client
        .call(
            "file.rename",
            json!({ "project_id": id, "from": "src/main.rs", "to": "src/lib.rs" }),
        )
        .await
        .unwrap();
    assert_eq!(renamed["entry"]["path"], json!("src/lib.rs"));

    let stat = client
        .call(
            "file.stat",
            json!({ "project_id": id, "path": "src/lib.rs" }),
        )
        .await
        .unwrap();
    assert_eq!(stat["entry"]["kind"], json!("file"));

    let deleted = client
        .call("file.delete", json!({ "project_id": id, "path": "src" }))
        .await
        .unwrap();
    assert_eq!(deleted["deleted"], json!(true));

    // --- guard ------------------------------------------------------------
    disable_ide(&client).await;
    let err = client
        .call("file.list", json!({ "project_id": id }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn project_lifecycle_over_ipc() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- open -------------------------------------------------------------
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(workspace.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(workspace.join("README.md"), "needle in a haystack\n").unwrap();

    let opened = client
        .call(
            "project.open",
            json!({ "path": workspace.to_string_lossy() }),
        )
        .await
        .unwrap();
    let project = &opened["project"];
    let id = project["id"].as_str().unwrap().to_string();
    assert_eq!(project["name"], json!("workspace"));
    assert_eq!(project["trust"], json!("untrusted"));
    assert_eq!(project["vcs"], json!(null));

    // Re-opening the same folder is idempotent.
    let again = client
        .call(
            "project.open",
            json!({ "path": workspace.to_string_lossy() }),
        )
        .await
        .unwrap();
    assert_eq!(again["project"]["id"], json!(id));

    // --- get / list -------------------------------------------------------
    let fetched = client
        .call("project.get", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(fetched["project"]["id"], json!(id));

    let missing = client
        .call("project.get", json!({ "project_id": "nope" }))
        .await
        .unwrap();
    assert_eq!(missing["project"], json!(null));

    let listed = client.call("project.list", json!({})).await.unwrap();
    assert_eq!(listed["projects"].as_array().unwrap().len(), 1);

    // --- files / search (allowed while untrusted: read-only) --------------
    let files = client
        .call(
            "project.files",
            json!({ "project_id": id, "query": "main" }),
        )
        .await
        .unwrap();
    assert_eq!(files["files"][0], json!("src/main.rs"));

    let hits = client
        .call(
            "project.search",
            json!({ "project_id": id, "query": "needle", "limit": 5 }),
        )
        .await
        .unwrap();
    let hits = hits["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["path"], json!("README.md"));
    assert_eq!(hits[0]["line"], json!(1));

    // --- trust ------------------------------------------------------------
    let trusted = client
        .call(
            "project.trust",
            json!({ "project_id": id, "trusted": true }),
        )
        .await
        .unwrap();
    assert_eq!(trusted["project"]["trust"], json!("trusted"));

    // --- update -----------------------------------------------------------
    let updated = client
        .call(
            "project.update",
            json!({ "project_id": id, "name": "Renamed" }),
        )
        .await
        .unwrap();
    assert_eq!(updated["project"]["name"], json!("Renamed"));

    // --- unknown project surfaces a stable error --------------------------
    let err = client
        .call(
            "project.trust",
            json!({ "project_id": "nope", "trusted": true }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- close ------------------------------------------------------------
    let closed = client
        .call("project.close", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(closed["closed"], json!(true));
    let listed = client.call("project.list", json!({})).await.unwrap();
    assert!(listed["projects"].as_array().unwrap().is_empty());

    // --- guard: a disabled capability's command is refused ----------------
    disable_ide(&client).await;
    let err = client.call("project.list", json!({})).await.unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
