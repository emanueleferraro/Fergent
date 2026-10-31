//! Semantic validation of a workflow document (spec §1.1, §1.9, §1.8, §5.1).
//!
//! [`validate`] never short-circuits: it walks the whole document and returns
//! every issue it finds, so the editor can show them all at once and a save
//! can report a complete list. Errors block save and run; warnings do not
//! (spec §1.9).
//!
//! The JSON Schema in [`crate::schema`] covers shape; this module covers
//! meaning: graph structure, cross-references, node configs, template
//! compilation, and context reachability.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::document::{
    config, AppManifest, Node, NodeKind, TriggerKind, WorkflowDocument, DEFAULT_NODE_TIMEOUT_S,
    MAX_ITERATIONS_CAP,
};
use crate::error::WorkflowError;
use crate::expr::Condition;
use crate::graph::Graph;
use crate::template::Template;

/// The largest `http` response a node may accept (spec §1.6).
pub const MAX_HTTP_BYTES: u64 = 8 * 1024 * 1024;

/// The largest `parallel` a loop may declare (spec §1.6).
pub const MAX_LOOP_PARALLEL: u32 = 4;

/// The cron-trigger concurrency cap (spec §2.7).
pub const MAX_TRIGGER_CONCURRENCY: u32 = 4;

const APP_INPUT_TYPES: [&str; 6] = ["string", "number", "boolean", "array", "select", "markdown"];
const APP_OUTPUT_TYPES: [&str; 5] = ["markdown", "text", "json", "table", "link"];
const APP_BLOCKS: [&str; 8] = [
    "form", "button", "output", "history", "markdown", "text", "divider", "columns",
];
const HTTP_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// One validation finding.
///
/// `path` is a JSON pointer-style location inside the document (for example
/// `nodes[2].config.expr`), `node` the owning node id when there is one, and
/// `code` a stable machine-readable identifier for the rule that fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Where in the document the issue is.
    pub path: String,
    /// The owning node, if the issue belongs to one.
    pub node: Option<String>,
    /// A stable issue code, for example `cycle` or `unknown_source`.
    pub code: &'static str,
    /// A human-readable explanation.
    pub message: String,
}

/// The result of validating a document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validation {
    /// Blocking issues.
    pub errors: Vec<Issue>,
    /// Non-blocking issues (spec §1.9).
    pub warnings: Vec<Issue>,
}

impl Validation {
    /// Whether the document may be saved and run.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Whether any issue uses `code`.
    pub fn has_code(&self, code: &str) -> bool {
        self.errors
            .iter()
            .chain(self.warnings.iter())
            .any(|issue| issue.code == code)
    }

    /// Every message, errors first, for logs and `workflow_invalid` payloads.
    pub fn messages(&self) -> Vec<String> {
        self.errors
            .iter()
            .chain(self.warnings.iter())
            .map(|issue| format!("{}: {}", issue.path, issue.message))
            .collect()
    }

    /// Collapse the errors into one [`WorkflowError::Invalid`].
    pub fn into_result(self) -> Result<(), WorkflowError> {
        if self.is_valid() {
            return Ok(());
        }
        let detail = self
            .errors
            .iter()
            .map(|issue| format!("{}: {}", issue.path, issue.message))
            .collect::<Vec<_>>()
            .join("; ");
        Err(WorkflowError::Invalid(format!(
            "{} error(s): {detail}",
            self.errors.len()
        )))
    }
}

/// Validate a document against the rules of spec §1.1 and the lints of §1.9.
pub fn validate(doc: &WorkflowDocument) -> Validation {
    let mut validation = Validation::default();
    let graph = Graph::build(doc);

    check_identity(doc, &mut validation);
    check_triggers(doc, &mut validation);
    check_nodes(doc, &mut validation);
    check_loops(doc, &mut validation);
    check_edges(doc, &mut validation);
    check_cycles(&graph, &mut validation);
    check_reachability(doc, &graph, &mut validation);
    check_templates(doc, &graph, &mut validation);
    check_app(doc.app.as_ref(), &mut validation);
    check_lints(doc, &graph, &mut validation);

    validation
}

fn issue(
    node: Option<&str>,
    path: impl Into<String>,
    code: &'static str,
    message: impl Into<String>,
) -> Issue {
    Issue {
        path: path.into(),
        node: node.map(str::to_string),
        code,
        message: message.into(),
    }
}

// ---------------------------------------------------------------------------
// Identity and structural rules (§1.1 rules 3 and 4)
// ---------------------------------------------------------------------------

fn check_identity(doc: &WorkflowDocument, v: &mut Validation) {
    if !is_workflow_id(&doc.id) {
        v.errors.push(issue(
            None,
            "id",
            "invalid_id",
            format!(
                "`{}` is not a valid workflow id; use lowercase letters, digits, and `-`",
                doc.id
            ),
        ));
    }
    if doc.name.trim().is_empty() {
        v.errors
            .push(issue(None, "name", "missing_name", "a name is required"));
    }
    if !is_version(&doc.version) {
        v.errors.push(issue(
            None,
            "version",
            "invalid_version",
            format!("`{}` is not `major.minor.patch`", doc.version),
        ));
    }

    if doc.triggers.is_empty() {
        v.errors.push(issue(
            None,
            "triggers",
            "missing_trigger",
            "a workflow needs at least one trigger",
        ));
    }
    if !doc.nodes.iter().any(|node| node.kind == NodeKind::End) {
        v.errors.push(issue(
            None,
            "nodes",
            "missing_end",
            "a workflow needs at least one `end` node",
        ));
    }

    let mut seen_triggers = BTreeSet::new();
    for (index, trigger) in doc.triggers.iter().enumerate() {
        if !is_node_id(&trigger.id) {
            v.errors.push(issue(
                None,
                format!("triggers[{index}].id"),
                "invalid_id",
                format!("`{}` must match ^[a-z][a-z0-9_]*$", trigger.id),
            ));
        }
        if !seen_triggers.insert(trigger.id.as_str()) {
            v.errors.push(issue(
                None,
                format!("triggers[{index}].id"),
                "duplicate_trigger_id",
                format!("trigger id `{}` is declared twice", trigger.id),
            ));
        }
    }

    let mut seen_nodes = BTreeSet::new();
    for (index, node) in doc.nodes.iter().enumerate() {
        if !is_node_id(&node.id) {
            v.errors.push(issue(
                Some(&node.id),
                format!("nodes[{index}].id"),
                "invalid_node_id",
                format!("`{}` must match ^[a-z][a-z0-9_]*$", node.id),
            ));
        }
        if !seen_nodes.insert(node.id.as_str()) {
            v.errors.push(issue(
                Some(&node.id),
                format!("nodes[{index}].id"),
                "duplicate_node_id",
                format!("node id `{}` is declared twice", node.id),
            ));
        }
        if let NodeKind::Plugin(raw) = &node.kind {
            if plugin_parts(raw).is_none() {
                v.errors.push(issue(
                    Some(&node.id),
                    format!("nodes[{index}].type"),
                    "invalid_node_type",
                    format!("`{raw}` is not `plugin:<plugin-id>:<node-id>`"),
                ));
            }
        }
        if node.timeout_s == Some(0) {
            v.errors.push(issue(
                Some(&node.id),
                format!("nodes[{index}].timeout_s"),
                "invalid_timeout",
                "a timeout of 0 seconds is not meaningful; omit the field to use the default",
            ));
        }
    }
}

fn plugin_parts(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix("plugin:")?;
    let (plugin, node) = rest.split_once(':')?;
    if plugin.is_empty() || node.is_empty() {
        return None;
    }
    Some((plugin, node))
}

fn is_workflow_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_node_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_version(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

// ---------------------------------------------------------------------------
// Triggers (§2.1–§2.6)
// ---------------------------------------------------------------------------

fn check_triggers(doc: &WorkflowDocument, v: &mut Validation) {
    let mut routes: BTreeMap<(String, String), &str> = BTreeMap::new();

    for (index, trigger) in doc.triggers.iter().enumerate() {
        let at = |field: &str| format!("triggers[{index}].{field}");
        match &trigger.kind {
            TriggerKind::Manual | TriggerKind::Ai { .. } => {}
            TriggerKind::Cron {
                schedule,
                timezone,
                concurrency,
                ..
            } => {
                if !is_cron(schedule) {
                    v.errors.push(issue(
                        None,
                        at("schedule"),
                        "invalid_cron",
                        format!("`{schedule}` is not a five-field cron expression"),
                    ));
                }
                if let Some(zone) = timezone {
                    if !is_timezone(zone) {
                        v.errors.push(issue(
                            None,
                            at("timezone"),
                            "invalid_timezone",
                            format!("`{zone}` is not an IANA timezone"),
                        ));
                    }
                }
                if let Some(limit) = concurrency {
                    if !(1..=MAX_TRIGGER_CONCURRENCY).contains(limit) {
                        v.errors.push(issue(
                            None,
                            at("concurrency"),
                            "invalid_concurrency",
                            format!("concurrency must be between 1 and {MAX_TRIGGER_CONCURRENCY}"),
                        ));
                    }
                }
            }
            TriggerKind::Webhook { path, method, auth } => {
                if !path.starts_with('/') {
                    v.errors.push(issue(
                        None,
                        at("path"),
                        "invalid_webhook",
                        "a webhook path must start with `/`",
                    ));
                }
                if !HTTP_METHODS.contains(&method.to_uppercase().as_str()) {
                    v.errors.push(issue(
                        None,
                        at("method"),
                        "invalid_webhook",
                        format!("`{method}` is not an HTTP method"),
                    ));
                }
                if auth != "bearer" {
                    v.errors.push(issue(
                        None,
                        at("auth"),
                        "invalid_webhook",
                        "`bearer` is the only supported webhook auth scheme",
                    ));
                }
                let route = (method.to_uppercase(), path.clone());
                if let Some(previous) = routes.insert(route, &trigger.id) {
                    v.errors.push(issue(
                        None,
                        at("path"),
                        "duplicate_webhook",
                        format!(
                            "`{} {}` is already routed to trigger `{previous}`",
                            method.to_uppercase(),
                            path
                        ),
                    ));
                }
            }
            TriggerKind::File { path, .. } => {
                if path.trim().is_empty() {
                    v.errors.push(issue(
                        None,
                        at("path"),
                        "invalid_file_trigger",
                        "a file trigger needs a glob",
                    ));
                }
            }
            TriggerKind::Event { topic, filter } => {
                if topic.trim().is_empty() {
                    v.errors.push(issue(
                        None,
                        at("topic"),
                        "invalid_event_trigger",
                        "an event trigger needs a topic",
                    ));
                }
                if let Some(pointer) = filter {
                    if !pointer.is_empty() && !pointer.starts_with('/') {
                        v.errors.push(issue(
                            None,
                            at("filter"),
                            "invalid_event_trigger",
                            "an event filter must be a JSON pointer starting with `/`",
                        ));
                    }
                }
            }
        }
    }
}

fn is_cron(schedule: &str) -> bool {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    fields.len() == 5
        && fields.iter().all(|field| {
            !field.is_empty()
                && field
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '*' | '/' | ',' | '-'))
        })
}

fn is_timezone(zone: &str) -> bool {
    if zone.is_empty() || zone.contains(char::is_whitespace) {
        return false;
    }
    if zone == "UTC" {
        return true;
    }
    zone.split('/')
        .all(|part| !part.is_empty() && !part.contains(char::is_whitespace))
        && zone.contains('/')
}

// ---------------------------------------------------------------------------
// Node configs (§1.2, Appendix C)
// ---------------------------------------------------------------------------

/// A typed view of a node's config, reporting an `invalid_config` on failure.
fn config_of<T: DeserializeOwned>(node: &Node, index: usize, v: &mut Validation) -> Option<T> {
    match node.config_as::<T>() {
        Ok(config) => Some(config),
        Err(err) => {
            v.errors.push(issue(
                Some(&node.id),
                format!("nodes[{index}].config"),
                "invalid_config",
                err.to_string(),
            ));
            None
        }
    }
}

fn check_nodes(doc: &WorkflowDocument, v: &mut Validation) {
    for (index, node) in doc.nodes.iter().enumerate() {
        check_node_config(doc, node, index, v);
    }
}

fn check_node_config(doc: &WorkflowDocument, node: &Node, index: usize, v: &mut Validation) {
    match &node.kind {
        NodeKind::Trigger => {
            if let Some(cfg) = config_of::<config::TriggerConfig>(node, index, v) {
                if doc.trigger(&cfg.trigger_id).is_none() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.trigger_id"),
                        "unknown_trigger",
                        format!("no trigger `{}` is declared", cfg.trigger_id),
                    ));
                }
            }
        }
        NodeKind::Llm => {
            if let Some(cfg) = config_of::<config::LlmConfig>(node, index, v) {
                if cfg.prompt.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.prompt"),
                        "invalid_config",
                        "a prompt is required",
                    ));
                }
                if let Some(temperature) = cfg.temperature {
                    if !(0.0..=2.0).contains(&temperature) {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.temperature"),
                            "invalid_config",
                            "temperature must be between 0 and 2",
                        ));
                    }
                }
                if cfg.max_tokens == Some(0) {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.max_tokens"),
                        "invalid_config",
                        "max_tokens must be at least 1",
                    ));
                }
            }
        }
        NodeKind::Tool => {
            if let Some(cfg) = config_of::<config::ToolConfig>(node, index, v) {
                if cfg.tool.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.tool"),
                        "invalid_config",
                        "a tool id is required",
                    ));
                }
            }
        }
        NodeKind::Agent => {
            if let Some(cfg) = config_of::<config::AgentConfig>(node, index, v) {
                if cfg.agent.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.agent"),
                        "invalid_config",
                        "an agent id is required",
                    ));
                }
                if cfg.task.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.task"),
                        "invalid_config",
                        "a task is required",
                    ));
                }
            }
        }
        NodeKind::Code => {
            if let Some(cfg) = config_of::<config::CodeConfig>(node, index, v) {
                if !matches!(cfg.language.as_str(), "typescript" | "rust") {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.language"),
                        "invalid_config",
                        format!(
                            "`{}` is not a supported language; use `typescript` or `rust`",
                            cfg.language
                        ),
                    ));
                }
                if cfg.source.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.source"),
                        "invalid_config",
                        "source is required",
                    ));
                }
            }
        }
        NodeKind::Http => {
            if let Some(cfg) = config_of::<config::HttpConfig>(node, index, v) {
                if !HTTP_METHODS.contains(&cfg.method.to_uppercase().as_str()) {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.method"),
                        "invalid_config",
                        format!("`{}` is not an HTTP method", cfg.method),
                    ));
                }
                if cfg.url.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.url"),
                        "invalid_config",
                        "a url is required",
                    ));
                }
                if let Some(max) = cfg.max_bytes {
                    if max > MAX_HTTP_BYTES {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.max_bytes"),
                            "invalid_config",
                            format!("max_bytes may not exceed {MAX_HTTP_BYTES} (8 MiB)"),
                        ));
                    }
                }
            }
        }
        NodeKind::Condition => {
            if let Some(cfg) = config_of::<config::ConditionConfig>(node, index, v) {
                if let Err(err) = Condition::parse(&cfg.expr) {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.expr"),
                        "invalid_expression",
                        err.to_string(),
                    ));
                }
            }
        }
        NodeKind::Loop => {
            if let Some(cfg) = config_of::<config::LoopConfig>(node, index, v) {
                if cfg.items.is_some() == cfg.count.is_some() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config"),
                        "invalid_config",
                        "a loop needs exactly one of `items` or `count`",
                    ));
                }
                if let Some(limit) = cfg.max_iterations {
                    if limit == 0 || limit > MAX_ITERATIONS_CAP {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.max_iterations"),
                            "invalid_config",
                            format!("max_iterations must be between 1 and {MAX_ITERATIONS_CAP}"),
                        ));
                    }
                }
                if let Some(parallel) = cfg.parallel {
                    if !(1..=MAX_LOOP_PARALLEL).contains(&parallel) {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.parallel"),
                            "invalid_config",
                            format!("parallel must be between 1 and {MAX_LOOP_PARALLEL}"),
                        ));
                    }
                }
                if cfg.body.is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.body"),
                        "invalid_config",
                        "a loop needs at least one body node",
                    ));
                }
            }
        }
        NodeKind::Subworkflow => {
            if let Some(cfg) = config_of::<config::SubworkflowConfig>(node, index, v) {
                if cfg.workflow.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.workflow"),
                        "invalid_config",
                        "a workflow id is required",
                    ));
                } else if cfg.workflow == doc.id {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.workflow"),
                        "subworkflow_cycle",
                        "a workflow may not call itself",
                    ));
                }
                if let Some(version) = &cfg.version {
                    let ok = version.as_str() == Some("latest")
                        || version.as_u64().is_some_and(|value| value > 0);
                    if !ok {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.version"),
                            "invalid_config",
                            "version must be `latest` or a positive integer",
                        ));
                    }
                }
            }
        }
        NodeKind::Human => {
            if let Some(cfg) = config_of::<config::HumanConfig>(node, index, v) {
                if cfg.prompt.trim().is_empty() {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.prompt"),
                        "invalid_config",
                        "a prompt is required",
                    ));
                }
                if let Some(mode) = cfg.on_timeout.as_deref() {
                    let ok = mode == "fail"
                        || mode
                            .strip_prefix("continue:")
                            .is_some_and(|option| cfg.options.iter().any(|item| item == option));
                    if !ok {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.on_timeout"),
                            "invalid_config",
                            "on_timeout must be `fail` or `continue:<option>` naming a declared option",
                        ));
                    }
                }
            }
        }
        NodeKind::Transform => {
            if let Some(cfg) = config_of::<config::TransformConfig>(node, index, v) {
                for (op_index, op) in cfg.ops.iter().enumerate() {
                    if let Err(err) = op.validate() {
                        v.errors.push(issue(
                            Some(&node.id),
                            format!("nodes[{index}].config.ops[{op_index}]"),
                            "invalid_config",
                            err.to_string(),
                        ));
                    }
                }
            }
        }
        NodeKind::End => {
            let _ = config_of::<config::EndConfig>(node, index, v);
        }
        NodeKind::Plugin(_) => {}
    }
}

/// Loop bodies: ids exist, no node is owned twice, and control flow stays sane.
fn check_loops(doc: &WorkflowDocument, v: &mut Validation) {
    let mut owners: BTreeMap<String, String> = BTreeMap::new();

    for (index, node) in doc.nodes.iter().enumerate() {
        if node.kind != NodeKind::Loop {
            continue;
        }
        let Ok(cfg) = node.config_as::<config::LoopConfig>() else {
            continue; // already reported by `check_node_config`
        };
        for id in &cfg.body {
            let Some(target) = doc.node(id) else {
                v.errors.push(issue(
                    Some(&node.id),
                    format!("nodes[{index}].config.body"),
                    "unknown_node",
                    format!("loop body references unknown node `{id}`"),
                ));
                continue;
            };
            if id == &node.id {
                v.errors.push(issue(
                    Some(&node.id),
                    format!("nodes[{index}].config.body"),
                    "invalid_config",
                    "a loop may not own itself",
                ));
                continue;
            }
            if target.kind == NodeKind::Trigger {
                v.errors.push(issue(
                    Some(&node.id),
                    format!("nodes[{index}].config.body"),
                    "invalid_config",
                    format!("loop body may not contain trigger node `{id}`"),
                ));
            }
            if let Some(previous) = owners.insert(id.clone(), node.id.clone()) {
                if previous != node.id {
                    v.errors.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}].config.body"),
                        "loop_scope_conflict",
                        format!("node `{id}` is already owned by loop `{previous}`"),
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Edges (§1.1 rule 5, §1.4)
// ---------------------------------------------------------------------------

fn check_edges(doc: &WorkflowDocument, v: &mut Validation) {
    for (index, edge) in doc.edges.iter().enumerate() {
        let Some(source) = doc.node(&edge.from) else {
            v.errors.push(issue(
                None,
                format!("edges[{index}].from"),
                "unknown_node",
                format!("edge starts at unknown node `{}`", edge.from),
            ));
            continue;
        };
        let Some(target) = doc.node(&edge.to) else {
            v.errors.push(issue(
                None,
                format!("edges[{index}].to"),
                "unknown_node",
                format!("edge ends at unknown node `{}`", edge.to),
            ));
            continue;
        };

        if let Some(port) = &edge.from_port {
            if !source.kind.output_ports().contains(&port.as_str()) {
                v.errors.push(issue(
                    Some(&source.id),
                    format!("edges[{index}].from_port"),
                    "unknown_port",
                    format!(
                        "`{}` has no output port `{port}`; expected one of {:?}",
                        source.kind.as_str(),
                        source.kind.output_ports()
                    ),
                ));
            }
        }
        if let Some(port) = &edge.to_port {
            if !target.kind.input_ports().contains(&port.as_str()) {
                v.errors.push(issue(
                    Some(&target.id),
                    format!("edges[{index}].to_port"),
                    "unknown_port",
                    format!(
                        "`{}` has no input port `{port}`; expected one of {:?}",
                        target.kind.as_str(),
                        target.kind.input_ports()
                    ),
                ));
            }
        }

        if target.kind == NodeKind::Trigger {
            v.errors.push(issue(
                Some(&target.id),
                format!("edges[{index}].to"),
                "trigger_has_inbound",
                "a trigger node is an entry point and cannot have inbound edges",
            ));
        }
        if source.kind == NodeKind::End {
            v.errors.push(issue(
                Some(&source.id),
                format!("edges[{index}].from"),
                "end_has_outbound",
                "an `end` node is terminal and cannot have outbound edges",
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Graph rules (§1.1 rules 1 and 2)
// ---------------------------------------------------------------------------

fn check_cycles(graph: &Graph, v: &mut Validation) {
    for cycle in graph.cycles() {
        let first = cycle.first().map(String::as_str);
        v.errors.push(issue(
            first,
            "edges",
            "cycle",
            format!(
                "the graph is not acyclic: {} are mutually reachable",
                cycle
                    .iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
}

fn check_reachability(doc: &WorkflowDocument, graph: &Graph, v: &mut Validation) {
    let reachable = graph.reachable_from_triggers();
    for (index, node) in doc.nodes.iter().enumerate() {
        if graph.is_loop_body(&node.id) {
            continue; // owned by a loop, which is itself checked
        }
        if !reachable.contains(&node.id) {
            v.errors.push(issue(
                Some(&node.id),
                format!("nodes[{index}]"),
                "unreachable_node",
                format!("node `{}` cannot be reached from any trigger", node.id),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Templates (§1.5, Appendix D) and the unknown-source lint (§1.9)
// ---------------------------------------------------------------------------

/// A string in a document that is compiled as a template.
struct TemplateUse<'a> {
    path: String,
    text: &'a str,
    in_config: bool,
}

fn check_templates(doc: &WorkflowDocument, graph: &Graph, v: &mut Validation) {
    let root_writes = root_writes_by_node(doc);

    for (index, node) in doc.nodes.iter().enumerate() {
        for usage in template_uses(node, index) {
            let template = match Template::parse(usage.text) {
                Ok(template) => template,
                Err(err) => {
                    v.errors.push(issue(
                        Some(&node.id),
                        usage.path,
                        "invalid_template",
                        err.to_string(),
                    ));
                    continue;
                }
            };

            if template.reads_secret() && !usage.in_config {
                v.errors.push(issue(
                    Some(&node.id),
                    usage.path.clone(),
                    "secret_outside_config",
                    "secrets may only be read in node config; inputs and outputs are persisted",
                ));
            }

            for path in template.context_paths() {
                if let Some(reason) = unresolvable(doc, graph, &root_writes, &node.id, &path) {
                    v.warnings.push(issue(
                        Some(&node.id),
                        usage.path.clone(),
                        "unknown_source",
                        reason,
                    ));
                }
            }
        }

        if node.kind == NodeKind::Condition {
            if let Ok(cfg) = node.config_as::<config::ConditionConfig>() {
                if let Ok(condition) = Condition::parse(&cfg.expr) {
                    for path in condition.context_paths() {
                        if let Some(reason) =
                            unresolvable(doc, graph, &root_writes, &node.id, &path)
                        {
                            v.warnings.push(issue(
                                Some(&node.id),
                                format!("nodes[{index}].config.expr"),
                                "unknown_source",
                                reason,
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// Every template-bearing string in a node, with its document path.
///
/// All strings are candidates, so a `{{ … }}` anywhere in a config, input, or
/// output is compiled. The one exclusion is a `code` node's `source`, which is
/// source code and may legitimately contain braces.
fn template_uses<'a>(node: &'a Node, index: usize) -> Vec<TemplateUse<'a>> {
    let mut out = Vec::new();
    for (key, value) in &node.config {
        if node.kind == NodeKind::Code && key == "source" {
            continue;
        }
        collect(
            &format!("nodes[{index}].config.{key}"),
            value,
            true,
            &mut out,
        );
    }
    for (key, value) in &node.inputs {
        collect(
            &format!("nodes[{index}].inputs.{key}"),
            value,
            false,
            &mut out,
        );
    }
    for (key, value) in &node.outputs {
        collect(
            &format!("nodes[{index}].outputs.{key}"),
            value,
            false,
            &mut out,
        );
    }
    out
}

fn collect<'a>(path: &str, value: &'a Value, in_config: bool, out: &mut Vec<TemplateUse<'a>>) {
    match value {
        Value::String(text) => out.push(TemplateUse {
            path: path.to_string(),
            text,
            in_config,
        }),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect(&format!("{path}[{index}]"), item, in_config, out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                collect(&format!("{path}.{key}"), item, in_config, out);
            }
        }
        _ => {}
    }
}

/// The `$root.<key>` outputs each node writes into the top-level context.
fn root_writes_by_node(doc: &WorkflowDocument) -> BTreeMap<&str, Vec<String>> {
    doc.nodes
        .iter()
        .map(|node| {
            let keys = node
                .outputs
                .keys()
                .filter_map(|key| key.strip_prefix("$root.").map(str::to_string))
                .collect();
            (node.id.as_str(), keys)
        })
        .collect()
}

/// Why `path` cannot be resolved at `node_id`, if it cannot.
fn unresolvable(
    doc: &WorkflowDocument,
    graph: &Graph,
    root_writes: &BTreeMap<&str, Vec<String>>,
    node_id: &str,
    path: &str,
) -> Option<String> {
    let root = root_segment(path);

    if root == "variables" {
        let name = path.strip_prefix("variables.").map(root_segment);
        return match name {
            None => None,
            Some(name) if doc.variables.contains_key(name) => None,
            Some(name) => Some(format!("no variable `{name}` is declared")),
        };
    }

    if !graph.has_node(root) {
        return Some(format!(
            "`{root}` is not a node id or a built-in context key"
        ));
    }
    if root == node_id || graph.is_upstream(root, node_id) {
        return None;
    }
    if root_writes
        .get(root)
        .is_some_and(|keys| keys.iter().any(|key| key == root))
    {
        return None;
    }
    let written_by_upstream = root_writes.iter().any(|(id, keys)| {
        *id != root && keys.iter().any(|key| key == root) && graph.is_upstream(id, node_id)
    });
    if written_by_upstream {
        return None;
    }
    Some(format!(
        "`{root}` is not produced by any node upstream of `{node_id}`"
    ))
}

fn root_segment(path: &str) -> &str {
    let end = path.find(['.', '[']).unwrap_or(path.len());
    &path[..end]
}

// ---------------------------------------------------------------------------
// App manifest (§5.1)
// ---------------------------------------------------------------------------

fn check_app(app: Option<&AppManifest>, v: &mut Validation) {
    let Some(app) = app else {
        return;
    };

    if !is_workflow_id(&app.id) {
        v.errors.push(issue(
            None,
            "app.id",
            "invalid_id",
            format!("`{}` is not a valid app id", app.id),
        ));
    }
    if app.name.trim().is_empty() {
        v.errors.push(issue(
            None,
            "app.name",
            "missing_name",
            "an app name is required",
        ));
    }
    if let Some(icon) = &app.icon {
        let ok = is_node_id(icon) && !icon.contains('/');
        if !ok {
            v.errors.push(issue(
                None,
                "app.icon",
                "invalid_icon",
                "an icon must be a shell icon-set name, not a path or URL",
            ));
        }
    }

    let mut input_ids = BTreeSet::new();
    for (index, input) in app.inputs.iter().enumerate() {
        let at = |field: &str| format!("app.inputs[{index}].{field}");
        if !input_ids.insert(input.id.as_str()) {
            v.errors.push(issue(
                None,
                format!("app.inputs[{index}].id"),
                "duplicate_id",
                format!("input `{}` is declared twice", input.id),
            ));
        }
        if !APP_INPUT_TYPES.contains(&input.kind.as_str()) {
            v.errors.push(issue(
                None,
                at("type"),
                "invalid_app_input",
                format!("`{}` is not an app input type", input.kind),
            ));
        }
        if input.kind == "select" && input.options.is_empty() {
            v.errors.push(issue(
                None,
                at("options"),
                "invalid_app_input",
                "a `select` input needs at least one option",
            ));
        }
        if input.label.trim().is_empty() {
            v.errors.push(issue(
                None,
                at("label"),
                "missing_label",
                "an input needs a label",
            ));
        }
    }

    let mut output_ids = BTreeSet::new();
    for (index, output) in app.outputs.iter().enumerate() {
        if !output_ids.insert(output.id.as_str()) {
            v.errors.push(issue(
                None,
                format!("app.outputs[{index}].id"),
                "duplicate_id",
                format!("output `{}` is declared twice", output.id),
            ));
        }
        if !APP_OUTPUT_TYPES.contains(&output.kind.as_str()) {
            v.errors.push(issue(
                None,
                format!("app.outputs[{index}].type"),
                "invalid_app_output",
                format!("`{}` is not an app output type", output.kind),
            ));
        }
    }

    for (index, block) in app.layout.iter().enumerate() {
        let at = |field: &str| format!("app.layout[{index}].{field}");
        let Some(object) = block.as_object() else {
            v.errors.push(issue(
                None,
                format!("app.layout[{index}]"),
                "invalid_app_layout",
                "a layout block must be an object",
            ));
            continue;
        };
        let Some(kind) = object.get("type").and_then(Value::as_str) else {
            v.errors.push(issue(
                None,
                at("type"),
                "invalid_app_layout",
                "a layout block needs a `type`",
            ));
            continue;
        };
        if !APP_BLOCKS.contains(&kind) {
            v.errors.push(issue(
                None,
                at("type"),
                "invalid_app_layout",
                format!("`{kind}` is not a layout block type"),
            ));
            continue;
        }
        match kind {
            "form" => {
                for field in object
                    .get("fields")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let name = field.as_str().unwrap_or_default();
                    if !input_ids.contains(name) {
                        v.errors.push(issue(
                            None,
                            at("fields"),
                            "unknown_reference",
                            format!("form references undeclared input `{name}`"),
                        ));
                    }
                }
            }
            "output" => {
                let name = object
                    .get("ref")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !output_ids.contains(name) {
                    v.errors.push(issue(
                        None,
                        at("ref"),
                        "unknown_reference",
                        format!("output block references undeclared output `{name}`"),
                    ));
                }
            }
            "button" => {
                if object.get("label").and_then(Value::as_str).is_none() {
                    v.errors.push(issue(
                        None,
                        at("label"),
                        "invalid_app_layout",
                        "a button needs a label",
                    ));
                }
                if object.get("action").and_then(Value::as_str) != Some("run") {
                    v.errors.push(issue(
                        None,
                        at("action"),
                        "invalid_app_layout",
                        "the only supported button action is `run`",
                    ));
                }
            }
            "history" => {
                if let Some(limit) = object.get("limit") {
                    if limit.as_u64().is_none_or_zero() {
                        v.errors.push(issue(
                            None,
                            at("limit"),
                            "invalid_app_layout",
                            "a history limit must be a positive integer",
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

/// `Option<u64>` helper so the `history` check reads cleanly.
trait IsNoneOrZero {
    fn is_none_or_zero(&self) -> bool;
}

impl IsNoneOrZero for Option<u64> {
    fn is_none_or_zero(&self) -> bool {
        matches!(self, Some(0))
    }
}

// ---------------------------------------------------------------------------
// Lints (§1.9)
// ---------------------------------------------------------------------------

fn check_lints(doc: &WorkflowDocument, graph: &Graph, v: &mut Validation) {
    let mut reaches_end = BTreeSet::new();
    for node in &doc.nodes {
        if node.kind == NodeKind::End {
            for other in &doc.nodes {
                if other.id != node.id && graph.is_upstream(&other.id, &node.id) {
                    reaches_end.insert(other.id.as_str());
                }
            }
        }
    }

    for (index, node) in doc.nodes.iter().enumerate() {
        if !graph.is_loop_body(&node.id)
            && !reaches_end.contains(node.id.as_str())
            && node.kind != NodeKind::End
        {
            v.warnings.push(issue(
                Some(&node.id),
                format!("nodes[{index}]"),
                "unused_node",
                format!("node `{}` cannot reach an `end` node", node.id),
            ));
        }

        let ports = node.kind.output_ports();
        if !ports.is_empty() {
            let mut wired: BTreeSet<&str> = BTreeSet::new();
            for edge in doc.edges.iter().filter(|edge| edge.from == node.id) {
                match edge.from_port.as_deref() {
                    Some(port) => {
                        wired.insert(port);
                    }
                    None => {
                        wired.insert(ports[0]);
                    }
                }
            }
            for port in ports {
                if !wired.contains(port) {
                    v.warnings.push(issue(
                        Some(&node.id),
                        format!("nodes[{index}]"),
                        "unreachable_port",
                        format!("output port `{port}` of `{}` is not connected", node.id),
                    ));
                }
            }
        }

        if let Some(retry) = &node.retry {
            let timeout_ms = node.timeout_s.unwrap_or(DEFAULT_NODE_TIMEOUT_S) * 1000;
            let total = backoff_total_ms(retry);
            if retry.max > 0 && total > u128::from(timeout_ms) {
                v.warnings.push(issue(
                    Some(&node.id),
                    format!("nodes[{index}].retry"),
                    "backoff_exceeds_timeout",
                    format!("total backoff {total} ms exceeds the node timeout of {timeout_ms} ms"),
                ));
            }
        }
    }
}

fn backoff_total_ms(retry: &crate::document::RetryPolicy) -> u128 {
    let mut total: u128 = 0;
    let mut delay = retry.backoff_ms as f64;
    for _ in 0..retry.max {
        total += delay as u128;
        delay *= retry.backoff_multiplier.max(1.0);
    }
    total
}
