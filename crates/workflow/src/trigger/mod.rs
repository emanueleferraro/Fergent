//! The trigger runtime (spec §2).
//!
//! Every run starts from exactly one trigger. This module turns the six
//! declared trigger kinds ([`crate::document::TriggerKind`]) into runs:
//!
//! - [`registry`] — the persisted `workflow_triggers` rows.
//! - [`cron`] — a single scheduler task driving every cron trigger.
//! - [`webhook`] — the loopback HTTP listener (§2.3).
//! - [`file`] — the `notify` watcher over a project glob (§2.4).
//! - [`event`] — the kernel event-bus subscription (§2.5).
//! - [`manual`], [`ai`] — the two direct entry points (§2.6).
//!
//! The pieces are composed by the kernel service: [`registry`] is pure
//! storage, [`cron`] owns the clock, and [`webhook`]/[`file`]/[`event`] own
//! their own resources. Each reports a fire through a [`FireFn`], so this
//! module never depends on the engine and a fire is easy to observe in tests.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::WorkflowError;

pub mod ai;
pub mod cron;
pub mod event;
pub mod file;
pub mod manual;
pub mod registry;
pub mod webhook;

pub use ai::fire_ai;
pub use cron::{
    CronSchedule, CronScheduler, CronTrigger, CronTriggerConfig, MAX_CATCH_UP_WINDOW_MS,
};
pub use event::{EventTrigger, EventTriggerConfig};
pub use file::{FileTrigger, FileTriggerConfig};
pub use manual::{fire, fire_manual, test_trigger, MANUAL_TRIGGER_ID};
pub use registry::{TriggerRecord, TriggerRegistry};
pub use webhook::{
    FireOutcome, MemoryTokenStore, TokenStore, WebhookListener, WebhookRequest, WebhookResponse,
    WebhookRouter, MAX_BODY_BYTES,
};

/// A trigger firing, before it becomes a run.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerFire {
    /// The workflow to run.
    pub workflow_id: String,
    /// The trigger that fired.
    pub trigger_id: String,
    /// `manual`, `ai`, `cron`, `webhook`, `file`, or `event`.
    pub kind: &'static str,
    /// Who asked for the run: `user`, `agent:<id>`, or `trigger:<id>`
    /// (spec §3.2).
    pub actor: String,
    /// The payload exposed to the graph as `ctx.trigger`.
    pub inputs: serde_json::Value,
}

/// Starts a run for a fired trigger. Returns the new run id.
///
/// The kernel installs a closure that looks the document up and calls
/// [`crate::Engine::start`]; tests install one that records the fire.
pub type FireFn = Arc<dyn Fn(TriggerFire) -> Result<String, WorkflowError> + Send + Sync>;

/// A monotonic-enough wall clock, injectable so tests control time.
pub trait Clock: Send + Sync {
    /// Current time in milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as i64)
    }
}

/// A clock tests move by hand.
#[derive(Debug, Clone)]
pub struct ManualClock {
    now: Arc<std::sync::atomic::AtomicI64>,
}

impl ManualClock {
    /// A clock frozen at `now_ms`.
    pub fn at(now_ms: i64) -> Self {
        Self {
            now: Arc::new(std::sync::atomic::AtomicI64::new(now_ms)),
        }
    }

    /// Move the clock to an absolute instant.
    pub fn set(&self, now_ms: i64) {
        self.now.store(now_ms, std::sync::atomic::Ordering::SeqCst);
    }

    /// Advance the clock.
    pub fn advance(&self, delta_ms: i64) {
        self.now
            .fetch_add(delta_ms, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now.load(std::sync::atomic::Ordering::SeqCst)
    }
}
