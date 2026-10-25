//! Phase 3 end-to-end: open a project, trust it, edit a file through the
//! kernel, register a language server, run a terminal command, drive the
//! spec/plan/tasks approval workflow, and commit the result.
//!
//! The language server and the shell are optional on CI hosts: a missing
//! executable is a valid `not_installed` registration and a missing shell
//! skips the terminal step. Everything else is deterministic.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::ErrorCode;
use serde_json::json;
use tokio::sync::watch;

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("phase3.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-phase3-{}-{index}",
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

fn repository(root: &Path) {
    let repository = git2::Repository::init(root).unwrap();
    let mut config = repository.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    config.set_bool("core.autocrlf", false).unwrap();
}

fn assert_code(err: fergents_api::ApiError, expected: ErrorCode) {
    match err {
        fergents_api::ApiError::Rpc { code, .. } => assert_eq!(code, expected.as_i32()),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn phase3_end_to_end() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    repository(&workspace);
    std::fs::write(workspace.join("src/lib.rs"), "pub fn one() -> i32 { 1 }\n").unwrap();

    // --- 1. open: the project starts untrusted -------------------------------
    let opened = client
        .call(
            "project.open",
            json!({ "path": workspace.to_string_lossy() }),
        )
        .await
        .unwrap();
    let id = opened["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(opened["project"]["trust"], json!("untrusted"));

    // Restricted mode: no writes until trust is granted (spec decision D3).
    assert_code(
        client
            .call(
                "file.write",
                json!({ "project_id": id, "path": "src/lib.rs", "content": "x\n" }),
            )
            .await
            .unwrap_err(),
        ErrorCode::ProjectUntrusted,
    );

    // --- 2. trust ------------------------------------------------------------
    let trusted = client
        .call(
            "project.trust",
            json!({ "project_id": id, "trusted": true }),
        )
        .await
        .unwrap();
    assert_eq!(trusted["project"]["trust"], json!("trusted"));

    // --- 3. edit through the kernel, with etag conflict protection -----------
    let read = client
        .call(
            "file.read",
            json!({ "project_id": id, "path": "src/lib.rs" }),
        )
        .await
        .unwrap();
    let etag = read["file"]["etag"].as_str().unwrap().to_string();
    let written = client
        .call(
            "file.write",
            json!({
                "project_id": id,
                "path": "src/lib.rs",
                "content": "pub fn one() -> i32 { 2 }\n",
                "base_etag": etag,
            }),
        )
        .await
        .unwrap();
    assert_ne!(written["file"]["etag"], json!(etag));

    // A stale etag is refused instead of clobbering the newer file.
    assert_code(
        client
            .call(
                "file.write",
                json!({
                    "project_id": id,
                    "path": "src/lib.rs",
                    "content": "pub fn one() -> i32 { 3 }\n",
                    "base_etag": etag,
                }),
            )
            .await
            .unwrap_err(),
        ErrorCode::FileConflict,
    );

    // --- 4. language server registry and diagnostics -------------------------
    let added = client
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
    assert_eq!(added["server"]["installed"], json!(false));
    assert_eq!(added["server"]["status"], json!("not_installed"));

    let diagnostics = client
        .call(
            "lsp.diagnostics.list",
            json!({ "project_id": id, "path": "src/lib.rs" }),
        )
        .await
        .unwrap();
    assert!(diagnostics["diagnostics"].as_array().unwrap().is_empty());

    // --- 5. terminal: run a command in a visible tab -------------------------
    if let Ok(opened) = client
        .call(
            "term.open",
            json!({ "project_id": id, "cols": 80, "rows": 24 }),
        )
        .await
    {
        let session_id = opened["session"]["id"].as_str().unwrap().to_string();
        client
            .call(
                "term.write",
                json!({ "session_id": session_id, "data": "echo phase3-marker\r" }),
            )
            .await
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = false;
        while Instant::now() < deadline {
            let read = client
                .call("term.read", json!({ "session_id": session_id }))
                .await
                .unwrap();
            if read["output"]
                .as_str()
                .unwrap_or_default()
                .contains("phase3-marker")
            {
                seen = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(seen, "the shell never echoed the marker");
        client
            .call("term.close", json!({ "session_id": session_id }))
            .await
            .unwrap();
    } else {
        // No usable shell on this host; the PTY layer is covered by the
        // `fergents-terminal` suite.
    }

    // --- 6. agent workflow: spec → plan → tasks → completed ------------------
    let run = client
        .call(
            "spec.start",
            json!({ "slug": "phase3", "task": "Bump one() to 2", "mode": "supervised" }),
        )
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["state"], json!("awaiting_spec_approval"));
    assert_eq!(run["pending_gate"], json!("spec"));

    let approved = client
        .call("run.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(approved["state"], json!("awaiting_plan_approval"));

    let approved = client
        .call("run.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(approved["state"], json!("awaiting_task_approval"));

    let approved = client
        .call("run.approve", json!({ "run_id": run_id }))
        .await
        .unwrap();
    assert_eq!(approved["state"], json!("completed"));

    let bundle = client
        .call("spec.get", json!({ "run_id": run_id }))
        .await
        .unwrap();
    for gate in ["spec", "plan", "task"] {
        assert!(
            bundle["artifacts"][gate]
                .as_str()
                .is_some_and(|artifact: &str| artifact.contains("Bump one() to 2")),
            "the {gate} artifact must record the task"
        );
    }

    // --- 7. commit the edit --------------------------------------------------
    client
        .call(
            "git.stage",
            json!({ "project_id": id, "paths": ["src/lib.rs"], "actor": "user" }),
        )
        .await
        .unwrap();
    let committed = client
        .call(
            "git.commit",
            json!({ "project_id": id, "message": "Bump one() to 2", "actor": "user" }),
        )
        .await
        .unwrap();
    assert_eq!(committed["commit"]["summary"], json!("Bump one() to 2"));

    let log = client
        .call("git.log", json!({ "project_id": id, "limit": 10 }))
        .await
        .unwrap();
    assert_eq!(log["commits"].as_array().unwrap().len(), 1);
    assert_eq!(log["commits"][0]["id"], committed["commit"]["id"]);

    let status = client
        .call("git.status", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(status["status"]["clean"], json!(true));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
