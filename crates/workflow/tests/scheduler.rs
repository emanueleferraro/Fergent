//! The scheduler: topological dispatch, branch skipping, and loop scopes
//! (spec §1.1, §3.1).

use std::collections::BTreeMap;

use fergents_workflow::{Decision, Graph, Outcome, Scheduler, WorkflowDocument};

fn document(yaml: &str) -> WorkflowDocument {
    WorkflowDocument::from_yaml(yaml).expect("document parses")
}

fn graph(yaml: &str) -> Graph {
    Graph::build(&document(yaml))
}

const LINEAR: &str = r#"
id: linear
name: Linear
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: parse
    type: transform
    config: { ops: [] }
  - id: finish
    type: end
edges:
  - { from: start, from_port: event, to: parse, to_port: input }
  - { from: parse, from_port: output, to: finish, to_port: result }
"#;

#[test]
fn nodes_run_in_topological_order() {
    let graph = graph(LINEAR);
    let scheduler = Scheduler::new(&graph);

    assert_eq!(scheduler.order(), ["start", "parse", "finish"]);
    assert_eq!(scheduler.end_nodes(), ["finish"]);
    assert!(scheduler.is_end("finish"));
    assert!(!scheduler.is_end("parse"));

    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();

    assert_eq!(scheduler.next(&outcomes), Some(("start", Decision::Run)));
    outcomes.insert("start".to_string(), Outcome::done(["event"]));

    assert_eq!(scheduler.next(&outcomes), Some(("parse", Decision::Run)));
    outcomes.insert("parse".to_string(), Outcome::done(["output"]));

    assert_eq!(scheduler.next(&outcomes), Some(("finish", Decision::Run)));
    outcomes.insert("finish".to_string(), Outcome::done(Vec::<String>::new()));

    assert_eq!(scheduler.next(&outcomes), None);
}

#[test]
fn a_branch_a_condition_did_not_activate_is_skipped() {
    let graph = graph(
        r#"
id: branch
name: Branch
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: pick
    type: condition
    config: { expr: "{{ true }}" }
  - id: "yes"
    type: transform
    config: { ops: [] }
  - id: "no"
    type: transform
    config: { ops: [] }
  - id: finish
    type: end
edges:
  - { from: start, from_port: event, to: pick, to_port: value }
  - { from: pick, from_port: "true", to: yes, to_port: input }
  - { from: pick, from_port: "false", to: no, to_port: input }
  - { from: yes, from_port: output, to: finish, to_port: result }
  - { from: no, from_port: output, to: finish, to_port: result }
"#,
    );
    let scheduler = Scheduler::new(&graph);
    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();

    outcomes.insert("start".to_string(), Outcome::done(["event"]));
    outcomes.insert("pick".to_string(), Outcome::done(["true"]));

    assert_eq!(scheduler.decision("yes", &outcomes), Decision::Run);
    assert_eq!(scheduler.decision("no", &outcomes), Decision::Skip);
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Wait);

    outcomes.insert("no".to_string(), Outcome::Skipped);
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Wait);

    outcomes.insert("yes".to_string(), Outcome::done(["output"]));
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Run);
}

#[test]
fn a_failed_branch_only_continues_when_it_is_asked_to() {
    let graph = graph(
        r#"
id: continued
name: Continued
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: work
    type: transform
    config: { ops: [] }
  - id: finish
    type: end
edges:
  - { from: start, from_port: event, to: work, to_port: input }
  - { from: work, from_port: output, to: finish, to_port: result }
"#,
    );
    let scheduler = Scheduler::new(&graph);
    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();
    outcomes.insert("start".to_string(), Outcome::done(["event"]));

    outcomes.insert("work".to_string(), Outcome::Failed { continued: false });
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Skip);

    outcomes.insert("work".to_string(), Outcome::Failed { continued: true });
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Run);
}

const LOOPED: &str = r#"
id: looped
name: Looped
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: each
    type: loop
    config:
      items: "{{ trigger }}"
      body: [work, inner_end]
  - id: work
    type: transform
    config: { ops: [] }
  - id: inner_end
    type: end
  - id: finish
    type: end
edges:
  - { from: start, from_port: event, to: each, to_port: items }
  - { from: work, from_port: output, to: inner_end, to_port: result }
  - { from: each, from_port: results, to: finish, to_port: result }
"#;

#[test]
fn a_loop_body_schedules_in_its_own_scope() {
    let graph = graph(LOOPED);

    let top = Scheduler::new(&graph);
    assert_eq!(top.order(), ["start", "each", "finish"]);
    assert_eq!(top.end_nodes(), ["finish"]);

    let body = Scheduler::for_scope(&graph, Some("each"));
    assert_eq!(body.order(), ["work", "inner_end"]);
    assert_eq!(body.end_nodes(), ["inner_end"]);
    assert_eq!(graph.scope("work"), Some("each"));
    assert_eq!(graph.scope("each"), None);
}

#[test]
fn pending_reports_the_nodes_that_have_not_settled() {
    let graph = graph(LINEAR);
    let scheduler = Scheduler::new(&graph);
    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();

    assert_eq!(scheduler.pending(&outcomes), ["start", "parse", "finish"]);

    outcomes.insert("start".to_string(), Outcome::done(["event"]));
    assert_eq!(scheduler.pending(&outcomes), ["parse", "finish"]);
}

#[test]
fn a_node_with_no_inbound_edges_runs_immediately() {
    let graph = graph(
        r#"
id: lonely
name: Lonely
nodes:
  - id: start
    type: trigger
    config: { trigger_id: manual }
  - id: finish
    type: end
"#,
    );
    let scheduler = Scheduler::new(&graph);
    let outcomes: BTreeMap<String, Outcome> = BTreeMap::new();

    assert_eq!(scheduler.decision("start", &outcomes), Decision::Run);
    assert_eq!(scheduler.decision("finish", &outcomes), Decision::Run);
}
