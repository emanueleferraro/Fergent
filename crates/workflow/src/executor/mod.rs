//! Node executors (spec §1.2, §1.3.1).
//!
//! Each node kind has one module. Executors are pure with respect to the run
//! store: they receive a narrow [`NodeContext`] and never persist anything.
//! Effects that need the kernel (an LLM call, a tool, an agent, sandboxed
//! code, an HTTP request, a child workflow) go through the [`NodeHost`] trait,
//! which the kernel implements in task 5.5.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde_json::Value;

use crate::document::{
    config::{AgentConfig, CodeConfig, HttpConfig, LlmConfig, SubworkflowConfig, ToolConfig},
    Node, NodeKind,
};
use crate::error::WorkflowError;
use crate::template::{RenderContext, SecretResolver};

mod agent;
mod code;
mod condition;
mod end;
mod http;
mod human;
mod llm;
mod r#loop;
mod subworkflow;
mod tool;
mod r#transform;
mod trigger;

pub use condition::ConditionResult;

/// What a node produced: its output and the ports it activated.
#[derive(Debug, Clone, PartialEq)]
pub struct Execution {
    /// The node output, stored under the node id (or its `outputs` mapping).
    pub output: Value,
    /// The activated output ports. Empty means every port.
    pub ports: Vec<String>,
}

impl Execution {
    /// A node that activated every port.
    pub fn new(output: Value) -> Self {
        Self {
            output,
            ports: Vec::new(),
        }
    }

    /// A node that activated exactly `ports`.
    pub fn with_ports(output: Value, ports: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            output,
            ports: ports.into_iter().map(Into::into).collect(),
        }
    }
}

/// A cooperative cancellation handle shared by a run and its nodes.
#[derive(Debug, Clone)]
pub struct Cancellation {
    tx: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancellation {
    /// A fresh, uncancelled handle.
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(false);
        Self { tx: Arc::new(tx) }
    }

    /// Request cancellation. Idempotent.
    pub fn cancel(&self) {
        let _ = self.tx.send(true);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }

    /// Wait until cancellation is requested.
    pub async fn cancelled(&self) {
        let mut rx = self.tx.subscribe();
        if *rx.borrow() {
            return;
        }
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }
}

/// Kernel services an executor may call.
///
/// Every method has a default that fails with `workflow_node_unavailable`, so
/// a host only overrides what it can actually do (tests implement just the
/// subset they exercise).
pub trait NodeHost: Send + Sync {
    /// An LLM completion through the router.
    fn llm<'a>(&'a self, _config: &'a LlmConfig, _ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("llm")) })
    }

    /// A kernel, plugin, or MCP tool call.
    fn tool<'a>(&'a self, _config: &'a ToolConfig, _ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("tool")) })
    }

    /// An agent run to completion.
    fn agent<'a>(&'a self, _config: &'a AgentConfig, _ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("agent")) })
    }

    /// Sandboxed code.
    fn code<'a>(&'a self, _config: &'a CodeConfig, _ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("code")) })
    }

    /// A permissioned HTTP request.
    fn http<'a>(&'a self, _config: &'a HttpConfig, _ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("http")) })
    }

    /// A nested workflow run.
    fn subworkflow<'a>(
        &'a self,
        _config: &'a SubworkflowConfig,
        _ctx: &'a NodeContext,
    ) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("subworkflow")) })
    }

    /// A plugin-contributed node type.
    fn plugin<'a>(
        &'a self,
        _kind: &'a str,
        _config: &'a Value,
        _ctx: &'a NodeContext,
    ) -> HostFuture<'a> {
        Box::pin(async { Err(unavailable("plugin")) })
    }
}

/// A future returned by [`NodeHost`] methods.
pub type HostFuture<'a> = BoxFuture<'a, Result<Value, WorkflowError>>;

/// The permission gate applied before a node executes (spec §3.10).
pub trait Gates: Send + Sync {
    /// Check that the run is allowed to execute `node`.
    fn check(&self, node: &Node) -> Result<(), WorkflowError>;
}

/// A gate that allows everything, for tests and dry runs.
pub struct AllowAll;

impl Gates for AllowAll {
    fn check(&self, _node: &Node) -> Result<(), WorkflowError> {
        Ok(())
    }
}

/// A budget observation point (spec §3.9). Task 5.4 supplies the real ledger.
pub trait BudgetSink: Send + Sync {
    /// Record usage against the run budget.
    fn charge(&self, usage: Usage) -> Result<(), WorkflowError>;
}

/// One usage sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    /// Prompt and completion tokens.
    pub tokens: u64,
    /// Cost in US dollars.
    pub cost_usd: f64,
    /// Tool calls made.
    pub tool_calls: u32,
}

/// A budget sink that never rejects, for tests and dry runs.
pub struct NoBudget;

impl BudgetSink for NoBudget {
    fn charge(&self, _usage: Usage) -> Result<(), WorkflowError> {
        Ok(())
    }
}

/// Runs one iteration of a `loop` body (spec §1.6).
///
/// The engine implements this because only it owns the body subgraph and the
/// scheduler; [`r#loop`] owns iteration planning and aggregation.
pub trait LoopRunner: Send + Sync {
    /// Execute the body once with `{{ item }}` and `{{ index }}` bound, and
    /// return the iteration's aggregated output.
    fn run_iteration<'a>(
        &'a self,
        ctx: &'a NodeContext,
        index: u32,
        item: Option<Value>,
    ) -> BoxFuture<'a, Result<Value, WorkflowError>>;
}

/// Everything an executor may use, and nothing more.
pub struct NodeContext {
    /// The owning run.
    pub run_id: String,
    /// The workflow id.
    pub workflow_id: String,
    /// The loop iteration, when the node runs inside a `loop`.
    pub iteration: Option<u32>,
    /// The attempt number, starting at 1.
    pub attempt: u32,
    /// The resolved `inputs` object (§1.4).
    pub inputs: Value,
    /// The trigger payload, exposed to `trigger` nodes.
    pub trigger: Value,
    /// The injected clock.
    pub now: DateTime<Utc>,
    /// The run's cancellation handle.
    pub cancel: Cancellation,
    /// Secrets, resolved at execution time only.
    pub secrets: Arc<dyn SecretResolver + Send + Sync>,
    /// A render context over the run context.
    pub render: RenderContext,
    /// Kernel services.
    pub host: Arc<dyn NodeHost>,
    /// Permission gate.
    pub gates: Arc<dyn Gates>,
    /// Budget sink.
    pub budget: Arc<dyn BudgetSink>,
    /// Loop body runner.
    pub loops: Arc<dyn LoopRunner>,
}

impl std::fmt::Debug for NodeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeContext")
            .field("run_id", &self.run_id)
            .field("workflow_id", &self.workflow_id)
            .field("iteration", &self.iteration)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

/// Execute `node` (spec §1.3.1).
///
/// The caller has already resolved `ctx.inputs`; this function checks the
/// permission gate, dispatches on the node kind, and returns the output. It
/// does not touch the store, validate `outputs`, or apply `on_error` — those
/// belong to the engine's execution envelope.
pub async fn execute(node: &Node, ctx: &NodeContext) -> Result<Execution, WorkflowError> {
    ctx.gates.check(node)?;
    if ctx.cancel.is_cancelled() {
        return Err(WorkflowError::Interrupted(
            "the run was cancelled before the node started".to_string(),
        ));
    }
    match &node.kind {
        NodeKind::Trigger => Ok(trigger::execute(ctx)),
        NodeKind::Llm => llm::execute(node, ctx).await,
        NodeKind::Tool => tool::execute(node, ctx).await,
        NodeKind::Agent => agent::execute(node, ctx).await,
        NodeKind::Code => code::execute(node, ctx).await,
        NodeKind::Http => http::execute(node, ctx).await,
        NodeKind::Condition => condition::execute(node, ctx),
        NodeKind::Loop => r#loop::execute(node, ctx).await,
        NodeKind::Subworkflow => subworkflow::execute(node, ctx).await,
        NodeKind::Human => human::execute(node, ctx),
        NodeKind::Transform => r#transform::execute(node, ctx),
        NodeKind::End => end::execute(node, ctx),
        NodeKind::Plugin(kind) => {
            let config = Value::Object(
                node.config
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            );
            let output = ctx.host.plugin(kind, &config, ctx).await?;
            Ok(Execution::new(output))
        }
    }
}

fn unavailable(kind: &str) -> WorkflowError {
    WorkflowError::NodeUnavailable(format!("no `{kind}` host is installed"))
}
