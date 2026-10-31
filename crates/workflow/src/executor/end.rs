//! The `end` node (spec §1.2).
//!
//! A terminal node: it carries the run's result and has no output ports.

use serde_json::Value;

use super::{Execution, NodeContext};
use crate::document::{config::EndConfig, Node};
use crate::error::WorkflowError;
use crate::template::Template;

/// Resolve the run result.
pub fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: EndConfig = node.config_as()?;
    let output = match config.result.as_deref() {
        Some(source) if !source.is_empty() => Template::parse(source)?.render(&ctx.render)?,
        _ => ctx.inputs.get("result").cloned().unwrap_or(Value::Null),
    };
    Ok(Execution::new(output))
}
