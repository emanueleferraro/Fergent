//! The Context Viewer backend over the wire: inspect, toggle, budget, gating.

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("context.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-context-{}-{index}",
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
async fn inspect_toggle_gate_and_budget() {
    let (_dir, socket, handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- inspect: sections carry tokens + enabled -------------------------
    let snapshot = client
        .call(
            "context.inspect",
            json!({ "session_id": "s1", "input": "what is MCP?" }),
        )
        .await
        .unwrap();
    assert!(snapshot["total_tokens"].as_u64().unwrap() > 0);
    let sections = snapshot["sections"].as_array().unwrap();
    let names: Vec<&str> = sections
        .iter()
        .map(|section| section["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"input"));
    assert!(names.contains(&"capabilities"));
    assert!(names.contains(&"rag"), "rag present when search enabled");

    // --- context.built event persisted ------------------------------------
    let events = client
        .call("events.replay", json!({ "topics": [], "limit": 100 }))
        .await
        .unwrap();
    let events = events["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|event| event["kind"] == json!("context.built")));

    // --- toggle a section off ---------------------------------------------
    let settings = client
        .call(
            "context.set_section",
            json!({ "section": "rag", "enabled": false }),
        )
        .await
        .unwrap();
    assert_eq!(settings["enabled"]["rag"], json!(false));

    let snapshot = client
        .call(
            "context.inspect",
            json!({ "session_id": "s1", "input": "again" }),
        )
        .await
        .unwrap();
    let rag = snapshot["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["name"] == json!("rag"))
        .unwrap();
    assert_eq!(rag["enabled"], json!(false));
    assert_eq!(rag["tokens"], json!(0));

    // --- RAG gating: disable capability.search ----------------------------
    client
        .call(
            "context.set_section",
            json!({ "section": "rag", "enabled": true }),
        )
        .await
        .unwrap();
    client
        .call(
            "capability.set",
            json!({ "id": "capability.search", "enabled": false }),
        )
        .await
        .unwrap();
    let snapshot = client
        .call(
            "context.inspect",
            json!({ "session_id": "s1", "input": "x" }),
        )
        .await
        .unwrap();
    let names: Vec<&str> = snapshot["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"rag"), "rag absent when search disabled");

    // --- budget -----------------------------------------------------------
    let settings = client
        .call("context.set_budget", json!({ "budget": 8192 }))
        .await
        .unwrap();
    assert_eq!(settings["budget"], json!(8192));
    let snapshot = client
        .call(
            "context.inspect",
            json!({ "session_id": "s1", "input": "y" }),
        )
        .await
        .unwrap();
    assert_eq!(snapshot["budget"], json!(8192));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
