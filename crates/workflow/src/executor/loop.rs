//! The `loop` node (spec §1.2, §1.6).
//!
//! The executor plans iterations and aggregates results; running the body
//! subgraph is the engine's job, reached through [`LoopRunner`].

use serde_json::{json, Value};

use super::{Execution, NodeContext};
use crate::document::{config::LoopConfig, Node, DEFAULT_MAX_ITERATIONS, MAX_ITERATIONS_CAP};
use crate::error::WorkflowError;
use crate::template::render_value;

/// Iterate the body and aggregate its results.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    let config: LoopConfig = node.config_as()?;
    let plan = plan(&config, ctx)?;
    let parallel = config.parallel.unwrap_or(1).clamp(1, 4) as usize;

    let mut results = Vec::with_capacity(plan.len());
    for (chunk, items) in plan.chunks(parallel).enumerate() {
        if ctx.cancel.is_cancelled() {
            return Err(WorkflowError::Interrupted(format!(
                "loop node `{}` was cancelled",
                node.id
            )));
        }
        let base = chunk * parallel;
        let futures = items.iter().enumerate().map(|(offset, item)| {
            let index = (base + offset) as u32;
            ctx.loops.run_iteration(ctx, index, item.clone())
        });
        for result in futures::future::join_all(futures).await {
            results.push(result?);
        }
    }

    Ok(Execution::with_ports(
        json!({ "results": results }),
        ["results"],
    ))
}

/// The iteration plan: one entry per iteration.
fn plan(config: &LoopConfig, ctx: &NodeContext) -> Result<Vec<Option<Value>>, WorkflowError> {
    let limit = config
        .max_iterations
        .unwrap_or(DEFAULT_MAX_ITERATIONS)
        .min(MAX_ITERATIONS_CAP) as usize;

    let planned = match (&config.items, &config.count) {
        (Some(_), Some(_)) => {
            return Err(WorkflowError::Invalid(
                "a loop declares both `items` and `count`".to_string(),
            ))
        }
        (Some(items), None) => match render_value(items, &ctx.render)? {
            Value::Array(values) => values.into_iter().map(Some).collect(),
            Value::Null => Vec::new(),
            other => {
                return Err(WorkflowError::Invalid(format!(
                    "loop `items` must render to an array, got {}",
                    crate::template::as_text(&other)
                )))
            }
        },
        (None, Some(count)) => {
            let count = match render_value(count, &ctx.render)? {
                Value::Number(number) => number.as_u64().unwrap_or(0),
                Value::String(text) => text.trim().parse().unwrap_or(0),
                Value::Null => 0,
                other => {
                    return Err(WorkflowError::Invalid(format!(
                        "loop `count` must render to a number, got {}",
                        crate::template::as_text(&other)
                    )))
                }
            };
            (0..count).map(|index| Some(Value::from(index))).collect()
        }
        (None, None) => {
            return Err(WorkflowError::Invalid(
                "a loop needs either `items` or `count`".to_string(),
            ))
        }
    };

    if planned.len() > limit {
        return Err(WorkflowError::IterationLimit(format!(
            "the loop would run {} iterations, above the limit of {limit}",
            planned.len()
        )));
    }
    Ok(planned)
}
