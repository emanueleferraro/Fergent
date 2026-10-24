//! Language-server sessions against a deterministic fake server (task 3.3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_events::EventBus;
use fergents_ide::ProjectService;
use fergents_lsp::{LspError, LspService, ServerStatus};
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

fn fake_server() -> String {
    env!("CARGO_BIN_EXE_fake-lsp-server").to_string()
}

struct Stack {
    lsp: LspService,
    projects: Arc<ProjectService>,
    bus: Arc<EventBus>,
}

fn stack(dir: &Path) -> Stack {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    let projects = Arc::new(ProjectService::new(storage.clone(), bus.clone()));
    let lsp = LspService::with_timeout(
        storage,
        bus.clone(),
        projects.clone(),
        Duration::from_secs(10),
    );
    Stack { lsp, projects, bus }
}

fn project(stack: &Stack, dir: &Path, trusted: bool) -> String {
    let root = dir.join("workspace");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let project = stack.projects.open(root.to_str().unwrap()).unwrap();
    if trusted {
        stack.projects.set_trust(&project.id, true).unwrap();
    }
    project.id
}

#[test]
fn a_registered_server_starts_and_serves_requests() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    assert!(stack.lsp.servers().unwrap().is_empty());
    let record = stack
        .lsp
        .add_server("rust", &fake_server(), &[], &BTreeMap::new(), "user", true)
        .unwrap();
    assert_eq!(record.language_id, "rust");
    assert!(record.installed);
    assert_eq!(stack.lsp.servers().unwrap().len(), 1);

    let started = stack.lsp.start(&id, "rust").unwrap();
    assert_eq!(started.status, ServerStatus::Ready);
    assert_eq!(
        stack.lsp.status(&id, "rust").unwrap().status,
        ServerStatus::Ready
    );

    stack
        .lsp
        .document_open(&id, "src/main.rs", "fn main() {}\n", None)
        .unwrap();
    assert_eq!(stack.lsp.document_version(&id, "src/main.rs"), Some(1));

    let hover = stack.lsp.hover(&id, "src/main.rs", 0, 3).unwrap();
    assert_eq!(hover["contents"]["value"], serde_json::json!("fake hover"));

    let completion = stack.lsp.completion(&id, "src/main.rs", 0, 3).unwrap();
    assert_eq!(
        completion["items"][0]["label"],
        serde_json::json!("fake_item")
    );

    let resolved = stack
        .lsp
        .completion_resolve(&id, "src/main.rs", completion["items"][0].clone())
        .unwrap();
    assert_eq!(
        resolved["detail"],
        serde_json::json!("resolved by the fake server")
    );

    assert_eq!(
        stack.lsp.symbols(&id, "src/main.rs").unwrap(),
        serde_json::json!([])
    );
    assert_eq!(
        stack.lsp.references(&id, "src/main.rs", 0, 3).unwrap(),
        serde_json::json!([])
    );

    let version = stack
        .lsp
        .document_change(&id, "src/main.rs", "fn main() { }\n", None)
        .unwrap();
    assert_eq!(version, 2);
    assert_eq!(stack.lsp.document_version(&id, "src/main.rs"), Some(2));

    stack.lsp.document_save(&id, "src/main.rs", None).unwrap();
    stack.lsp.document_close(&id, "src/main.rs").unwrap();
    assert_eq!(stack.lsp.document_version(&id, "src/main.rs"), None);

    assert!(stack.lsp.stop(&id, "rust").unwrap());
    assert_eq!(
        stack.lsp.status(&id, "rust").unwrap().status,
        ServerStatus::Stopped
    );
}

#[test]
fn diagnostics_are_stored_and_published() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);
    stack
        .lsp
        .add_server("rust", &fake_server(), &[], &BTreeMap::new(), "user", true)
        .unwrap();
    stack.lsp.start(&id, "rust").unwrap();
    stack
        .lsp
        .document_open(&id, "src/main.rs", "fn main() {}\n", None)
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut diagnostics = Vec::new();
    while Instant::now() < deadline {
        diagnostics = stack.lsp.diagnostics(&id, Some("src/main.rs"));
        if !diagnostics.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, "src/main.rs");
    assert_eq!(diagnostics[0].severity, 2);
    assert_eq!(diagnostics[0].message, "fake warning");
    assert_eq!(diagnostics[0].code.as_deref(), Some("F001"));

    let events = stack
        .bus
        .replay(&["lsp.diagnostics".to_string()], None, None)
        .unwrap();
    assert!(events.iter().any(|event| {
        event.payload["project_id"] == serde_json::json!(id)
            && event.payload["path"] == serde_json::json!("src/main.rs")
    }));

    // `window/logMessage` is surfaced as `lsp.log`.
    let logs = stack
        .bus
        .replay(&["lsp.log".to_string()], None, None)
        .unwrap();
    assert!(!logs.is_empty());

    assert!(stack.lsp.stop(&id, "rust").unwrap());
}

#[test]
fn untrusted_projects_cannot_start_servers() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), false);
    stack
        .lsp
        .add_server("rust", &fake_server(), &[], &BTreeMap::new(), "user", true)
        .unwrap();

    assert!(matches!(
        stack.lsp.start(&id, "rust"),
        Err(LspError::Project(_))
    ));
}

#[test]
fn unknown_languages_and_documents_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    assert!(matches!(
        stack.lsp.status(&id, "haskell"),
        Err(LspError::UnknownServer(_))
    ));
    assert!(matches!(
        stack.lsp.document_open(&id, "README.md", "# hi", None),
        Err(LspError::UnknownServer(_))
    ));
    assert!(matches!(
        stack.lsp.document_change(&id, "src/main.rs", "x", None),
        Err(LspError::DocumentNotOpen(_))
    ));
}

#[test]
fn registered_servers_can_be_removed_and_missing_ones_are_not_installed() {
    let temp = tempfile::tempdir().unwrap();
    let stack = stack(temp.path());
    let id = project(&stack, temp.path(), true);

    let record = stack
        .lsp
        .add_server(
            "rust",
            "/nonexistent/definitely-not-a-language-server",
            &[],
            &BTreeMap::new(),
            "user",
            true,
        )
        .unwrap();
    assert!(!record.installed);

    // A missing executable is reported, not treated as a crash.
    let started = stack.lsp.start(&id, "rust").unwrap();
    assert_eq!(started.status, ServerStatus::NotInstalled);

    assert!(stack.lsp.remove_server(&record.id).unwrap());
    assert!(stack.lsp.servers().unwrap().is_empty());
}
