//! The run driver: lifecycle, steps, checkpoints, hosts, gates, and loops
//! (spec §1.3.1, §3.1–§3.7).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fergents_events::EventBus;
use fergents_storage::{SqliteStorage, Storage};
use fergents_workflow::document::config::ToolConfig;
use fergents_workflow::document::Node;
use fergents_workflow::executor::{HostFuture, NodeContext, NodeHost};
use fergents_workflow::{Engine, Gates, NewRun, RunRecord, RunState, RunStore, StepState};
use fergents_workflow::{NodeKind, WorkflowDocument, WorkflowError};
use serde_json::{json, Value};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

struct Harness {
    store: Arc<RunStore>,
    bus: Arc<EventBus>,
}

impl Harness {
    fn new() -> Self {
        let storage: Arc<dyn Storage> =
            Arc::new(SqliteStorage::open_in_memory(&migrations_dir()).expect("migrations apply"));
        let store = Arc::new(RunStore::new(Arc::clone(&storage)));
        let bus = Arc::new(EventBus::new(storage, 256));
        Self { store, bus }
    }

    fn engine(&self) -> Engine {
        Engine::new(Arc::clone(&self.store), Arc::clone(&self.bus))
    }

    fn create_run(&self, doc: &WorkflowDocument, inputs: Option<Value>) -> String {
        self.store
            .create_run(NewRun {
                workflow_id: &doc.id,
                workflow_version: 1,
                parent_run_id: None,
                trigger_kind: "manual",
                trigger_id: Some("manual"),
                actor: "user",
                dry_run: false,
                inputs,
            })
            .expect("run created")
            .id
    }

    async fn execute(&self, doc: &WorkflowDocument, inputs: Option<Value>) -> RunRecord {
        let run_id = self.create_run(doc, inputs);
        self.engine()
            .execute(doc, &run_id)
            .await
            .expect("run executes")
    }

    fn events(&self, topic: &str) -> Vec<fergents_protocol::Event> {
        self.bus
            .replay(&[topic.to_string()], None, None)
            .expect("events replay")
    }
}

fn document(yaml: &str) -> WorkflowDocument {
    WorkflowDocument::from_yaml(yaml).expect("document parses")
}

const GREET: &str = r#"
id: greet
name: Greet
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: prepare
    type: transform
    inputs:
      event: "{{ start.event }}"
    config:
      ops: []
      template: "Hello, {{ result.event.name }}!"
  - id: finish
    type: end
    config:
      result: "{{ prepare }}"
edges:
  - { from: start, from_port: event, to: prepare, to_port: input }
  - { from: prepare, from_port: output, to: finish, to_port: result }
"#;

#[tokio::test]
async fn a_linear_run_completes_and_records_every_step() {
    let h = Harness::new();
    let doc = document(GREET);

    let run = h.execute(&doc, Some(json!({ "name": "Ada" }))).await;

    assert_eq!(run.state, RunState::Done);
    assert!(run.error.is_none());
    assert!(run.started_at.is_some());
    assert!(run.finished_at.is_some());

    let context = run.context.clone().expect("context is persisted");
    assert_eq!(context["result"], json!("Hello, Ada!"));
    assert_eq!(context["prepare"], json!("Hello, Ada!"));

    let steps = h.store.steps(&run.id).expect("steps read");
    let ids: Vec<&str> = steps.iter().map(|step| step.node_id.as_str()).collect();
    assert_eq!(ids, ["start", "prepare", "finish"]);
    for step in &steps {
        assert_eq!(step.state, StepState::Done);
        assert_eq!(step.attempt, 1);
        assert!(step.finished_at.is_some());
    }
    assert_eq!(steps[1].outputs, Some(json!("Hello, Ada!")));

    let started = h.events("workflow.run.started");
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].payload["workflow_id"], json!("greet"));

    let finished = h.events("workflow.run.finished");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].payload["state"], json!("done"));
}

#[tokio::test]
async fn every_completed_node_writes_a_checkpoint() {
    let h = Harness::new();
    let doc = document(GREET);

    let run = h.execute(&doc, Some(json!({ "name": "Ada" }))).await;

    let mut checkpoints = h.store.checkpoints(&run.id, 10).expect("checkpoints read");
    checkpoints.reverse();
    assert_eq!(checkpoints.len(), 3);
    assert_eq!(checkpoints[0].node_id.as_deref(), Some("start"));
    assert_eq!(checkpoints[2].node_id.as_deref(), Some("finish"));
    assert!(checkpoints[2].seq > checkpoints[0].seq);
}

#[tokio::test]
async fn start_persists_the_run_before_driving_it() {
    let h = Harness::new();
    let doc = document(GREET);
    let engine = h.engine();

    let run_id = engine
        .start(
            &doc,
            NewRun {
                workflow_id: &doc.id,
                workflow_version: 1,
                parent_run_id: None,
                trigger_kind: "manual",
                trigger_id: Some("manual"),
                actor: "user",
                dry_run: false,
                inputs: Some(json!({ "name": "Ada" })),
            },
        )
        .expect("run starts");

    assert!(h.store.get_run(&run_id).expect("read").is_some());

    let created = h.events("workflow.run.created");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].payload["run_id"], json!(run_id));
    assert_eq!(created[0].payload["workflow_id"], json!("greet"));

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let run = h.store.get_run(&run_id).expect("read").expect("run exists");
        if run.state.is_terminal() {
            assert_eq!(run.state, RunState::Done);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the run did not finish in time"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

const APPROVAL: &str = r#"
id: approval
name: Approval
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: approve
    type: human
    inputs:
      prompt: '{{ trigger.prompt | default("Ship it?") }}'
      decision: "{{ trigger.decision | default(null) }}"
      notes: "{{ trigger.notes | default(null) }}"
    config:
      prompt: "Ship it?"
      options: [yes, no]
  - id: finish
    type: end
    config:
      result: "{{ approve.decision }}"
edges:
  - { from: start, from_port: event, to: approve, to_port: prompt }
  - { from: approve, from_port: decision, to: finish, to_port: result }
"#;

#[tokio::test]
async fn a_human_node_pauses_the_run() {
    let h = Harness::new();
    let doc = document(APPROVAL);

    let run = h.execute(&doc, None).await;

    assert_eq!(run.state, RunState::WaitingHuman);
    assert!(run.finished_at.is_none());

    let steps = h.store.steps(&run.id).expect("steps read");
    let approve = steps
        .iter()
        .find(|step| step.node_id == "approve")
        .expect("the human step is recorded");
    assert_eq!(approve.state, StepState::Interrupted);
    assert!(approve.error.is_some());

    let requested = h.events("workflow.human.requested");
    assert_eq!(requested.len(), 1);
    assert_eq!(requested[0].payload["node_id"], json!("approve"));
    assert_eq!(requested[0].payload["options"], json!(["yes", "no"]));

    let finished = h.events("workflow.run.finished");
    assert_eq!(finished[0].payload["state"], json!("waiting_human"));
}

const BROKEN: &str = r#"
id: broken
name: Broken
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: prepare
    type: transform
    inputs:
      value: "{{ missing.path }}"
    config: { ops: [] }
  - id: finish
    type: end
edges:
  - { from: start, from_port: event, to: prepare, to_port: input }
  - { from: prepare, from_port: output, to: finish, to_port: result }
"#;

#[tokio::test]
async fn an_unresolvable_template_fails_the_run() {
    let h = Harness::new();
    let doc = document(BROKEN);

    let run = h.execute(&doc, None).await;

    assert_eq!(run.state, RunState::Failed);
    let error = run.error.expect("the run records its error");
    assert!(
        error.starts_with("workflow_template_missing"),
        "unexpected error: {error}"
    );

    let steps = h.store.steps(&run.id).expect("steps read");
    let ids: Vec<&str> = steps.iter().map(|step| step.node_id.as_str()).collect();
    assert_eq!(ids, ["start"]);
}

const FANOUT: &str = r#"
id: fanout
name: Fanout
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: each
    type: loop
    config:
      items: "{{ trigger.items }}"
      body: [work]
  - id: work
    type: transform
    inputs:
      item: "{{ item }}"
    config:
      ops: []
      template: "{{ result.item }}"
  - id: finish
    type: end
    config:
      result: "{{ each.results }}"
edges:
  - { from: start, from_port: event, to: each, to_port: items }
  - { from: each, from_port: results, to: finish, to_port: result }
"#;

#[tokio::test]
async fn a_loop_node_runs_its_body_for_every_item() {
    let h = Harness::new();
    let doc = document(FANOUT);

    let run = h
        .execute(&doc, Some(json!({ "items": ["a", "b", "c"] })))
        .await;

    assert_eq!(run.state, RunState::Done);
    let context = run.context.clone().expect("context is persisted");
    assert_eq!(context["each"]["results"], json!(["a", "b", "c"]));
    assert_eq!(context["result"], json!(["a", "b", "c"]));

    let steps = h.store.steps(&run.id).expect("steps read");
    let body: Vec<i64> = steps
        .iter()
        .filter(|step| step.node_id == "work")
        .filter_map(|step| step.iteration)
        .collect();
    assert_eq!(body, [0, 1, 2]);
    for step in steps.iter().filter(|step| step.node_id == "work") {
        assert_eq!(step.state, StepState::Done);
    }
}

struct Rejects(&'static str);

impl Gates for Rejects {
    fn check(&self, node: &Node) -> Result<(), WorkflowError> {
        if node.id == self.0 {
            return Err(WorkflowError::Invalid(format!(
                "node `{}` is not allowed in this run",
                node.id
            )));
        }
        Ok(())
    }
}

#[tokio::test]
async fn a_gate_can_reject_a_node() {
    let h = Harness::new();
    let doc = document(GREET);
    let engine = h.engine().with_gates(Arc::new(Rejects("prepare")));

    let run_id = h.create_run(&doc, Some(json!({ "name": "Ada" })));
    let run = engine.execute(&doc, &run_id).await.expect("run executes");

    assert_eq!(run.state, RunState::Failed);
    let error = run.error.expect("the run records its error");
    assert!(
        error.starts_with("workflow_invalid"),
        "unexpected error: {error}"
    );

    let steps = h.store.steps(&run.id).expect("steps read");
    let prepare = steps
        .iter()
        .find(|step| step.node_id == "prepare")
        .expect("the rejected step is recorded");
    assert_eq!(prepare.state, StepState::Failed);
}

struct EchoHost;

impl NodeHost for EchoHost {
    fn tool<'a>(&'a self, config: &'a ToolConfig, ctx: &'a NodeContext) -> HostFuture<'a> {
        Box::pin(async move {
            Ok(json!({
                "tool": config.tool,
                "name": ctx.inputs.get("name").cloned().unwrap_or(Value::Null),
            }))
        })
    }
}

const ECHO: &str = r#"
id: echo
name: Echo
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: call
    type: tool
    inputs:
      name: "{{ trigger.name }}"
    config:
      tool: greet.echo
    outputs:
      text: "{{ result.name }}"
  - id: finish
    type: end
    config:
      result: "{{ call }}"
edges:
  - { from: start, from_port: event, to: call, to_port: args }
  - { from: call, from_port: result, to: finish, to_port: result }
"#;

#[tokio::test]
async fn the_host_serves_tool_nodes() {
    let h = Harness::new();
    let doc = document(ECHO);
    let engine = h.engine().with_host(Arc::new(EchoHost));

    let run_id = h.create_run(&doc, Some(json!({ "name": "Ada" })));
    let run = engine.execute(&doc, &run_id).await.expect("run executes");

    assert_eq!(run.state, RunState::Done);
    let context = run.context.clone().expect("context is persisted");
    assert_eq!(context["call"], json!({ "text": "Ada" }));
    assert_eq!(context["result"], json!({ "text": "Ada" }));
    assert_eq!(
        doc.node("call").map(|node| &node.kind),
        Some(&NodeKind::Tool)
    );
}
