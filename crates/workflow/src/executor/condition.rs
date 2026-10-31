//! The `condition` node (spec §1.2, §1.6).
//!
//! Conditions use the closed expression language of [`crate::expr`]: no
//! `eval`, no function calls, no arbitrary code.

use serde_json::json;

use super::{Execution, NodeContext};
use crate::document::{config::ConditionConfig, Node};
use crate::error::WorkflowError;
use crate::expr::Condition;

/// The outcome of a condition expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditionResult {
    /// Whether the condition held.
    pub matched: bool,
}

impl ConditionResult {
    /// Evaluate `config.expr` against the run context.
    pub fn evaluate(config: &ConditionConfig, ctx: &NodeContext) -> Result<Self, WorkflowError> {
        let condition = Condition::parse(&config.expr)?;
        Ok(Self {
            matched: condition.eval(&ctx.render)?,
        })
    }

    /// The port this result activates.
    pub fn port(self) -> &'static str {
        if self.matched {
            "true"
        } else {
            "false"
        }
    }
}

/// Evaluate the branch and activate exactly one port.
pub fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: ConditionConfig = node.config_as()?;
    let result = ConditionResult::evaluate(&config, ctx)?;
    Ok(Execution::with_ports(
        json!({ "result": result.matched }),
        [result.port()],
    ))
}
