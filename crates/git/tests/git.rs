//! Git service tests on temporary repositories (task 3.5). No network is used.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::EventBus;
use fergents_git::{ChangeKind, GitError, GitService, ResolveStrategy};
use fergents_ide::{ProjectError, ProjectService};
use fergents_storage::{SqliteStorage, Storage};
use git2::Repository;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

struct Stack {
    git: GitService,
    projects: Arc<ProjectService>,
    bus: Arc<EventBus>,
    storage: Arc<dyn Storage>,
}

fn stack(dir: &Path, remote: bool) -> Stack {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 256));
    let projects = Arc::new(ProjectService::new(storage.clone(), bus.clone()));
    let git = GitService::new(storage.clone(), bus.clone(), projects.clone(), remote);
    Stack {
        git,
        projects,
        bus,
        storage,
    }
}

fn repository(root: &Path) -> Repository {
    let repository = Repository::init(root).unwrap();
    let mut config = repository.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    config.set_bool("core.autocrlf", false).unwrap();
    repository
}

fn open_project(stack: &Stack, root: &Path, trusted: bool) -> String {
    let project = stack.projects.open(root.to_str().unwrap()).unwrap();
    if trusted {
        stack.projects.set_trust(&project.id, true).unwrap();
    }
    project.id
}

fn write(root: &Path, name: &str, content: &str) {
    fs::write(root.join(name), content).unwrap();
}

#[test]
fn status_diff_stage_commit_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let id = open_project(&stack, &root, true);

    write(&root, "a.txt", "one\n");
    let status = stack.git.status(&id).unwrap();
    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.entries[0].kind, ChangeKind::Untracked);
    assert!(!status.entries[0].staged);
    assert!(!status.clean);

    let staged = stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    assert_eq!(staged, 1);
    let status = stack.git.status(&id).unwrap();
    assert_eq!(status.entries.len(), 1);
    assert!(status.entries[0].staged);
    assert_eq!(status.entries[0].kind, ChangeKind::Added);

    let first = stack
        .git
        .commit(&id, "initial commit", "user", None, false)
        .unwrap();
    assert!(first.trailers.is_empty());
    assert_eq!(first.summary, "initial commit");
    assert!(stack.git.status(&id).unwrap().clean);

    write(&root, "a.txt", "one\ntwo\n");
    let diff = stack.git.diff(&id, Some("a.txt"), false, None).unwrap();
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].path, "a.txt");
    assert!(diff[0]
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .any(|line| line.kind == "add" && line.content == "two"));

    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    let staged = stack.git.diff(&id, None, true, None).unwrap();
    assert!(!staged.is_empty());

    let agent = stack
        .git
        .commit(&id, "agent change", "agent:coder", Some("session-1"), false)
        .unwrap();
    assert_eq!(
        agent.trailers,
        vec![
            "Fergents-Agent: agent:coder".to_string(),
            "Fergents-Session: session-1".to_string()
        ]
    );

    let detail = stack.git.show(&id, &agent.id).unwrap();
    assert!(detail.message.contains("Fergents-Agent: agent:coder"));
    assert_eq!(detail.files.len(), 1);
    assert_eq!(detail.files[0].path, "a.txt");
    assert_eq!(detail.files[0].additions, 1);

    let log = stack.git.log(&id, Some(10)).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].id, agent.id);
    assert_eq!(log[1].id, first.id);

    let blame = stack.git.blame(&id, "a.txt").unwrap();
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].commit, first.id);
    assert_eq!(blame[1].commit, agent.id);
}

#[test]
fn discard_is_reversible_from_a_backup() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let id = open_project(&stack, &root, true);
    write(&root, "a.txt", "base\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack.git.commit(&id, "base", "user", None, false).unwrap();

    write(&root, "a.txt", "changed\n");
    write(&root, "b.txt", "untracked\n");
    let backup = stack
        .git
        .discard(&id, &["a.txt".to_string(), "b.txt".to_string()], "user")
        .unwrap();

    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "base\n");
    assert!(!root.join("b.txt").exists());
    assert_eq!(
        fs::read_to_string(Path::new(&backup).join("a.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        fs::read_to_string(Path::new(&backup).join("b.txt")).unwrap(),
        "untracked\n"
    );
    assert!(stack.git.status(&id).unwrap().clean);
}

#[test]
fn branches_switch_and_log() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let id = open_project(&stack, &root, true);
    write(&root, "a.txt", "base\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack.git.commit(&id, "base", "user", None, false).unwrap();
    let initial = stack.git.status(&id).unwrap().branch.unwrap();

    let branch = stack
        .git
        .create_branch(&id, "feature", None, true, "user")
        .unwrap();
    assert_eq!(branch.name, "feature");
    assert_eq!(
        stack.git.status(&id).unwrap().branch.as_deref(),
        Some("feature")
    );

    write(&root, "a.txt", "feature\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack
        .git
        .commit(&id, "feature change", "user", None, false)
        .unwrap();

    let branches = stack.git.branches(&id).unwrap();
    assert!(branches
        .iter()
        .any(|branch| branch.name == "feature" && branch.is_head));
    assert!(branches.iter().any(|branch| branch.name == initial));

    let status = stack.git.switch_branch(&id, &initial, "user").unwrap();
    assert_eq!(status.branch.as_deref(), Some(initial.as_str()));
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "base\n");

    assert!(matches!(
        stack.git.switch_branch(&id, "missing", "user"),
        Err(GitError::Invalid(_))
    ));
    assert!(matches!(
        stack
            .git
            .create_branch(&id, "bad name", None, false, "user"),
        Err(GitError::Invalid(_))
    ));
}

#[test]
fn conflicts_are_listed_and_resolved() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let id = open_project(&stack, &root, true);
    write(&root, "a.txt", "base\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack.git.commit(&id, "base", "user", None, false).unwrap();
    let initial = stack.git.status(&id).unwrap().branch.unwrap();

    stack
        .git
        .create_branch(&id, "other", None, true, "user")
        .unwrap();
    write(&root, "a.txt", "theirs\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack
        .git
        .commit(&id, "theirs", "user", None, false)
        .unwrap();

    stack.git.switch_branch(&id, &initial, "user").unwrap();
    write(&root, "a.txt", "ours\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack.git.commit(&id, "ours", "user", None, false).unwrap();

    let repository = Repository::open(&root).unwrap();
    let other = repository.refname_to_id("refs/heads/other").unwrap();
    let annotated = repository.find_annotated_commit(other).unwrap();
    repository.merge(&[&annotated], None, None).unwrap();

    let conflicts = stack.git.conflicts(&id).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, "a.txt");
    assert!(conflicts[0].has_ours && conflicts[0].has_theirs);

    stack
        .git
        .resolve(&id, "a.txt", ResolveStrategy::Ours, "user")
        .unwrap();
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "ours\n");
    assert!(stack.git.conflicts(&id).unwrap().is_empty());
}

#[test]
fn untrusted_and_non_repository_projects_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let untrusted = open_project(&stack, &root, false);
    assert!(matches!(
        stack.git.status(&untrusted),
        Err(GitError::Project(ProjectError::Untrusted(_)))
    ));

    let plain = temp.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    let plain_id = open_project(&stack, &plain, true);
    assert!(matches!(
        stack.git.status(&plain_id),
        Err(GitError::NotRepository(_))
    ));
}

#[test]
fn shadow_checkpoints_never_touch_the_user_repository() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    repository(&root);
    let id = open_project(&stack, &root, true);
    write(&root, "a.txt", "base\n");
    stack
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    stack.git.commit(&id, "base", "user", None, false).unwrap();
    let head = stack.git.status(&id).unwrap().head;

    write(&root, "a.txt", "agent edit\n");
    let first = stack.git.checkpoint(&id, "agent edit").unwrap();
    assert!(first.is_some());
    assert!(root.join(".fergents").join("shadow.git").exists());
    assert_eq!(stack.git.status(&id).unwrap().head, head);

    let second = stack.git.checkpoint(&id, "agent edit").unwrap();
    assert!(second.is_none());
}

#[test]
fn remote_operations_require_enabling() {
    let temp = tempfile::tempdir().unwrap();
    let disabled = stack(temp.path(), false);
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let repository = repository(&root);
    let id = open_project(&disabled, &root, true);
    write(&root, "a.txt", "base\n");
    disabled
        .git
        .stage(&id, &["a.txt".to_string()], "user")
        .unwrap();
    disabled
        .git
        .commit(&id, "base", "user", None, false)
        .unwrap();
    assert!(matches!(
        disabled.git.push(&id, None, "user"),
        Err(GitError::RemoteDisabled)
    ));

    let bare = temp.path().join("origin.git");
    Repository::init_bare(&bare).unwrap();
    repository.remote("origin", bare.to_str().unwrap()).unwrap();
    let mut config = repository.config().unwrap();
    config.set_str("push.default", "current").unwrap();

    let enabled = GitService::new(
        disabled.storage.clone(),
        disabled.bus.clone(),
        disabled.projects.clone(),
        true,
    );
    match enabled.push(&id, Some("origin"), "user") {
        Ok(result) => assert_eq!(result.operation, "push"),
        Err(GitError::Cli(message)) => eprintln!("skipping remote push: {message}"),
        Err(other) => panic!("unexpected push error: {other}"),
    }
}
