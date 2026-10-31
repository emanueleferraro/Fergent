//! The `human` node (spec §1.2, §3.7).
//!
//! A human node cannot complete on its own: the engine pauses the run in
//! `waiting_human` and resumes it once `workflow.run.approve` or
//! `workflow.run.reject` records a decision. This module owns the decision
//! half: it validates the recorded decision and turns it into node output.

use serde_json::{json, Value};

use super::{Execution, NodeContext};
use crate::document::{config::HumanConfig, Node};
use crate::error::WorkflowError;

/// Finish the node with the decision recorded on the run.
///
/// Returns [`WorkflowError::Interrupted`] while no decision is present; the
/// engine maps that to the `waiting_human` state (§3.7).
pub fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: HumanConfig = node.config_as()?;
    if config.options.is_empty() {
        return Err(WorkflowError::Invalid(format!(
            "human node `{}` declares no options",
            node.id
        )));
    }
    let Some(decision) = ctx.inputs.get("decision").and_then(Value::as_str) else {
        return Err(WorkflowError::Interrupted(format!(
            "human node `{}` is waiting for a decision",
            node.id
        )));
    };
    if !config.options.iter().any(|option| option == decision) {
        return Err(WorkflowError::Invalid(format!(
            "human node `{}` got the decision `{decision}`, which is not one of {:?}",
            node.id, config.options
        )));
    }
    let notes = ctx.inputs.get("notes").cloned().unwrap_or(Value::Null);
    Ok(Execution::with_ports(
        json!({ "decision": decision, "notes": notes }),
        ["decision"],
    ))
}
