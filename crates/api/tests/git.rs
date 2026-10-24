//! The git surface over the wire: status, staging, commits, branches, and the
//! `capability.git` / `capability.git-remote` dispatch guards (task 3.5).

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
        SocketPath::from_path(dir.join("git.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-git-{}-{index}",
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

async fn open_project(client: &KernelClient, dir: &Path, trusted: bool) -> String {
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    repository(&workspace);
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

fn assert_code(err: fergents_api::ApiError, expected: ErrorCode) {
    match err {
        fergents_api::ApiError::Rpc { code, .. } => assert_eq!(code, expected.as_i32()),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn git_review_stage_and_commit_over_ipc() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let workspace = dir.path().join("workspace");
    let id = open_project(&client, dir.path(), false).await;

    // Untrusted projects cannot read the repository.
    assert_code(
        client
            .call("git.status", json!({ "project_id": id }))
            .await
            .unwrap_err(),
        ErrorCode::ProjectUntrusted,
    );

    client
        .call(
            "project.trust",
            json!({ "project_id": id, "trusted": true }),
        )
        .await
        .unwrap();

    std::fs::write(workspace.join("a.txt"), "one\n").unwrap();
    let status = client
        .call("git.status", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(status["status"]["clean"], json!(false));
    assert_eq!(status["status"]["entries"][0]["kind"], json!("untracked"));

    let staged = client
        .call(
            "git.stage",
            json!({ "project_id": id, "paths": ["a.txt"], "actor": "user" }),
        )
        .await
        .unwrap();
    assert_eq!(staged["paths"], json!(1));

    let committed = client
        .call(
            "git.commit",
            json!({ "project_id": id, "message": "initial commit", "actor": "user" }),
        )
        .await
        .unwrap();
    assert_eq!(committed["commit"]["summary"], json!("initial commit"));
    assert_eq!(committed["commit"]["trailers"], json!([]));
    let first = committed["commit"]["id"].as_str().unwrap().to_string();

    std::fs::write(workspace.join("a.txt"), "one\ntwo\n").unwrap();
    let diff = client
        .call("git.diff", json!({ "project_id": id, "path": "a.txt" }))
        .await
        .unwrap();
    assert_eq!(diff["files"][0]["path"], json!("a.txt"));
    assert!(diff["files"][0]["hunks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hunk| {
            hunk["lines"]
                .as_array()
                .unwrap()
                .iter()
                .any(|line| line["kind"] == json!("add") && line["content"] == json!("two"))
        }));

    client
        .call(
            "git.stage",
            json!({ "project_id": id, "paths": ["a.txt"], "actor": "user" }),
        )
        .await
        .unwrap();
    let agent = client
        .call(
            "git.commit",
            json!({
                "project_id": id,
                "message": "agent change",
                "actor": "agent:coder",
                "session_id": "01J9SESSION",
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        agent["commit"]["trailers"],
        json!([
            "Fergents-Agent: agent:coder",
            "Fergents-Session: 01J9SESSION"
        ])
    );

    let log = client
        .call("git.log", json!({ "project_id": id, "limit": 10 }))
        .await
        .unwrap();
    assert_eq!(log["commits"].as_array().unwrap().len(), 2);
    assert_eq!(log["commits"][1]["id"], json!(first));

    let show = client
        .call(
            "git.show",
            json!({ "project_id": id, "commit": agent["commit"]["id"] }),
        )
        .await
        .unwrap();
    assert!(show["message"]
        .as_str()
        .unwrap()
        .contains("Fergents-Agent: agent:coder"));

    let blame = client
        .call("git.blame", json!({ "project_id": id, "path": "a.txt" }))
        .await
        .unwrap();
    assert_eq!(blame["lines"].as_array().unwrap().len(), 2);

    // Branches.
    let branch = client
        .call(
            "git.branch.create",
            json!({ "project_id": id, "name": "feature", "checkout": true, "actor": "user" }),
        )
        .await
        .unwrap();
    assert_eq!(branch["branch"]["name"], json!("feature"));
    let branches = client
        .call("git.branches", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(branches["branches"].as_array().unwrap().len(), 2);
    assert!(branches["branches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|branch| branch["name"] == json!("feature") && branch["is_head"] == json!(true)));

    let conflicts = client
        .call("git.conflicts", json!({ "project_id": id }))
        .await
        .unwrap();
    assert_eq!(conflicts["conflicts"], json!([]));

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn git_remote_and_capability_guards() {
    let (dir, socket, handle, _kernel) = start_stack().await;
    let client = KernelClient::connect(&socket).await.unwrap();
    let id = open_project(&client, dir.path(), true).await;

    // `capability.git-remote` is off by default.
    assert_code(
        client
            .call("git.push", json!({ "project_id": id }))
            .await
            .unwrap_err(),
        ErrorCode::CapabilityDisabled,
    );

    // Disabling `capability.git` (after its dependent) closes the whole surface.
    client
        .call(
            "capability.set",
            json!({ "id": "capability.git-remote", "enabled": false }),
        )
        .await
        .unwrap();
    client
        .call(
            "capability.set",
            json!({ "id": "capability.git", "enabled": false }),
        )
        .await
        .unwrap();
    assert_code(
        client
            .call("git.status", json!({ "project_id": id }))
            .await
            .unwrap_err(),
        ErrorCode::CapabilityDisabled,
    );

    let _ = client.call("kernel.shutdown", json!({})).await;
    handle.join().await.unwrap();
}
