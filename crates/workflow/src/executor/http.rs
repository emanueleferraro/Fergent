//! The `http` node (spec §1.2, §1.6).
//!
//! A normal permissioned call: `net.http` with a domain scope, an 8 MiB
//! response cap, and no redirects outside the granted domains.

use super::{Execution, NodeContext};
use crate::document::{config::HttpConfig, Node};
use crate::error::WorkflowError;

/// Perform the request and return `status`/`body`/`headers`.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: HttpConfig = node.config_as()?;
    let output = ctx.host.http(&config, ctx).await?;
    Ok(Execution::new(output))
}
