//! The workflow document model.
//!
//! A workflow is a YAML (or JSON) document: identity, requested permissions,
//! variables, triggers, a DAG of nodes and edges, and an optional app manifest.
//! The types here are the Rust mirror of `schema/workflow.schema.json`.
//!
//! Free-form maps are `serde_json::Value` on purpose: the document is
//! schema-driven and must accept plugin node configs without a kernel release.
//! Built-in node configs get typed views in [`crate::validate`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::WorkflowError;

/// A JSON object keyed by string.
pub type JsonMap = BTreeMap<String, serde_json::Value>;

/// The default document version when the YAML omits one.
fn default_version() -> String {
    "0.1.0".to_string()
}

/// The default node timeout, in seconds (spec §1.3).
pub const DEFAULT_NODE_TIMEOUT_S: u64 = 60;

/// The default loop iteration limit (spec §1.6).
pub const DEFAULT_MAX_ITERATIONS: u32 = 100;

/// The hard loop iteration cap (spec §1.6).
pub const MAX_ITERATIONS_CAP: u32 = 1000;

/// The maximum subworkflow nesting depth (spec §1.1 rule 8).
pub const MAX_SUBWORKFLOW_DEPTH: u32 = 5;

/// A complete workflow document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowDocument {
    /// Stable, unique workflow id.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional one-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Document version, `major.minor.patch`.
    #[serde(default = "default_version")]
    pub version: String,
    /// Free-form tags for search and grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Permissions the workflow requests, reviewed on enable and publish.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub permissions: PermissionsBlock,
    /// Variables available to templates as `{{ variables.<name> }}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: JsonMap,
    /// Entry points. At least one is required (spec §1.1 rule 3).
    #[serde(default)]
    pub triggers: Vec<TriggerSpec>,
    /// Graph nodes.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// Graph edges.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Optional app manifest (spec §5.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppManifest>,
}

impl WorkflowDocument {
    /// Parse a YAML document.
    pub fn from_yaml(source: &str) -> Result<Self, WorkflowError> {
        Ok(serde_yaml::from_str(source)?)
    }

    /// Serialize to YAML.
    pub fn to_yaml(&self) -> Result<String, WorkflowError> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Parse a JSON document.
    pub fn from_json(source: &str) -> Result<Self, WorkflowError> {
        Ok(serde_json::from_str(source)?)
    }

    /// Serialize to canonical JSON (key order is stable, so the output is a
    /// suitable digest input).
    pub fn to_json(&self) -> Result<String, WorkflowError> {
        Ok(serde_json::to_string(self)?)
    }

    /// The SHA-256 of the canonical JSON form, hex encoded.
    ///
    /// Used to pin versions and to refuse resuming a run against a document
    /// that changed underneath it (spec §3.6).
    pub fn digest(&self) -> Result<String, WorkflowError> {
        use sha2::{Digest, Sha256};
        let json = self.to_json()?;
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    /// Look a node up by id.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Look a trigger up by id.
    pub fn trigger(&self, id: &str) -> Option<&TriggerSpec> {
        self.triggers.iter().find(|trigger| trigger.id == id)
    }
}

/// The permission block: a map of permission category to a request.
///
/// `llm.call: true` becomes [`PermissionRequest::Enabled`]; `net.http: [...]`
/// becomes [`PermissionRequest::Scopes`].
pub type PermissionsBlock = BTreeMap<String, PermissionRequest>;

/// A permission request value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRequest {
    /// A boolean grant.
    Enabled(bool),
    /// A scoped grant (URL globs, path globs, tool names).
    Scopes(Vec<String>),
}

impl PermissionRequest {
    /// Whether the request grants anything at all.
    pub fn is_granted(&self) -> bool {
        match self {
            PermissionRequest::Enabled(flag) => *flag,
            PermissionRequest::Scopes(scopes) => !scopes.is_empty(),
        }
    }

    /// The requested scopes, or an empty slice for a plain boolean grant.
    pub fn scopes(&self) -> &[String] {
        match self {
            PermissionRequest::Enabled(_) => &[],
            PermissionRequest::Scopes(scopes) => scopes,
        }
    }
}

/// A trigger declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerSpec {
    /// Unique within the document.
    pub id: String,
    /// Kind-specific configuration.
    #[serde(flatten)]
    pub kind: TriggerKind,
}

/// The six trigger kinds of spec §2.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerKind {
    /// Fired by the UI, the CLI, or `workflow.run`.
    Manual,
    /// Fired by an agent through the `workflow.run` tool.
    Ai {
        /// What the workflow is for, shown to the agent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Fired on a schedule.
    Cron {
        /// A strict five-field cron expression.
        schedule: String,
        /// IANA zone; defaults to the user's configured timezone.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
        /// What to do with fires missed while the kernel was down.
        #[serde(default)]
        catch_up: CatchUp,
        /// Maximum concurrent runs for this trigger (cap 4).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concurrency: Option<u32>,
    },
    /// Fired by a loopback HTTP request.
    Webhook {
        /// Route path, must start with `/`.
        path: String,
        /// HTTP method; defaults to `POST`.
        #[serde(default = "default_webhook_method")]
        method: String,
        /// Authentication scheme; `bearer` is the only supported value.
        #[serde(default = "default_webhook_auth")]
        auth: String,
    },
    /// Fired when files under a project change.
    File {
        /// A glob relative to the project root.
        path: String,
        /// Watched event kinds; all kinds by default.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        events: Vec<FileEvent>,
    },
    /// Fired by a kernel event bus topic.
    Event {
        /// The topic, for example `plugin.installed`.
        topic: String,
        /// An optional JSON-pointer filter against the event payload.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
    },
}

fn default_webhook_method() -> String {
    "POST".to_string()
}

fn default_webhook_auth() -> String {
    "bearer".to_string()
}

impl TriggerKind {
    /// The stable kind name recorded on every run.
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerKind::Manual => "manual",
            TriggerKind::Ai { .. } => "ai",
            TriggerKind::Cron { .. } => "cron",
            TriggerKind::Webhook { .. } => "webhook",
            TriggerKind::File { .. } => "file",
            TriggerKind::Event { .. } => "event",
        }
    }
}

/// The missed-fire policy for cron triggers (spec §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUp {
    /// Drop missed fires.
    #[default]
    Skip,
    /// Run once if any fire was missed.
    RunOnce,
    /// Run once per missed fire, bounded by 24 h.
    RunAll,
}

/// A watched filesystem event kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEvent {
    /// A file appeared.
    Create,
    /// A file changed.
    Modify,
    /// A file disappeared.
    Delete,
    /// A file moved or was renamed.
    Rename,
}

/// A graph node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique within the document, `^[a-z][a-z0-9_]*$`.
    pub id: String,
    /// The node type.
    #[serde(rename = "type")]
    pub kind: NodeKind,
    /// Type-specific configuration (see spec Appendix C).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: JsonMap,
    /// Inputs, resolved as templates before execution.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: JsonMap,
    /// Outputs, resolved as templates after execution.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: JsonMap,
    /// Retry policy; absent means no retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Per-node timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u64>,
    /// What to do when the node fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<OnError>,
    /// What to do with this node when an interrupted run resumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumePolicy>,
}

impl Node {
    /// The node's effective timeout.
    pub fn timeout_s(&self) -> u64 {
        self.timeout_s.unwrap_or(DEFAULT_NODE_TIMEOUT_S)
    }

    /// The node's effective error policy.
    pub fn on_error(&self) -> OnError {
        self.on_error.unwrap_or(OnError::Fail)
    }

    /// The node's effective resume policy: side-effecting nodes default to
    /// `fail`, pure nodes to `retry` (spec §1.3).
    pub fn resume(&self) -> ResumePolicy {
        self.resume.unwrap_or_else(|| {
            if self.kind.has_side_effects() {
                ResumePolicy::Fail
            } else {
                ResumePolicy::Retry
            }
        })
    }

    /// A typed view of `config`.
    pub fn config_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, WorkflowError> {
        let value = serde_json::to_value(&self.config)?;
        serde_json::from_value(value).map_err(|err| {
            WorkflowError::Invalid(format!(
                "node `{}` has an invalid `{}` config: {err}",
                self.id,
                self.kind.as_str()
            ))
        })
    }
}

/// The node types of spec §1.2, plus plugin-contributed types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeKind {
    /// Entry point.
    Trigger,
    /// An LLM call through the router.
    Llm,
    /// A kernel, plugin, or MCP tool call.
    Tool,
    /// An agent run to completion.
    Agent,
    /// Sandboxed code.
    Code,
    /// An HTTP request.
    Http,
    /// A branch.
    Condition,
    /// Iteration over a subgraph.
    Loop,
    /// Another workflow.
    Subworkflow,
    /// A human decision.
    Human,
    /// Declarative data mapping.
    Transform,
    /// Terminal node.
    End,
    /// A plugin node type, stored as `plugin:<plugin-id>:<node-id>`.
    Plugin(String),
}

impl NodeKind {
    /// Every built-in node type, in spec order.
    pub const BUILTIN: [NodeKind; 12] = [
        NodeKind::Trigger,
        NodeKind::Llm,
        NodeKind::Tool,
        NodeKind::Agent,
        NodeKind::Code,
        NodeKind::Http,
        NodeKind::Condition,
        NodeKind::Loop,
        NodeKind::Subworkflow,
        NodeKind::Human,
        NodeKind::Transform,
        NodeKind::End,
    ];

    /// The wire name.
    pub fn as_str(&self) -> &str {
        match self {
            NodeKind::Trigger => "trigger",
            NodeKind::Llm => "llm",
            NodeKind::Tool => "tool",
            NodeKind::Agent => "agent",
            NodeKind::Code => "code",
            NodeKind::Http => "http",
            NodeKind::Condition => "condition",
            NodeKind::Loop => "loop",
            NodeKind::Subworkflow => "subworkflow",
            NodeKind::Human => "human",
            NodeKind::Transform => "transform",
            NodeKind::End => "end",
            NodeKind::Plugin(id) => id,
        }
    }

    /// Parse a wire name.
    pub fn parse(name: &str) -> Option<NodeKind> {
        let kind = match name {
            "trigger" => NodeKind::Trigger,
            "llm" => NodeKind::Llm,
            "tool" => NodeKind::Tool,
            "agent" => NodeKind::Agent,
            "code" => NodeKind::Code,
            "http" => NodeKind::Http,
            "condition" => NodeKind::Condition,
            "loop" => NodeKind::Loop,
            "subworkflow" => NodeKind::Subworkflow,
            "human" => NodeKind::Human,
            "transform" => NodeKind::Transform,
            "end" => NodeKind::End,
            other if other.starts_with("plugin:") => NodeKind::Plugin(other.to_string()),
            _ => return None,
        };
        Some(kind)
    }

    /// Whether this is a built-in type.
    pub fn is_builtin(&self) -> bool {
        !matches!(self, NodeKind::Plugin(_))
    }

    /// Whether the node can cause effects outside the run (spec §1.3 default
    /// resume policy).
    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            NodeKind::Tool
                | NodeKind::Agent
                | NodeKind::Code
                | NodeKind::Http
                | NodeKind::Human
                | NodeKind::Subworkflow
                | NodeKind::Plugin(_)
        )
    }

    /// The declared input ports (spec §1.2).
    ///
    /// Input ports are the keys a node's `inputs` map may use, and the names an
    /// edge's `to_port` may reference.
    pub fn input_ports(&self) -> &'static [&'static str] {
        match self {
            NodeKind::Trigger => &[],
            NodeKind::Llm => &["prompt", "context"],
            NodeKind::Tool => &["args"],
            NodeKind::Agent => &["task", "context"],
            NodeKind::Code => &["input"],
            NodeKind::Http => &["url", "body"],
            NodeKind::Condition => &["value"],
            NodeKind::Loop => &["items"],
            NodeKind::Subworkflow => &["inputs"],
            NodeKind::Human => &["prompt"],
            NodeKind::Transform => &["input"],
            NodeKind::End => &["result"],
            NodeKind::Plugin(_) => &[],
        }
    }

    /// The declared output ports (spec §1.2).
    pub fn output_ports(&self) -> &'static [&'static str] {
        match self {
            NodeKind::Trigger => &["event"],
            NodeKind::Llm => &["text", "usage"],
            NodeKind::Tool => &["result"],
            NodeKind::Agent => &["result", "artifacts"],
            NodeKind::Code => &["output"],
            NodeKind::Http => &["status", "body", "headers"],
            NodeKind::Condition => &["true", "false"],
            NodeKind::Loop => &["results"],
            NodeKind::Subworkflow => &["outputs"],
            NodeKind::Human => &["decision", "notes"],
            NodeKind::Transform => &["output"],
            NodeKind::End => &[],
            NodeKind::Plugin(_) => &[],
        }
    }
}

impl Serialize for NodeKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        NodeKind::parse(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown node type `{raw}`; expected a built-in type or `plugin:<plugin>:<node>`"
            ))
        })
    }
}

/// A directed connection between two nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    /// Source node id.
    pub from: String,
    /// Source port; defaults to the node's primary output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    /// Target node id.
    pub to: String,
    /// Target port; defaults to the node's primary input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

/// A loop body: the node ids a `loop` node owns, validated as its own DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopBody {
    /// The `loop` node that owns the body.
    pub owner: String,
    /// The node ids listed in the loop's `body` config, in declaration order.
    pub nodes: Vec<String>,
}

/// A node retry policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    /// Maximum attempts after the first failure.
    #[serde(default)]
    pub max: u32,
    /// Base backoff in milliseconds.
    #[serde(default)]
    pub backoff_ms: u64,
    /// Exponential multiplier between attempts.
    #[serde(default = "one")]
    pub backoff_multiplier: f64,
    /// Whether to add jitter.
    #[serde(default)]
    pub jitter: bool,
    /// Error classes that are retryable; empty means the node default set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_on: Vec<String>,
}

fn one() -> f64 {
    1.0
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max: 0,
            backoff_ms: 0,
            backoff_multiplier: 1.0,
            jitter: false,
            retry_on: Vec::new(),
        }
    }
}

/// What to do when a node fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// Fail the run.
    #[default]
    Fail,
    /// Record the failure and continue to downstream nodes.
    Continue,
    /// Retry according to the node's policy.
    Retry,
    /// Run the node's compensation steps, then fail.
    Compensate,
}

/// What to do with a node when an interrupted run resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumePolicy {
    /// Treat the node as done.
    Skip,
    /// Re-execute the node.
    Retry,
    /// Refuse to resume the run.
    #[default]
    Fail,
}

/// The optional app manifest (spec §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppManifest {
    /// Stable app id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Shell icon-set name, never a URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// One-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<AppInput>,
    /// Declared outputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<AppOutput>,
    /// Declarative layout blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<serde_json::Value>,
}

/// An app input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInput {
    /// Unique within the manifest.
    pub id: String,
    /// `string`, `number`, `boolean`, `array`, `select`, or `markdown`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Field label.
    pub label: String,
    /// Default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Whether the field must be provided.
    #[serde(default)]
    pub required: bool,
    /// Choices for a `select` input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// An app output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppOutput {
    /// Unique within the manifest.
    pub id: String,
    /// `markdown`, `text`, `json`, `table`, or `link`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Display label.
    pub label: String,
}

/// Typed configs for the built-in node types (spec Appendix C).
pub mod config {
    use serde::{Deserialize, Serialize};

    /// `trigger`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TriggerConfig {
        /// Which trigger spec this node represents.
        pub trigger_id: String,
    }

    /// `llm`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct LlmConfig {
        /// Model route, for example `ollama:qwen2.5:7b`.
        pub model: String,
        /// The prompt template.
        pub prompt: String,
        /// Optional system prompt.
        #[serde(default)]
        pub system: Option<String>,
        /// Sampling temperature.
        #[serde(default)]
        pub temperature: Option<f64>,
        /// Output token cap.
        #[serde(default)]
        pub max_tokens: Option<u32>,
        /// Ask for a JSON object response.
        #[serde(default)]
        pub json: bool,
    }

    /// `tool`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ToolConfig {
        /// Tool registry id.
        pub tool: String,
        /// Arguments, templates allowed.
        #[serde(default)]
        pub args: serde_json::Value,
    }

    /// `agent`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct AgentConfig {
        /// Agent id.
        pub agent: String,
        /// The task description template.
        pub task: String,
        /// A narrowed budget.
        #[serde(default)]
        pub budget: Option<serde_json::Value>,
        /// A narrowed tool list.
        #[serde(default)]
        pub tools: Option<Vec<String>>,
    }

    /// `code`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct CodeConfig {
        /// `typescript` or `rust`.
        pub language: String,
        /// The source.
        pub source: String,
        /// Memory cap in MiB.
        #[serde(default)]
        pub memory_mb: Option<u32>,
        /// Fuel cap.
        #[serde(default)]
        pub fuel: Option<u64>,
    }

    /// `http`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HttpConfig {
        /// HTTP method.
        pub method: String,
        /// Request URL template.
        pub url: String,
        /// Request headers.
        #[serde(default)]
        pub headers: Option<serde_json::Value>,
        /// Request body template.
        #[serde(default)]
        pub body: Option<serde_json::Value>,
        /// Response size cap in bytes.
        #[serde(default)]
        pub max_bytes: Option<u64>,
    }

    /// `condition`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ConditionConfig {
        /// A closed expression.
        pub expr: String,
    }

    /// `loop`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct LoopConfig {
        /// Items to iterate; mutually exclusive with `count`.
        #[serde(default)]
        pub items: Option<serde_json::Value>,
        /// A fixed iteration count; mutually exclusive with `items`.
        #[serde(default)]
        pub count: Option<serde_json::Value>,
        /// Iteration cap, default 100, hard cap 1000.
        #[serde(default)]
        pub max_iterations: Option<u32>,
        /// Concurrent iterations, default 1, cap 4.
        #[serde(default)]
        pub parallel: Option<u32>,
        /// Node ids that form the loop body.
        #[serde(default)]
        pub body: Vec<String>,
    }

    /// `subworkflow`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SubworkflowConfig {
        /// Target workflow id.
        pub workflow: String,
        /// `latest` or a pinned integer version.
        #[serde(default)]
        pub version: Option<serde_json::Value>,
        /// Inputs passed to the child run.
        #[serde(default)]
        pub inputs: serde_json::Value,
    }

    /// `human`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct HumanConfig {
        /// The question shown to the user.
        pub prompt: String,
        /// The allowed decisions.
        #[serde(default)]
        pub options: Vec<String>,
        /// Timeout in seconds.
        #[serde(default)]
        pub timeout_s: Option<u64>,
        /// What to do on timeout.
        #[serde(default)]
        pub on_timeout: Option<String>,
    }

    /// `transform`.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TransformConfig {
        /// Declarative operations.
        #[serde(default)]
        pub ops: Vec<crate::expr::TransformOp>,
        /// An optional output template applied after the operations.
        #[serde(default)]
        pub template: Option<String>,
    }

    /// `end`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct EndConfig {
        /// The run result template.
        #[serde(default)]
        pub result: Option<String>,
    }
}
