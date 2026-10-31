//! The run driver (spec §3.1–§3.5).
//!
//! [`Engine`] owns the loop that turns a [`WorkflowDocument`] into persisted
//! runs: it pins a version, drives the [`Scheduler`], records every step and
//! checkpoint, and emits `workflow.*` events. Node behaviour lives in
//! [`crate::executor`]; this module only sequences it.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use futures::future::BoxFuture;
use serde_json::{json, Value};

use crate::checkpoint::Checkpointer;
use crate::context::{default_artifacts_dir, RunContext};
use crate::document::{Node, NodeKind, WorkflowDocument};
use crate::error::WorkflowError;
use crate::executor::{
    self, AllowAll, BudgetSink, Cancellation, Execution, Gates, LoopRunner, NoBudget, NodeContext,
    NodeHost, Usage,
};
use crate::graph::Graph;
use crate::scheduler::{Decision, Outcome, Scheduler};
use crate::store::{NewRun, RunRecord, RunState, RunStore, StepState};
use crate::template::{NoSecrets, SecretResolver};

/// How a scope stopped.
enum Halt {
    /// Every node settled and no work is left.
    Done,
    /// A `human` node is waiting for a decision.
    WaitingHuman,
    /// A node failed the run.
    Failed(WorkflowError),
}

/// One resolved scheduling scope: the whole document, or a loop body.
struct Scope<'a> {
    doc: &'a Arc<WorkflowDocument>,
    graph: &'a Arc<Graph>,
    scheduler: &'a Scheduler,
    scope: Option<&'a str>,
    iteration: Option<u32>,
    run_id: &'a str,
    checkpoint: &'a Checkpointer,
    cancel: &'a Cancellation,
}

/// The workflow run driver.
///
/// Cloning is cheap — every field is an `Arc` — so a spawned task can hold its
/// own handle while the caller keeps its `run_id`.
#[derive(Clone)]
pub struct Engine {
    store: Arc<RunStore>,
    bus: Arc<fergents_events::EventBus>,
    host: Arc<dyn NodeHost>,
    gates: Arc<dyn Gates>,
    budget: Arc<dyn BudgetSink>,
    secrets: Arc<dyn SecretResolver + Send + Sync>,
}

impl Engine {
    /// An engine over `store` and `bus`, with no kernel host installed.
    pub fn new(store: Arc<RunStore>, bus: Arc<fergents_events::EventBus>) -> Self {
        Self {
            store,
            bus,
            host: Arc::new(NoHost),
            gates: Arc::new(AllowAll),
            budget: Arc::new(NoBudget),
            secrets: Arc::new(NoSecrets),
        }
    }

    /// Install the kernel services node executors may call (§1.2).
    pub fn with_host(mut self, host: Arc<dyn NodeHost>) -> Self {
        self.host = host;
        self
    }

    /// Install the permission gate (§3.10).
    pub fn with_gates(mut self, gates: Arc<dyn Gates>) -> Self {
        self.gates = gates;
        self
    }

    /// Install the budget ledger (§3.9).
    pub fn with_budget(mut self, budget: Arc<dyn BudgetSink>) -> Self {
        self.budget = budget;
        self
    }

    /// Install the secret resolver used by `{{ secrets.… }}`.
    pub fn with_secrets(mut self, secrets: Arc<dyn SecretResolver + Send + Sync>) -> Self {
        self.secrets = secrets;
        self
    }

    /// The run store this engine writes to.
    pub fn store(&self) -> &Arc<RunStore> {
        &self.store
    }

    /// Persist a run and drive it on the engine task (spec §3.2, §7).
    ///
    /// Returns as soon as the run is stored; progress is observable through
    /// `workflow.*` events and the run store.
    pub fn start(&self, doc: &WorkflowDocument, new: NewRun<'_>) -> Result<String, WorkflowError> {
        let record = self.store.create_run(new)?;
        let run_id = record.id.clone();
        self.emit(
            "workflow.run.created",
            json!({
                "run_id": record.id,
                "workflow_id": record.workflow_id,
                "workflow_version": record.workflow_version,
                "trigger_kind": record.trigger_kind,
                "trigger_id": record.trigger_id,
                "actor": record.actor,
                "dry_run": record.dry_run,
            }),
        );

        let engine = self.clone();
        let doc = doc.clone();
        let spawned = run_id.clone();
        tokio::spawn(async move {
            if let Err(err) = engine.execute(&doc, &spawned).await {
                tracing::warn!(run_id = %spawned, error = %err, "workflow run failed");
            }
        });
        Ok(run_id)
    }

    /// Drive an existing run to a terminal or pausing state.
    ///
    /// The run keeps the version it was created with (spec §3.6), so the
    /// document must still carry that version.
    pub async fn execute(
        &self,
        doc: &WorkflowDocument,
        run_id: &str,
    ) -> Result<RunRecord, WorkflowError> {
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| WorkflowError::NotFound(run_id.to_string()))?;

        self.store.transition(run_id, RunState::Running, None)?;
        self.emit(
            "workflow.run.started",
            json!({
                "run_id": run_id,
                "workflow_id": doc.id,
                "workflow_version": run.workflow_version,
                "dry_run": run.dry_run,
            }),
        );

        let doc = Arc::new(doc.clone());
        let graph = Arc::new(Graph::build(&doc));
        let scheduler = Scheduler::new(&graph);
        let checkpoint = Checkpointer::for_document(run_id, &doc, run.workflow_version)?;
        let cancel = Cancellation::new();

        let mut context = RunContext::for_run(run_id);
        context.set_trigger(run.inputs.clone().unwrap_or(Value::Null))?;
        context.set_variables(&doc.variables)?;

        let scope = Scope {
            doc: &doc,
            graph: &graph,
            scheduler: &scheduler,
            scope: None,
            iteration: None,
            run_id,
            checkpoint: &checkpoint,
            cancel: &cancel,
        };
        let halt = self.run_scope(&scope, &mut context).await?;

        let (state, error) = match halt {
            Halt::Done => {
                context.set(
                    "result",
                    terminal_result(&scheduler, &graph, None, &context)?,
                )?;
                self.store.set_context(run_id, &context.snapshot())?;
                (RunState::Done, None)
            }
            Halt::WaitingHuman => (RunState::WaitingHuman, None),
            Halt::Failed(err) => (RunState::Failed, Some(format!("{}: {err}", err.code()))),
        };

        let record = self.store.transition(run_id, state, error.as_deref())?;
        self.emit(
            "workflow.run.finished",
            json!({
                "run_id": run_id,
                "workflow_id": doc.id,
                "state": state.as_str(),
                "error": error,
            }),
        );
        Ok(record)
    }

    /// Drive one scheduling scope.
    ///
    /// Checkpoints and the run's live context are only written for the
    /// top-level scope; loop iterations work on their own context copy.
    async fn run_scope(
        &self,
        scope: &Scope<'_>,
        context: &mut RunContext,
    ) -> Result<Halt, WorkflowError> {
        let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();

        while let Some((node_id, decision)) = scope.scheduler.next(&outcomes) {
            if scope.cancel.is_cancelled() {
                return Ok(Halt::Failed(WorkflowError::Interrupted(
                    "the run was cancelled".to_string(),
                )));
            }
            let node = scope.doc.node(node_id).ok_or_else(|| {
                WorkflowError::Invalid(format!("scheduler selected unknown node `{node_id}`"))
            })?;

            match decision {
                Decision::Wait => unreachable!("`next` never yields a waiting node"),
                Decision::Skip => {
                    let step =
                        self.store
                            .start_step(scope.run_id, node_id, scope.iteration, 1, None)?;
                    self.store
                        .finish_step(&step, StepState::Skipped, None, None)?;
                    self.emit_step(scope, node_id, &step, StepState::Skipped);
                    outcomes.insert(node_id.to_string(), Outcome::Skipped);
                }
                Decision::Run => match self.run_node(scope, node, context).await {
                    Ok(outcome) => {
                        outcomes.insert(node_id.to_string(), outcome);
                    }
                    Err(halt) => return Ok(halt),
                },
            }
        }

        Ok(Halt::Done)
    }

    /// Execute one node with its full execution envelope (spec §1.3.1).
    async fn run_node(
        &self,
        scope: &Scope<'_>,
        node: &Node,
        context: &mut RunContext,
    ) -> Result<Outcome, Halt> {
        let render = context.render_context(
            scope.run_id,
            &scope.doc.id,
            Utc::now(),
            Arc::clone(&self.secrets),
        );
        let inputs = match context.resolve_inputs(node, &render) {
            Ok(inputs) => inputs,
            Err(err) => return Err(Halt::Failed(err)),
        };

        let step = self
            .store
            .start_step(scope.run_id, &node.id, scope.iteration, 1, Some(&inputs))
            .map_err(Halt::Failed)?;
        self.emit_step(scope, &node.id, &step, StepState::Running);

        let trigger = context
            .get("trigger")
            .map_err(Halt::Failed)?
            .unwrap_or(Value::Null);
        let node_ctx = NodeContext {
            run_id: scope.run_id.to_string(),
            workflow_id: scope.doc.id.clone(),
            iteration: scope.iteration,
            attempt: 1,
            inputs,
            trigger,
            now: Utc::now(),
            cancel: scope.cancel.clone(),
            secrets: Arc::clone(&self.secrets),
            render: render.clone(),
            host: Arc::clone(&self.host),
            gates: Arc::clone(&self.gates),
            budget: Arc::clone(&self.budget),
            loops: self.loops_for(scope, node),
        };

        let execution = match executor::execute(node, &node_ctx).await {
            Ok(execution) => execution,
            Err(err) => {
                let waiting =
                    node.kind == NodeKind::Human && matches!(err, WorkflowError::Interrupted(_));
                if waiting {
                    let text = err.to_string();
                    self.store
                        .finish_step(&step, StepState::Interrupted, None, Some(&text))
                        .map_err(Halt::Failed)?;
                    self.emit_step(scope, &node.id, &step, StepState::Interrupted);
                    self.emit_human_requested(scope, node);
                    return Err(Halt::WaitingHuman);
                }
                let text = format!("{}: {err}", err.code());
                self.store
                    .finish_step(&step, StepState::Failed, None, Some(&text))
                    .map_err(Halt::Failed)?;
                self.emit_step(scope, &node.id, &step, StepState::Failed);
                return Err(Halt::Failed(err));
            }
        };

        if let Err(err) = check_ports(node, &execution) {
            let text = format!("{}: {err}", err.code());
            self.store
                .finish_step(
                    &step,
                    StepState::Failed,
                    Some(&execution.output),
                    Some(&text),
                )
                .map_err(Halt::Failed)?;
            self.emit_step(scope, &node.id, &step, StepState::Failed);
            return Err(Halt::Failed(err));
        }

        if let Err(err) = context.store_outputs(node, &execution.output, &render) {
            let text = format!("{}: {err}", err.code());
            self.store
                .finish_step(
                    &step,
                    StepState::Failed,
                    Some(&execution.output),
                    Some(&text),
                )
                .map_err(Halt::Failed)?;
            self.emit_step(scope, &node.id, &step, StepState::Failed);
            return Err(Halt::Failed(err));
        }

        self.store
            .finish_step(&step, StepState::Done, Some(&execution.output), None)
            .map_err(Halt::Failed)?;
        self.emit_step(scope, &node.id, &step, StepState::Done);

        if scope.scope.is_none() {
            self.store
                .set_context(scope.run_id, &context.snapshot())
                .map_err(Halt::Failed)?;
            scope
                .checkpoint
                .save(&self.store, Some(&node.id), context)
                .map_err(Halt::Failed)?;
        }

        Ok(Outcome::done(execution.ports))
    }

    /// The loop runner for `node`, bound to its body.
    fn loops_for(&self, scope: &Scope<'_>, node: &Node) -> Arc<dyn LoopRunner> {
        if node.kind != NodeKind::Loop {
            return Arc::new(NoLoops);
        }
        Arc::new(ScopeLoops {
            engine: self.clone(),
            doc: Arc::clone(scope.doc),
            graph: Arc::clone(scope.graph),
            loop_id: node.id.clone(),
            run_id: scope.run_id.to_string(),
            checkpoint: scope.checkpoint.clone(),
            cancel: scope.cancel.clone(),
        })
    }

    fn emit(&self, kind: &str, payload: Value) {
        if let Err(err) = self.bus.publish(kind, "runs", payload) {
            tracing::warn!(kind, error = %err, "failed to publish workflow event");
        }
    }

    fn emit_step(&self, scope: &Scope<'_>, node_id: &str, step: &str, state: StepState) {
        self.emit(
            "workflow.step",
            json!({
                "run_id": scope.run_id,
                "workflow_id": scope.doc.id,
                "node_id": node_id,
                "step_id": step,
                "iteration": scope.iteration,
                "state": state.as_str(),
            }),
        );
    }

    fn emit_human_requested(&self, scope: &Scope<'_>, node: &Node) {
        self.emit(
            "workflow.human.requested",
            json!({
                "run_id": scope.run_id,
                "workflow_id": scope.doc.id,
                "node_id": node.id,
                "prompt": node.config.get("prompt").cloned().unwrap_or(Value::Null),
                "options": node.config.get("options").cloned().unwrap_or(Value::Null),
            }),
        );
    }
}

/// Runs one iteration of a `loop` body against its own context copy.
struct ScopeLoops {
    engine: Engine,
    doc: Arc<WorkflowDocument>,
    graph: Arc<Graph>,
    loop_id: String,
    run_id: String,
    checkpoint: Checkpointer,
    cancel: Cancellation,
}

impl LoopRunner for ScopeLoops {
    fn run_iteration<'a>(
        &'a self,
        ctx: &'a NodeContext,
        index: u32,
        item: Option<Value>,
    ) -> BoxFuture<'a, Result<Value, WorkflowError>> {
        Box::pin(async move {
            let mut context = RunContext::from_snapshot_with_artifacts(
                &ctx.render.data,
                default_artifacts_dir(&self.run_id),
            );
            context.set("index", Value::from(index))?;
            if let Some(item) = item {
                context.set("item", item)?;
            }

            let scheduler = Scheduler::for_scope(&self.graph, Some(&self.loop_id));
            let scope = Scope {
                doc: &self.doc,
                graph: &self.graph,
                scheduler: &scheduler,
                scope: Some(&self.loop_id),
                iteration: Some(index),
                run_id: &self.run_id,
                checkpoint: &self.checkpoint,
                cancel: &self.cancel,
            };
            match self.engine.run_scope(&scope, &mut context).await? {
                Halt::Done => {
                    terminal_result(&scheduler, &self.graph, Some(&self.loop_id), &context)
                }
                Halt::WaitingHuman => Err(WorkflowError::Interrupted(format!(
                    "a human node inside loop `{}` is waiting for a decision",
                    self.loop_id
                ))),
                Halt::Failed(err) => Err(err),
            }
        })
    }
}

/// A loop runner that cannot run anything; only the engine installs real ones.
struct NoLoops;

impl LoopRunner for NoLoops {
    fn run_iteration<'a>(
        &'a self,
        _ctx: &'a NodeContext,
        _index: u32,
        _item: Option<Value>,
    ) -> BoxFuture<'a, Result<Value, WorkflowError>> {
        Box::pin(async {
            Err(WorkflowError::NodeUnavailable(
                "no loop runner is installed".to_string(),
            ))
        })
    }
}

/// A host with no kernel services; every call reports `workflow_node_unavailable`.
struct NoHost;

impl NodeHost for NoHost {}

impl BudgetSink for NoHost {
    fn charge(&self, _usage: Usage) -> Result<(), WorkflowError> {
        Ok(())
    }
}

/// Reject ports a node type does not declare (spec §1.3.1).
fn check_ports(node: &Node, execution: &Execution) -> Result<(), WorkflowError> {
    let declared = node.kind.output_ports();
    for port in &execution.ports {
        if !declared.contains(&port.as_str()) {
            return Err(WorkflowError::Invalid(format!(
                "node `{}` activated undeclared port `{port}`",
                node.id
            )));
        }
    }
    Ok(())
}

/// The value a scope produced: the `end` nodes at the top level, or the
/// terminal nodes of a loop body.
fn terminal_result(
    scheduler: &Scheduler,
    graph: &Graph,
    scope: Option<&str>,
    context: &RunContext,
) -> Result<Value, WorkflowError> {
    let terminals: Vec<String> = match scope {
        None => scheduler.end_nodes(),
        Some(scope) => scheduler
            .order()
            .iter()
            .filter(|id| graph.scope(id) == Some(scope))
            .filter(|id| {
                !graph
                    .outgoing(id)
                    .any(|edge| graph.scope(&edge.to) == Some(scope))
            })
            .cloned()
            .collect(),
    };

    let mut values: Vec<(String, Value)> = Vec::new();
    for id in &terminals {
        if let Some(value) = context.get(id)? {
            values.push((id.clone(), value));
        }
    }

    Ok(match values.len() {
        0 => Value::Null,
        1 => values.pop().map_or(Value::Null, |(_, value)| value),
        _ => Value::Object(values.into_iter().collect()),
    })
}
