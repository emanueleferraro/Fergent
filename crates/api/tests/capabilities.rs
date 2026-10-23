//! Capability registry over the wire: list, get, set, register, and the
//! dispatch guard.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::{json, Value};
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("capabilities.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-capabilities-{}-{index}",
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

fn find<'a>(caps: &'a Value, id: &str) -> &'a Value {
    caps["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cap| cap["id"] == id)
        .unwrap_or_else(|| panic!("capability {id} not found"))
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_list_get_set_register_and_guard() {
    let (_dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    // --- list -------------------------------------------------------------
    let listed = client.call("capability.list", json!({})).await.unwrap();
    let caps = listed["capabilities"].as_array().unwrap();
    assert!(caps.len() >= 20, "expected the seeded catalogue");
    assert_eq!(find(&listed, "capability.search")["enabled"], json!(true));
    assert_eq!(find(&listed, "capability.charts")["enabled"], json!(false));

    // --- get --------------------------------------------------------------
    let view = client
        .call("capability.get", json!({ "id": "capability.charts" }))
        .await
        .unwrap();
    assert_eq!(view["id"], json!("capability.charts"));
    assert_eq!(view["hot_toggle"], json!(true));
    assert!(view["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d == "capability.chat"));

    let err = client
        .call("capability.get", json!({ "id": "capability.nope" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- guard: a disabled capability's command is refused -----------------
    let disabled = client
        .call(
            "capability.set",
            json!({ "id": "capability.search", "enabled": false }),
        )
        .await
        .unwrap();
    assert_eq!(disabled["enabled"], json!(false));

    let err = client
        .call("search.query", json!({ "query": "anything" }))
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, message } => {
            assert_eq!(code, ErrorCode::CapabilityDisabled.as_i32());
            assert!(message.contains("capability_disabled"), "got: {message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- re-enable --------------------------------------------------------
    let enabled = client
        .call(
            "capability.set",
            json!({ "id": "capability.search", "enabled": true }),
        )
        .await
        .unwrap();
    assert_eq!(enabled["enabled"], json!(true));

    // --- dependency enforcement over the wire -----------------------------
    // `capability.charts` depends on `capability.chat`; disable chat first.
    // (chat has enabled dependents, so this must be refused instead.)
    let err = client
        .call(
            "capability.set",
            json!({ "id": "capability.chat", "enabled": false }),
        )
        .await
        .unwrap_err();
    match err {
        fergents_api::ApiError::Rpc { code, .. } => {
            assert_eq!(code, ErrorCode::InvalidParams.as_i32())
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // --- plugin registration ---------------------------------------------
    let registered = client
        .call(
            "capability.register",
            json!({
                "id": "plugin.acme.thing",
                "name": "Acme Thing",
                "version": "1.2.3",
                "source": "plugin:acme",
                "default_enabled": true,
                "hot_toggle": true,
            }),
        )
        .await
        .unwrap();
    assert_eq!(registered["id"], json!("plugin.acme.thing"));
    assert_eq!(registered["source"], json!("plugin:acme"));
    assert_eq!(registered["enabled"], json!(true));

    let relisted = client.call("capability.list", json!({})).await.unwrap();
    assert_eq!(find(&relisted, "plugin.acme.thing")["enabled"], json!(true));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

/// Task 2.20: the ecosystem extensions ship off, allocate when enabled, and
/// disappear from the enabled set when disabled again.
#[tokio::test(flavor = "multi_thread")]
async fn ecosystem_extensions_are_capability_gated() {
    const EXTENSIONS: [&str; 5] = [
        "capability.voice",
        "capability.ombra",
        "capability.agent-web",
        "capability.plugin-factory",
        "capability.workflow-debugger",
    ];

    let (_dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();

    let listed = client.call("capability.list", json!({})).await.unwrap();
    for id in EXTENSIONS {
        let view = find(&listed, id);
        assert_eq!(view["enabled"], json!(false), "{id} must start disabled");
        assert_eq!(view["default_enabled"], json!(false), "{id} default");
    }

    for id in EXTENSIONS {
        let enabled = client
            .call(
                "capability.set",
                json!({ "id": id, "enabled": true, "auto_enable_deps": true }),
            )
            .await
            .unwrap();
        assert_eq!(enabled["enabled"], json!(true), "{id} must enable");

        let listed = client.call("capability.list", json!({})).await.unwrap();
        assert_eq!(find(&listed, id)["enabled"], json!(true));

        let disabled = client
            .call("capability.set", json!({ "id": id, "enabled": false }))
            .await
            .unwrap();
        assert_eq!(disabled["enabled"], json!(false), "{id} must disable");

        let listed = client.call("capability.list", json!({})).await.unwrap();
        assert_eq!(find(&listed, id)["enabled"], json!(false));
    }

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
