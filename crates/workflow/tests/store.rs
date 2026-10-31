//! The run, step, and checkpoint store (spec §2, §3.2, §3.3).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_storage::SqliteStorage;
use fergents_workflow::store::CHECKPOINT_KEEP;
use fergents_workflow::{NewRun, RunState, RunStore, StepState};
use serde_json::json;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn store() -> RunStore {
    let storage = SqliteStorage::open_in_memory(&migrations_dir()).expect("migrations apply");
    RunStore::new(Arc::new(storage))
}

fn new_run(workflow_id: &'static str) -> NewRun<'static> {
    NewRun {
        workflow_id,
        workflow_version: 1,
        parent_run_id: None,
        trigger_kind: "manual",
        trigger_id: None,
        actor: "user",
        dry_run: false,
        inputs: None,
    }
}

#[test]
fn a_run_moves_through_its_lifecycle() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");

    assert_eq!(run.state, RunState::Queued);
    assert_eq!(run.workflow_version, 1);
    assert_eq!(run.trigger_kind, "manual");
    assert!(run.started_at.is_none());
    assert!(run.finished_at.is_none());

    let running = store
        .transition(&run.id, RunState::Running, None)
        .expect("run starts");
    assert_eq!(running.state, RunState::Running);
    let started = running.started_at.expect("started_at is stamped");
    assert!(running.finished_at.is_none());

    let done = store
        .transition(&run.id, RunState::Done, None)
        .expect("run finishes");
    assert_eq!(done.state, RunState::Done);
    assert_eq!(done.started_at, Some(started));
    assert!(done.finished_at.expect("finished_at is stamped") >= started);
    assert!(done.error.is_none());
}

#[test]
fn a_failed_run_keeps_its_error_and_stays_terminal() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");

    let failed = store
        .transition(&run.id, RunState::Failed, Some("workflow_invalid: boom"))
        .expect("run fails");

    assert_eq!(failed.state, RunState::Failed);
    assert!(failed.finished_at.is_some());
    assert_eq!(failed.error.as_deref(), Some("workflow_invalid: boom"));
    assert!(RunState::Failed.is_terminal());
    assert!(!RunState::WaitingHuman.is_terminal());
}

#[test]
fn a_paused_run_is_not_stamped_as_finished() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");

    let paused = store
        .transition(&run.id, RunState::Paused, None)
        .expect("run pauses");

    assert_eq!(paused.state, RunState::Paused);
    assert!(paused.finished_at.is_none());
}

#[test]
fn steps_keep_their_attempts_in_order() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");

    let first = store
        .start_step(&run.id, "start", None, 1, Some(&json!({ "seed": 1 })))
        .expect("step starts");
    store
        .finish_step(&first, StepState::Done, Some(&json!({ "event": {} })), None)
        .expect("step finishes");

    let second = store
        .start_step(&run.id, "parse", Some(0), 2, None)
        .expect("step starts");
    store
        .finish_step(
            &second,
            StepState::Failed,
            None,
            Some("workflow_invalid: boom"),
        )
        .expect("step fails");

    let steps = store.steps(&run.id).expect("steps read");
    assert_eq!(steps.len(), 2);

    assert_eq!(steps[0].node_id, "start");
    assert_eq!(steps[0].state, StepState::Done);
    assert_eq!(steps[0].attempt, 1);
    assert_eq!(steps[0].iteration, None);
    assert_eq!(steps[0].inputs, Some(json!({ "seed": 1 })));
    assert_eq!(steps[0].outputs, Some(json!({ "event": {} })));
    assert!(steps[0].started_at.is_some());
    assert!(steps[0].finished_at.is_some());

    assert_eq!(steps[1].node_id, "parse");
    assert_eq!(steps[1].state, StepState::Failed);
    assert_eq!(steps[1].attempt, 2);
    assert_eq!(steps[1].iteration, Some(0));
    assert_eq!(steps[1].error.as_deref(), Some("workflow_invalid: boom"));
}

#[test]
fn checkpoints_are_ordered_and_pruned() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");

    for n in 1..=(CHECKPOINT_KEEP + 5) {
        store
            .write_checkpoint(&run.id, Some("node"), &json!({ "n": n }))
            .expect("checkpoint written");
    }

    assert_eq!(
        store.checkpoint_count(&run.id).expect("counted"),
        CHECKPOINT_KEEP
    );

    let latest = store
        .latest_checkpoint(&run.id)
        .expect("read")
        .expect("a checkpoint exists");
    assert_eq!(latest.seq, CHECKPOINT_KEEP + 5);
    assert_eq!(latest.context, json!({ "n": CHECKPOINT_KEEP + 5 }));
    assert_eq!(latest.node_id.as_deref(), Some("node"));

    let all = store
        .checkpoints(&run.id, CHECKPOINT_KEEP as usize)
        .expect("read");
    assert_eq!(all.len() as i64, CHECKPOINT_KEEP);
    assert_eq!(all.last().expect("oldest kept").seq, 6);
}

#[test]
fn runs_list_newest_first_and_filter_by_workflow() {
    let store = store();
    let a = store.create_run(new_run("alpha")).expect("run created");
    let b = store.create_run(new_run("beta")).expect("run created");

    let all = store.list_runs(None, 10).expect("listed");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, b.id);
    assert_eq!(all[1].id, a.id);

    let alpha = store.list_runs(Some("alpha"), 10).expect("filtered");
    assert_eq!(alpha.len(), 1);
    assert_eq!(alpha[0].id, a.id);

    let limited = store.list_runs(None, 1).expect("limited");
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].id, b.id);
}

#[test]
fn an_unknown_run_reads_back_as_none() {
    let store = store();

    assert!(store
        .get_run("01J00000000000000000000000")
        .expect("read")
        .is_none());
    assert!(store.list_runs(None, 10).expect("listed").is_empty());
}

#[test]
fn the_context_snapshot_is_mirrored_on_the_run() {
    let store = store();
    let run = store.create_run(new_run("demo")).expect("run created");
    assert!(run.context.is_none());

    store
        .set_context(&run.id, &json!({ "parse": { "text": "hi" } }))
        .expect("context written");

    let read = store.get_run(&run.id).expect("read").expect("run exists");
    assert_eq!(read.context, Some(json!({ "parse": { "text": "hi" } })));
}
