//! CLI integration tests: command handlers against a running kernel and a TUI
//! render smoke test (SubTask 8.4).

use std::path::Path;
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_cli::{commands, tui};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::KERNEL_VERSION;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tokio::sync::watch;

/// A unique, platform-appropriate address for a test endpoint.
///
/// Unix uses a socket file under `dir`; Windows uses a unique named pipe
/// (a filesystem path is not a valid Windows endpoint).
fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(dir.join("t.sock").to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-cli-test-{}-{index}",
            std::process::id()
        ))
    }
}

/// Start a kernel + server on a temp socket and wait until it accepts.
async fn start_kernel() -> (tempfile::TempDir, SocketPath, ServerHandle) {
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
    let handle = ServerHandle::spawn(kernel, socket.clone(), tx, rx);
    for _ in 0..100 {
        if connect_stream(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (dir, socket, handle)
}

/// Shut a running kernel down and join the server task.
async fn shutdown(socket: &SocketPath, handle: ServerHandle) {
    if let Ok(client) = KernelClient::connect(socket).await {
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
    }
    handle.join().await.unwrap();
}

#[tokio::test]
async fn status_returns_kernel_health() {
    let (_dir, socket, handle) = start_kernel().await;
    let health = commands::status(&socket).await.unwrap();
    assert_eq!(health.kernel_version, KERNEL_VERSION);
    shutdown(&socket, handle).await;
}

#[tokio::test]
async fn brain_list_is_empty_in_phase_0() {
    let (_dir, socket, handle) = start_kernel().await;
    let result = commands::brain_list(&socket, false).await.unwrap();
    assert!(result.brains.is_empty());
    shutdown(&socket, handle).await;
}

#[test]
fn tui_renders_placeholder_panel() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| tui::draw(frame, None, &[])).unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(rendered.contains("Workspace"), "placeholder panel missing");
    assert!(rendered.contains("fergentsd"), "header missing");
    assert!(rendered.contains("q / Esc: quit"), "footer missing");
}
