//! Compare over the wire: one prompt, several models, one session per side.

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
        SocketPath::from_path(dir.join("compare.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-compare-{}-{index}",
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

async fn prepare(client: &KernelClient) {
    client
        .call(
            "capability.set",
            json!({ "id": "capability.compare", "enabled": true, "auto_enable_deps": true }),
        )
        .await
        .unwrap();
    client
        .call(
            "provider.add",
            json!({ "id": "local", "kind": "mock", "models": ["mock-1", "mock-2"] }),
        )
        .await
        .unwrap();
    client
        .call(
            "agent.create",
            json!({
                "definition": "---\nid: assistant\nname: Assistant\nmodel: mock-1\ntools: []\n---\n\nYou are a helpful assistant.\n"
            }),
        )
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn compare_runs_each_model_in_its_own_session() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    prepare(&client).await;

    let result = client
        .call(
            "chat.compare.start",
            json!({
                "agent_id": "assistant",
                "prompt": "hello",
                "models": ["mock-1", "mock-2"],
                "title": "Compare"
            }),
        )
        .await
        .unwrap();

    let sides = result["sides"].as_array().unwrap();
    assert_eq!(sides.len(), 2);
    let left = &sides[0];
    let right = &sides[1];
    assert_ne!(left["session_id"], right["session_id"]);
    assert_eq!(left["model"], "mock-1");
    assert_eq!(right["model"], "mock-2");
    assert_eq!(left["content"], "echo: hello");
    assert_eq!(right["content"], "echo: hello");
    assert!(left["error"].is_null());

    // Each side holds the user prompt and the assistant reply.
    let messages = client
        .call(
            "chat.message.list",
            json!({ "session_id": left["session_id"] }),
        )
        .await
        .unwrap();
    let messages = messages["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
}

#[tokio::test(flavor = "multi_thread")]
async fn compare_requires_the_capability_and_two_models() {
    let (_dir, socket, _handle) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    prepare(&client).await;

    let err = client
        .call(
            "chat.compare.start",
            json!({ "agent_id": "assistant", "prompt": "hello", "models": ["mock-1"] }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("at least two models"));

    client
        .call(
            "capability.set",
            json!({ "id": "capability.compare", "enabled": false, "auto_enable_deps": false }),
        )
        .await
        .unwrap();
    let err = client
        .call(
            "chat.compare.start",
            json!({
                "agent_id": "assistant",
                "prompt": "hello",
                "models": ["mock-1", "mock-2"]
            }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("capability.compare"));
}
