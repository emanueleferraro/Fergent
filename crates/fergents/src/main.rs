//! The Fergents command-line interface and TUI.
//!
//! `fergents` connects to a running `fergentsd` kernel over the local socket
//! and exposes `status`, `events tail`, and `brain list`, plus an interactive
//! Ratatui shell with a placeholder panel (`ARCHITECTURE.md` §5.2).

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fergents_api::SocketPath;
use fergents_cli::{commands, tui};
use fergents_core::default_data_dir;

/// The Fergents CLI.
#[derive(Parser, Debug)]
#[command(
    name = "fergents",
    version,
    about = "The Fergents command-line interface."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the kernel health and state.
    Status,
    /// Event bus operations.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Brain operations.
    Brain {
        #[command(subcommand)]
        command: BrainCommand,
    },
    /// Launch the interactive TUI.
    Tui,
}

#[derive(Subcommand, Debug)]
enum EventsCommand {
    /// Stream events from the bus until interrupted.
    Tail,
}

#[derive(Subcommand, Debug)]
enum BrainCommand {
    /// List the registered brains.
    List,
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

    let data_dir = default_data_dir();
    let socket = SocketPath::default_for(&data_dir);

    let result = runtime.block_on(async {
        match cli.command {
            Some(Command::Status) => commands::status(&socket).await.map(|health| {
                println!(
                    "status={} storage={} uptime={}ms kernel={} protocol=v{}",
                    health.status,
                    health.storage,
                    health.uptime_ms,
                    health.kernel_version,
                    health.protocol_version
                );
            }),
            Some(Command::Events {
                command: EventsCommand::Tail,
            }) => commands::events_tail(&socket).await,
            Some(Command::Brain {
                command: BrainCommand::List,
            }) => commands::brain_list(&socket).await.map(|result| {
                if result.brains.is_empty() {
                    println!("no brains registered");
                } else {
                    for brain in result.brains {
                        match brain.role {
                            Some(role) => {
                                println!(
                                    "{} {} {} ({})",
                                    brain.brain_id, brain.name, brain.path, role
                                );
                            }
                            None => println!("{} {} {}", brain.brain_id, brain.name, brain.path),
                        }
                    }
                }
            }),
            Some(Command::Tui) | None => tui::run(&socket).await,
        }
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
