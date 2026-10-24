//! PTY session tests (task 3.4). Each test skips itself when the host has no
//! usable shell.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_events::EventBus;
use fergents_ide::ProjectService;
use fergents_storage::{SqliteStorage, Storage};
use fergents_terminal::{SessionState, TerminalError, TerminalService, DEFAULT_READ_BYTES};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

struct Stack {
    terminal: TerminalService,
    projects: Arc<ProjectService>,
    bus: Arc<EventBus>,
    storage: Arc<dyn Storage>,
}

fn stack(dir: &Path) -> Stack {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 256));
    let projects = Arc::new(ProjectService::new(storage.clone(), bus.clone()));
    let terminal = fergents_terminal::TerminalService::with_limits(
        storage.clone(),
        bus.clone(),
        projects.clone(),
        64 * 1024,
        std::time::Duration::from_millis(5),
    );
    Stack {
        terminal,
        projects,
        bus,
        storage,
    }
}

fn project(stack: &Stack, dir: &Path, trusted: bool) -> String {
    let root = dir.join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let project = stack.projects.open(root.to_str().unwrap()).unwrap();
    if trusted {
        stack.projects.set_trust(&project.id, true).unwrap();
    }
    project.id
}

fn wait_for(f: impl Fn() -> bool, deadline: Duration) -> bool {
    let end = Instant::now() + deadline;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn open_write_read_resize_and_close() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    let session = match stack
        .terminal
        .open(&id, None, None, Some(100), Some(30), "user")
    {
        Ok(session) => session,
        Err(err) => {
            // No usable shell on this host: the remaining behaviour is covered
            // by the ring and shell unit tests.
            eprintln!("skipping: {err}");
            return;
        }
    };
    assert_eq!(session.state, SessionState::Running);
    assert_eq!(session.cols, 100);

    stack.terminal.resize(&session.id, 120, 40).unwrap();
    stack
        .terminal
        .write(&session.id, "echo fergents-terminal-marker\r")
        .unwrap();

    let seen = wait_for(
        || {
            stack
                .terminal
                .read(&session.id, None, None)
                .map(|text| text.contains("fergents-terminal-marker"))
                .unwrap_or(false)
        },
        Duration::from_secs(15),
    );
    assert!(seen, "the shell never echoed the marker");

    let listed = stack.terminal.list(Some(&id)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].cols, 120);
    assert_eq!(listed[0].rows, 40);

    let output_events = stack
        .bus
        .replay(&["term.output".to_string()], None, None)
        .unwrap();
    assert!(output_events
        .iter()
        .any(|event| event.payload["session_id"] == serde_json::json!(session.id)));

    assert!(stack.terminal.close(&session.id).unwrap());
    assert!(!stack.terminal.close(&session.id).unwrap());

    let listed = stack.terminal.list(Some(&id)).unwrap();
    assert_eq!(listed[0].state, SessionState::Closed);
    assert!(listed[0].closed_at.is_some());

    let opened = stack
        .bus
        .replay(&["term.opened".to_string()], None, None)
        .unwrap();
    let closed = stack
        .bus
        .replay(&["term.closed".to_string()], None, None)
        .unwrap();
    assert!(!opened.is_empty());
    assert!(!closed.is_empty());
}

#[test]
fn reads_are_bounded_and_redacted() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    let session = match stack.terminal.open(&id, None, None, None, None, "user") {
        Ok(session) => session,
        Err(err) => {
            eprintln!("skipping: {err}");
            return;
        }
    };
    stack
        .terminal
        .write(&session.id, "echo PASSWORD=secret-value\r")
        .unwrap();
    wait_for(
        || {
            stack
                .terminal
                .read(&session.id, None, None)
                .map(|text| text.contains("PASSWORD"))
                .unwrap_or(false)
        },
        Duration::from_secs(15),
    );

    let tail = stack.terminal.read(&session.id, None, None).unwrap();
    assert!(tail.contains("PASSWORD=[redacted]"));
    assert!(!tail.contains("secret-value"));

    let bounded = stack.terminal.read(&session.id, None, Some(8)).unwrap();
    assert!(bounded.len() <= 8);
    assert!(tail.len() <= DEFAULT_READ_BYTES + 1);

    stack.terminal.close(&session.id).unwrap();
}

#[test]
fn untrusted_projects_and_unknown_shells_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), false);

    let err = stack
        .terminal
        .open(&id, None, None, None, None, "user")
        .unwrap_err();
    assert!(matches!(err, TerminalError::Project(_)));

    stack.projects.set_trust(&id, true).unwrap();
    let err = stack
        .terminal
        .open(&id, Some("/usr/bin/curl"), None, None, None, "user")
        .unwrap_err();
    assert!(matches!(err, TerminalError::ShellNotAllowed(_)));

    // Nothing was spawned.
    assert!(stack.terminal.list(Some(&id)).unwrap().is_empty());
}

#[test]
fn exec_opens_a_visible_session_and_is_audited() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    let session = match stack.terminal.exec(&id, "echo fergents-exec", "agent:test") {
        Ok(session) => session,
        Err(err) => {
            eprintln!("skipping: {err}");
            return;
        }
    };
    assert_eq!(session.actor, "agent:test");
    assert_eq!(stack.terminal.list(Some(&id)).unwrap().len(), 1);

    let seen = wait_for(
        || {
            stack
                .terminal
                .read(&session.id, None, None)
                .map(|text| text.contains("fergents-exec"))
                .unwrap_or(false)
        },
        Duration::from_secs(15),
    );
    assert!(seen, "the exec output never appeared");

    let rows = stack
        .storage
        .query("SELECT * FROM audit_log WHERE action = 'term.exec'", &[])
        .unwrap();
    assert_eq!(rows.len(), 1);
    let args = rows[0]
        .get("args_json")
        .and_then(|value| value.as_str())
        .unwrap();
    assert!(args.contains("fergents-exec"));

    stack.terminal.close(&session.id).unwrap();
}

#[test]
fn reconcile_closes_sessions_left_running() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    let session = match stack.terminal.open(&id, None, None, None, None, "user") {
        Ok(session) => session,
        Err(err) => {
            eprintln!("skipping: {err}");
            return;
        }
    };

    let affected = stack.terminal.reconcile().unwrap();
    assert_eq!(affected, 1);
    let listed = stack.terminal.list(Some(&id)).unwrap();
    assert_eq!(listed[0].state, SessionState::Closed);
    assert!(listed[0].closed_at.is_some());

    let _ = stack.terminal.close(&session.id);
}
