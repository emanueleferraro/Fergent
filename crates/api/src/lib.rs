//! Fergents local IPC transport and JSON-RPC server.
//!
//! Clients and the kernel talk over a local socket — a Windows named pipe
//! (`\\.\pipe\fergentsd`) or a Unix domain socket
//! (`$XDG_RUNTIME_DIR/fergentsd.sock`, falling back to
//! `~/.fergents/fergentsd.sock`) — using a JSON-RPC 2.0 protocol with
//! newline-delimited JSON messages (`ARCHITECTURE.md` §3.3, §3.4).

mod client;
mod error;
mod server;
mod transport;

pub use client::KernelClient;
pub use error::ApiError;
pub use server::{serve, ServerHandle};
pub use transport::{connect_stream, BoxStream, IpcListener, SocketPath};
