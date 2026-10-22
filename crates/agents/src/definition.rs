//! Agent definitions: the `agents/<id>.md` file format (`AGENTS.md` §4).
//!
//! A definition is YAML frontmatter followed by the Markdown system prompt.
//! This module parses, validates, and serializes it.

use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// The loop pattern an agent uses (`AGENTS.md` §5.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopKind {
    /// Reason → Act → Observe, repeated (default).
    #[default]
    React,
    /// Plan all steps, then execute.
    PlanExecute,
    /// Reflect after failure and retry with a new strategy.
    Reflexion,
    /// Explore multiple branches, prune, pick the best.
    Tot,
    /// Two agents argue, a third decides.
    Debate,
    /// Split, process in parallel, merge.
    MapReduce,
}

/// Per-run budgets (`AGENTS.md` §13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    /// Maximum total tokens (input + output) per run.
    #[serde(default = "default_max_tokens")]
    pub max_tokens_per_run: u64,
    /// Maximum spend in USD per run.
    #[serde(default = "default_max_cost")]
    pub max_cost_usd_per_run: f64,
    /// Maximum wall-clock seconds per run.
    #[serde(default = "default_max_wall_time")]
    pub max_wall_time_s: u64,
    /// Maximum tool calls per run.
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls_per_run: u32,
    /// Maximum sub-agents spawned per run.
    #[serde(default = "default_max_sub_agents")]
    pub max_sub_agents: u32,
    /// Maximum executor iterations per run.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_max_tokens() -> u64 {
    200_000
}
fn default_max_cost() -> f64 {
    2.0
}
fn default_max_wall_time() -> u64 {
    900
}
fn default_max_tool_calls() -> u32 {
    100
}
fn default_max_sub_agents() -> u32 {
    4
}
fn default_max_iterations() -> u32 {
    50
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tokens_per_run: default_max_tokens(),
            max_cost_usd_per_run: default_max_cost(),
            max_wall_time_s: default_max_wall_time(),
            max_tool_calls_per_run: default_max_tool_calls(),
            max_sub_agents: default_max_sub_agents(),
            max_iterations: default_max_iterations(),
        }
    }
}

/// Memory settings (`AGENTS.md` §8). The tiers themselves arrive in task 2.5.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Enable episodic memory.
    #[serde(default)]
    pub episodic: bool,
    /// Enable semantic (fact) memory.
    #[serde(default)]
    pub semantic: bool,
    /// Working-context window in tokens.
    #[serde(default)]
    pub working_window: Option<u64>,
    /// Summarize history at this fraction of the window.
    #[serde(default)]
    pub summarize_at: Option<f32>,
    /// Number of RAG chunks to inject per turn.
    #[serde(default)]
    pub rag_top_k: Option<u32>,
}

/// Sub-agent settings (`AGENTS.md` §9).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubAgentsConfig {
    /// Whether this agent may spawn sub-agents.
    #[serde(default)]
    pub allow: bool,
    /// Maximum nesting depth.
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Tools a spawned child may use by default.
    #[serde(default)]
    pub default_tools: Vec<String>,
}

/// A parsed agent definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Stable, unique id (lowercase `[a-z0-9-]`, first char alphanumeric).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// Semver-ish version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Preferred model id or route.
    pub model: String,
    /// Fallback models, tried in order.
    #[serde(default)]
    pub model_fallback: Vec<String>,
    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Output token cap.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Whether to use extended thinking when available.
    #[serde(default)]
    pub reasoning: bool,
    /// The loop pattern (`loop` in the frontmatter).
    #[serde(default, rename = "loop")]
    pub loop_kind: LoopKind,
    /// Tool names this agent may call.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Skill ids this agent may invoke.
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP server ids this agent may use.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// Per-run budgets.
    #[serde(default)]
    pub limits: Limits,
    /// Memory settings.
    #[serde(default)]
    pub memory: MemoryConfig,
    /// Sub-agent settings.
    #[serde(default)]
    pub sub_agents: SubAgentsConfig,
    /// Free-form categorization tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// UI icon (emoji or path).
    #[serde(default)]
    pub icon: Option<String>,
    /// UI accent color.
    #[serde(default)]
    pub color: Option<String>,
    /// The Markdown system prompt (the file body).
    #[serde(skip)]
    pub system_prompt: String,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

impl AgentDefinition {
    /// Parse a definition from a `agents/<id>.md` document.
    pub fn parse(markdown: &str) -> Result<Self, AgentError> {
        let (frontmatter, body) = split_frontmatter(markdown)?;
        let mut definition: AgentDefinition = serde_yaml::from_str(frontmatter)
            .map_err(|e| AgentError::InvalidDefinition(format!("frontmatter: {e}")))?;
        definition.system_prompt = body.trim().to_string();
        definition.validate()?;
        Ok(definition)
    }

    /// Serialize a definition back to a `agents/<id>.md` document.
    pub fn to_markdown(&self) -> Result<String, AgentError> {
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| AgentError::InvalidDefinition(format!("frontmatter: {e}")))?;
        Ok(format!("---\n{}---\n\n{}\n", yaml, self.system_prompt))
    }

    /// Validate the definition (`schemas/agent.schema.json`, `AGENTS.md` §21).
    pub fn validate(&self) -> Result<(), AgentError> {
        if !valid_id(&self.id) {
            return Err(AgentError::InvalidDefinition(format!(
                "invalid id {:?}: must match [a-z0-9][a-z0-9-]*",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(AgentError::InvalidDefinition("name is empty".to_string()));
        }
        if self.model.trim().is_empty() {
            return Err(AgentError::InvalidDefinition("model is empty".to_string()));
        }
        if let Some(temperature) = self.temperature {
            if !(0.0..=2.0).contains(&temperature) {
                return Err(AgentError::InvalidDefinition(format!(
                    "temperature {temperature} out of range [0, 2]"
                )));
            }
        }
        Ok(())
    }
}

/// Whether `id` matches `[a-z0-9][a-z0-9-]*`.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Split a document into its YAML frontmatter and Markdown body.
fn split_frontmatter(markdown: &str) -> Result<(&str, &str), AgentError> {
    let text = markdown.trim_start_matches('\u{feff}');
    let rest = text
        .strip_prefix("---")
        .ok_or_else(|| AgentError::InvalidDefinition("missing frontmatter".to_string()))?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .ok_or_else(|| {
            AgentError::InvalidDefinition("frontmatter must start on its own line".to_string())
        })?;

    let mut consumed = 0usize;
    for line in rest.split_inclusive('\n') {
        let delimiter = line.trim_end_matches(['\r', '\n']);
        if delimiter == "---" {
            let front = &rest[..consumed];
            let body = &rest[consumed + line.len()..];
            return Ok((front, body));
        }
        consumed += line.len();
    }
    Err(AgentError::InvalidDefinition(
        "unterminated frontmatter".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\nid: researcher\nname: Researcher\nmodel: mock-1\nloop: plan_execute\ntools:\n  - brain.search\n---\n\nYou are the Researcher.\n";

    #[test]
    fn parses_and_round_trips() {
        let definition = AgentDefinition::parse(DOC).unwrap();
        assert_eq!(definition.id, "researcher");
        assert_eq!(definition.loop_kind, LoopKind::PlanExecute);
        assert_eq!(definition.tools, vec!["brain.search"]);
        assert_eq!(definition.system_prompt, "You are the Researcher.");

        let markdown = definition.to_markdown().unwrap();
        let reparsed = AgentDefinition::parse(&markdown).unwrap();
        assert_eq!(reparsed, definition);
    }

    #[test]
    fn rejects_bad_ids_and_missing_fields() {
        let bad_id = DOC.replace("id: researcher", "id: Researcher");
        assert!(AgentDefinition::parse(&bad_id).is_err());
        assert!(AgentDefinition::parse("# no frontmatter").is_err());
        let no_model = "---\nid: x\nname: X\n---\nbody\n";
        assert!(AgentDefinition::parse(no_model).is_err());
    }

    #[test]
    fn defaults_are_applied() {
        let definition =
            AgentDefinition::parse("---\nid: x\nname: X\nmodel: mock-1\n---\nhi\n").unwrap();
        assert_eq!(definition.loop_kind, LoopKind::React);
        assert_eq!(definition.limits.max_iterations, 50);
        assert_eq!(definition.version, "0.1.0");
    }
}
