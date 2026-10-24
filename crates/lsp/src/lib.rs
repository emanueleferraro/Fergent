//! Fergents LSP subsystem: a framed JSON-RPC client over stdio, the global
//! server registry, per-project sessions, document sync, requests, and
//! diagnostics (task 3.3).

mod client;
mod codec;
mod error;
mod model;
mod registry;
mod service;

pub use client::{LspClient, NotificationHandler};
pub use codec::{read_message, write_message};
pub use error::LspError;
pub use model::{Diagnostic, ServerRecord, ServerStatus};
pub use registry::{command_on_path, default_command, file_uri, language_for_path};
pub use service::LspService;
