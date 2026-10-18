//! Task 15 end-to-end test: kernel wiring, dispatch, and the brain runtime.
//!
//! Boots a real [`Kernel`](fergents_core::Kernel) + API server on a temporary
//! data directory and drives the brain flow over the socket: `brain.create`,
//! the `brain.created` event, `doc.write`, and `search.query`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::{BrainCreateResult, DocWriteResult, SearchQueryResult};
use tokio::sync::watch;

/// A unique, platform-appropriate address for a test endpoint.
fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("task15.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-task15-{}-{index}",
            std::process::id()
        ))
    }
}

/// Start a kernel + server on a temp socket and wait until it accepts.
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

    // Wait for the listener to be ready.
    for _ in 0..100 {
        if connect_stream(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (dir, socket, handle, kernel)
}

#[tokio::test(flavor = "multi_thread")]
async fn brain_create_write_and_search_end_to_end() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let mut client = KernelClient::connect(&socket).await.unwrap();
    assert!(!client.session_id().is_empty());

    // Subscribe before creating so the `brain.created` event is not missed.
    client.subscribe(&[]).await.unwrap();

    // --- brain.create -----------------------------------------------------
    let brain_path = dir.path().join("test-brain");
    let create: BrainCreateResult = client
        .call_typed(
            "brain.create",
            &serde_json::json!({
                "name": "Test",
                "path": brain_path.to_string_lossy(),
                "template": "personal",
            }),
        )
        .await
        .unwrap();
    let brain_id = create.brain.brain_id.clone();
    assert!(!brain_id.is_empty(), "brain.create returned an empty id");

    // --- brain.created event ---------------------------------------------
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = client.next_event().await.expect("event stream closed");
            if event.kind == "brain.created" {
                return event;
            }
        }
    })
    .await
    .expect("timed out waiting for brain.created");
    assert_eq!(
        event.payload.get("brain_id").and_then(|v| v.as_str()),
        Some(brain_id.as_str()),
    );

    // --- doc.write --------------------------------------------------------
    const TOKEN: &str = "zephyrianquokka";
    let write: DocWriteResult = client
        .call_typed(
            "doc.write",
            &serde_json::json!({
                "document_id": "notes/hello.md",
                "content": format!("# Hello\n\nThe {TOKEN} protocol converges here.\n"),
            }),
        )
        .await
        .unwrap();
    assert_eq!(write.document.path, "notes/hello.md");

    // --- search.query -----------------------------------------------------
    let found = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let result: SearchQueryResult = client
                .call_typed("search.query", &serde_json::json!({ "query": TOKEN }))
                .await
                .unwrap();
            if !result.results.is_empty() {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("no search results for the written note");

    assert!(
        found.results.iter().any(|hit| hit.content.contains(TOKEN)),
        "expected a hit containing the note token",
    );

    // --- shutdown ---------------------------------------------------------
    let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
    handle.join().await.unwrap();
}
