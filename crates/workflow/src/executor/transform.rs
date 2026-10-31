//! The `transform` node (spec §1.2, §1.6).
//!
//! Declarative `pick`/`map`/`filter`/`merge`/`sort` operations, optionally
//! followed by an output template. Anything more complex belongs in a `code`
//! node.

use super::{Execution, NodeContext};
use crate::document::{config::TransformConfig, Node};
use crate::error::WorkflowError;
use crate::expr::apply_ops;
use crate::template::Template;

/// Apply the transform pipeline to the node inputs.
pub fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: TransformConfig = node.config_as()?;
    let value = apply_ops(&config.ops, ctx.inputs.clone(), &ctx.render)?;
    let output = match config.template.as_deref() {
        Some(source) => {
            let template = Template::parse(source)?;
            let mut render = ctx.render.clone();
            render.result = Some(value);
            template.render(&render)?
        }
        None => value,
    };
    Ok(Execution::new(output))
}
