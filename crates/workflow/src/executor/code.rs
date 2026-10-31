//! The `code` node (spec §1.2, §1.6).
//!
//! Execution happens in the Phase 4 sandbox; the host owns compilation,
//! caching by source hash, and the resource caps.

use super::{Execution, NodeContext};
use crate::document::{config::CodeConfig, Node};
use crate::error::WorkflowError;

/// Run sandboxed code and return its output.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: CodeConfig = node.config_as()?;
    let output = ctx.host.code(&config, ctx).await?;
    Ok(Execution::new(output))
}
