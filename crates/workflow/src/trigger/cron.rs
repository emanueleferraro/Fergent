//! Cron triggers (spec §2.2).
//!
//! A schedule is a strict five-field expression (`minute hour day month
//! weekday`). [`CronScheduler`] is the single task that arms and fires every
//! cron trigger; the next fire is persisted in `workflow_triggers.next_fire_at`
//! so a restart neither drifts nor double-fires.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    Datelike, Duration as ChronoDuration, LocalResult, NaiveDateTime, TimeZone, Timelike,
};
use chrono_tz::Tz;
use serde_json::json;

use crate::document::CatchUp;
use crate::error::WorkflowError;
use crate::executor::Cancellation;
use crate::trigger::registry::TriggerRegistry;
use crate::trigger::{Clock, FireFn, TriggerFire};

/// The look-back bound for catch-up fires (spec §2.2: 24 h).
pub const MAX_CATCH_UP_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// The most catch-up fires one tick may start (matches the queue cap, §2.7).
pub const MAX_CATCH_UP_FIRES: usize = 32;

/// How far ahead [`CronSchedule::next_fire_after`] searches before giving up.
const MAX_SEARCH_DAYS: i64 = 366 * 5;

/// A parsed five-field cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    text: String,
    minute: Field,
    hour: Field,
    day_of_month: Field,
    month: Field,
    day_of_week: Field,
}

impl CronSchedule {
    /// Parse a strict five-field expression.
    pub fn parse(source: &str) -> Result<Self, WorkflowError> {
        let fields: Vec<&str> = source.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(WorkflowError::Invalid(format!(
                "`{source}` is not a five-field cron expression"
            )));
        }
        Ok(Self {
            text: fields.join(" "),
            minute: Field::parse(fields[0], 0, 59)?,
            hour: Field::parse(fields[1], 0, 23)?,
            day_of_month: Field::parse(fields[2], 1, 31)?,
            month: Field::parse(fields[3], 1, 12)?,
            day_of_week: Field::parse_weekday(fields[4])?,
        })
    }

    /// The canonical expression text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The first fire strictly after `now_ms`, in UTC milliseconds.
    ///
    /// Wall-clock times that fall in a DST gap are skipped (the next valid
    /// instant fires); ambiguous times resolve to the earlier instant, so an
    /// overlap fires once (spec §2.2).
    pub fn next_fire_after(&self, now_ms: i64, tz: Tz) -> Result<Option<i64>, WorkflowError> {
        let local_now = tz
            .timestamp_millis_opt(now_ms)
            .single()
            .ok_or_else(|| WorkflowError::Invalid("timestamp is out of range".to_string()))?;
        let mut candidate = local_now
            .naive_local()
            .with_second(0)
            .and_then(|dt| dt.with_nanosecond(0))
            .ok_or_else(|| WorkflowError::Invalid("timestamp is out of range".to_string()))?
            + ChronoDuration::minutes(1);

        for _ in 0..(MAX_SEARCH_DAYS * 24 * 60) {
            if !self.month.matches(candidate.month()) || !self.day_matches(&candidate) {
                candidate = next_midnight(candidate)?;
                continue;
            }
            if !self.hour.matches(candidate.hour()) {
                candidate = next_hour(candidate)?;
                continue;
            }
            if !self.minute.matches(candidate.minute()) {
                candidate += ChronoDuration::minutes(1);
                continue;
            }
            if let Some(instant) = resolve(tz, &candidate) {
                if instant > now_ms {
                    return Ok(Some(instant));
                }
            }
            candidate += ChronoDuration::minutes(1);
        }
        Ok(None)
    }

    fn day_matches(&self, dt: &NaiveDateTime) -> bool {
        let dom = self.day_of_month.matches(dt.day());
        let dow = self
            .day_of_week
            .matches(dt.weekday().num_days_from_sunday());
        match (self.day_of_month.any, self.day_of_week.any) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        }
    }
}

impl FromStr for CronSchedule {
    type Err = WorkflowError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::parse(source)
    }
}

/// Resolve a naive wall-clock time to a UTC instant.
fn resolve(tz: Tz, naive: &NaiveDateTime) -> Option<i64> {
    match tz.from_local_datetime(naive) {
        LocalResult::Single(instant) => Some(instant.timestamp_millis()),
        LocalResult::Ambiguous(first, second) => {
            Some(first.timestamp_millis().min(second.timestamp_millis()))
        }
        LocalResult::None => None,
    }
}

fn next_midnight(dt: NaiveDateTime) -> Result<NaiveDateTime, WorkflowError> {
    (dt + ChronoDuration::days(1))
        .date()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| WorkflowError::Invalid("date out of range".to_string()))
}

fn next_hour(dt: NaiveDateTime) -> Result<NaiveDateTime, WorkflowError> {
    (dt + ChronoDuration::hours(1))
        .with_minute(0)
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .ok_or_else(|| WorkflowError::Invalid("date out of range".to_string()))
}

/// One cron field: a 64-slot membership table plus whether it was `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    values: [bool; 64],
    any: bool,
}

impl Field {
    fn parse(source: &str, min: u32, max: u32) -> Result<Self, WorkflowError> {
        let mut values = [false; 64];
        let mut any = false;

        for part in source.split(',') {
            let (range, step) = match part.split_once('/') {
                Some((range, step)) => {
                    let step: u32 = step
                        .parse()
                        .map_err(|_| invalid(source, "a step must be a number"))?;
                    if step == 0 {
                        return Err(invalid(source, "a step must be positive"));
                    }
                    (range, step)
                }
                None => (part, 1),
            };

            let (low, high) = if range == "*" {
                if step == 1 && source == "*" {
                    any = true;
                }
                (min, max)
            } else if let Some((low, high)) = range.split_once('-') {
                (
                    low.parse()
                        .map_err(|_| invalid(source, "a range must be numeric"))?,
                    high.parse()
                        .map_err(|_| invalid(source, "a range must be numeric"))?,
                )
            } else {
                let value: u32 = range
                    .parse()
                    .map_err(|_| invalid(source, "a value must be numeric"))?;
                (value, if step > 1 { max } else { value })
            };

            if low < min || high > max || low > high {
                return Err(invalid(
                    source,
                    &format!("values must be between {min} and {max}"),
                ));
            }

            let mut value = low;
            while value <= high {
                values[value as usize] = true;
                value += step;
            }
        }

        if !values.iter().any(|set| *set) {
            return Err(invalid(source, "the field is empty"));
        }
        Ok(Self { values, any })
    }

    /// Parse a day-of-week field, accepting both `0` and `7` for Sunday.
    fn parse_weekday(source: &str) -> Result<Self, WorkflowError> {
        let mut field = Self::parse(source, 0, 7)?;
        field.values[0] |= field.values[7];
        field.values[7] = false;
        Ok(field)
    }

    fn matches(&self, value: u32) -> bool {
        self.values.get(value as usize).copied().unwrap_or(false)
    }
}

fn invalid(source: &str, reason: &str) -> WorkflowError {
    WorkflowError::Invalid(format!("`{source}` is not a valid cron field: {reason}"))
}

/// A cron trigger, bound to its registry row, clock, and fire callback.
#[derive(Debug, Clone)]
pub struct CronTriggerConfig {
    /// The owning workflow.
    pub workflow_id: String,
    /// The trigger id within the document.
    pub trigger_id: String,
    /// The five-field expression.
    pub schedule: String,
    /// An IANA timezone; falls back to the runtime default.
    pub timezone: Option<String>,
    /// What to do with windows missed while the kernel was down.
    pub catch_up: CatchUp,
    /// The declared concurrency (recorded; enforced by the runner, §2.7).
    pub concurrency: Option<u32>,
}

/// Drives one cron trigger.
pub struct CronTrigger {
    config: CronTriggerConfig,
    schedule: CronSchedule,
    tz: Tz,
    registry: Arc<TriggerRegistry>,
    clock: Arc<dyn Clock>,
    fire: FireFn,
}

impl CronTrigger {
    /// Build a trigger, resolving the timezone against `default_timezone`.
    pub fn new(
        config: CronTriggerConfig,
        default_timezone: &str,
        registry: Arc<TriggerRegistry>,
        clock: Arc<dyn Clock>,
        fire: FireFn,
    ) -> Result<Self, WorkflowError> {
        let schedule = CronSchedule::parse(&config.schedule)?;
        let zone = config.timezone.as_deref().unwrap_or(default_timezone);
        let tz = Tz::from_str(zone)
            .map_err(|_| WorkflowError::Invalid(format!("`{zone}` is not an IANA timezone")))?;
        Ok(Self {
            config,
            schedule,
            tz,
            registry,
            clock,
            fire,
        })
    }

    /// The trigger configuration.
    pub fn config(&self) -> &CronTriggerConfig {
        &self.config
    }

    /// The parsed schedule.
    pub fn schedule(&self) -> &CronSchedule {
        &self.schedule
    }

    /// The resolved timezone.
    pub fn timezone(&self) -> Tz {
        self.tz
    }

    /// Ensure the row is armed. Returns the persisted next fire, if any.
    pub fn arm(&self) -> Result<Option<i64>, WorkflowError> {
        let row =
            self.registry
                .ensure(&self.config.workflow_id, &self.config.trigger_id, "cron")?;
        if let Some(next) = row.next_fire_at {
            return Ok(Some(next));
        }
        let next = self
            .schedule
            .next_fire_after(self.clock.now_ms(), self.tz)?;
        self.registry
            .set_next_fire(&self.config.workflow_id, &self.config.trigger_id, next)?;
        Ok(next)
    }

    /// Fire every window that is due right now. Returns the runs started.
    pub fn tick(&self) -> Result<usize, WorkflowError> {
        let now = self.clock.now_ms();
        let row =
            self.registry
                .ensure(&self.config.workflow_id, &self.config.trigger_id, "cron")?;
        if !row.enabled {
            return Ok(0);
        }

        let Some(first) = row.next_fire_at else {
            let next = self.schedule.next_fire_after(now, self.tz)?;
            self.registry
                .set_next_fire(&self.config.workflow_id, &self.config.trigger_id, next)?;
            return Ok(0);
        };
        if first > now {
            return Ok(0);
        }

        let mut instants = vec![first];
        let mut cursor = first;
        while instants.len() <= MAX_CATCH_UP_FIRES {
            match self.schedule.next_fire_after(cursor, self.tz)? {
                Some(next) if next <= now => {
                    instants.push(next);
                    cursor = next;
                }
                _ => break,
            }
        }

        let window_start = now - MAX_CATCH_UP_WINDOW_MS;
        let in_window: Vec<i64> = instants
            .iter()
            .copied()
            .filter(|instant| *instant >= window_start)
            .collect();
        let due: Vec<i64> = match self.config.catch_up {
            // A single due window is on time; more than one means downtime.
            CatchUp::Skip if instants.len() == 1 => in_window,
            CatchUp::Skip => Vec::new(),
            CatchUp::RunOnce => in_window.last().copied().into_iter().collect(),
            CatchUp::RunAll => in_window,
        };

        // Persist the next fire before starting runs: a crash after this
        // point cannot replay the window (task 5.3 acceptance).
        let next = self.schedule.next_fire_after(now, self.tz)?;
        self.registry
            .set_next_fire(&self.config.workflow_id, &self.config.trigger_id, next)?;

        let mut started = 0;
        let mut failure: Option<String> = None;
        for instant in due {
            let fired = (self.fire)(TriggerFire {
                workflow_id: self.config.workflow_id.clone(),
                trigger_id: self.config.trigger_id.clone(),
                kind: "cron",
                actor: format!("trigger:{}", self.config.trigger_id),
                inputs: json!({ "scheduled_at": instant, "fired_at": now }),
            });
            match fired {
                Ok(_) => {
                    started += 1;
                    self.registry.mark_fired(
                        &self.config.workflow_id,
                        &self.config.trigger_id,
                        now,
                    )?;
                }
                Err(err) => failure = Some(format!("{}: {err}", err.code())),
            }
        }
        if let Some(error) = failure {
            self.registry
                .mark_error(&self.config.workflow_id, &self.config.trigger_id, &error)?;
        }
        Ok(started)
    }
}

/// The single task that drives every cron trigger (spec §2.2).
pub struct CronScheduler {
    triggers: Vec<CronTrigger>,
    poll: Duration,
}

impl CronScheduler {
    /// A scheduler polling every `poll`.
    pub fn new(poll: Duration) -> Self {
        Self {
            triggers: Vec::new(),
            poll,
        }
    }

    /// Add a trigger.
    pub fn push(&mut self, trigger: CronTrigger) {
        self.triggers.push(trigger);
    }

    /// The registered triggers.
    pub fn triggers(&self) -> &[CronTrigger] {
        &self.triggers
    }

    /// Arm every trigger, then fire whatever is due. Returns the runs started.
    pub fn tick(&self) -> usize {
        let mut started = 0;
        for trigger in &self.triggers {
            if let Err(err) = trigger.arm() {
                tracing::warn!(
                    workflow_id = %trigger.config.workflow_id,
                    trigger_id = %trigger.config.trigger_id,
                    error = %err,
                    "failed to arm a cron trigger"
                );
                continue;
            }
            match trigger.tick() {
                Ok(count) => started += count,
                Err(err) => tracing::warn!(
                    workflow_id = %trigger.config.workflow_id,
                    trigger_id = %trigger.config.trigger_id,
                    error = %err,
                    "cron trigger failed"
                ),
            }
        }
        started
    }

    /// Run until `cancel` is requested.
    pub async fn run(self, cancel: Cancellation) {
        let mut ticker = tokio::time::interval(self.poll);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    self.tick();
                }
            }
        }
    }
}
