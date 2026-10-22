//! The spec → plan → tasks workflow: gates, autonomy modes, and artifacts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_agents::{AutonomyMode, Gate, StartWorkflow, WorkflowService};
use fergents_events::EventBus;
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn service() -> (tempfile::TempDir, WorkflowService) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 256));
    let storage_dyn: Arc<dyn Storage> = storage;
    let service = WorkflowService::new(storage_dyn, bus, dir.path().join("specs"));
    (dir, service)
}

fn start(
    service: &WorkflowService,
    slug: &str,
    mode: AutonomyMode,
) -> fergents_agents::WorkflowRun {
    service
        .start(StartWorkflow {
            slug: slug.to_string(),
            task: "Add a widget".to_string(),
            session_id: None,
            mode,
        })
        .unwrap()
}

#[test]
fn generate_pause_approve_and_continue() {
    let (dir, service) = service();
    let run = start(&service, "widget", AutonomyMode::Supervised);
    assert_eq!(run.state, "awaiting_spec_approval");
    assert_eq!(run.pending_gate, Some(Gate::Spec));
    assert!(dir.path().join("specs/widget/spec.md").exists());

    // Approving writes the plan and pauses there.
    let run = service.approve(&run.id, None).unwrap();
    assert_eq!(run.state, "awaiting_plan_approval");
    assert!(dir.path().join("specs/widget/plan.md").exists());
    assert!(service.artifact(&run.id, Gate::Plan).unwrap().is_some());

    // Then tasks, then completion.
    let run = service.approve(&run.id, None).unwrap();
    assert_eq!(run.state, "awaiting_task_approval");
    assert!(dir.path().join("specs/widget/tasks.md").exists());

    let run = service.approve(&run.id, None).unwrap();
    assert_eq!(run.state, "completed");
    assert!(run.pending_gate.is_none());
    for artifact in ["spec.md", "plan.md", "tasks.md"] {
        assert!(dir.path().join("specs/widget").join(artifact).exists());
    }

    let approvals = service.approvals(&run.id).unwrap();
    assert_eq!(approvals.len(), 3);
    assert!(approvals
        .iter()
        .all(|approval| approval.status == "approved"));
}

#[test]
fn edits_before_approval_are_picked_up() {
    let (dir, service) = service();
    let run = start(&service, "edited", AutonomyMode::Supervised);

    std::fs::write(
        dir.path().join("specs/edited/spec.md"),
        "# Spec\n\nHand-edited by the user.\n",
    )
    .unwrap();

    let run = service.approve(&run.id, None).unwrap();
    let approvals = service.approvals(&run.id).unwrap();
    assert_eq!(approvals[0].status, "edited");

    // The plan continues from the edited file, not the template.
    let plan = service.artifact(&run.id, Gate::Plan).unwrap().unwrap();
    assert!(plan.contains("Hand-edited by the user."));
}

#[test]
fn autonomy_modes_change_the_pauses() {
    // Batch: pauses at spec and plan, then runs to completion.
    let (_dir, batch_service) = service();
    let run = start(&batch_service, "batch", AutonomyMode::Batch);
    assert!(run.is_awaiting());
    let run = batch_service.approve(&run.id, None).unwrap();
    assert!(run.is_awaiting());
    let run = batch_service.approve(&run.id, None).unwrap();
    assert_eq!(run.state, "completed");

    // Auto: never pauses; every gate is audited.
    let (_dir2, auto_service) = service();
    let run = start(&auto_service, "auto", AutonomyMode::Auto);
    assert_eq!(run.state, "completed");
    let approvals = auto_service.approvals(&run.id).unwrap();
    assert_eq!(approvals.len(), 3);
    assert!(approvals
        .iter()
        .all(|approval| approval.status == "approved"));
}

#[test]
fn reject_stops_the_run_and_guards_inputs() {
    let (_dir, service) = service();
    let run = start(&service, "rejected", AutonomyMode::Supervised);
    let run = service.reject(&run.id, Some("not now")).unwrap();
    assert_eq!(run.state, "rejected");
    assert!(run.pending_gate.is_none());
    let approvals = service.approvals(&run.id).unwrap();
    assert_eq!(approvals[0].status, "rejected");

    // Invalid slugs are refused before anything is written.
    assert!(service
        .start(StartWorkflow {
            slug: "../escape".to_string(),
            task: "x".to_string(),
            session_id: None,
            mode: AutonomyMode::Supervised,
        })
        .is_err());

    // Listing sees the run; approving a finished run is an error.
    assert_eq!(service.list(10).unwrap().len(), 1);
    assert!(service.approve(&run.id, None).is_err());
    assert!(service.approve("missing", None).is_err());
}
