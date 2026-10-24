//! Terminal session model (spec Appendix B).

use serde::{Deserialize, Serialize};

/// The lifecycle state of a terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The shell process is running.
    Running,
    /// The shell exited; the scrollback is still readable.
    Exited,
    /// The session was closed and its scrollback dropped.
    Closed,
}

impl SessionState {
    /// The wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Closed => "closed",
        }
    }

    /// Parse the persisted representation (fail closed: unknown values are
    /// treated as closed).
    pub fn parse(raw: &str) -> Self {
        match raw {
            "running" => Self::Running,
            "exited" => Self::Exited,
            _ => Self::Closed,
        }
    }
}

/// One terminal session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSession {
    /// The session id (ULID).
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// The shell executable.
    pub shell: String,
    /// The working directory.
    pub cwd: String,
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
    /// Lifecycle state.
    pub state: SessionState,
    /// The exit code, once the shell exited.
    pub exit_code: Option<i64>,
    /// `user` or `agent:<id>`.
    pub actor: String,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Close time (ms since the epoch).
    pub closed_at: Option<i64>,
}
