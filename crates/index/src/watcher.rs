//! The filesystem watcher (`SEARCH.md` §13.1; `SECURITY.md` §7).
//!
//! Watches the vault recursively and translates `notify` events into queue
//! jobs, honouring the same `.brainignore` / built-in ignores as the scanner so
//! the watcher never escapes the brain's filesystem scope. It only enqueues —
//! the same [`crate::pipeline::Indexer`] processing path serves watcher and
//! scan triggers alike.

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde_json::json;

use crate::error::IndexError;
use crate::events::EventSink;
use crate::queue::{IndexJobOp, IndexQueue};
use crate::scan::VaultScanner;

/// A running vault watcher. Drop it (or call [`VaultWatcher::stop`]) to stop.
pub struct VaultWatcher {
    watcher: Option<notify::RecommendedWatcher>,
    handle: Option<JoinHandle<()>>,
}

impl VaultWatcher {
    /// Start watching `root`, enqueueing jobs into `queue` and emitting
    /// `file.*` / `index.queued` events through `sink`.
    pub fn start(
        root: PathBuf,
        brain_id: String,
        queue: IndexQueue,
        sink: Arc<dyn EventSink>,
    ) -> Result<Self, IndexError> {
        let scanner = VaultScanner::new(root.clone())?;
        let (tx, rx) = channel::<notify::Result<Event>>();

        let watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(|e| IndexError::Io(e.to_string()))?;

        let handle = std::thread::Builder::new()
            .name("fergents-watcher".to_string())
            .spawn(move || {
                while let Ok(res) = rx.recv() {
                    let Ok(event) = res else { continue };
                    handle_event(&scanner, &brain_id, &queue, &sink, &event);
                }
            })
            .map_err(|e| IndexError::Io(e.to_string()))?;

        let mut watcher = Self {
            watcher: Some(watcher),
            handle: Some(handle),
        };
        if let Some(inner) = watcher.watcher.as_mut() {
            inner
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| IndexError::Io(e.to_string()))?;
        }
        Ok(watcher)
    }

    /// Stop watching and join the worker thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        // Dropping the notify watcher drops its callback (and its sender), so
        // the worker thread observes a closed channel and exits.
        drop(self.watcher.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn handle_event(
    scanner: &VaultScanner,
    brain_id: &str,
    queue: &IndexQueue,
    sink: &Arc<dyn EventSink>,
    event: &Event,
) {
    // Rename: `notify` reports the old and new paths together.
    if let EventKind::Modify(notify::event::ModifyKind::Name(_)) = event.kind {
        if event.paths.len() == 2 {
            let from = scanner.relative(&event.paths[0]);
            let to = scanner.relative(&event.paths[1]);
            if !scanner.is_ignored(&to, false) {
                enqueue(queue, sink, brain_id, &to, IndexJobOp::Upsert);
            }
            sink.emit("file.renamed", json!({ "from_path": from, "to_path": to }));
            return;
        }
    }

    for path in &event.paths {
        let rel = scanner.relative(path);
        if rel.is_empty() || scanner.is_ignored(&rel, path.is_dir()) {
            continue;
        }
        let (op, kind) = match event.kind {
            EventKind::Remove(_) => (IndexJobOp::Delete, "file.deleted"),
            EventKind::Create(_) => (IndexJobOp::Upsert, "file.created"),
            _ => (IndexJobOp::Upsert, "file.changed"),
        };
        enqueue(queue, sink, brain_id, &rel, op);
        sink.emit(kind, json!({ "path": rel }));
    }
}

fn enqueue(
    queue: &IndexQueue,
    sink: &Arc<dyn EventSink>,
    brain_id: &str,
    rel_path: &str,
    op: IndexJobOp,
) {
    if queue.enqueue(rel_path, op, 0).is_ok() {
        sink.emit(
            "index.queued",
            json!({ "brain_id": brain_id, "path": rel_path, "op": op.as_str() }),
        );
    }
}
