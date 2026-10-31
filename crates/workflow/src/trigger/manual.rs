//! The manual trigger (spec §2.6).
//!
//! A manual run starts from the canvas, the app surface, the CLI, or
//! `workflow.run`. It carries no schedule and no configuration: the only
//! variability is the payload the caller supplies, which becomes
//! `ctx.trigger`.

use serde_json::{json, Value};

use crate::error::WorkflowError;
use crate::trigger::{FireFn, TriggerFire};

/// The trigger id used when the document declares none.
pub const MANUAL_TRIGGER_ID: &str = "manual";

/// Start a run for `kind`, recording `actor`.
///
/// Shared by [`fire_manual`] and [`crate::trigger::fire_ai`] so both entry
/// points behave identically apart from the recorded actor and kind.
pub fn fire(
    fire: &FireFn,
    workflow_id: &str,
    trigger_id: Option<&str>,
    kind: &'static str,
    actor: &str,
    inputs: Value,
) -> Result<String, WorkflowError> {
    fire(TriggerFire {
        workflow_id: workflow_id.to_string(),
        trigger_id: trigger_id.unwrap_or(MANUAL_TRIGGER_ID).to_string(),
        kind,
        actor: actor.to_string(),
        inputs,
    })
}

/// Start a manual run as `actor` (spec §3.2 records `user`, `agent:<id>`, or
/// `trigger:<id>`).
pub fn fire_manual(
    fire: &FireFn,
    workflow_id: &str,
    trigger_id: Option<&str>,
    actor: &str,
    inputs: Value,
) -> Result<String, WorkflowError> {
    self::fire(fire, workflow_id, trigger_id, "manual", actor, inputs)
}

/// Start a synthetic run for the editor's test button.
///
/// The payload is marked so a run can be recognised as a test; the engine
/// still records the run, which is what makes its step history inspectable.
pub fn test_trigger(
    fire: &FireFn,
    workflow_id: &str,
    trigger_id: Option<&str>,
    inputs: Value,
) -> Result<String, WorkflowError> {
    let payload = json!({ "test": true, "inputs": inputs });
    self::fire(fire, workflow_id, trigger_id, "manual", "user", payload)
}
