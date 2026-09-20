//! Alarm-driven priority work queue for Cloudflare Durable Objects.
//!
//! A Durable Object has exactly **one** alarm, and every concern that wants to
//! be woken has to share it. Hand-rolling that share — a `match` over a couple
//! of stored phase keys, each arm re-arming the alarm itself — is where DOs go
//! quiet, in three recurring ways:
//!
//! 1. **A lost alarm.** Some path finishes its work and forgets to re-arm. The
//!    DO has no timer left and nothing will ever wake it again. There is no
//!    error and no log line; it simply stops.
//! 2. **Starvation.** The dispatch `match` is written in an order that reads
//!    sensibly, and a low-priority job that can never succeed sits at the front
//!    of it, consuming every firing while something urgent behind it never
//!    runs. (This is real: a stuck CID backfill in `collection-ownership`
//!    permanently starved the import watchdog that would have rescued it.)
//! 3. **Silent death.** Work exhausts its retries and is skipped from then on.
//!    A DO whose work has quietly died looks exactly like a DO with no work.
//!
//! This queue exists so none of those are possible to write. Priority is
//! declared per task variant rather than encoded in a `match` arm's position;
//! **every** mutating operation re-arms the alarm as part of itself; and dead
//! items stay visible through [`WorkQueue::dead`] and [`WorkQueue::status`]
//! instead of vanishing.
//!
//! # Using it
//!
//! Define a task enum, then let [`WorkQueue::run_next`] drive the DO's alarm:
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, PartialEq, Clone)]
//! #[serde(rename_all = "snake_case")]
//! enum Job { ImportWatchdog, SyncPage, CidBackfill }
//!
//! impl WorkTask for Job {
//!     fn priority(&self) -> u32 {
//!         match self {
//!             // The rescue path outranks the work it rescues.
//!             Job::ImportWatchdog => 100,
//!             Job::SyncPage => 50,
//!             Job::CidBackfill => 10,
//!         }
//!     }
//!     fn max_attempts(&self) -> u32 { 6 }
//! }
//!
//! async fn alarm(&self) -> Result<Response> {
//!     WorkQueue::<Job>::run_next(&self.state.storage(), |item| async move {
//!         match item.task {
//!             Job::ImportWatchdog => self.retry_import().await,
//!             /* … */
//!         }
//!     })
//!     .await?;
//!     Response::ok("ok")
//! }
//! ```
//!
//! # Delivery guarantees
//!
//! **At-least-once.** An item is not removed until its handler returns
//! [`TaskResult::Done`], so a DO evicted or crashing mid-task runs that task
//! again from the start. **Handlers must be idempotent.** There is deliberately
//! no in-flight lease: a lease that outlives a crash is indistinguishable from
//! a task still running, and recovering from that needs a timeout — which is
//! another alarm, which is the thing this module exists to make reliable.
//!
//! # Priority
//!
//! Strict priority, then FIFO within a band. There is no aging: a high-priority
//! task that is always ready will starve lower bands for ever. That is the
//! intent — priority is for "this must run before that", e.g. a watchdog ahead
//! of the work it watches — but it does mean priority bands want to be few and
//! meaningfully ordered, not a per-task opinion.
//!
//! # Size
//!
//! The whole queue is one value under one key, read and rewritten on every
//! operation. That keeps it working on DOs without SQLite, at the cost of being
//! O(n) per op. It is sized for tens of items — a DO's own concerns — not for
//! a backlog of work. Fan work out to a Cloudflare Queue if you need thousands.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::future::Future;
use std::marker::PhantomData;
use worker_stack::worker::{Result, Storage};

const QUEUE_KEY: &str = "_work_queue";
const NEXT_ID_KEY: &str = "_wq_next_id";

/// Backoff ceiling for the default [`WorkTask::retry_delay_ms`].
const DEFAULT_MAX_BACKOFF_MS: u64 = 15 * 60 * 1000;
/// First retry delay for the default [`WorkTask::retry_delay_ms`].
const DEFAULT_BASE_BACKOFF_MS: u64 = 1000;

// ─── Trait ───────────────────────────────────────────────────────────────────

/// A DO's task enum: what kinds of work it can be woken to do.
pub trait WorkTask: Serialize + DeserializeOwned + PartialEq + Clone {
    /// Higher runs first. See the module note on starvation — bands should be
    /// few and ordered by "must precede", not by how important each task feels.
    fn priority(&self) -> u32;

    /// Attempts before the item is considered dead and stops being scheduled.
    fn max_attempts(&self) -> u32;

    /// How long to wait before the retry after `attempts` failures.
    ///
    /// Defaults to exponential backoff from 1s, capped at 15 minutes. Override
    /// for work with its own cadence — a rate-limited upstream that tells you
    /// when to come back, say.
    fn retry_delay_ms(&self, attempts: u32) -> u64 {
        DEFAULT_BASE_BACKOFF_MS
            .saturating_mul(1u64 << attempts.min(20))
            .min(DEFAULT_MAX_BACKOFF_MS)
    }
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// A single work item in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem<T> {
    pub id: u64,
    pub task: T,
    /// Unix ms before which this item is not eligible to run.
    pub run_at: u64,
    pub attempts: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
    pub created_at: u64,
    /// Task-specific payload, opaque to the queue. Part of the dedupe key.
    pub context: Option<String>,
}

impl<T> WorkItem<T> {
    /// Whether this item still has attempts left, and so is still scheduled.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.attempts < self.max_attempts
    }

    /// Whether this item has exhausted its attempts. Dead items are never run
    /// again, and never re-arm the alarm, but are kept so
    /// [`WorkQueue::dead`] can report them rather than them just vanishing.
    #[must_use]
    pub fn is_dead(&self) -> bool {
        !self.is_live()
    }
}

/// What a handler decided about the item it was given.
pub enum TaskResult {
    /// Completed. The item is removed.
    Done,
    /// Failed transiently. Retried after `delay_ms`, up to `max_attempts`.
    Retry { delay_ms: u64, error: String },
    /// Failed in a way retrying cannot fix. The item goes straight to dead.
    Failed { error: String },
}

/// What [`WorkQueue::run_next`] did, for the caller to log.
#[derive(Debug, PartialEq, Eq)]
pub enum WorkOutcome {
    /// Nothing was ready. The alarm is armed for the next scheduled item, or
    /// cleared if there is none.
    Idle,
    /// The item completed and was removed.
    Completed { id: u64 },
    /// The item failed and will be retried.
    Retrying { id: u64, attempts: u32 },
    /// The item exhausted its attempts, or failed permanently. It is now dead
    /// and will not run again — see [`WorkQueue::dead`].
    Dead { id: u64 },
}

/// A snapshot of the queue, for status endpoints and operator logs.
///
/// `dead > 0` is the signal that work has stopped without anyone being told.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueStatus {
    /// Live items eligible to run now.
    pub ready: usize,
    /// Live items waiting for their `run_at`.
    pub scheduled: usize,
    /// Items that exhausted their attempts.
    pub dead: usize,
    /// Unix ms the alarm is (or should be) set for.
    pub next_run_at: Option<u64>,
}

// ─── WorkQueue ───────────────────────────────────────────────────────────────

/// Priority work queue over a DO's storage, generic in the DO's task type.
pub struct WorkQueue<T: WorkTask>(PhantomData<T>);

impl<T: WorkTask> WorkQueue<T> {
    /// Add a work item and arm the alarm.
    ///
    /// - `delay_ms`: minimum wait before the item may run.
    /// - `context`: optional task-specific payload; part of the dedupe key.
    /// - `dedupe`: skip the insert when a live item with the same task variant
    ///   and context already exists.
    ///
    /// Returns `true` if an item was inserted, `false` if deduped.
    pub async fn enqueue(
        storage: &Storage,
        task: T,
        delay_ms: u64,
        context: Option<&str>,
        dedupe: bool,
    ) -> Result<bool> {
        let mut items = load_queue::<T>(storage).await;

        if dedupe {
            let ctx = context.map(ToString::to_string);
            if items
                .iter()
                .any(|item| item.task == task && item.context == ctx && item.is_live())
            {
                return Ok(false);
            }
        }

        let id = next_id(storage).await;
        let now = now_ms();

        items.push(WorkItem {
            id,
            max_attempts: task.max_attempts(),
            task,
            run_at: now.saturating_add(delay_ms),
            attempts: 0,
            last_error: None,
            created_at: now,
            context: context.map(ToString::to_string),
        });

        Self::save_and_rearm(storage, items).await?;
        Ok(true)
    }

    /// Run the highest-priority ready item, then re-arm the alarm.
    ///
    /// This is the whole of a DO's `alarm()`. One item per firing: the alarm is
    /// immediately re-armed if more work is ready, which keeps each invocation
    /// inside the DO's wall clock and lets a newly-enqueued higher-priority
    /// item overtake at the next firing rather than waiting out a batch.
    ///
    /// The alarm is re-armed whatever the handler does — including when it
    /// panics into an `Err`, and including when nothing was ready. A DO driven
    /// by this cannot lose its timer.
    pub async fn run_next<F, Fut>(storage: &Storage, run: F) -> Result<WorkOutcome>
    where
        F: FnOnce(WorkItem<T>) -> Fut,
        Fut: Future<Output = TaskResult>,
    {
        let Some(item) = Self::peek_next(storage).await? else {
            Self::sync_alarm(storage).await?;
            return Ok(WorkOutcome::Idle);
        };

        let id = item.id;
        let retry_delay = item.task.retry_delay_ms(item.attempts);

        match run(item).await {
            TaskResult::Done => {
                Self::complete(storage, id).await?;
                Ok(WorkOutcome::Completed { id })
            }
            TaskResult::Retry { delay_ms, error } => {
                // `delay_ms == 0` means "the handler has no opinion" — use the
                // task's own backoff rather than hot-looping the alarm.
                let delay = if delay_ms == 0 { retry_delay } else { delay_ms };
                if Self::fail(storage, id, &error, delay).await? {
                    Ok(WorkOutcome::Retrying {
                        id,
                        attempts: Self::attempts_of(storage, id).await,
                    })
                } else {
                    Ok(WorkOutcome::Dead { id })
                }
            }
            TaskResult::Failed { error } => {
                Self::kill(storage, id, &error).await?;
                Ok(WorkOutcome::Dead { id })
            }
        }
    }

    /// The highest-priority ready item, without removing it.
    ///
    /// Ready means `run_at <= now` and the item is live. Ordered by priority
    /// descending, then `run_at` ascending (FIFO within a band).
    pub async fn peek_next(storage: &Storage) -> Result<Option<WorkItem<T>>> {
        let items = load_queue::<T>(storage).await;
        Ok(pick_next(&items, now_ms()).cloned())
    }

    /// Remove a completed item and re-arm.
    pub async fn complete(storage: &Storage, id: u64) -> Result<()> {
        let mut items = load_queue::<T>(storage).await;
        items.retain(|item| item.id != id);
        Self::save_and_rearm(storage, items).await
    }

    /// Record a failure — increment attempts, set the backoff, store the error
    /// — and re-arm.
    ///
    /// Returns `true` if the item is still live, `false` if it is now dead.
    pub async fn fail(
        storage: &Storage,
        id: u64,
        error: &str,
        retry_delay_ms: u64,
    ) -> Result<bool> {
        let mut items = load_queue::<T>(storage).await;
        let now = now_ms();

        let mut live = false;
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            item.attempts += 1;
            item.last_error = Some(error.to_string());
            item.run_at = now.saturating_add(retry_delay_ms);
            live = item.is_live();
        }

        Self::save_and_rearm(storage, items).await?;
        Ok(live)
    }

    /// Kill an item outright, without spending its remaining attempts.
    ///
    /// For failures retrying cannot fix. The item is kept, dead, so it shows up
    /// in [`Self::dead`] rather than disappearing.
    pub async fn kill(storage: &Storage, id: u64, error: &str) -> Result<()> {
        let mut items = load_queue::<T>(storage).await;
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            item.attempts = item.max_attempts;
            item.last_error = Some(error.to_string());
        }
        Self::save_and_rearm(storage, items).await
    }

    /// Set the alarm to the earliest live `run_at`, or clear it when there is
    /// no live work.
    ///
    /// Called by every mutating operation on this type; public because a DO
    /// that re-hydrates after eviction may want to assert its alarm.
    pub async fn sync_alarm(storage: &Storage) -> Result<()> {
        let items = load_queue::<T>(storage).await;
        Self::arm(storage, &items).await
    }

    /// Remove every live item matching a task variant, and re-arm.
    pub async fn cancel(storage: &Storage, task: &T) -> Result<u32> {
        let mut items = load_queue::<T>(storage).await;
        let before = items.len();
        items.retain(|item| &item.task != task);
        let removed = (before - items.len()) as u32;
        if removed > 0 {
            Self::save_and_rearm(storage, items).await?;
        }
        Ok(removed)
    }

    /// Whether any live item matching `task` exists.
    pub async fn has_active(storage: &Storage, task: &T) -> bool {
        load_queue::<T>(storage)
            .await
            .iter()
            .any(|item| &item.task == task && item.is_live())
    }

    /// Every item that exhausted its attempts, newest failure last.
    ///
    /// This is the one that matters operationally: a DO that has gone quiet is
    /// either idle or has dead work, and only this call tells them apart.
    pub async fn dead(storage: &Storage) -> Vec<WorkItem<T>> {
        load_queue::<T>(storage)
            .await
            .into_iter()
            .filter(WorkItem::is_dead)
            .collect()
    }

    /// A snapshot of the queue for status endpoints and operator logs.
    pub async fn status(storage: &Storage) -> QueueStatus {
        summarise(&load_queue::<T>(storage).await, now_ms())
    }

    /// Drop dead items, and re-arm.
    pub async fn prune_dead(storage: &Storage) -> Result<u32> {
        let mut items = load_queue::<T>(storage).await;
        let before = items.len();
        items.retain(WorkItem::is_live);
        let removed = (before - items.len()) as u32;
        if removed > 0 {
            Self::save_and_rearm(storage, items).await?;
        }
        Ok(removed)
    }

    /// Persist the queue and arm the alarm from it — the single path every
    /// mutation goes through, so re-arming cannot be forgotten at a call site.
    async fn save_and_rearm(storage: &Storage, items: Vec<WorkItem<T>>) -> Result<()> {
        save_queue(storage, &items).await?;
        Self::arm(storage, &items).await
    }

    async fn arm(storage: &Storage, items: &[WorkItem<T>]) -> Result<()> {
        match next_wake_delay_ms(items, now_ms()) {
            Some(delay) => {
                storage
                    .set_alarm(std::time::Duration::from_millis(delay))
                    .await
            }
            // Nothing live: clear rather than leave a stale alarm to fire into
            // an empty queue.
            None => match storage.delete_alarm().await {
                Ok(()) => Ok(()),
                // Deleting an alarm that was never set is not an error worth
                // propagating out of a bookkeeping call.
                Err(_) => Ok(()),
            },
        }
    }

    async fn attempts_of(storage: &Storage, id: u64) -> u32 {
        load_queue::<T>(storage)
            .await
            .iter()
            .find(|item| item.id == id)
            .map_or(0, |item| item.attempts)
    }
}

// ─── Pure queue logic ────────────────────────────────────────────────────────
//
// Split out from the storage wrappers so the ordering, readiness, backoff and
// alarm-time rules are testable without a Durable Object. `now` is a parameter
// rather than a clock read for the same reason.

/// The item that should run at `now`: highest priority, then earliest `run_at`.
fn pick_next<T: WorkTask>(items: &[WorkItem<T>], now: u64) -> Option<&WorkItem<T>> {
    items
        .iter()
        .filter(|item| item.is_live() && item.run_at <= now)
        .min_by(|a, b| {
            b.task
                .priority()
                .cmp(&a.task.priority())
                .then(a.run_at.cmp(&b.run_at))
        })
}

/// Milliseconds until the alarm should next fire, or `None` for "no live work,
/// clear the alarm". A ready item yields `1` rather than `0` — a zero-delay
/// alarm is not reliably distinguishable from an unset one.
fn next_wake_delay_ms<T: WorkTask>(items: &[WorkItem<T>], now: u64) -> Option<u64> {
    items
        .iter()
        .filter(|item| item.is_live())
        .map(|item| item.run_at)
        .min()
        .map(|run_at| run_at.saturating_sub(now).max(1))
}

/// Count the queue by state.
fn summarise<T: WorkTask>(items: &[WorkItem<T>], now: u64) -> QueueStatus {
    let mut status = QueueStatus::default();
    for item in items {
        if item.is_dead() {
            status.dead += 1;
        } else if item.run_at <= now {
            status.ready += 1;
        } else {
            status.scheduled += 1;
        }
    }
    status.next_run_at = items
        .iter()
        .filter(|item| item.is_live())
        .map(|item| item.run_at)
        .min();
    status
}

// ─── Storage helpers ─────────────────────────────────────────────────────────

async fn load_queue<T: WorkTask>(storage: &Storage) -> Vec<WorkItem<T>> {
    storage
        .get(QUEUE_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or_default()
}

async fn save_queue<T: WorkTask>(storage: &Storage, items: &[WorkItem<T>]) -> Result<()> {
    if items.is_empty() {
        storage.delete(QUEUE_KEY).await?;
    } else {
        storage.put(QUEUE_KEY, items).await?;
    }
    Ok(())
}

async fn next_id(storage: &Storage) -> u64 {
    let current: u64 = storage.get(NEXT_ID_KEY).await.unwrap_or(None).unwrap_or(0);
    let next = current + 1;
    let _ = storage.put(NEXT_ID_KEY, next).await;
    next
}

fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum Job {
        /// The rescue path — must outrank the work it rescues.
        Watchdog,
        Sync,
        Backfill,
    }

    impl WorkTask for Job {
        fn priority(&self) -> u32 {
            match self {
                Job::Watchdog => 100,
                Job::Sync => 50,
                Job::Backfill => 10,
            }
        }
        fn max_attempts(&self) -> u32 {
            3
        }
    }

    fn item(id: u64, task: Job, run_at: u64, attempts: u32) -> WorkItem<Job> {
        WorkItem {
            id,
            max_attempts: task.max_attempts(),
            task,
            run_at,
            attempts,
            last_error: None,
            created_at: 0,
            context: None,
        }
    }

    /// The bug this module exists to prevent: a low-priority job that can never
    /// succeed must not consume the firings a watchdog needs.
    #[test]
    fn a_failing_low_priority_item_cannot_starve_a_watchdog() {
        let items = vec![
            item(1, Job::Backfill, 0, 2), // retried twice, still live, ready
            item(2, Job::Watchdog, 0, 0),
        ];
        assert_eq!(pick_next(&items, 1_000).map(|i| i.id), Some(2));
    }

    #[test]
    fn within_a_band_the_earliest_runs_first() {
        let items = vec![item(1, Job::Sync, 500, 0), item(2, Job::Sync, 100, 0)];
        assert_eq!(pick_next(&items, 1_000).map(|i| i.id), Some(2));
    }

    #[test]
    fn an_item_is_not_picked_before_its_run_at() {
        let items = vec![item(1, Job::Watchdog, 5_000, 0)];
        assert!(pick_next(&items, 1_000).is_none());
        assert_eq!(pick_next(&items, 5_000).map(|i| i.id), Some(1));
    }

    #[test]
    fn a_dead_item_is_never_picked() {
        // Exhausted at max_attempts, even though it is ready.
        let items = vec![item(1, Job::Watchdog, 0, 3)];
        assert!(pick_next(&items, 1_000).is_none());
    }

    /// An empty or all-dead queue must CLEAR the alarm, not leave a stale one.
    #[test]
    fn no_live_work_means_no_alarm() {
        let empty: Vec<WorkItem<Job>> = vec![];
        assert_eq!(next_wake_delay_ms(&empty, 1_000), None);

        let all_dead = vec![item(1, Job::Sync, 0, 3)];
        assert_eq!(next_wake_delay_ms(&all_dead, 1_000), None);
    }

    #[test]
    fn the_alarm_tracks_the_earliest_live_item() {
        let items = vec![
            item(1, Job::Backfill, 9_000, 0),
            item(2, Job::Watchdog, 3_000, 0),
        ];
        assert_eq!(next_wake_delay_ms(&items, 1_000), Some(2_000));
    }

    /// A ready item still yields a positive delay — a zero-delay alarm is not
    /// reliably distinguishable from no alarm at all.
    #[test]
    fn an_overdue_item_arms_immediately_but_not_at_zero() {
        let items = vec![item(1, Job::Sync, 500, 0)];
        assert_eq!(next_wake_delay_ms(&items, 10_000), Some(1));
    }

    /// The alarm must be driven by live items only — otherwise a dead item's
    /// stale `run_at` keeps waking a DO that will never act on it.
    #[test]
    fn a_dead_item_does_not_hold_the_alarm_open() {
        let items = vec![
            item(1, Job::Sync, 1_000, 3),     // dead, earliest
            item(2, Job::Backfill, 8_000, 0), // live
        ];
        assert_eq!(next_wake_delay_ms(&items, 2_000), Some(6_000));
    }

    #[test]
    fn backoff_grows_and_then_caps() {
        assert_eq!(Job::Sync.retry_delay_ms(0), 1_000);
        assert_eq!(Job::Sync.retry_delay_ms(1), 2_000);
        assert_eq!(Job::Sync.retry_delay_ms(4), 16_000);
        assert_eq!(Job::Sync.retry_delay_ms(60), DEFAULT_MAX_BACKOFF_MS);
    }

    /// Dead work must stay countable — a quiet DO is either idle or broken, and
    /// this is what tells them apart.
    #[test]
    fn status_separates_ready_scheduled_and_dead() {
        let items = vec![
            item(1, Job::Sync, 500, 0),       // ready
            item(2, Job::Backfill, 9_000, 0), // scheduled
            item(3, Job::Watchdog, 0, 3),     // dead
        ];
        let status = summarise(&items, 1_000);
        assert_eq!(status.ready, 1);
        assert_eq!(status.scheduled, 1);
        assert_eq!(status.dead, 1);
        assert_eq!(status.next_run_at, Some(500));
    }

    #[test]
    fn an_idle_queue_reports_no_next_run() {
        let items: Vec<WorkItem<Job>> = vec![item(1, Job::Sync, 0, 3)];
        let status = summarise(&items, 1_000);
        assert_eq!(status.dead, 1);
        assert_eq!(status.next_run_at, None);
    }

    #[test]
    fn is_live_flips_exactly_at_max_attempts() {
        assert!(item(1, Job::Sync, 0, 2).is_live());
        assert!(item(1, Job::Sync, 0, 3).is_dead());
    }
}
