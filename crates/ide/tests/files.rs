//! File-operation tests (task 3.2).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_events::EventBus;
use fergents_ide::{file_tools, FileService, ProjectError, ProjectService};
use fergents_reversibility::ReversibilityService;
use fergents_storage::{SqliteStorage, Storage};

struct Stack {
    projects: Arc<ProjectService>,
    files: Arc<FileService>,
    reversibility: Arc<ReversibilityService>,
    bus: Arc<EventBus>,
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

fn stack(dir: &Path) -> Stack {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    let projects = Arc::new(ProjectService::new(storage.clone(), bus.clone()));
    let reversibility = Arc::new(ReversibilityService::new(
        storage,
        bus.clone(),
        dir.join("backups"),
    ));
    let files = Arc::new(FileService::new(
        projects.clone(),
        reversibility.clone(),
        bus.clone(),
    ));
    Stack {
        projects,
        files,
        reversibility,
        bus,
    }
}

fn trusted_project(stack: &Stack, dir: &Path) -> String {
    let root = dir.join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let project = stack.projects.open(root.to_str().unwrap()).unwrap();
    stack.projects.set_trust(&project.id, true).unwrap();
    project.id
}

#[test]
fn write_read_roundtrip_and_etag_changes() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());

    let first = stack
        .files
        .write(&id, "src/main.rs", "fn main() {}\n", None)
        .unwrap();
    assert_eq!(first.path, "src/main.rs");
    assert_eq!(first.language_id, "rust");
    assert!(!first.truncated);
    assert_eq!(first.etag.len(), 64);

    let read = stack.files.read(&id, "src/main.rs").unwrap();
    assert_eq!(read.content, "fn main() {}\n");
    assert_eq!(read.etag, first.etag);
    assert_eq!(read.size, 13);

    let second = stack
        .files
        .write(
            &id,
            "src/main.rs",
            "fn main() { print!(\"hi\"); }\n",
            Some(&read.etag),
        )
        .unwrap();
    assert_ne!(second.etag, read.etag);
    assert_eq!(
        stack.files.read(&id, "src/main.rs").unwrap().content,
        "fn main() { print!(\"hi\"); }\n"
    );
}

#[test]
fn stale_etag_conflicts_and_leaves_content_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());

    let original = stack.files.write(&id, "a.txt", "one\n", None).unwrap();
    // An out-of-band change (another editor) after our read.
    std::fs::write(temp.path().join("workspace/a.txt"), "two\n").unwrap();

    let err = stack
        .files
        .write(&id, "a.txt", "three\n", Some(&original.etag))
        .unwrap_err();
    assert!(matches!(err, ProjectError::Conflict(_)));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("workspace/a.txt")).unwrap(),
        "two\n"
    );

    // A base etag for a missing file is also a conflict.
    let err = stack
        .files
        .write(&id, "missing.txt", "x", Some(&original.etag))
        .unwrap_err();
    assert!(matches!(err, ProjectError::Conflict(_)));
    assert!(!temp.path().join("workspace/missing.txt").exists());
}

#[test]
fn mutations_require_trust() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let root = temp.path().join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let project = stack.projects.open(root.to_str().unwrap()).unwrap();

    let err = stack
        .files
        .write(&project.id, "a.txt", "hi", None)
        .unwrap_err();
    assert!(matches!(err, ProjectError::Untrusted(_)));
    assert!(!root.join("a.txt").exists());

    // Reads stay available in restricted mode.
    assert!(stack.files.list(&project.id, "").is_ok());
}

#[test]
fn create_mkdir_rename_delete() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());

    let created = stack
        .files
        .create(&id, "notes/todo.md", "# todo\n")
        .unwrap();
    assert_eq!(created.language_id, "markdown");
    assert!(matches!(
        stack
            .files
            .create(&id, "notes/todo.md", "again")
            .unwrap_err(),
        ProjectError::Conflict(_)
    ));

    let dir = stack.files.mkdir(&id, "src").unwrap();
    assert_eq!(dir.kind, "dir");

    let renamed = stack
        .files
        .rename(&id, "notes/todo.md", "src/todo.md")
        .unwrap();
    assert_eq!(renamed.path, "src/todo.md");
    assert!(!temp.path().join("workspace/notes/todo.md").exists());

    let entries = stack.files.list(&id, "").unwrap();
    let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["notes", "src"]);

    assert!(stack.files.delete(&id, "src").unwrap());
    assert!(!temp.path().join("workspace/src").exists());
    assert!(!stack.files.delete(&id, "src").unwrap());
}

#[test]
fn deleted_files_can_be_restored_from_backups() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());
    stack
        .files
        .write(&id, "keep.txt", "precious\n", None)
        .unwrap();

    stack.files.delete(&id, "keep.txt").unwrap();
    assert!(!temp.path().join("workspace/keep.txt").exists());

    let backups = stack.reversibility.list(10).unwrap();
    assert!(!backups.is_empty(), "delete must be backed up");
    stack.reversibility.restore(&backups[0].id).unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("workspace/keep.txt")).unwrap(),
        "precious\n"
    );
}

#[test]
fn reads_are_bounded_and_report_binary() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());
    let root = temp.path().join("workspace");

    let big = "a".repeat((fergents_ide::MAX_READ_BYTES + 1024) as usize);
    std::fs::write(root.join("big.txt"), &big).unwrap();
    let read = stack.files.read(&id, "big.txt").unwrap();
    assert!(read.truncated);
    assert_eq!(read.size, big.len() as i64);
    assert_eq!(read.content.len(), fergents_ide::MAX_READ_BYTES as usize);

    std::fs::write(root.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
    let binary = stack.files.read(&id, "blob.bin").unwrap();
    assert!(binary.binary);
    assert!(binary.content.is_empty());
}

#[test]
fn containment_is_enforced_on_every_operation() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());

    assert!(matches!(
        stack.files.read(&id, "../escape.txt").unwrap_err(),
        ProjectError::Escape(_)
    ));
    assert!(matches!(
        stack
            .files
            .write(&id, "../escape.txt", "x", None)
            .unwrap_err(),
        ProjectError::Escape(_)
    ));
    assert!(matches!(
        stack.files.delete(&id, "../escape.txt").unwrap_err(),
        ProjectError::Escape(_)
    ));
    assert!(matches!(
        stack
            .files
            .rename(&id, "a.txt", "../escape.txt")
            .unwrap_err(),
        ProjectError::Escape(_)
    ));
}

#[test]
fn the_watcher_emits_file_changed_for_external_writes() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());
    stack.files.watch(&id).unwrap();
    assert!(stack.files.is_watching(&id));

    // `file.saved` during setup must not be mistaken for an external change.
    let already = stack
        .bus
        .replay(&["file.changed".to_string()], None, None)
        .unwrap();

    std::fs::write(temp.path().join("workspace/external.txt"), "hello\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = false;
    while Instant::now() < deadline {
        let events = stack
            .bus
            .replay(&["file.changed".to_string()], None, None)
            .unwrap();
        if events.len() > already.len()
            && events.iter().any(|event| {
                event.payload["path"] == serde_json::json!("external.txt")
                    && event.payload["project_id"] == serde_json::json!(id)
            })
        {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(seen, "expected a file.changed event for external.txt");

    assert!(stack.files.unwatch(&id));
    assert!(!stack.files.is_watching(&id));
}

#[tokio::test]
async fn agent_file_tools_are_project_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = trusted_project(&stack, temp.path());

    let tools = file_tools(stack.files.clone());
    let names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
    assert_eq!(names, vec!["file.read", "file.write"]);

    let write = tools
        .iter()
        .find(|tool| tool.name() == "file.write")
        .unwrap();
    write
        .call(serde_json::json!({
            "project_id": id,
            "path": "agent.txt",
            "content": "from the agent\n"
        }))
        .await
        .unwrap();

    let read = tools
        .iter()
        .find(|tool| tool.name() == "file.read")
        .unwrap();
    let value = read
        .call(serde_json::json!({ "project_id": id, "path": "agent.txt" }))
        .await
        .unwrap();
    assert_eq!(value["content"], serde_json::json!("from the agent\n"));

    // A stale etag is surfaced as a tool error, not a panic.
    let err = write
        .call(serde_json::json!({
            "project_id": id,
            "path": "agent.txt",
            "content": "stale",
            "base_etag": "deadbeef"
        }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("file conflict"), "got: {err}");

    // Untrusted projects are refused, and no file appears.
    let other = temp.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let untrusted = stack.projects.open(other.to_str().unwrap()).unwrap();
    let err = write
        .call(serde_json::json!({
            "project_id": untrusted.id,
            "path": "x.txt",
            "content": "nope"
        }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not trusted"), "got: {err}");
}
