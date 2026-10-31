//! The `subworkflow` node (spec §1.2, §1.6).
//!
//! The nested run is a real run of its own (its own budget, its own trace);
//! the host starts it and returns its outputs.

use super::{Execution, NodeContext};
use crate::document::{config::SubworkflowConfig, Node};
use crate::error::WorkflowError;

/// Run a child workflow and return its `outputs`.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: SubworkflowConfig = node.config_as()?;
    let output = ctx.host.subworkflow(&config, ctx).await?;
    Ok(Execution::new(output))
}
