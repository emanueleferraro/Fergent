//! Context checkpoints (spec §3.3, §3.6).
//!
//! A checkpoint is written after each completed step: the node id, the step
//! sequence, and a snapshot of the run context. Large values inside the
//! snapshot are already artifact references, so a checkpoint stays small.
//!
//! Checkpoints carry the document digest they were written against. Restoring
//! against a document that changed underneath the run fails with
//! `workflow_version_conflict` rather than silently resuming on stale nodes.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::context::RunContext;
use crate::document::WorkflowDocument;
use crate::error::WorkflowError;
use crate::store::{CheckpointRecord, RunStore};

/// The envelope marker written into every checkpoint.
pub const ENVELOPE: &str = "fergents.workflow.checkpoint.v1";

/// Writes and restores the checkpoints of one run.
#[derive(Debug, Clone)]
pub struct Checkpointer {
    run_id: String,
    workflow_id: String,
    version: i64,
    digest: String,
}

/// A checkpoint that has been read back and verified.
#[derive(Debug, Clone)]
pub struct RestoredCheckpoint {
    /// The step sequence number of the snapshot.
    pub seq: i64,
    /// The node whose completion produced the snapshot.
    pub node_id: Option<String>,
    /// The restored context.
    pub context: RunContext,
    /// When the snapshot was written.
    pub created_at: i64,
}

impl Checkpointer {
    /// A checkpointer for a run pinned to `digest`.
    pub fn new(
        run_id: impl Into<String>,
        workflow_id: impl Into<String>,
        version: i64,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            workflow_id: workflow_id.into(),
            version,
            digest: digest.into(),
        }
    }

    /// A checkpointer for a run of `doc`, pinned at `version`.
    pub fn for_document(
        run_id: &str,
        doc: &WorkflowDocument,
        version: i64,
    ) -> Result<Self, WorkflowError> {
        Ok(Self::new(run_id, doc.id.clone(), version, doc.digest()?))
    }

    /// The document digest this run is pinned to.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The run this checkpointer belongs to.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Write a checkpoint for `context` and return its sequence number.
    pub fn save(
        &self,
        store: &RunStore,
        node_id: Option<&str>,
        context: &RunContext,
    ) -> Result<i64, WorkflowError> {
        let envelope = json!({
            "envelope": ENVELOPE,
            "workflow_id": self.workflow_id,
            "version": self.version,
            "digest": self.digest,
            "context": context.snapshot(),
        });
        store.write_checkpoint(&self.run_id, node_id, &envelope)
    }

    /// Read the newest checkpoint, or `None` when the run has none.
    pub fn load(
        &self,
        store: &RunStore,
        artifacts_dir: impl Into<PathBuf>,
    ) -> Result<Option<RestoredCheckpoint>, WorkflowError> {
        match store.latest_checkpoint(&self.run_id)? {
            Some(record) => Self::restore(&record, &self.digest, artifacts_dir).map(Some),
            None => Ok(None),
        }
    }

    /// Decode `record`, verifying its digest against `expected`.
    pub fn restore(
        record: &CheckpointRecord,
        expected: &str,
        artifacts_dir: impl Into<PathBuf>,
    ) -> Result<RestoredCheckpoint, WorkflowError> {
        let envelope = record.context.as_object().ok_or_else(|| {
            WorkflowError::Invalid(format!(
                "checkpoint {} of run `{}` is not an object",
                record.seq, record.run_id
            ))
        })?;

        match envelope.get("envelope").and_then(Value::as_str) {
            Some(ENVELOPE) => {}
            Some(other) => {
                return Err(WorkflowError::Invalid(format!(
                    "checkpoint {} of run `{}` has unknown envelope `{other}`",
                    record.seq, record.run_id
                )))
            }
            None => {
                return Err(WorkflowError::Invalid(format!(
                    "checkpoint {} of run `{}` is missing its envelope",
                    record.seq, record.run_id
                )))
            }
        }

        let found = envelope
            .get("digest")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                WorkflowError::Invalid(format!(
                    "checkpoint {} of run `{}` has no document digest",
                    record.seq, record.run_id
                ))
            })?;
        if found != expected {
            return Err(WorkflowError::VersionConflict(format!(
                "checkpoint {} of run `{}` was written against document digest {found}, \
                 the current document is {expected}",
                record.seq, record.run_id
            )));
        }

        let snapshot = envelope
            .get("context")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        Ok(RestoredCheckpoint {
            seq: record.seq,
            node_id: record.node_id.clone(),
            context: RunContext::from_snapshot_with_artifacts(&snapshot, artifacts_dir),
            created_at: record.created_at,
        })
    }
}
