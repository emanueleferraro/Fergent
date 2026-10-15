//! The Fergents kernel daemon (`fergentsd`).

mod config;
mod logs;

use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

use clap::{Parser, Subcommand};
use config::GlobalConfig;
use fergents_api::{KernelClient, ServerHandle, SocketPath};
use fergents_core::{default_data_dir, find_migrations_dir, Kernel, KernelConfig};
use fergents_protocol::HealthSnapshot;
use fergents_supervisor::{shutdown_signal, PidFile};
use tokio::sync::watch;
use tracing_subscriber::prelude::*;

/// The Fergents kernel daemon.
#[derive(Parser, Debug)]
#[command(name = "fergentsd", version, about = "The Fergents kernel daemon.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the kernel daemon (detached by default).
    Start {
        /// Run in the current process instead of detaching.
        #[arg(long)]
        foreground: bool,
        /// Internal: run the daemon loop (used by the detached child).
        #[arg(long, hide = true)]
        run: bool,
    },
    /// Stop a running daemon.
    Stop,
    /// Show daemon status.
    Status,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("error: failed to start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let data_dir = default_data_dir();
    let socket = SocketPath::default_for(&data_dir);

    match cli.command {
        Command::Start { foreground, run } => {
            if !foreground && !run {
                spawn_detached(&data_dir)
            } else {
                run_daemon(&data_dir, socket).await
            }
        }
        Command::Stop => stop(&data_dir, &socket).await,
        Command::Status => status(&data_dir, &socket).await,
    }
}

/// Resolve the migrations directory: `$FERGENTS_MIGRATIONS_DIR`, then walk up
/// from the current directory and the executable path.
fn resolve_migrations_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("FERGENTS_MIGRATIONS_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return Ok(path);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(dir) = find_migrations_dir(&cwd) {
            return Ok(dir);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = find_migrations_dir(&exe) {
            return Ok(dir);
        }
    }
    anyhow::bail!("could not locate the `migrations/` directory; set FERGENTS_MIGRATIONS_DIR")
}

/// Spawn a detached child running `start --run`, then report readiness.
fn spawn_detached(data_dir: &std::path::Path) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let logs = data_dir.join("logs");
    std::fs::create_dir_all(&logs)?;
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("fergentsd.stdout.log"))?;
    let err = out.try_clone()?;
    let mut command = std::process::Command::new(exe);
    command
        .args(["start", "--run"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    detach(&mut command);
    let child = command.spawn()?;
    println!("fergentsd spawned (pid {})", child.id());
    Ok(())
}

/// Detach the child from the parent's session so it survives the parent's
/// terminal closing.
///
/// Unix: create a new session with `setsid` (no controlling terminal), so the
/// child is not killed by `SIGHUP` when the parent shell exits. Windows: start
/// a detached process in its own process group.
#[cfg(unix)]
fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` is async-signal-safe, and the closure runs in the child
    // after `fork` and before `exec`, where only async-signal-safe calls are
    // permitted.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn detach(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

async fn run_daemon(data_dir: &std::path::Path, socket: SocketPath) -> anyhow::Result<()> {
    let migrations_dir = resolve_migrations_dir()?;
    let global_config = GlobalConfig::load(data_dir)?;
    let _guard = init_logging(data_dir)?;

    // Prune old logs now, then once per day for the daemon's lifetime.
    logs::prune_on_startup(&data_dir.join("logs"), &global_config.log);

    // Single-instance enforcement: if a kernel already answers, refuse to start.
    if KernelClient::connect(&socket).await.is_ok() {
        let pid = PidFile::in_dir(data_dir).read().unwrap_or(0);
        anyhow::bail!("another instance is running (PID {pid})");
    }

    std::fs::create_dir_all(data_dir)?;
    let pid_file = PidFile::in_dir(data_dir);

    let kernel = Kernel::start(&KernelConfig {
        data_dir: data_dir.to_path_buf(),
        migrations_dir,
        event_capacity: 4096,
    })
    .map_err(|e| anyhow::anyhow!("failed to start kernel: {e}"))?;

    pid_file.write_current()?;

    let (shutdown_tx, _) = watch::channel(false);
    let _prune_task = tokio::spawn(logs::run_daily_prune(
        data_dir.join("logs"),
        global_config.log.clone(),
        shutdown_tx.subscribe(),
    ));
    let server = ServerHandle::spawn(
        kernel.clone(),
        socket.clone(),
        shutdown_tx.clone(),
        shutdown_tx.subscribe(),
    );

    tracing::info!(socket = %socket.path, pid = std::process::id(), "fergentsd started");

    let mut wait_rx = shutdown_tx.subscribe();
    tokio::select! {
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received");
        }
        _ = wait_for_shutdown(&mut wait_rx) => {
            tracing::info!("shutdown requested by client");
        }
    }

    kernel
        .announce_stopping()
        .map_err(|e| anyhow::anyhow!("failed to announce stopping: {e}"))?;
    let _ = shutdown_tx.send(true);
    let _ = server.join().await;
    pid_file.remove()?;
    tracing::info!("fergentsd stopped");
    Ok(())
}

async fn wait_for_shutdown(rx: &mut watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}

async fn stop(data_dir: &std::path::Path, socket: &SocketPath) -> anyhow::Result<()> {
    match KernelClient::connect(socket).await {
        Ok(client) => {
            let pid = PidFile::in_dir(data_dir).read().unwrap_or(0);
            let _ = client.call("kernel.shutdown", serde_json::json!({})).await;
            println!("stopping (pid {pid})");
        }
        Err(_) => {
            println!("not running");
        }
    }
    Ok(())
}

async fn status(data_dir: &std::path::Path, socket: &SocketPath) -> anyhow::Result<()> {
    let pid_file = PidFile::in_dir(data_dir);
    match KernelClient::connect(socket).await {
        Ok(client) => {
            let health: HealthSnapshot = client.call_typed("system.health", &()).await?;
            let pid = pid_file
                .read()
                .map_or_else(|| "?".to_string(), |p| p.to_string());
            println!(
                "running (pid {pid}) status={} storage={} uptime={}ms kernel={}",
                health.status, health.storage, health.uptime_ms, health.kernel_version
            );
        }
        Err(_) => {
            if pid_file.read().is_some() {
                pid_file.remove()?;
                println!("not running (stale pid file removed)");
            } else {
                println!("not running");
            }
        }
    }
    Ok(())
}

/// Initialize tracing: JSON logs to a daily-rotating file under
/// `<data_dir>/logs/`, and to stderr as well.
fn init_logging(
    data_dir: &std::path::Path,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    let logs = data_dir.join("logs");
    std::fs::create_dir_all(&logs)?;
    let file_appender = tracing_appender::rolling::daily(&logs, "fergentsd.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking);
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let subscriber = tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer);
    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(Some(guard))
}
