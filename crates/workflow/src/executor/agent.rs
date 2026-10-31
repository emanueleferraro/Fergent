//! The `agent` node (spec §1.2, §1.6).
//!
//! The child agent inherits the run's budget but can never widen it.

use super::{Execution, NodeContext};
use crate::document::{config::AgentConfig, Node};
use crate::error::WorkflowError;

/// Run an agent to completion and return `result`/`artifacts`.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: AgentConfig = node.config_as()?;
    let output = ctx.host.agent(&config, ctx).await?;
    Ok(Execution::new(output))
}
