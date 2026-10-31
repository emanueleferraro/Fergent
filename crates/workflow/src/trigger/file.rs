//! File triggers (spec §2.4).
//!
//! A trigger watches one project root recursively, filters `notify` events
//! through a glob, and coalesces bursts into a single run. Containment is
//! checked on the canonicalised path, so a symlink that points outside the
//! root never fires (spec §2.4, `SECURITY.md` §7).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use globset::{Glob, GlobMatcher};
use notify::{EventKind, RecursiveMode, Watcher};
use serde_json::json;

use crate::document::FileEvent;
use crate::error::WorkflowError;
use crate::trigger::{Clock, FireFn, SystemClock, TriggerFire};

/// The quiet period that ends a burst of writes (spec §2.4: 300 ms).
pub const DEFAULT_DEBOUNCE_MS: u64 = 300;

/// The minimum gap between two runs for the same trigger (spec §2.4: 2 s).
pub const DEFAULT_COALESCE_MS: i64 = 2_000;

/// Directories that never produce a fire.
const IGNORED: [&str; 4] = [".git", "node_modules", "target", "trash"];

/// A file trigger's declaration plus its watch settings.
#[derive(Debug, Clone)]
pub struct FileTriggerConfig {
    /// The owning workflow.
    pub workflow_id: String,
    /// The trigger id within the document.
    pub trigger_id: String,
    /// The watched project root.
    pub root: PathBuf,
    /// A glob relative to `root`.
    pub glob: String,
    /// Watched event kinds; empty means all of them.
    pub events: Vec<FileEvent>,
    /// The debounce quiet period.
    pub debounce_ms: u64,
    /// The coalescing window.
    pub coalesce_ms: i64,
}

impl FileTriggerConfig {
    /// A configuration with the spec defaults for debounce and coalescing.
    pub fn new(
        workflow_id: impl Into<String>,
        trigger_id: impl Into<String>,
        root: impl Into<PathBuf>,
        glob: impl Into<String>,
        events: Vec<FileEvent>,
    ) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            trigger_id: trigger_id.into(),
            root: root.into(),
            glob: glob.into(),
            events,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            coalesce_ms: DEFAULT_COALESCE_MS,
        }
    }
}

/// A running file watcher. Drop it (or call [`FileTrigger::stop`]) to stop.
pub struct FileTrigger {
    hook: Arc<Hook>,
    watcher: Option<notify::RecommendedWatcher>,
    handle: Option<JoinHandle<()>>,
}

impl FileTrigger {
    /// Start watching the configured root.
    pub fn start(config: FileTriggerConfig, fire: FireFn) -> Result<Self, WorkflowError> {
        Self::start_with_clock(config, Arc::new(SystemClock), fire)
    }

    /// Start watching with an injected clock.
    pub fn start_with_clock(
        config: FileTriggerConfig,
        clock: Arc<dyn Clock>,
        fire: FireFn,
    ) -> Result<Self, WorkflowError> {
        let hook = Arc::new(Hook::new(config, clock, fire)?);
        let root = hook.root.clone();
        let debounce = Duration::from_millis(hook.debounce_ms);

        let (tx, rx) = channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(watcher_error)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(watcher_error)?;

        let thread_hook = Arc::clone(&hook);
        let thread = std::thread::Builder::new()
            .name(format!("fergents-workflow-file-{}", hook.trigger_id))
            .spawn(move || pump(rx, thread_hook, debounce))?;

        Ok(Self {
            hook,
            watcher: Some(watcher),
            handle: Some(thread),
        })
    }

    /// The owning workflow.
    pub fn workflow_id(&self) -> &str {
        &self.hook.workflow_id
    }

    /// The trigger id.
    pub fn trigger_id(&self) -> &str {
        &self.hook.trigger_id
    }

    /// The canonicalised watched root.
    pub fn root(&self) -> &Path {
        &self.hook.root
    }

    /// Evaluate one change. Returns whether a run started.
    ///
    /// The watcher thread calls this for every coalesced burst; tests call it
    /// directly to exercise containment, globs, and coalescing deterministically.
    pub fn observe(&self, path: &Path, kind: FileEvent) -> Result<bool, WorkflowError> {
        self.hook.observe(path, kind)
    }

    /// Stop watching and join the worker thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        // Dropping the watcher drops its callback, closing the channel, so the
        // worker thread exits.
        drop(self.watcher.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for FileTrigger {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Hook {
    workflow_id: String,
    trigger_id: String,
    root: PathBuf,
    matcher: GlobMatcher,
    events: Vec<FileEvent>,
    coalesce_ms: i64,
    debounce_ms: u64,
    clock: Arc<dyn Clock>,
    last_fire: Mutex<Option<i64>>,
    fire: FireFn,
}

impl Hook {
    fn new(
        config: FileTriggerConfig,
        clock: Arc<dyn Clock>,
        fire: FireFn,
    ) -> Result<Self, WorkflowError> {
        let root = std::fs::canonicalize(&config.root)?;
        let glob = Glob::new(&config.glob).map_err(|err| {
            WorkflowError::Invalid(format!("`{}` is not a glob: {err}", config.glob))
        })?;
        Ok(Self {
            workflow_id: config.workflow_id,
            trigger_id: config.trigger_id,
            root,
            matcher: glob.compile_matcher(),
            events: config.events,
            coalesce_ms: config.coalesce_ms,
            debounce_ms: config.debounce_ms,
            clock,
            last_fire: Mutex::new(None),
            fire,
        })
    }

    fn observe(&self, path: &Path, kind: FileEvent) -> Result<bool, WorkflowError> {
        if !self.events.is_empty() && !self.events.contains(&kind) {
            return Ok(false);
        }
        let Some(relative) = self.relative(path) else {
            return Ok(false);
        };
        let normalized = relative.to_string_lossy().replace('\\', "/");
        if !self.matcher.is_match(&normalized) {
            return Ok(false);
        }

        let now = self.clock.now_ms();
        {
            let mut last = self.last_fire.lock().expect("file trigger poisoned");
            if let Some(at) = *last {
                if now - at < self.coalesce_ms {
                    return Ok(false);
                }
            }
            *last = Some(now);
        }

        (self.fire)(TriggerFire {
            workflow_id: self.workflow_id.clone(),
            trigger_id: self.trigger_id.clone(),
            kind: "file",
            actor: format!("trigger:{}", self.trigger_id),
            inputs: json!({ "path": normalized, "event": event_name(kind) }),
        })?;
        Ok(true)
    }

    /// The path relative to the root, or `None` when it escapes the root or
    /// falls under an ignored directory.
    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let resolved = if path.exists() {
            std::fs::canonicalize(path).ok()?
        } else {
            // A delete event names a path that is already gone: resolve the
            // parent and keep the file name.
            let parent = std::fs::canonicalize(path.parent()?).ok()?;
            parent.join(path.file_name()?)
        };
        let relative = resolved.strip_prefix(&self.root).ok()?.to_path_buf();
        let ignored = relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| IGNORED.contains(&name))
        });
        if ignored {
            None
        } else {
            Some(relative)
        }
    }
}

/// Read watcher events, debounce each burst, and observe the newest one.
fn pump(rx: Receiver<notify::Result<notify::Event>>, hook: Arc<Hook>, debounce: Duration) {
    while let Ok(first) = rx.recv() {
        let Some(latest) = first.ok().as_ref().and_then(classify) else {
            continue;
        };
        let mut latest = latest;
        while let Ok(res) = rx.recv_timeout(debounce) {
            if let Some(next) = res.ok().as_ref().and_then(classify) {
                latest = next;
            }
        }
        if let Err(err) = hook.observe(&latest.0, latest.1) {
            tracing::warn!(
                workflow_id = %hook.workflow_id,
                trigger_id = %hook.trigger_id,
                error = %err,
                "file trigger failed"
            );
        }
    }
}

fn classify(event: &notify::Event) -> Option<(PathBuf, FileEvent)> {
    match event.kind {
        EventKind::Create(_) => event
            .paths
            .first()
            .map(|path| (path.clone(), FileEvent::Create)),
        EventKind::Remove(_) => event
            .paths
            .first()
            .map(|path| (path.clone(), FileEvent::Delete)),
        EventKind::Modify(notify::event::ModifyKind::Name(_)) => event
            .paths
            .last()
            .map(|path| (path.clone(), FileEvent::Rename)),
        EventKind::Modify(_) => event
            .paths
            .first()
            .map(|path| (path.clone(), FileEvent::Modify)),
        _ => None,
    }
}

/// The wire name recorded in `trigger.event`.
fn event_name(kind: FileEvent) -> &'static str {
    match kind {
        FileEvent::Create => "create",
        FileEvent::Modify => "modify",
        FileEvent::Delete => "delete",
        FileEvent::Rename => "rename",
    }
}

fn watcher_error(err: notify::Error) -> WorkflowError {
    WorkflowError::Invalid(format!("file watcher: {err}"))
}
