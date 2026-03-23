//! KV-backed priority work queue for Cloudflare Durable Objects.
//!
//! Replaces the fragile single-key alarm dispatch pattern with a proper queue.
//! Each DO defines a task enum implementing [`WorkTask`], then uses
//! [`WorkQueue<T>`] to enqueue, dequeue, and manage work items.
//!
//! The queue is stored as a `Vec<WorkItem<T>>` under a single KV key,
//! so it works on all DOs regardless of SQLite availability.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;
use worker_stack::worker::{Result, Storage};

const QUEUE_KEY: &str = "_work_queue";
const NEXT_ID_KEY: &str = "_wq_next_id";

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Trait for DO task enums. Declares priority and retry behavior per variant.
pub trait WorkTask: Serialize + DeserializeOwned + PartialEq + Clone {
    /// Higher = more urgent. Used to sort ready items in [`WorkQueue::peek_next`].
    fn priority(&self) -> u32;
    /// Maximum number of attempts before the item is considered dead.
    fn max_attempts(&self) -> u32;
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// A single work item in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem<T> {
    pub id: u64,
    pub task: T,
    pub run_at: u64,
    pub attempts: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
    pub created_at: u64,
    pub context: Option<String>,
}

/// Result of executing a work item, returned by the DO's task handler.
pub enum TaskResult {
    /// Task completed successfully. Will be removed from the queue.
    Done,
    /// Task failed transiently. Will be retried after `delay_ms`.
    Retry { delay_ms: u64, error: String },
    /// Task failed permanently. Will be marked as exhausted.
    Failed { error: String },
}

// ─── WorkQueue ───────────────────────────────────────────────────────────────

/// KV-backed priority work queue, generic over the DO's task type.
pub struct WorkQueue<T: WorkTask>(PhantomData<T>);

impl<T: WorkTask> WorkQueue<T> {
    /// Add a work item to the queue and sync the alarm.
    ///
    /// - `delay_ms`: minimum delay before the item is eligible to run.
    /// - `context`: optional JSON string with task-specific data.
    /// - `dedupe`: if true, skip insert when an active item with the same
    ///   task variant and context already exists.
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
            let ctx = context.map(|s| s.to_string());
            let already_exists = items.iter().any(|item| {
                item.task == task && item.context == ctx && item.attempts < item.max_attempts
            });
            if already_exists {
                return Ok(false);
            }
        }

        let id = next_id(storage).await;
        let now = now_ms();

        items.push(WorkItem {
            id,
            max_attempts: task.max_attempts(),
            task,
            run_at: now + delay_ms,
            attempts: 0,
            last_error: None,
            created_at: now,
            context: context.map(|s| s.to_string()),
        });

        save_queue(storage, &items).await?;
        Self::sync_alarm(storage).await?;
        Ok(true)
    }

    /// Get the highest-priority ready item without removing it.
    ///
    /// Ready means `run_at <= now` and `attempts < max_attempts`.
    /// Sorted by priority (descending), then `run_at` (ascending / FIFO).
    pub async fn peek_next(storage: &Storage) -> Result<Option<WorkItem<T>>> {
        let items = load_queue::<T>(storage).await;
        let now = now_ms();

        let mut ready: Vec<&WorkItem<T>> = items
            .iter()
            .filter(|item| item.run_at <= now && item.attempts < item.max_attempts)
            .collect();

        // Sort: highest priority first, then earliest run_at (FIFO within band)
        ready.sort_by(|a, b| {
            b.task
                .priority()
                .cmp(&a.task.priority())
                .then(a.run_at.cmp(&b.run_at))
        });

        Ok(ready.first().map(|item| (*item).clone()))
    }

    /// Remove a completed item from the queue.
    pub async fn complete(storage: &Storage, id: u64) -> Result<()> {
        let mut items = load_queue::<T>(storage).await;
        items.retain(|item| item.id != id);
        save_queue(storage, &items).await
    }

    /// Record a failure: increment attempts, set backoff, store error.
    ///
    /// Returns `true` if the item is still retryable, `false` if exhausted.
    pub async fn fail(
        storage: &Storage,
        id: u64,
        error: &str,
        retry_delay_ms: u64,
    ) -> Result<bool> {
        let mut items = load_queue::<T>(storage).await;
        let now = now_ms();

        let mut retryable = false;
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            item.attempts += 1;
            item.last_error = Some(error.to_string());
            item.run_at = now + retry_delay_ms;
            retryable = item.attempts < item.max_attempts;
        }

        save_queue(storage, &items).await?;
        Ok(retryable)
    }

    /// Set the DO alarm to the earliest `run_at` in the queue.
    ///
    /// If the queue is empty or all items are exhausted, no alarm is set.
    /// If the earliest item is already ready (`run_at <= now`), sets alarm
    /// to fire immediately (1ms delay).
    pub async fn sync_alarm(storage: &Storage) -> Result<()> {
        let items = load_queue::<T>(storage).await;
        let now = now_ms();

        let earliest = items
            .iter()
            .filter(|item| item.attempts < item.max_attempts)
            .map(|item| item.run_at)
            .min();

        if let Some(run_at) = earliest {
            let delay = if run_at <= now {
                1 // fire immediately
            } else {
                run_at - now
            };
            storage
                .set_alarm(std::time::Duration::from_millis(delay))
                .await?;
        }

        Ok(())
    }

    /// Remove all active items matching a task variant.
    pub async fn cancel(storage: &Storage, task: &T) -> Result<u32> {
        let mut items = load_queue::<T>(storage).await;
        let before = items.len();
        items.retain(|item| &item.task != task);
        let removed = (before - items.len()) as u32;
        if removed > 0 {
            save_queue(storage, &items).await?;
        }
        Ok(removed)
    }

    /// Check whether any active (non-exhausted) item matching `task` exists.
    pub async fn has_active(storage: &Storage, task: &T) -> bool {
        let items = load_queue::<T>(storage).await;
        items
            .iter()
            .any(|item| &item.task == task && item.attempts < item.max_attempts)
    }

    /// Remove exhausted items (attempts >= max_attempts).
    pub async fn prune_dead(storage: &Storage) -> Result<u32> {
        let mut items = load_queue::<T>(storage).await;
        let before = items.len();
        items.retain(|item| item.attempts < item.max_attempts);
        let removed = (before - items.len()) as u32;
        if removed > 0 {
            save_queue(storage, &items).await?;
        }
        Ok(removed)
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

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
