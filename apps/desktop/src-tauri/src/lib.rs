//! Fergents desktop shell: the Tauri v2 backend.
//!
//! A webview cannot open the kernel's local socket (a Windows named pipe or a
//! Unix domain socket), so this crate proxies the kernel protocol:
//!
//! - `kernel_call` / `kernel_health` open a short-lived connection and issue a
//!   JSON-RPC command, returning the `result`.
//! - `kernel_ensure_running` connects, and if no kernel answers, spawns the
//!   `fergentsd` **sidecar** (`tauri.conf.json` → `bundle.externalBin`).
//! - `kernel_subscribe` opens a long-lived connection, subscribes to topics,
//!   and forwards every kernel event to the webview as a `kernel://event`
//!   Tauri event.
//!
//! ## Producing the sidecar binary
//!
//! Tauri requires the sidecar file to be named `fergentsd-<target-triple>` in
//! `src-tauri/binaries/`. Build the daemon and copy it, e.g.:
//!
//! ```text
//! cargo build -p fergentsd
//! cp target/debug/fergentsd apps/desktop/src-tauri/binaries/fergentsd-<triple>
//! ```
//!
//! where `<triple>` is the host target triple (`rustc -vV | grep host`), for
//! example `x86_64-unknown-linux-gnu` or `x86_64-pc-windows-msvc`. The bundler
//! strips the triple suffix on packaging.
//!
//! ## Detachment
//!
//! The sidecar is launched as `fergentsd start`, which forks a detached daemon
//! and exits (see `crates/fergentsd`). The app therefore never owns the kernel
//! lifecycle: closing the desktop app leaves the kernel running.

use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, SocketPath};
use fergents_core::default_data_dir;
use fergents_protocol::{HealthSnapshot, KERNEL_VERSION};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;

/// `client_kind` reported to the kernel during the `hello` handshake.
const CLIENT_KIND: &str = "desktop";
/// Tauri event name carrying a forwarded kernel event.
const KERNEL_EVENT: &str = "kernel://event";
/// Sidecar binary name (declared in `tauri.conf.json`).
const SIDECAR: &str = "fergentsd";
/// How long to wait for a freshly spawned kernel to become reachable.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll interval while waiting for the kernel socket to accept connections.
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// Shared state for the Tauri commands.
struct AppState {
    socket: SocketPath,
}

impl AppState {
    fn new() -> Self {
        let data_dir = default_data_dir();
        Self {
            socket: SocketPath::default_for(&data_dir),
        }
    }
}

/// Connect and complete the `hello` handshake as the desktop client.
async fn connect(socket: &SocketPath) -> Result<KernelClient, String> {
    KernelClient::connect_as(socket, CLIENT_KIND, KERNEL_VERSION)
        .await
        .map_err(|error| format!("kernel not reachable: {error}"))
}

/// Whether the kernel socket currently accepts a connection.
async fn is_reachable(socket: &SocketPath) -> bool {
    connect_stream(socket).await.is_ok()
}

/// Spawn the `fergentsd` sidecar; the daemon detaches itself.
fn spawn_sidecar(app: &AppHandle) -> Result<(), String> {
    let command = app.shell().sidecar(SIDECAR).map_err(|e| e.to_string())?;
    let (mut events, child) = command.args(["start"]).spawn().map_err(|e| e.to_string())?;
    // Drain the launcher's output until it exits. Keeping the child alive here
    // avoids killing the launcher before it has forked the detached daemon; the
    // daemon itself is not a child of this process and survives app exit.
    tauri::async_runtime::spawn(async move {
        let _child = child;
        while events.recv().await.is_some() {}
    });
    Ok(())
}

/// Return the current kernel health snapshot.
#[tauri::command]
async fn kernel_health(state: State<'_, AppState>) -> Result<HealthSnapshot, String> {
    let client = connect(&state.socket).await?;
    client
        .call_typed("system.health", &())
        .await
        .map_err(|e| e.to_string())
}

/// Send an arbitrary kernel command and return its JSON result.
#[tauri::command]
async fn kernel_call(
    state: State<'_, AppState>,
    method: String,
    params: Value,
) -> Result<Value, String> {
    let client = connect(&state.socket).await?;
    client
        .call(&method, params)
        .await
        .map_err(|e| e.to_string())
}

/// Ensure a kernel is reachable, spawning the sidecar if necessary.
///
/// Returns `true` when a kernel was spawned, `false` when one was already
/// running.
#[tauri::command]
async fn kernel_ensure_running(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    if is_reachable(&state.socket).await {
        return Ok(false);
    }
    spawn_sidecar(&app)?;

    let deadline = tokio::time::Instant::now() + SPAWN_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if is_reachable(&state.socket).await {
            return Ok(true);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err("kernel did not become reachable after spawning".to_string())
}

/// Subscribe to event topics and forward them to the webview.
#[tauri::command]
async fn kernel_subscribe(
    app: AppHandle,
    state: State<'_, AppState>,
    topics: Vec<String>,
) -> Result<(), String> {
    let mut client = connect(&state.socket).await?;
    client.subscribe(&topics).await.map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = client.next_event().await {
            let _ = app.emit(KERNEL_EVENT, event);
        }
    });
    Ok(())
}

/// Build and run the desktop application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            kernel_health,
            kernel_call,
            kernel_ensure_running,
            kernel_subscribe
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Fergents desktop app");
}
