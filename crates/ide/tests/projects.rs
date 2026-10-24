//! Project layer tests (task 3.1).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::EventBus;
use fergents_ide::{ProjectError, ProjectService, TrustState};
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .unwrap()
}

fn service(dir: &Path) -> Arc<ProjectService> {
    let storage: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open(&dir.join("global.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 64));
    Arc::new(ProjectService::new(storage, bus))
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn opens_a_project_and_detects_git() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("src/main.rs"), "fn main() {}\n");

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    assert_eq!(project.name, "repo");
    assert_eq!(project.vcs.as_deref(), Some("git"));
    assert_eq!(project.trust, TrustState::Untrusted);
    assert!(!project.is_trusted());
    assert_eq!(projects.list().unwrap().len(), 1);

    // Re-opening the same path is idempotent.
    let again = projects.open(root.to_str().unwrap()).unwrap();
    assert_eq!(again.id, project.id);
    assert_eq!(projects.list().unwrap().len(), 1);
}

#[test]
fn ignore_rules_and_fuzzy_file_list() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join(".gitignore"), "node_modules/\n");
    write(&root.join("node_modules/pkg/index.js"), "ignored\n");
    write(&root.join("src/main.rs"), "fn main() {}\n");
    write(&root.join("main.rs"), "fn other() {}\n");

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    let files = projects.files(&project.id, "", Some(50)).unwrap();
    assert!(files.contains(&"main.rs".to_string()));
    assert!(files.contains(&"src/main.rs".to_string()));
    assert!(!files.iter().any(|file| file.contains("node_modules")));

    let ranked = projects.files(&project.id, "main", Some(5)).unwrap();
    assert_eq!(ranked[0], "main.rs");
}

#[test]
fn search_is_bounded_and_skips_binary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(&root.join("a.txt"), "first line\nneedle here\nlast\n");
    write(&root.join("b.txt"), "needle again\n");
    std::fs::write(
        root.join("blob.bin"),
        [0u8, b'n', b'e', b'e', b'd', b'l', b'e'],
    )
    .unwrap();

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    let hits = projects.search(&project.id, "needle", Some(10)).unwrap();
    assert!(hits.iter().any(|hit| hit.path == "a.txt" && hit.line == 2));
    assert!(hits.iter().any(|hit| hit.path == "b.txt"));
    assert!(!hits.iter().any(|hit| hit.path == "blob.bin"));

    let capped = projects.search(&project.id, "needle", Some(1)).unwrap();
    assert_eq!(capped.len(), 1);
}

#[test]
fn trust_transitions_are_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();
    assert!(matches!(
        projects.require_trusted(&project.id),
        Err(ProjectError::Untrusted(_))
    ));

    let trusted = projects.set_trust(&project.id, true).unwrap();
    assert!(trusted.is_trusted());
    assert!(projects.require_trusted(&project.id).is_ok());

    // A fresh service over the same store sees the persisted trust state.
    let reopened = service(temp.path());
    assert!(reopened.get(&project.id).unwrap().unwrap().is_trusted());

    let untrusted = projects.set_trust(&project.id, false).unwrap();
    assert_eq!(untrusted.trust, TrustState::Untrusted);
}

#[test]
fn update_renames_and_links_a_brain() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    let updated = projects
        .update(
            &project.id,
            Some("Renamed"),
            Some(Some("brain-1".to_string())),
            Some(serde_json::json!({ "tab_size": 2 })),
        )
        .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.brain_id.as_deref(), Some("brain-1"));
    assert_eq!(updated.settings.unwrap()["tab_size"], 2);

    let cleared = projects
        .update(&project.id, None, Some(None), None)
        .unwrap();
    assert!(cleared.brain_id.is_none());
    assert_eq!(cleared.name, "Renamed");
}

#[test]
fn containment_rejects_escapes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    write(&root.join("src/main.rs"), "");

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    assert!(projects.resolve(&project, "src/main.rs").is_ok());
    assert!(matches!(
        projects.resolve(&project, "../escape.txt"),
        Err(ProjectError::Escape(_))
    ));
    assert!(matches!(
        projects.resolve(&project, "/etc/passwd"),
        Err(ProjectError::Escape(_))
    ));
}

#[cfg(unix)]
#[test]
fn containment_rejects_symlink_escapes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    assert!(matches!(
        projects.resolve(&project, "link/secret.txt"),
        Err(ProjectError::Escape(_))
    ));
}

#[test]
fn editor_layouts_are_persisted_per_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let projects = service(temp.path());
    let project = projects.open(root.to_str().unwrap()).unwrap();

    // Nothing saved yet.
    assert!(projects.layout(&project.id).unwrap().is_none());

    let layout = serde_json::json!({
        "kind": "split",
        "direction": "row",
        "ratio": 0.5,
        "first": { "kind": "pane", "view": "chat" },
        "second": { "kind": "pane", "view": "ide" }
    });
    let saved = projects.set_layout(&project.id, layout.clone()).unwrap();
    assert_eq!(saved, layout);

    // A fresh service over the same store sees the persisted layout.
    let reopened = service(temp.path());
    assert_eq!(reopened.layout(&project.id).unwrap().unwrap(), layout);

    // Saving again replaces (does not duplicate) the row.
    let replaced = serde_json::json!({ "kind": "pane", "view": "ide" });
    reopened.set_layout(&project.id, replaced.clone()).unwrap();
    assert_eq!(projects.layout(&project.id).unwrap().unwrap(), replaced);

    // Non-object layouts are rejected.
    assert!(matches!(
        projects.set_layout(&project.id, serde_json::json!("wide")),
        Err(ProjectError::Invalid(_))
    ));
    // Unknown projects are rejected.
    assert!(matches!(
        projects.layout("nope"),
        Err(ProjectError::Unknown(_))
    ));
}

#[test]
fn unknown_project_is_reported() {
    let temp = tempfile::tempdir().unwrap();
    let projects = service(temp.path());
    assert!(matches!(projects.get("nope"), Ok(None)));
    assert!(matches!(
        projects.require("nope"),
        Err(ProjectError::Unknown(_))
    ));
    assert!(matches!(
        projects.set_trust("nope", true),
        Err(ProjectError::Unknown(_))
    ));
}
