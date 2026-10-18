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
    /// Search the brains.
    Search {
        /// The query string.
        query: String,
        /// Maximum number of results.
        #[arg(long = "top-k")]
        top_k: Option<i64>,
        /// Search mode: bm25, vector, or hybrid.
        #[arg(long)]
        mode: Option<String>,
        /// Restrict the search to one brain id.
        #[arg(long)]
        brain: Option<String>,
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
    List {
        /// Include archived brains.
        #[arg(long)]
        all: bool,
    },
    /// Create a new brain.
    Create {
        /// Human-readable name.
        name: String,
        /// Filesystem path for the brain folder.
        #[arg(long)]
        path: Option<String>,
        /// Template to seed from (blank, personal, research, coding, team).
        #[arg(long)]
        template: Option<String>,
    },
    /// Open a brain by id.
    Open {
        /// The brain id.
        id: String,
    },
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
                command: BrainCommand::List { all },
            }) => commands::brain_list(&socket, all).await.map(|result| {
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
            Some(Command::Brain {
                command:
                    BrainCommand::Create {
                        name,
                        path,
                        template,
                    },
            }) => commands::brain_create(&socket, &name, path.as_deref(), template.as_deref())
                .await
                .map(|result| {
                    println!(
                        "created brain {} {} ({})",
                        result.brain.brain_id, result.brain.name, result.state
                    );
                }),
            Some(Command::Brain {
                command: BrainCommand::Open { id },
            }) => commands::brain_open(&socket, &id).await.map(|result| {
                println!(
                    "opened brain {} {} ({})",
                    result.brain.brain_id, result.brain.name, result.state
                );
            }),
            Some(Command::Search {
                query,
                top_k,
                mode,
                brain,
            }) => commands::search(&socket, &query, top_k, mode.as_deref(), brain.as_deref())
                .await
                .map(|result| {
                    if result.results.is_empty() {
                        println!("no results");
                    } else {
                        for hit in result.results {
                            println!("{:.4} {} {}", hit.score, hit.path, hit.title);
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
