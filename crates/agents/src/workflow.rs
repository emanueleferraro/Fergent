//! The spec → plan → tasks workflow with approval gates
//! (spec: Spec / Plan / Tasks Workflow with Approval Gates, decision D7).
//!
//! Artifacts are written under `<data_dir>/specs/<slug>/` (`spec.md`,
//! `plan.md`, `tasks.md`). Gates are persisted in `runs` / `run_steps` /
//! `approvals`, and a run pauses in an `awaiting_*_approval` state until the
//! client approves or rejects it. On approval the artifact is re-read from
//! disk, so edits made before approval are what the workflow continues from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::{now_ms, EventBus};
use fergents_storage::{Storage, Value};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// Key under which the artifact map also records the slug.
const SLUG_KEY: &str = "slug";

/// How often a run pauses for human approval (decision D7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyMode {
    /// Pause at every gate.
    #[default]
    Supervised,
    /// Pause at the spec and plan gates; tasks run without asking.
    Batch,
    /// Never pause; every automatic gate is audited.
    Auto,
}

impl AutonomyMode {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            AutonomyMode::Supervised => "supervised",
            AutonomyMode::Batch => "batch",
            AutonomyMode::Auto => "auto",
        }
    }

    /// Parse a label, defaulting to [`AutonomyMode::Supervised`].
    pub fn parse(value: &str) -> Self {
        match value {
            "batch" => AutonomyMode::Batch,
            "auto" => AutonomyMode::Auto,
            _ => AutonomyMode::Supervised,
        }
    }

    /// Whether the run pauses at `gate` in this mode.
    pub fn pauses_at(self, gate: Gate) -> bool {
        match self {
            AutonomyMode::Supervised => true,
            AutonomyMode::Batch => gate != Gate::Task,
            AutonomyMode::Auto => false,
        }
    }
}

/// An approval gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// The `spec.md` gate.
    Spec,
    /// The `plan.md` gate.
    Plan,
    /// The `tasks.md` gate.
    Task,
    /// A file-diff gate (used by reversibility, task 2.9).
    Diff,
}

impl Gate {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            Gate::Spec => "spec",
            Gate::Plan => "plan",
            Gate::Task => "task",
            Gate::Diff => "diff",
        }
    }

    /// Parse a label.
    pub fn parse(value: &str) -> Option<Gate> {
        match value {
            "spec" => Some(Gate::Spec),
            "plan" => Some(Gate::Plan),
            "task" => Some(Gate::Task),
            "diff" => Some(Gate::Diff),
            _ => None,
        }
    }

    /// The artifact file name (`spec.md`, ...).
    pub fn artifact(self) -> &'static str {
        match self {
            Gate::Spec => "spec.md",
            Gate::Plan => "plan.md",
            Gate::Task => "tasks.md",
            Gate::Diff => "diff.md",
        }
    }

    /// The next gate in the sequence, if any.
    pub fn next(self) -> Option<Gate> {
        match self {
            Gate::Spec => Some(Gate::Plan),
            Gate::Plan => Some(Gate::Task),
            Gate::Task | Gate::Diff => None,
        }
    }

    /// The run state while waiting at this gate.
    pub fn waiting_state(self) -> &'static str {
        match self {
            Gate::Spec => "awaiting_spec_approval",
            Gate::Plan => "awaiting_plan_approval",
            Gate::Task => "awaiting_task_approval",
            Gate::Diff => "awaiting_diff_approval",
        }
    }

    fn order(self) -> u32 {
        match self {
            Gate::Spec => 0,
            Gate::Plan => 1,
            Gate::Task => 2,
            Gate::Diff => 3,
        }
    }
}

/// A persisted workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// The run id.
    pub id: String,
    /// The originating chat session, when any.
    pub session_id: Option<String>,
    /// The artifact slug.
    pub slug: String,
    /// The autonomy mode.
    pub mode: AutonomyMode,
    /// The raw `runs.state` label (`awaiting_spec_approval`, `completed`, ...).
    pub state: String,
    /// The gate currently awaiting approval.
    pub pending_gate: Option<Gate>,
    /// Written artifact paths, keyed by gate label.
    pub artifacts: BTreeMap<String, String>,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
}

impl WorkflowRun {
    /// Whether the run is waiting for approval.
    pub fn is_awaiting(&self) -> bool {
        self.pending_gate.is_some()
    }
}

/// Parameters for starting a workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct StartWorkflow {
    /// A URL-safe slug; used as the artifact directory name.
    pub slug: String,
    /// The task the workflow is for.
    pub task: String,
    /// Optional session id.
    pub session_id: Option<String>,
    /// The autonomy mode.
    pub mode: AutonomyMode,
}

/// Reads and writes workflow runs, and generates artifacts on disk.
pub struct WorkflowService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    root: PathBuf,
}

impl WorkflowService {
    /// Wrap `storage`, writing artifacts under `root` (`<data_dir>/specs`).
    pub fn new(storage: Arc<dyn Storage>, bus: Arc<EventBus>, root: impl Into<PathBuf>) -> Self {
        Self {
            storage,
            bus,
            root: root.into(),
        }
    }

    /// The artifact directory for a slug.
    pub fn artifact_dir(&self, slug: &str) -> PathBuf {
        self.root.join(slug)
    }

    /// Start a run: write `spec.md`, then pause or advance per the mode.
    pub fn start(&self, request: StartWorkflow) -> Result<WorkflowRun, AgentError> {
        let slug = validate_slug(&request.slug)?;
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO runs (id, session_id, kind, mode, state, created_at, updated_at) \
             VALUES (?1, ?2, 'spec_workflow', ?3, 'running', ?4, ?4)",
            &[
                Value::String(id.clone()),
                request
                    .session_id
                    .clone()
                    .map_or(Value::Null, Value::String),
                Value::String(request.mode.as_str().to_string()),
                Value::from(now),
            ],
        )?;
        self.insert_step(&id, Gate::Spec)?;

        let mut run = WorkflowRun {
            id: id.clone(),
            session_id: request.session_id,
            slug: slug.clone(),
            mode: request.mode,
            state: "running".to_string(),
            pending_gate: None,
            artifacts: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        };
        run.artifacts.insert(SLUG_KEY.to_string(), slug.clone());
        self.write_artifact(&slug, Gate::Spec, &spec_template(&request.task))?;
        self.record_artifact(&mut run, Gate::Spec)?;
        self.emit(
            "run.created",
            &run,
            serde_json::json!({ "mode": run.mode.as_str() }),
        );

        self.advance(run, Gate::Spec)
    }

    /// Fetch a run.
    pub fn get(&self, run_id: &str) -> Result<Option<WorkflowRun>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM runs WHERE id = ?1",
            &[Value::String(run_id.to_string())],
        )?;
        Ok(rows.first().map(|row| self.run_from_row(row)))
    }

    /// List workflow runs, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<WorkflowRun>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM runs WHERE kind = 'spec_workflow' ORDER BY created_at DESC LIMIT ?1",
            &[Value::from(limit as i64)],
        )?;
        Ok(rows.iter().map(|row| self.run_from_row(row)).collect())
    }

    /// Read an artifact from disk (the source of truth after user edits).
    pub fn artifact(&self, run_id: &str, gate: Gate) -> Result<Option<String>, AgentError> {
        let Some(run) = self.get(run_id)? else {
            return Ok(None);
        };
        let path = self.artifact_dir(&run.slug).join(gate.artifact());
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(Some(content)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(AgentError::Io(err)),
        }
    }

    /// The pending approvals of a run.
    pub fn approvals(&self, run_id: &str) -> Result<Vec<Approval>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM approvals WHERE run_id = ?1 ORDER BY created_at, rowid",
            &[Value::String(run_id.to_string())],
        )?;
        Ok(rows
            .iter()
            .map(|row| Approval {
                id: str_col(row, "id"),
                gate: Gate::parse(&str_col(row, "gate")).unwrap_or(Gate::Spec),
                status: str_col(row, "status"),
                created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            })
            .collect())
    }

    /// Approve the pending gate, then continue to the next gate or finish.
    ///
    /// The artifact is re-read from disk first, so edits made before approval
    /// are what the workflow continues from.
    pub fn approve(&self, run_id: &str, gate: Option<Gate>) -> Result<WorkflowRun, AgentError> {
        let mut run = self
            .get(run_id)?
            .ok_or_else(|| AgentError::UnknownRun(run_id.to_string()))?;
        let gate = gate
            .or(run.pending_gate)
            .ok_or_else(|| AgentError::Tool(format!("run {run_id} has no pending approval")))?;
        let content = self.artifact(run_id, gate)?.ok_or_else(|| {
            AgentError::Tool(format!("missing artifact for gate {}", gate.as_str()))
        })?;
        let edited = self.decide_approval(run_id, gate, "approved")?;
        self.finish_step(run_id, gate, if edited { "edited" } else { "approved" })?;
        self.emit(
            "run.approved",
            &run,
            serde_json::json!({ "gate": gate.as_str(), "edited": edited }),
        );

        match gate.next() {
            Some(next_gate) => {
                self.write_artifact(&run.slug, next_gate, &template_for(next_gate, &content))?;
                self.insert_step(run_id, next_gate)?;
                self.record_artifact(&mut run, next_gate)?;
                self.advance(run, next_gate)
            }
            None => Ok(self.complete(run)),
        }
    }

    /// Reject the pending gate; the run stops.
    pub fn reject(&self, run_id: &str, reason: Option<&str>) -> Result<WorkflowRun, AgentError> {
        let mut run = self
            .get(run_id)?
            .ok_or_else(|| AgentError::UnknownRun(run_id.to_string()))?;
        if let Some(gate) = run.pending_gate {
            self.reject_approval(run_id, gate, reason)?;
            self.finish_step(run_id, gate, "rejected")?;
        }
        self.set_state(run_id, "rejected")?;
        run.state = "rejected".to_string();
        run.pending_gate = None;
        run.updated_at = now_ms();
        self.emit(
            "run.rejected",
            &run,
            serde_json::json!({ "reason": reason }),
        );
        Ok(run)
    }

    /// Pause at `gate`, or pass through when the mode says so.
    fn advance(&self, mut run: WorkflowRun, gate: Gate) -> Result<WorkflowRun, AgentError> {
        if run.mode.pauses_at(gate) {
            self.set_state(&run.id, gate.waiting_state())?;
            self.request_approval(&run.id, &run.slug, gate)?;
            run.state = gate.waiting_state().to_string();
            run.pending_gate = Some(gate);
            run.updated_at = now_ms();
            self.emit(
                "run.approval_requested",
                &run,
                serde_json::json!({ "gate": gate.as_str() }),
            );
            self.emit(
                "run.paused",
                &run,
                serde_json::json!({ "gate": gate.as_str() }),
            );
            return Ok(run);
        }

        // Auto-approved gate: recorded in `approvals` and audited.
        self.decide_approval_auto(&run.id, gate, &run.slug)?;
        self.finish_step(&run.id, gate, "auto_approved")?;
        match gate.next() {
            Some(next_gate) => {
                let content = self.artifact(&run.id, gate)?.unwrap_or_default();
                self.write_artifact(&run.slug, next_gate, &template_for(next_gate, &content))?;
                self.insert_step(&run.id, next_gate)?;
                self.record_artifact(&mut run, next_gate)?;
                self.advance(run, next_gate)
            }
            None => Ok(self.complete(run)),
        }
    }

    fn complete(&self, mut run: WorkflowRun) -> WorkflowRun {
        let _ = self.set_state(&run.id, "completed");
        run.state = "completed".to_string();
        run.pending_gate = None;
        run.updated_at = now_ms();
        self.emit("run.completed", &run, serde_json::json!({}));
        run
    }

    fn write_artifact(&self, slug: &str, gate: Gate, content: &str) -> Result<(), AgentError> {
        let dir = self.artifact_dir(slug);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(gate.artifact()), content)?;
        Ok(())
    }

    fn record_artifact(&self, run: &mut WorkflowRun, gate: Gate) -> Result<(), AgentError> {
        let path = self.artifact_dir(&run.slug).join(gate.artifact());
        run.artifacts.insert(
            gate.as_str().to_string(),
            path.to_string_lossy().into_owned(),
        );
        self.storage.execute(
            "UPDATE runs SET artifacts = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::String(serde_json::to_string(&run.artifacts)?),
                Value::from(now_ms()),
                Value::String(run.id.clone()),
            ],
        )?;
        Ok(())
    }

    fn set_state(&self, run_id: &str, state: &str) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE runs SET state = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::String(state.to_string()),
                Value::from(now_ms()),
                Value::String(run_id.to_string()),
            ],
        )?;
        Ok(())
    }

    fn insert_step(&self, run_id: &str, gate: Gate) -> Result<(), AgentError> {
        self.storage.execute(
            "INSERT INTO run_steps (id, run_id, seq, kind, state, started_at) \
             VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(run_id.to_string()),
                Value::from(i64::from(gate.order())),
                Value::String(gate.as_str().to_string()),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    fn finish_step(&self, run_id: &str, gate: Gate, state: &str) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE run_steps SET state = ?1, finished_at = ?2 \
             WHERE run_id = ?3 AND kind = ?4 AND finished_at IS NULL",
            &[
                Value::String(state.to_string()),
                Value::from(now_ms()),
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
            ],
        )?;
        Ok(())
    }

    fn request_approval(&self, run_id: &str, slug: &str, gate: Gate) -> Result<(), AgentError> {
        let path = self.artifact_dir(slug).join(gate.artifact());
        let sha = std::fs::read_to_string(&path)
            .map(|content| hash(&content))
            .unwrap_or(0);
        self.storage.execute(
            "INSERT INTO approvals (id, run_id, gate, ref, status, payload, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
                Value::String(path.to_string_lossy().into_owned()),
                Value::String(serde_json::json!({ "sha": sha }).to_string()),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    /// Record an automatic (mode-driven) approval.
    fn decide_approval_auto(&self, run_id: &str, gate: Gate, slug: &str) -> Result<(), AgentError> {
        let path = self.artifact_dir(slug).join(gate.artifact());
        self.storage.execute(
            "INSERT INTO approvals (id, run_id, gate, ref, status, decided_by, decided_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'approved', 'autonomy', ?5, ?5)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
                Value::String(path.to_string_lossy().into_owned()),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    /// Approve the newest pending approval for `gate`, reporting whether the
    /// artifact was edited on disk since the approval was requested.
    fn decide_approval(&self, run_id: &str, gate: Gate, status: &str) -> Result<bool, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM approvals WHERE run_id = ?1 AND gate = ?2 AND status = 'pending' \
             ORDER BY created_at DESC LIMIT 1",
            &[
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
            ],
        )?;
        let edited = rows.first().is_some_and(|row| {
            let expected = row
                .get("payload")
                .and_then(Value::as_str)
                .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
                .and_then(|payload| payload.get("sha").and_then(Value::as_i64))
                .unwrap_or(0);
            row.get("ref")
                .and_then(Value::as_str)
                .and_then(|path| std::fs::read_to_string(Path::new(path)).ok())
                .is_some_and(|content| hash(&content) != expected)
        });
        let final_status = if edited && status == "approved" {
            "edited"
        } else {
            status
        };
        self.storage.execute(
            "UPDATE approvals SET status = ?1, decided_by = 'user', decided_at = ?2 \
             WHERE run_id = ?3 AND gate = ?4 AND status = 'pending'",
            &[
                Value::String(final_status.to_string()),
                Value::from(now_ms()),
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
            ],
        )?;
        Ok(edited)
    }

    fn reject_approval(
        &self,
        run_id: &str,
        gate: Gate,
        reason: Option<&str>,
    ) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE approvals SET status = 'rejected', payload = ?1, decided_by = 'user', \
             decided_at = ?2 WHERE run_id = ?3 AND gate = ?4 AND status = 'pending'",
            &[
                Value::String(serde_json::json!({ "reason": reason }).to_string()),
                Value::from(now_ms()),
                Value::String(run_id.to_string()),
                Value::String(gate.as_str().to_string()),
            ],
        )?;
        Ok(())
    }

    fn emit(&self, kind: &str, run: &WorkflowRun, extra: serde_json::Value) {
        let mut payload = serde_json::json!({
            "run_id": run.id,
            "slug": run.slug,
            "state": run.state,
        });
        if let (Some(target), Some(source)) = (payload.as_object_mut(), extra.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        if let Err(err) = self.bus.publish(kind, "runs", payload) {
            tracing::warn!(kind, error = %err, "failed to publish run event");
        }
    }

    fn run_from_row(&self, row: &fergents_storage::Row) -> WorkflowRun {
        let state = str_col(row, "state");
        let artifacts: BTreeMap<String, String> = row
            .get("artifacts")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        WorkflowRun {
            id: str_col(row, "id"),
            session_id: row
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            slug: artifacts.get(SLUG_KEY).cloned().unwrap_or_default(),
            mode: AutonomyMode::parse(&str_col(row, "mode")),
            pending_gate: state
                .strip_prefix("awaiting_")
                .and_then(|rest| rest.strip_suffix("_approval"))
                .and_then(Gate::parse),
            state,
            artifacts,
            created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        }
    }
}

/// A gate decision record, as returned to clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    /// The approval id.
    pub id: String,
    /// The gate.
    pub gate: Gate,
    /// `pending`, `approved`, `rejected`, or `edited`.
    pub status: String,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
}

fn hash(content: &str) -> i64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish() as i64
}

fn validate_slug(slug: &str) -> Result<String, AgentError> {
    let valid = !slug.is_empty()
        && slug.len() <= 64
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if valid {
        Ok(slug.to_string())
    } else {
        Err(AgentError::InvalidDefinition(format!(
            "invalid slug {slug:?}: use [a-z0-9-_] (max 64)"
        )))
    }
}

fn spec_template(task: &str) -> String {
    format!(
        "# Spec\n\n## Task\n\n{task}\n\n## Why\n\n_Why this change is needed._\n\n\
         ## What changes\n\n_Describe the change._\n\n## Acceptance\n\n- [ ] ...\n"
    )
}

fn template_for(gate: Gate, previous: &str) -> String {
    match gate {
        Gate::Spec => spec_template(previous),
        Gate::Plan => format!(
            "# Plan\n\n_Derived from the approved spec._\n\n1. ...\n\n\
             ## Approved spec\n\n{previous}\n"
        ),
        Gate::Task => format!(
            "# Tasks\n\n_Derived from the approved plan._\n\n- [ ] 1. ...\n\n\
             ## Approved plan\n\n{previous}\n"
        ),
        Gate::Diff => format!("# Diff\n\n{previous}\n"),
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_and_modes_behave() {
        assert_eq!(Gate::Spec.next(), Some(Gate::Plan));
        assert_eq!(Gate::Plan.next(), Some(Gate::Task));
        assert_eq!(Gate::Task.next(), None);
        assert_eq!(Gate::parse("plan"), Some(Gate::Plan));
        assert_eq!(Gate::Spec.waiting_state(), "awaiting_spec_approval");

        assert!(AutonomyMode::Supervised.pauses_at(Gate::Task));
        assert!(AutonomyMode::Batch.pauses_at(Gate::Spec));
        assert!(!AutonomyMode::Batch.pauses_at(Gate::Task));
        assert!(!AutonomyMode::Auto.pauses_at(Gate::Spec));
        assert_eq!(
            AutonomyMode::parse(AutonomyMode::Batch.as_str()),
            AutonomyMode::Batch
        );
    }

    #[test]
    fn slugs_are_validated() {
        assert!(validate_slug("agent-runtime-phase-2").is_ok());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("../escape").is_err());
        assert!(validate_slug("Upper").is_err());
    }
}
