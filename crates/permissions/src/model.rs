//! Permission model types: modes, categories, decisions, and requests.

use serde::{Deserialize, Serialize};

/// The session permission mode (`ARCHITECTURE.md` §9.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Every action prompts.
    Manual,
    /// Only risky actions prompt (the default).
    #[default]
    Auto,
    /// No action prompts (policy denials still apply).
    Full,
}

impl Mode {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Manual => "manual",
            Mode::Auto => "auto",
            Mode::Full => "full",
        }
    }

    /// Parse a label, defaulting to [`Mode::Auto`].
    pub fn parse(value: &str) -> Self {
        match value {
            "manual" => Mode::Manual,
            "full" => Mode::Full,
            _ => Mode::Auto,
        }
    }
}

/// A permission category (`SECURITY.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Reading files.
    FsRead,
    /// Writing files.
    FsWrite,
    /// Running shell commands.
    ShellExec,
    /// Outbound HTTP.
    NetHttp,
    /// Peer-to-peer networking.
    NetP2p,
    /// Calling an LLM provider.
    LlmCall,
    /// Connecting to an MCP server.
    McpConnect,
    /// Installing a plugin.
    PluginInstall,
    /// Installing or changing a skill.
    SkillInstall,
    /// Invoking a skill (including chaining).
    SkillInvoke,
    /// Spawning a sub-agent.
    AgentSpawn,
    /// Editing a workflow.
    WorkflowEdit,
    /// Writing to a brain.
    BrainWrite,
    /// Changing kernel configuration.
    SystemConfig,
}

/// The permission categories and their tool-name inference.
impl Category {
    /// Every category.
    pub const ALL: [Category; 14] = [
        Category::FsRead,
        Category::FsWrite,
        Category::ShellExec,
        Category::NetHttp,
        Category::NetP2p,
        Category::LlmCall,
        Category::McpConnect,
        Category::PluginInstall,
        Category::SkillInstall,
        Category::SkillInvoke,
        Category::AgentSpawn,
        Category::WorkflowEdit,
        Category::BrainWrite,
        Category::SystemConfig,
    ];

    /// The persisted label (`fs.read`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            Category::FsRead => "fs.read",
            Category::FsWrite => "fs.write",
            Category::ShellExec => "shell.exec",
            Category::NetHttp => "net.http",
            Category::NetP2p => "net.p2p",
            Category::LlmCall => "llm.call",
            Category::McpConnect => "mcp.connect",
            Category::PluginInstall => "plugin.install",
            Category::SkillInstall => "skill.install",
            Category::SkillInvoke => "skill.invoke",
            Category::AgentSpawn => "agent.spawn",
            Category::WorkflowEdit => "workflow.edit",
            Category::BrainWrite => "brain.write",
            Category::SystemConfig => "system.config",
        }
    }

    /// Parse a label.
    pub fn parse(value: &str) -> Option<Category> {
        Category::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
    }

    /// Whether the category prompts in [`Mode::Auto`].
    pub fn is_risky(self) -> bool {
        matches!(
            self,
            Category::FsWrite
                | Category::ShellExec
                | Category::NetHttp
                | Category::NetP2p
                | Category::McpConnect
                | Category::PluginInstall
                | Category::SkillInstall
                | Category::SystemConfig
        )
    }

    /// Infer the category of a tool call from its name.
    pub fn for_tool(tool: &str) -> Category {
        match tool {
            "fs.write" | "fs.append" | "fs.delete" | "fs.move" => Category::FsWrite,
            tool if tool.starts_with("fs.") => Category::FsRead,
            "shell.exec" | "shell.spawn" => Category::ShellExec,
            "mcp.call" => Category::McpConnect,
            "brain.write" | "brain.create_note" | "brain.link" | "brain.tag" => {
                Category::BrainWrite
            }
            tool if tool.starts_with("memory.") => Category::BrainWrite,
            "agent.spawn" | "agent.call" | "agent.handoff" | "agent.message" => {
                Category::AgentSpawn
            }
            tool if tool.starts_with("workflow.") => Category::WorkflowEdit,
            tool if tool.starts_with("llm.") => Category::LlmCall,
            tool if tool.starts_with("plugin.") => Category::PluginInstall,
            "skill.invoke" => Category::SkillInvoke,
            tool if tool.starts_with("skill.") => Category::SkillInstall,
            "factory.build" => Category::ShellExec,
            "factory.install" => Category::PluginInstall,
            "factory.report" => Category::FsRead,
            tool if tool.starts_with("factory.") => Category::FsWrite,
            tool if tool.starts_with("web.") => Category::NetHttp,
            _ => Category::SystemConfig,
        }
    }
}

impl Serialize for Category {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Category {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Category::parse(&value).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown permission category {value:?}; expected one of fs.read, fs.write, \
                 shell.exec, net.http, net.p2p, llm.call, mcp.connect, plugin.install, \
                 skill.install, skill.invoke, agent.spawn, workflow.edit, brain.write, \
                 system.config"
            ))
        })
    }
}

/// A permission decision, ordered from least to most strict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// The action may proceed.
    Allow,
    /// The user must approve it.
    Prompt,
    /// The action is refused.
    Deny,
}

impl Decision {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Prompt => "prompt",
            Decision::Deny => "deny",
        }
    }

    /// Parse a label (also accepting `once`/`session` grants as allow).
    pub fn parse(value: &str) -> Option<Decision> {
        match value {
            "allow" | "once" | "session" => Some(Decision::Allow),
            "prompt" => Some(Decision::Prompt),
            "deny" => Some(Decision::Deny),
            _ => None,
        }
    }

    /// The stricter of two decisions.
    pub fn strictest(self, other: Decision) -> Decision {
        if other > self {
            other
        } else {
            self
        }
    }
}

/// Strictness ordering helper (mirrors [`Decision`]).
pub type Strictness = Decision;

/// A tool call awaiting a permission decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// The tool being called.
    pub tool: String,
    /// The call's arguments.
    pub arguments: serde_json::Value,
    /// Optional human-readable target (a path, a URL, a server id).
    pub target: Option<String>,
    /// The originating session.
    pub session_id: Option<String>,
    /// The acting agent.
    pub agent_id: Option<String>,
}

impl PermissionRequest {
    /// Build a request for `tool` with `arguments`.
    pub fn new(tool: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            arguments,
            target: None,
            session_id: None,
            agent_id: None,
        }
    }

    /// Attach a session.
    pub fn in_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Attach an agent.
    pub fn by_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// The inferred category.
    pub fn category(&self) -> Category {
        Category::for_tool(&self.tool)
    }
}

/// The resolved decision for a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolved {
    /// The final decision.
    pub decision: Decision,
    /// The category that was checked.
    pub category: Category,
    /// Why the decision was reached (policy rule note, mode, or override).
    pub reason: String,
    /// The permission-card payload (present when the decision is `Prompt`).
    pub card: Option<PermissionCard>,
}

/// The payload the UI renders in a permission card (`UI.md` §12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionCard {
    /// What the agent wants to do.
    pub action: String,
    /// The target of the action.
    pub target: Option<String>,
    /// Why it wants to.
    pub reason: String,
    /// The category label.
    pub category: String,
    /// The available choices.
    pub choices: Vec<String>,
}

/// The result of executing (or refusing) a call after resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionResult {
    /// The request that was checked.
    pub request: PermissionRequest,
    /// The resolution.
    pub resolved: Resolved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_tools_map_to_their_action_category() {
        assert_eq!(Category::for_tool("factory.build"), Category::ShellExec);
        assert_eq!(
            Category::for_tool("factory.install"),
            Category::PluginInstall
        );
        assert_eq!(Category::for_tool("factory.report"), Category::FsRead);
        assert_eq!(Category::for_tool("factory.scaffold"), Category::FsWrite);
        assert_eq!(Category::for_tool("factory.test"), Category::FsWrite);
        assert_eq!(Category::for_tool("factory.package"), Category::FsWrite);
    }
}
