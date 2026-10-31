//! The `tool` node (spec §1.2, §1.6).
//!
//! Kernel, plugin, and MCP tools all resolve through the same registry, and
//! the tool's own permission category applies.

use super::{Execution, NodeContext};
use crate::document::{config::ToolConfig, Node};
use crate::error::WorkflowError;

/// Call a tool and return its result on the `result` port.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: ToolConfig = node.config_as()?;
    let output = ctx.host.tool(&config, ctx).await?;
    Ok(Execution::new(output))
}
