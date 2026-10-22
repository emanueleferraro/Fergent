//! Reversibility: snapshots, restores, and run-level rollback.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::EventBus;
use fergents_reversibility::{BackupContext, ReversibilityService};
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn service() -> (tempfile::TempDir, Arc<SqliteStorage>, ReversibilityService) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 128));
    let storage_dyn: Arc<dyn Storage> = storage.clone();
    let service = ReversibilityService::new(storage_dyn, bus, dir.path().join("backups"));
    (dir, storage, service)
}

#[test]
fn a_write_is_backed_up_and_restorable() {
    let (dir, storage, service) = service();
    let file = dir.path().join("note.md");
    std::fs::write(&file, "original").unwrap();

    let backup = service
        .snapshot(&file, &BackupContext::new().for_run("run-1").by_agent("a1"))
        .unwrap();
    assert!(backup.existed);
    assert!(Path::new(&backup.backup_path).exists());

    // The agent mutates the file...
    std::fs::write(&file, "mutated").unwrap();

    // ...and the restore returns the exact prior state.
    let restored = service.restore(&backup.id).unwrap();
    assert_eq!(restored, file.to_string_lossy());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");

    // Both the snapshot and the restore are audited.
    let rows = storage
        .query(
            "SELECT * FROM audit_log WHERE action LIKE 'reversibility.%' ORDER BY rowid ASC",
            &[],
        )
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]["action"],
        serde_json::json!("reversibility.snapshot")
    );
    assert_eq!(rows[0]["reversible"], serde_json::json!(1));
    assert_eq!(
        rows[1]["action"],
        serde_json::json!("reversibility.restore")
    );
    assert_eq!(rows[0]["correlation_id"], serde_json::json!("run-1"));

    let listed = service.list(10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, backup.id);
    assert_eq!(listed[0].path, file.to_string_lossy());
    assert_eq!(listed[0].run_id.as_deref(), Some("run-1"));

    assert!(service.restore("missing").is_err());
}

#[test]
fn snapshotting_an_absent_file_deletes_on_restore() {
    let (dir, _storage, service) = service();
    let file = dir.path().join("created.md");
    let backup = service.snapshot(&file, &BackupContext::default()).unwrap();
    assert!(!backup.existed);

    std::fs::write(&file, "created by the agent").unwrap();
    service.restore(&backup.id).unwrap();
    assert!(!file.exists(), "restore removes what did not exist before");
}

#[test]
fn restore_run_rolls_back_every_file() {
    let (dir, _storage, service) = service();
    let first = dir.path().join("a.md");
    let second = dir.path().join("b.md");
    std::fs::write(&first, "a0").unwrap();
    std::fs::write(&second, "b0").unwrap();

    let context = BackupContext::new().for_run("run-9");
    let first_backup = service.snapshot(&first, &context).unwrap();
    let _second_backup = service.snapshot(&second, &context).unwrap();
    std::fs::write(&first, "a1").unwrap();
    std::fs::write(&second, "b1").unwrap();

    assert_eq!(service.backups_for_run("run-9").unwrap().len(), 2);
    let restored = service.restore_run("run-9").unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "a0");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "b0");

    // Restoring a second time is idempotent.
    service.restore_run("run-9").unwrap();
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "a0");
    assert!(service.backups_for_run("run-9").unwrap()[0].id != first_backup.id);
}
