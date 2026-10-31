//! The run context (spec §1.4, §1.5).
//!
//! The context is a JSON object shared by every node of a run. Nodes read it
//! through explicit `inputs` templates and write their outputs under their own
//! id, so the graph stays readable and every value flow is visible in the
//! document.
//!
//! Values larger than [`SPILL_THRESHOLD_BYTES`] are written to the run's
//! artifact directory and replaced by a `{ "$ref", "bytes", "sha256" }`
//! reference; the live context is capped at [`LIVE_CONTEXT_CAP_BYTES`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::document::{JsonMap, Node};
use crate::error::WorkflowError;
use crate::template::{render_value, RenderContext, SecretResolver};

/// Values above this size are spilled to an artifact file.
pub const SPILL_THRESHOLD_BYTES: usize = 1024 * 1024;

/// The live-context ceiling.
pub const LIVE_CONTEXT_CAP_BYTES: usize = 32 * 1024 * 1024;

/// The run context: node outputs, the trigger payload, and variables.
#[derive(Debug, Clone)]
pub struct RunContext {
    root: Map<String, Value>,
    artifacts_dir: Option<PathBuf>,
}

impl Default for RunContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RunContext {
    /// An empty context with no artifact directory.
    pub fn new() -> Self {
        Self {
            root: Map::new(),
            artifacts_dir: None,
        }
    }

    /// An empty context that spills large values into `dir`.
    pub fn with_artifacts(dir: impl Into<PathBuf>) -> Self {
        Self {
            root: Map::new(),
            artifacts_dir: Some(dir.into()),
        }
    }

    /// A context for `run_id`, spilling into
    /// `$FERGENTS_HOME/workflows/runs/<run-id>/artifacts`.
    pub fn for_run(run_id: &str) -> Self {
        Self::with_artifacts(default_artifacts_dir(run_id))
    }

    /// Rebuild a context from a checkpoint snapshot.
    pub fn from_snapshot(snapshot: &Value) -> Self {
        Self {
            root: snapshot.as_object().cloned().unwrap_or_default(),
            artifacts_dir: None,
        }
    }

    /// Rebuild a context from a checkpoint snapshot, keeping the spill
    /// directory.
    pub fn from_snapshot_with_artifacts(snapshot: &Value, dir: impl Into<PathBuf>) -> Self {
        Self {
            root: snapshot.as_object().cloned().unwrap_or_default(),
            artifacts_dir: Some(dir.into()),
        }
    }

    /// Attach an artifact directory.
    pub fn set_artifacts_dir(&mut self, dir: impl Into<PathBuf>) {
        self.artifacts_dir = Some(dir.into());
    }

    /// The artifact directory, if any.
    pub fn artifacts_dir(&self) -> Option<&Path> {
        self.artifacts_dir.as_deref()
    }

    /// The snapshot to persist.
    pub fn snapshot(&self) -> Value {
        Value::Object(self.root.clone())
    }

    /// Read a dotted path. Artifact references are resolved on demand.
    pub fn get(&self, path: &str) -> Result<Option<Value>, WorkflowError> {
        let Some(value) = lookup(&self.root, path) else {
            return Ok(None);
        };
        self.resolve_ref(value).map(Some)
    }

    /// Write a dotted path, creating intermediate objects.
    pub fn set(&mut self, path: &str, value: Value) -> Result<(), WorkflowError> {
        let segments = split_path(path)
            .ok_or_else(|| WorkflowError::Invalid(format!("invalid context path `{path}`")))?;
        let value = self.spill(value)?;
        let projected = self.live_bytes() + serde_json::to_vec(&value)?.len();
        if projected > LIVE_CONTEXT_CAP_BYTES {
            return Err(WorkflowError::BudgetExceeded(format!(
                "context would exceed {LIVE_CONTEXT_CAP_BYTES} bytes"
            )));
        }
        insert(&mut self.root, &segments, value);
        Ok(())
    }

    /// Write a node's output object under its id.
    pub fn set_node(&mut self, node_id: &str, value: Value) -> Result<(), WorkflowError> {
        self.set(node_id, value)
    }

    /// Make the trigger payload readable as `{{ trigger.… }}`.
    pub fn set_trigger(&mut self, payload: Value) -> Result<(), WorkflowError> {
        self.set("trigger", payload)
    }

    /// Make workflow variables readable as `{{ variables.… }}`.
    pub fn set_variables(&mut self, variables: &JsonMap) -> Result<(), WorkflowError> {
        let map = variables
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        self.set("variables", Value::Object(map))
    }

    /// A render context over this data.
    pub fn render_context(
        &self,
        run_id: &str,
        workflow_id: &str,
        now: DateTime<Utc>,
        secrets: Arc<dyn SecretResolver + Send + Sync>,
    ) -> RenderContext {
        RenderContext {
            data: self.snapshot(),
            result: None,
            run_id: run_id.to_string(),
            workflow_id: workflow_id.to_string(),
            now,
            secrets,
        }
    }

    /// Resolve a node's `inputs` templates into a JSON object.
    pub fn resolve_inputs(
        &self,
        node: &Node,
        render: &RenderContext,
    ) -> Result<Value, WorkflowError> {
        let mut out = Map::new();
        for (key, template) in &node.inputs {
            out.insert(key.clone(), render_value(template, render)?);
        }
        Ok(Value::Object(out))
    }

    /// Persist a node's result following its `outputs` mapping.
    ///
    /// With no mapping the result is stored under the node id. With a mapping,
    /// each key is a path inside the node's namespace, or `$root.<path>` to
    /// write the root context.
    pub fn store_outputs(
        &mut self,
        node: &Node,
        result: &Value,
        render: &RenderContext,
    ) -> Result<(), WorkflowError> {
        if node.outputs.is_empty() {
            return self.set_node(&node.id, result.clone());
        }
        let mut with_result = render.clone();
        with_result.result = Some(result.clone());
        for (key, template) in &node.outputs {
            let rendered = render_value(template, &with_result)?;
            match key.strip_prefix("$root.") {
                Some(path) if !path.is_empty() => self.set(path, rendered)?,
                Some(_) => {
                    return Err(WorkflowError::Invalid(format!(
                        "node `{}` has an empty `$root` output path",
                        node.id
                    )))
                }
                None => self.set(&format!("{}.{}", node.id, key), rendered)?,
            }
        }
        Ok(())
    }

    /// The serialized size of the live context.
    pub fn live_bytes(&self) -> usize {
        serde_json::to_vec(&Value::Object(self.root.clone())).map_or(0, |bytes| bytes.len())
    }

    /// Replace a large value with an artifact reference.
    fn spill(&self, value: Value) -> Result<Value, WorkflowError> {
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() <= SPILL_THRESHOLD_BYTES {
            return Ok(value);
        }
        let Some(dir) = &self.artifacts_dir else {
            return Ok(value);
        };
        std::fs::create_dir_all(dir)?;
        let sha256 = sha256_hex(&bytes);
        let name = format!("{sha256}.json");
        let path = dir.join(&name);
        if !path.exists() {
            std::fs::write(&path, &bytes)?;
        }
        Ok(json!({
            "$ref": name,
            "bytes": bytes.len(),
            "sha256": sha256,
        }))
    }

    /// Resolve `{ "$ref": … }` on demand.
    fn resolve_ref(&self, value: &Value) -> Result<Value, WorkflowError> {
        let Some(map) = value.as_object() else {
            return Ok(value.clone());
        };
        let Some(name) = map.get("$ref").and_then(Value::as_str) else {
            return Ok(value.clone());
        };
        let Some(dir) = &self.artifacts_dir else {
            return Ok(value.clone());
        };
        let path = dir.join(name);
        let bytes = std::fs::read(&path)?;
        if let Some(expected) = map.get("sha256").and_then(Value::as_str) {
            let actual = sha256_hex(&bytes);
            if actual != expected {
                return Err(WorkflowError::VersionConflict(format!(
                    "artifact `{name}` does not match its digest"
                )));
            }
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// `$FERGENTS_HOME/workflows/runs/<run-id>/artifacts`, with the same home
/// resolution as `fergents_core::default_data_dir` (the workflow crate does not
/// depend on the kernel crate).
pub fn default_artifacts_dir(run_id: &str) -> PathBuf {
    let home = std::env::var("FERGENTS_HOME").map_or_else(
        |_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".fergents")
        },
        PathBuf::from,
    );
    home.join("workflows")
        .join("runs")
        .join(run_id)
        .join("artifacts")
}

/// Split `a.b[0].c` into `["a", "b", "0", "c"]`.
fn split_path(path: &str) -> Option<Vec<String>> {
    if path.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if current.is_empty() {
                    return None;
                }
                segments.push(std::mem::take(&mut current));
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
                let mut index = String::new();
                loop {
                    match chars.next() {
                        Some(']') => break,
                        Some(digit) if digit.is_ascii_digit() => index.push(digit),
                        _ => return None,
                    }
                }
                if index.is_empty() {
                    return None;
                }
                segments.push(index);
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    if segments.is_empty() || segments.iter().any(String::is_empty) {
        return None;
    }
    Some(segments)
}

fn lookup<'a>(root: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let segments = split_path(path)?;
    let mut current: &Value = root.get(&segments[0])?;
    for segment in &segments[1..] {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn insert(root: &mut Map<String, Value>, segments: &[String], value: Value) {
    if segments.len() == 1 {
        root.insert(segments[0].clone(), value);
        return;
    }
    let entry = root
        .entry(segments[0].clone())
        .or_insert_with(|| Value::Object(Map::new()));
    match entry {
        Value::Object(map) => insert(map, &segments[1..], value),
        other => {
            *other = Value::Object(Map::new());
            if let Value::Object(map) = other {
                insert(map, &segments[1..], value);
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
