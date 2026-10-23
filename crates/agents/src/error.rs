//! Errors raised by the agent runtime.

use fergents_events::EventError;
use fergents_llm::ProviderError;
use fergents_storage::StorageError;

/// Errors raised by definitions, tools, runs, and sessions.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] EventError),
    /// A provider failure.
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    /// The agent definition is malformed or fails validation.
    #[error("invalid agent definition: {0}")]
    InvalidDefinition(String),
    /// No agent is registered with this id.
    #[error("unknown agent: {0}")]
    UnknownAgent(String),
    /// No tool is registered with this name.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// A tool returned an error.
    #[error("tool error: {0}")]
    Tool(String),
    /// A tool call was refused by the permission gate.
    #[error("permission denied for tool {tool}: {reason}")]
    PermissionDenied {
        /// The tool that was refused.
        tool: String,
        /// Why it was refused.
        reason: String,
    },
    /// No session is registered with this id.
    #[error("unknown session: {0}")]
    UnknownSession(String),
    /// The agent is disabled (`agent.disable`).
    #[error("agent is disabled: {0}")]
    Disabled(String),
    /// The agent definition does not permit sub-agents.
    #[error("agent {0} does not allow sub-agents")]
    SubAgentsNotAllowed(String),
    /// The sub-agent limit for this run was reached.
    #[error("sub-agent limit reached for {agent} (max {limit})")]
    SubAgentLimit {
        /// The agent that tried to spawn.
        agent: String,
        /// The configured cap.
        limit: u32,
    },
    /// No workflow run is registered with this id.
    #[error("unknown run: {0}")]
    UnknownRun(String),
    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl AgentError {
    /// A stable, machine-readable wire code (snake_case), used in error data.
    pub fn code(&self) -> &'static str {
        match self {
            AgentError::Storage(_) => "storage_error",
            AgentError::Event(_) => "event_error",
            AgentError::Provider(_) => "provider_error",
            AgentError::InvalidDefinition(_) => "agent_invalid_definition",
            AgentError::UnknownAgent(_) => "agent_unknown",
            AgentError::UnknownTool(_) => "agent_unknown_tool",
            AgentError::Tool(_) => "agent_tool_error",
            AgentError::PermissionDenied { .. } => "agent_permission_denied",
            AgentError::UnknownSession(_) => "agent_unknown_session",
            AgentError::Disabled(_) => "agent_disabled",
            AgentError::SubAgentsNotAllowed(_) => "agent_subagents_not_allowed",
            AgentError::SubAgentLimit { .. } => "agent_subagent_limit",
            AgentError::UnknownRun(_) => "run_unknown",
            AgentError::Io(_) => "io_error",
            AgentError::Serde(_) => "serde_error",
        }
    }
}
