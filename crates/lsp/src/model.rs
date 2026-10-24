//! LSP model types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The lifecycle state of a language server (spec Appendix A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    /// The executable is not on `PATH`.
    NotInstalled,
    /// The process is starting or handshaking.
    Starting,
    /// The server answered `initialize`.
    Ready,
    /// The server crashed too often and will not be restarted.
    Failed,
    /// Configured but not running.
    Stopped,
}

impl ServerStatus {
    /// The wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// A registered language-server configuration (spec Appendix B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRecord {
    /// The registry id (`""` for a built-in default that is not persisted).
    pub id: String,
    /// The language id (`rust`, `typescript`, ...).
    pub language_id: String,
    /// The executable.
    pub command: String,
    /// Arguments passed to the executable.
    pub args: Vec<String>,
    /// Extra environment variables (scrubbed before use).
    pub env: BTreeMap<String, String>,
    /// How the root URI is chosen (`project`).
    pub root_strategy: String,
    /// `builtin`, `user`, or `plugin`.
    pub source: String,
    /// Whether the server may start.
    pub enabled: bool,
    /// Current lifecycle state.
    pub status: ServerStatus,
    /// Whether `command` resolves on `PATH`.
    pub installed: bool,
}

/// One diagnostic published by a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Project-relative path.
    pub path: String,
    /// Zero-based start line.
    pub line: u32,
    /// Zero-based start character.
    pub column: u32,
    /// Zero-based end line.
    pub end_line: u32,
    /// Zero-based end character.
    pub end_column: u32,
    /// LSP severity (1 = error, 2 = warning, 3 = info, 4 = hint).
    pub severity: u8,
    /// The message.
    pub message: String,
    /// The producing tool, when given.
    pub source: Option<String>,
    /// The diagnostic code, when given.
    pub code: Option<String>,
}
