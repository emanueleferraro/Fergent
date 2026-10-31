//! The AI trigger (spec §2.6).
//!
//! Identical to [`crate::trigger::manual`] except that the actor is the agent
//! session that asked for the run. The permission for the call is checked by
//! the kernel (`workflow.run`), not here: this module only records who asked.

use serde_json::Value;

use crate::error::WorkflowError;
use crate::trigger::manual;
use crate::trigger::FireFn;

/// Start a run on behalf of `agent_id`.
///
/// The run records `trigger_kind = "ai"` and the actor `agent:<agent_id>`
/// (spec §2.6).
pub fn fire_ai(
    fire: &FireFn,
    workflow_id: &str,
    trigger_id: Option<&str>,
    agent_id: &str,
    inputs: Value,
) -> Result<String, WorkflowError> {
    let actor = format!("agent:{agent_id}");
    manual::fire(fire, workflow_id, trigger_id, "ai", &actor, inputs)
}
