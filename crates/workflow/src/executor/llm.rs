//! The `llm` node (spec §1.2, §1.6).
//!
//! The call itself belongs to the kernel: routing, retries at the provider
//! level, and usage accounting live behind [`NodeHost::llm`].

use super::{Execution, NodeContext};
use crate::document::{config::LlmConfig, Node};
use crate::error::WorkflowError;

/// Run a completion and return it on the `text`/`usage` ports.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: LlmConfig = node.config_as()?;
    let output = ctx.host.llm(&config, ctx).await?;
    Ok(Execution::new(output))
}
