//! The `trigger` node (spec §1.2).
//!
//! A trigger node is the entry point of a run. It has no config worth
//! executing and no side effects: it exposes the payload that started the run
//! and activates the `event` port.

use serde_json::json;

use super::{Execution, NodeContext};

/// Expose `ctx.trigger` as the node output on the `event` port.
pub fn execute(ctx: &NodeContext) -> Execution {
    Execution::with_ports(json!({ "event": ctx.trigger.clone() }), ["event"])
}
