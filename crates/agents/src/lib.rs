//! Fergents agent runtime (`AGENTS.md`).
//!
//! Agent definitions are files (`agents/<id>.md`); this crate parses and
//! validates them, executes the ReAct loop over a [`fergents_llm::Provider`],
//! runs tool calls through a permission gate, and persists sessions and
//! messages. Memory tiers (task 2.5), the context builder (2.6), the
//! spec/plan/tasks gates (2.7), the full permission engine (2.8), and
//! reversibility (2.9) build on top of this crate.

mod context;
mod definition;
mod error;
mod files;
mod memory;
mod memory_tools;
mod permissions;
mod runtime;
mod service;
mod session;
mod store;
mod tool;
mod workflow;

pub use context::{
    ContextBuilder, ContextSection, ContextSettings, ContextSnapshot, SectionInput, SectionKind,
    DEFAULT_BUDGET,
};
pub use definition::{valid_id, AgentDefinition, Limits, LoopKind, MemoryConfig, SubAgentsConfig};
pub use error::AgentError;
pub use files::{agent_dir, brain_path, read_brain, session_path, write_brain, write_session};
pub use memory::{MemoryEntry, MemoryKind, MemoryScope, MemoryStore};
pub use memory_tools::{memory_tools, ForgetTool, ListTool, RecallTool, RememberTool};
pub use permissions::{AllowAllGate, DenyListGate, PermissionDecision, PermissionGate};
pub use runtime::{
    AgentRuntime, NoopObserver, RecordingObserver, RunObserver, RunOutcome, StopReason,
    ToolInvocation,
};
pub use service::AgentService;
pub use session::{AgentMessageRecord, AgentSessionRecord, RunState};
pub use store::AgentStore;
pub use tool::{Tool, ToolFuture, ToolRegistry};
pub use workflow::{Approval, AutonomyMode, Gate, StartWorkflow, WorkflowRun, WorkflowService};
