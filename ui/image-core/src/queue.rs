//! Deciding what to fetch next, and whether to bother.
//!
//! The edge crate does the fetching; this decides the order, the parallelism,
//! and what happens when one fails. Keeping that here means the awkward parts
//! — a swipe changing priority mid-flight, the same asset requested from three
//! places, a flaky image retried twice then given up on — are tested without a
//! browser, which is the only way they get tested at all.

use std::collections::HashMap;

/// Where a request has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Queued,
    InFlight,
    Done,
    /// Failed and out of retries. Kept rather than forgotten so the same URL
    /// isn't re-queued on every frame by a caller that keeps asking.
    Failed,
}

/// What the edge reports back for a finished fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Worth another go — a network blip, a 5xx.
    Retryable,
    /// Never going to work: a 404, a decode failure, an unsupported format.
    /// Retrying only burns requests.
    Permanent,
}

#[derive(Debug, Clone)]
struct Entry {
    slot: Slot,
    /// Lower sorts first.
    priority: i32,
    attempts: u32,
}

/// Fetch scheduler for image URLs.
///
/// Deliberately not generic over a key type: every consumer keys by URL, and a
/// type parameter would buy nothing but noise at each call site.
#[derive(Debug)]
pub struct LoadQueue {
    entries: HashMap<String, Entry>,
    /// How many fetches may be outstanding at once.
    ///
    /// Not unlimited: a roster of 80 assets fired at once will stall a mobile
    /// connection and, on a runtime that decodes on the main thread, visibly
    /// hitch. Small is fine — images arriving steadily reads as loading, all
    /// at once after a pause reads as broken.
    concurrency: usize,
    /// Attempts allowed per URL, including the first.
    max_attempts: u32,
}

impl Default for LoadQueue {
    fn default() -> Self {
        Self::new(4, 3)
    }
}

impl LoadQueue {
    pub fn new(concurrency: usize, max_attempts: u32) -> Self {
        Self {
            entries: HashMap::new(),
            concurrency: concurrency.max(1),
            max_attempts: max_attempts.max(1),
        }
    }

    /// Ask for a URL at a priority (lower is sooner).
    ///
    /// Idempotent, and that is the point: three widgets wanting the same
    /// thumbnail must produce one fetch. A repeat request for something
    /// already queued only *raises* its priority — never lowers it, so a
    /// background prefetch cannot demote something the player is looking at.
    pub fn request(&mut self, url: impl Into<String>, priority: i32) {
        let url = url.into();
        match self.entries.get_mut(&url) {
            Some(entry) => {
                if entry.slot == Slot::Queued {
                    entry.priority = entry.priority.min(priority);
                }
            }
            None => {
                self.entries.insert(
                    url,
                    Entry {
                        slot: Slot::Queued,
                        priority,
                        attempts: 0,
                    },
                );
            }
        }
    }

    /// The next URLs to fetch, up to the concurrency limit.
    ///
    /// Marks them in-flight, so calling once per frame yields each URL once.
    /// Ties break on the URL so the order is deterministic — a test that
    /// depends on hash iteration order is a test that fails on Tuesdays.
    pub fn next_batch(&mut self) -> Vec<String> {
        let in_flight = self.count(Slot::InFlight);
        let capacity = self.concurrency.saturating_sub(in_flight);
        if capacity == 0 {
            return Vec::new();
        }

        let mut ready: Vec<(i32, &String)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.slot == Slot::Queued)
            .map(|(url, e)| (e.priority, url))
            .collect();
        ready.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));

        let taken: Vec<String> = ready
            .into_iter()
            .take(capacity)
            .map(|(_, url)| url.clone())
            .collect();

        for url in &taken {
            if let Some(entry) = self.entries.get_mut(url) {
                entry.slot = Slot::InFlight;
                entry.attempts += 1;
            }
        }
        taken
    }

    /// Report a finished fetch. Returns the slot it landed in.
    ///
    /// A retryable failure goes back to `Queued` **at its original priority**
    /// until attempts run out. Unknown URLs are ignored rather than panicking:
    /// a late reply for something [`Self::forget`]ten is normal, not a bug.
    pub fn finish(&mut self, url: &str, outcome: Outcome) -> Option<Slot> {
        let max_attempts = self.max_attempts;
        let entry = self.entries.get_mut(url)?;
        entry.slot = match outcome {
            Outcome::Ok => Slot::Done,
            Outcome::Permanent => Slot::Failed,
            Outcome::Retryable if entry.attempts < max_attempts => Slot::Queued,
            Outcome::Retryable => Slot::Failed,
        };
        Some(entry.slot)
    }

    pub fn slot(&self, url: &str) -> Option<Slot> {
        self.entries.get(url).map(|e| e.slot)
    }

    /// Drop a URL entirely, so a later request starts fresh.
    ///
    /// For a genuine retry-from-scratch — the roster reloaded, the player
    /// reopened the screen — as distinct from the automatic retry above.
    pub fn forget(&mut self, url: &str) {
        self.entries.remove(url);
    }

    pub fn count(&self, slot: Slot) -> usize {
        self.entries.values().filter(|e| e.slot == slot).count()
    }

    pub fn is_idle(&self) -> bool {
        self.count(Slot::Queued) == 0 && self.count(Slot::InFlight) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_url_requested_twice_is_fetched_once() {
        // The de-duplication three of the six surveyed loaders lacked.
        let mut q = LoadQueue::new(4, 3);
        q.request("a", 0);
        q.request("a", 0);
        q.request("a", 0);
        assert_eq!(q.next_batch(), vec!["a".to_string()]);
        assert!(q.next_batch().is_empty(), "already in flight");
    }

    #[test]
    fn priority_orders_the_batch_and_ties_are_deterministic() {
        let mut q = LoadQueue::new(2, 3);
        q.request("later", 10);
        q.request("first", 0);
        q.request("also-first", 0);
        // Both priority-0 entries come out before the priority-10 one, and the
        // tie breaks on the URL rather than on hash order.
        assert_eq!(
            q.next_batch(),
            vec!["also-first".to_string(), "first".to_string()]
        );
    }

    #[test]
    fn re_requesting_can_promote_but_never_demote() {
        // A background prefetch must not push something the player is looking
        // at to the back of the queue.
        let mut q = LoadQueue::new(1, 3);
        q.request("visible", 0);
        q.request("visible", 50); // a prefetch pass, lower priority
        q.request("other", 10);
        assert_eq!(q.next_batch(), vec!["visible".to_string()]);
    }

    #[test]
    fn concurrency_is_respected_and_recovers_as_work_finishes() {
        let mut q = LoadQueue::new(2, 3);
        for url in ["a", "b", "c", "d"] {
            q.request(url, 0);
        }
        assert_eq!(q.next_batch().len(), 2);
        assert!(q.next_batch().is_empty(), "at the limit");

        q.finish("a", Outcome::Ok);
        assert_eq!(q.next_batch().len(), 1, "one slot freed");
    }

    #[test]
    fn a_retryable_failure_goes_back_in_line_until_attempts_run_out() {
        // Retry that no surveyed loader had. Three attempts total.
        let mut q = LoadQueue::new(1, 3);
        q.request("flaky", 0);

        for expected in [Slot::Queued, Slot::Queued] {
            let url = q.next_batch();
            assert_eq!(url, vec!["flaky".to_string()]);
            assert_eq!(q.finish("flaky", Outcome::Retryable), Some(expected));
        }

        // Third attempt exhausts the budget.
        assert_eq!(q.next_batch(), vec!["flaky".to_string()]);
        assert_eq!(q.finish("flaky", Outcome::Retryable), Some(Slot::Failed));
        assert!(q.next_batch().is_empty(), "given up, not re-queued forever");
    }

    #[test]
    fn a_permanent_failure_is_not_retried_at_all() {
        // A 404 or an undecodable image: retrying only burns requests.
        let mut q = LoadQueue::new(1, 3);
        q.request("gone", 0);
        q.next_batch();
        assert_eq!(q.finish("gone", Outcome::Permanent), Some(Slot::Failed));
        assert!(q.next_batch().is_empty());
    }

    #[test]
    fn a_failed_url_is_not_re_queued_by_asking_again() {
        // A caller that requests every frame must not resurrect a dead URL —
        // that turns one 404 into a permanent request loop.
        let mut q = LoadQueue::new(1, 3);
        q.request("gone", 0);
        q.next_batch();
        q.finish("gone", Outcome::Permanent);

        q.request("gone", 0);
        assert!(q.next_batch().is_empty());
        assert_eq!(q.slot("gone"), Some(Slot::Failed));

        // `forget` is the explicit way back, for a genuine reload.
        q.forget("gone");
        q.request("gone", 0);
        assert_eq!(q.next_batch(), vec!["gone".to_string()]);
    }

    #[test]
    fn a_late_reply_for_a_forgotten_url_is_ignored() {
        // The screen closed while a fetch was in flight. Not a panic.
        let mut q = LoadQueue::new(1, 3);
        q.request("a", 0);
        q.next_batch();
        q.forget("a");
        assert_eq!(q.finish("a", Outcome::Ok), None);
    }

    #[test]
    fn idle_means_nothing_queued_and_nothing_in_flight() {
        let mut q = LoadQueue::new(1, 3);
        assert!(q.is_idle());
        q.request("a", 0);
        assert!(!q.is_idle());
        q.next_batch();
        assert!(!q.is_idle(), "in flight still counts as busy");
        q.finish("a", Outcome::Ok);
        assert!(q.is_idle());
    }
}
