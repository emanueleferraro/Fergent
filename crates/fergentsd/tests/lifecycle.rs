//! Daemon lifecycle and end-to-end tests.
//!
//! These tests drive the real `fergentsd` binary (via `CARGO_BIN_EXE_fergentsd`)
//! on an isolated data dir and socket, covering start → connect → command →
//! event → shutdown, single-instance enforcement, stale-PID handling, and
//! graceful shutdown (SubTasks 7.6 and 10.1).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, SocketPath};
use fergents_protocol::{HealthSnapshot, KERNEL_VERSION, PROTOCOL_VERSION};
use tokio::time::{sleep, timeout, Instant};

/// The path to the freshly built daemon binary.
const BIN: &str = env!("CARGO_BIN_EXE_fergentsd");

/// A running daemon plus the environment it was started with.
struct Daemon {
    child: Child,
    home: tempfile::TempDir,
    socket: SocketPath,
}

impl Daemon {
    /// Start a daemon in the foreground on an isolated data dir and socket.
    async fn start() -> Daemon {
        let home = tempfile::tempdir().expect("temp home");
        let socket = unique_socket(home.path());
        let child = Command::new(BIN)
            .args(["start", "--run"])
            .env("FERGENTS_HOME", home.path())
            .env("FERGENTS_SOCKET", &socket.path)
            .env("FERGENTS_MIGRATIONS_DIR", migrations_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn fergentsd");

        let daemon = Daemon {
            child,
            home,
            socket,
        };
        assert!(
            wait_until_reachable(&daemon.socket, Duration::from_secs(15)).await,
            "daemon did not become reachable"
        );
        daemon
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A unique socket address for this test, isolated from other tests.
///
/// `home` only participates in the Unix socket path; Windows uses a named pipe
/// addressed by process id, so the parameter is unused there by design. It is
/// kept so the signature is identical on every platform.
fn unique_socket(home: &Path) -> SocketPath {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let index = COUNTER.fetch_add(1, Ordering::SeqCst);
    #[cfg(unix)]
    let path = home
        .join(format!("fergentsd-{index}.sock"))
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let path = {
        let _ = home;
        format!(r"\\.\pipe\fergentsd-test-{}-{index}", std::process::id())
    };
    SocketPath::from_path(path)
}

/// The repository `migrations/` directory.
fn migrations_dir() -> PathBuf {
    fergents_core::find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("locate migrations directory")
}

/// Poll the socket until it accepts a connection or the deadline passes.
async fn wait_until_reachable(socket: &SocketPath, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if connect_stream(socket).await.is_ok() {
            return true;
        }
        sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Poll the child until it exits or the deadline passes.
async fn wait_for_exit(child: &mut Child, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(50)).await;
    }
}

/// Poll the socket until it refuses connections or the deadline passes.
async fn wait_until_unreachable(socket: &SocketPath, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if connect_stream(socket).await.is_err() {
            return true;
        }
        sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Build a `Command` for a one-shot `fergentsd` invocation in `home`.
fn daemon_command(home: &Path, socket: &SocketPath, args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .env("FERGENTS_HOME", home)
        .env("FERGENTS_SOCKET", &socket.path)
        .env("FERGENTS_MIGRATIONS_DIR", migrations_dir());
    command
}

#[tokio::test]
async fn end_to_end_start_connect_command_event_shutdown() {
    let mut daemon = Daemon::start().await;

    let mut client = KernelClient::connect(&daemon.socket).await.unwrap();
    let health: HealthSnapshot = client.call_typed("system.health", &()).await.unwrap();
    assert_eq!(health.kernel_version, KERNEL_VERSION);
    assert_eq!(health.protocol_version, PROTOCOL_VERSION);
    assert!(!client.session_id().is_empty());

    // Subscribe, then ask the kernel to stop: the shutdown path emits
    // `kernel.stopping`, which the client must observe before the socket closes.
    client.subscribe(&["kernel.".to_string()]).await.unwrap();
    sleep(Duration::from_millis(100)).await;
    let _ = client.call("kernel.shutdown", serde_json::json!({})).await;

    let event = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("event timeout")
        .expect("event stream ended");
    assert_eq!(event.kind, "kernel.stopping");

    assert!(
        wait_for_exit(&mut daemon.child, Duration::from_secs(15)).await,
        "daemon did not exit after shutdown"
    );
}

#[tokio::test]
async fn second_instance_refuses_to_start() {
    let primary = Daemon::start().await;

    let output = daemon_command(primary.home.path(), &primary.socket, &["start", "--run"])
        .output()
        .expect("run second instance");

    assert!(!output.status.success(), "second instance should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("another instance"),
        "unexpected stderr: {stderr}"
    );
}

#[tokio::test]
async fn status_reports_not_running_and_clears_stale_pid() {
    let home = tempfile::tempdir().unwrap();
    let socket = unique_socket(home.path());
    let pid_file = home.path().join("fergentsd.pid");
    std::fs::write(&pid_file, "999999").unwrap();

    let output = daemon_command(home.path(), &socket, &["status"])
        .output()
        .expect("run status");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("not running"),
        "unexpected stdout: {stdout}"
    );
    assert!(!pid_file.exists(), "stale pid file should be removed");
}

#[tokio::test]
async fn status_reports_running_against_live_kernel() {
    let daemon = Daemon::start().await;

    let output = daemon_command(daemon.home.path(), &daemon.socket, &["status"])
        .output()
        .expect("run status");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("running"), "unexpected stdout: {stdout}");
    assert!(
        stdout.contains(KERNEL_VERSION),
        "unexpected stdout: {stdout}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn graceful_shutdown_on_sigterm() {
    let mut daemon = Daemon::start().await;

    let status = Command::new("kill")
        .args(["-TERM", &daemon.child.id().to_string()])
        .status()
        .expect("send SIGTERM");
    assert!(status.success());

    assert!(
        wait_for_exit(&mut daemon.child, Duration::from_secs(15)).await,
        "daemon did not exit on SIGTERM"
    );
    assert!(
        !daemon.home.path().join("fergentsd.pid").exists(),
        "pid file should be removed on graceful shutdown"
    );
}

#[tokio::test]
async fn detached_start_survives_spawner_exit() {
    let home = tempfile::tempdir().unwrap();
    let socket = unique_socket(home.path());

    // `start` (without `--run`/`--foreground`) spawns a detached child and
    // returns; `output()` waits for that spawner process to exit.
    let output = daemon_command(home.path(), &socket, &["start"])
        .output()
        .expect("run detached start");
    assert!(
        output.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The spawner has exited; the detached daemon must still become reachable.
    assert!(
        wait_until_reachable(&socket, Duration::from_secs(15)).await,
        "detached daemon did not come up after the spawner exited"
    );

    // Clean up: ask the daemon to shut down, then confirm it goes away.
    if let Ok(client) = KernelClient::connect(&socket).await {
        let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
    }
    assert!(
        wait_until_unreachable(&socket, Duration::from_secs(15)).await,
        "detached daemon did not stop"
    );
}

#[tokio::test]
async fn stop_terminates_running_daemon() {
    let mut daemon = Daemon::start().await;

    let output = daemon_command(daemon.home.path(), &daemon.socket, &["stop"])
        .output()
        .expect("run stop");
    assert!(
        output.status.success(),
        "stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("stopped"),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    assert!(
        wait_for_exit(&mut daemon.child, Duration::from_secs(15)).await,
        "daemon did not exit after stop"
    );
    assert!(
        !daemon.home.path().join("fergentsd.pid").exists(),
        "pid file should be removed after stop"
    );
}

#[tokio::test]
async fn stop_reports_not_running_when_absent() {
    let home = tempfile::tempdir().unwrap();
    let socket = unique_socket(home.path());

    let output = daemon_command(home.path(), &socket, &["stop"])
        .output()
        .expect("run stop");
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("not running"),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn status_reports_dead_process_as_not_running() {
    let mut daemon = Daemon::start().await;

    // Kill the daemon without a graceful shutdown, leaving a stale PID file.
    let killed = Command::new("kill")
        .args(["-KILL", &daemon.child.id().to_string()])
        .status()
        .expect("send SIGKILL");
    assert!(killed.success());
    assert!(wait_for_exit(&mut daemon.child, Duration::from_secs(15)).await);
    assert!(
        daemon.home.path().join("fergentsd.pid").exists(),
        "pid file should remain after an abrupt kill"
    );

    let output = daemon_command(daemon.home.path(), &daemon.socket, &["status"])
        .output()
        .expect("run status");
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("not running"),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !daemon.home.path().join("fergentsd.pid").exists(),
        "stale pid file should be removed by status"
    );
}
