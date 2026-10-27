//! Phase 4 backend: capability gates, factory workspaces, marketplace indexes.

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
        SocketPath::from_path(dir.join("extensions.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-extensions-{}-{index}",
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

fn assert_disabled(error: fergents_api::ApiError) {
    match error {
        fergents_api::ApiError::Rpc { code, message } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32(), "{message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn plugin_skill_factory_and_marketplace_commands_are_gated() {
    let (_dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // Factory and marketplace ship off by default.
    let error = client
        .call(
            "factory.scaffold",
            json!({ "template": "skill-prompt", "id": "com.example.x", "name": "X" }),
        )
        .await
        .unwrap_err();
    assert_disabled(error);

    let error = client
        .call("marketplace.search", json!({}))
        .await
        .unwrap_err();
    assert_disabled(error);

    // Plugins and skills are on, then turned off: their commands are refused.
    for id in ["capability.plugins", "capability.skills"] {
        client
            .call("capability.set", json!({ "id": id, "enabled": false }))
            .await
            .unwrap();
    }
    let error = client.call("plugin.list", json!({})).await.unwrap_err();
    assert_disabled(error);
    let error = client.call("skill.list", json!({})).await.unwrap_err();
    assert_disabled(error);

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn factory_scaffolds_tests_and_reports() {
    let (_dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    client
        .call(
            "capability.set",
            json!({ "id": "capability.plugin-factory", "enabled": true, "auto_enable_deps": true }),
        )
        .await
        .unwrap();

    let scaffold = client
        .call(
            "factory.scaffold",
            json!({ "template": "skill-prompt", "id": "com.example.summary", "name": "Summary" }),
        )
        .await
        .unwrap();
    let run_id = scaffold["run_id"].as_str().unwrap().to_string();
    assert!(scaffold["path"].as_str().unwrap().contains("factory"));

    let build = client
        .call("factory.build", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(build["skipped"], json!(true));

    let report = client
        .call("factory.test", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["kind"], json!("skill"));

    let summary = client
        .call("factory.report", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert!(summary["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file == "SKILL.md"));

    let runs = client.call("factory.list", json!({})).await.unwrap();
    assert_eq!(runs["runs"].as_array().unwrap().len(), 1);

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn marketplace_refreshes_and_searches_a_local_index() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    client
        .call(
            "capability.set",
            json!({ "id": "capability.marketplace", "enabled": true, "auto_enable_deps": true }),
        )
        .await
        .unwrap();

    let index = json!({
        "schema": 1,
        "entries": [{
            "id": "com.example.echo",
            "kind": "plugin",
            "name": "Echo",
            "version": "1.2.0",
            "description": "Repeats text",
            "download": { "url": "https://example.com/echo.fergplugin", "sha256": "00" }
        }]
    });
    let index_path = dir.path().join("index.json");
    std::fs::write(&index_path, index.to_string()).unwrap();

    let source = client
        .call(
            "marketplace.sources.add",
            json!({
                "id": "local",
                "url": index_path.to_string_lossy(),
                "kind": "file",
            }),
        )
        .await
        .unwrap();
    assert_eq!(source["id"], json!("local"));

    let refreshed = client.call("marketplace.refresh", json!({})).await.unwrap();
    assert_eq!(refreshed["refreshed"][0]["entries"], json!(1));
    assert_eq!(refreshed["refreshed"][0]["signature"], json!("unsigned"));

    let found = client
        .call("marketplace.search", json!({ "query": "echo" }))
        .await
        .unwrap();
    let entries = found["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["version"], json!("1.2.0"));

    let entry = client
        .call(
            "marketplace.get",
            json!({ "source_id": "local", "kind": "plugin", "entry_id": "com.example.echo" }),
        )
        .await
        .unwrap();
    assert_eq!(entry["name"], json!("Echo"));

    let updates = client
        .call("marketplace.check_updates", json!({}))
        .await
        .unwrap();
    assert!(updates["updates"].as_array().unwrap().is_empty());

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
