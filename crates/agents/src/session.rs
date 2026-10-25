//! Session and message records (`agent_sessions`, `agent_messages`).

use serde::{Deserialize, Serialize};

/// The lifecycle state of an agent run (`AGENTS.md` §14.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// The session exists; no run has started.
    #[default]
    Created,
    /// A run is in progress.
    Running,
    /// The run was paused (budget/user).
    Paused,
    /// The run is waiting for a permission decision.
    WaitingPermission,
    /// The run finished successfully.
    Done,
    /// The run failed.
    Error,
    /// The user cancelled the run.
    Cancelled,
}

impl RunState {
    /// A stable, machine-readable label.
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Created => "created",
            RunState::Running => "running",
            RunState::Paused => "paused",
            RunState::WaitingPermission => "waiting_permission",
            RunState::Done => "done",
            RunState::Error => "error",
            RunState::Cancelled => "cancelled",
        }
    }

    /// Parse a state label, defaulting to [`RunState::Created`] when unknown.
    pub fn parse(value: &str) -> Self {
        match value {
            "running" => RunState::Running,
            "paused" => RunState::Paused,
            "waiting_permission" => RunState::WaitingPermission,
            "done" => RunState::Done,
            "error" => RunState::Error,
            "cancelled" => RunState::Cancelled,
            _ => RunState::Created,
        }
    }
}

/// A persisted agent session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionRecord {
    /// The session id.
    pub id: String,
    /// The agent that owns the session.
    pub agent_id: String,
    /// A human-readable title.
    pub title: Option<String>,
    /// The model pinned for this session, if any.
    pub model: Option<String>,
    /// The current state.
    pub state: RunState,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
    /// The rolling summary, when compaction has run.
    pub summary: Option<String>,
}

/// A persisted message in a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessageRecord {
    /// The message id.
    pub id: String,
    /// The owning session.
    pub session_id: String,
    /// The role label (`user`, `assistant`, `tool`).
    pub role: String,
    /// The message content.
    pub content: String,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_via_str() {
        for state in [
            RunState::Created,
            RunState::Running,
            RunState::Paused,
            RunState::WaitingPermission,
            RunState::Done,
            RunState::Error,
            RunState::Cancelled,
        ] {
            assert_eq!(RunState::parse(state.as_str()), state);
        }
        assert_eq!(RunState::parse("nonsense"), RunState::Created);
    }
}
