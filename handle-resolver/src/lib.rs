//! `HandleResolver` — a batching, deduplicating, self-limiting queue for
//! turning stake keys into `$handle` names.
//!
//! Every surface that shows a counterparty wants the same thing: a name if
//! there is one, the key if there isn't, and no thundering herd of lookups in
//! between. Each has so far solved it ad hoc — one resolves a single batch at
//! load and gives up on the rest, another keeps a `HashMap` beside a `Vec` of
//! "uncached" and re-asks whenever the view redraws.
//!
//! ## What makes this reliable rather than merely batched
//!
//! - **Deduplicated.** A key already known, already queued, or already in
//!   flight is never asked for twice, however many times a frame calls
//!   [`HandleResolver::want`]. Immediate-mode UIs call it every frame.
//! - **Negatively cached.** [`HandleResolver::apply`] takes the keys that were
//!   REQUESTED, not just the ones that came back, so a wallet with no handle
//!   is recorded as *having none* rather than sliding back into the queue to
//!   be asked about forever. Most wallets have no handle; without this the
//!   queue never drains.
//! - **Bounded retry.** A failed batch returns to the queue, but each key
//!   carries an attempt count and is abandoned after
//!   [`HandleResolver::max_attempts`]. A handle service that is down costs a
//!   few retries, not an infinite loop against a dead endpoint.
//! - **Batch-capped.** The service takes 100 keys per call, so
//!   [`HandleResolver::next_batch`] never hands back more than that.
//!
//! ## Shape of use
//!
//! Render calls `want` for whatever is on screen and `name` to draw it. A pump
//! — once per frame, or on a timer — calls `next_batch`, and whatever
//! transport the app has feeds the answer back through `apply` or `fail`.
//!
//! ```
//! use handle_resolver::HandleResolver;
//! use std::collections::HashMap;
//!
//! let mut r = HandleResolver::new();
//! r.want("stake1a");
//! r.want("stake1b");
//! r.want("stake1a"); // already queued — not asked twice
//!
//! let batch = r.next_batch().expect("two keys are waiting");
//! assert_eq!(batch.len(), 2);
//! assert!(r.next_batch().is_none(), "nothing left while that batch is in flight");
//!
//! let mut found = HashMap::new();
//! found.insert("stake1a".to_string(), "$alice".to_string());
//! r.apply(&batch, found);
//!
//! assert_eq!(r.name("stake1a"), Some("$alice"));
//! assert_eq!(r.name("stake1b"), None); // asked, genuinely has no handle
//! r.want("stake1b");
//! assert!(r.next_batch().is_none(), "and is not asked about again");
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

/// The handle service accepts this many keys per call.
pub const MAX_BATCH: usize = 100;

/// Give up on a key after this many failed attempts.
const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// What is known about one key.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// Waiting in the queue.
    Queued { attempts: u32 },
    /// Handed out in a batch, awaiting an answer.
    InFlight { attempts: u32 },
    /// It has a name.
    Named(String),
    /// It was asked about and has no handle. A real answer, and the reason
    /// the queue drains.
    Nameless,
    /// Repeatedly failed to resolve. Distinct from `Nameless`: we do not know
    /// whether it has a handle, only that asking isn't working.
    Abandoned,
}

/// What a caller should draw for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Never asked for.
    Unknown,
    /// Queued or in flight — a caller may show a placeholder.
    Pending,
    /// Resolved to a name.
    Named,
    /// Asked, and there is no handle. Draw the key.
    Nameless,
    /// Asking failed too many times. Draw the key.
    Abandoned,
}

#[derive(Debug, Clone)]
pub struct HandleResolver {
    entries: HashMap<String, Entry>,
    queue: VecDeque<String>,
    batch_size: usize,
    max_attempts: u32,
    /// Set when something resolved since the last check — the signal a UI
    /// needs to know a repaint is worth doing.
    dirty: bool,
}

impl Default for HandleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleResolver {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            queue: VecDeque::new(),
            batch_size: MAX_BATCH,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            dirty: false,
        }
    }

    /// Smaller batches for a transport with a tighter cap.
    pub fn with_batch_size(mut self, n: usize) -> Self {
        self.batch_size = n.clamp(1, MAX_BATCH);
        self
    }

    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    pub fn with_max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = n.max(1);
        self
    }

    /// Ask for a key's name. Safe to call every frame for everything on
    /// screen: anything already known, queued, in flight, or abandoned is
    /// ignored.
    pub fn want(&mut self, key: &str) {
        if self.entries.contains_key(key) {
            return;
        }
        self.entries
            .insert(key.to_string(), Entry::Queued { attempts: 0 });
        self.queue.push_back(key.to_string());
    }

    /// Ask for many at once, cheapest first — preserves queue order.
    pub fn want_all<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        for k in keys {
            self.want(k);
        }
    }

    /// Seed a name already known from elsewhere (a snapshot, another view), so
    /// it is never looked up again.
    pub fn insert_known(&mut self, key: &str, name: &str) {
        self.entries
            .insert(key.to_string(), Entry::Named(name.to_string()));
        self.dirty = true;
    }

    /// The resolved name, if there is one.
    pub fn name(&self, key: &str) -> Option<&str> {
        match self.entries.get(key) {
            Some(Entry::Named(n)) => Some(n.as_str()),
            _ => None,
        }
    }

    pub fn status(&self, key: &str) -> Status {
        match self.entries.get(key) {
            None => Status::Unknown,
            Some(Entry::Queued { .. }) | Some(Entry::InFlight { .. }) => Status::Pending,
            Some(Entry::Named(_)) => Status::Named,
            Some(Entry::Nameless) => Status::Nameless,
            Some(Entry::Abandoned) => Status::Abandoned,
        }
    }

    /// The next batch to send, marking those keys in flight. `None` when the
    /// queue is empty — including while a previous batch is outstanding,
    /// because those keys have left the queue.
    pub fn next_batch(&mut self) -> Option<Vec<String>> {
        if self.queue.is_empty() {
            return None;
        }
        let mut batch = Vec::with_capacity(self.batch_size.min(self.queue.len()));
        while batch.len() < self.batch_size {
            let Some(key) = self.queue.pop_front() else {
                break;
            };
            // The queue can hold a key whose entry moved on (seeded by
            // `insert_known`, say). Only ship what is genuinely still waiting.
            if let Some(Entry::Queued { attempts }) = self.entries.get(&key).cloned() {
                self.entries
                    .insert(key.clone(), Entry::InFlight { attempts });
                batch.push(key);
            }
        }
        (!batch.is_empty()).then_some(batch)
    }

    /// Record an answer. `requested` is the batch that was sent; every key in
    /// it that is absent from `found` is recorded as having no handle.
    ///
    /// Passing only `found` would be the bug this signature exists to prevent:
    /// unnamed keys would stay unresolved and be re-queued on the next frame,
    /// forever, which is most keys.
    pub fn apply(&mut self, requested: &[String], found: HashMap<String, String>) {
        let named: HashSet<&String> = found.keys().collect();
        for key in requested {
            if named.contains(key) {
                continue;
            }
            // Only settle keys still in flight — one re-requested and
            // re-queued in the meantime should not be clobbered.
            if matches!(self.entries.get(key), Some(Entry::InFlight { .. })) {
                self.entries.insert(key.clone(), Entry::Nameless);
            }
        }
        for (key, name) in found {
            self.entries.insert(key, Entry::Named(name));
        }
        self.dirty = true;
    }

    /// The batch did not come back. Returns keys to the queue, counting the
    /// attempt, and abandons any that have used up their allowance.
    pub fn fail(&mut self, requested: &[String]) {
        for key in requested {
            let Some(Entry::InFlight { attempts }) = self.entries.get(key).cloned() else {
                continue;
            };
            let attempts = attempts + 1;
            if attempts >= self.max_attempts {
                self.entries.insert(key.clone(), Entry::Abandoned);
            } else {
                self.entries.insert(key.clone(), Entry::Queued { attempts });
                self.queue.push_back(key.clone());
            }
        }
        self.dirty = true;
    }

    /// Keys waiting to be sent.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Keys sent and not yet answered.
    pub fn in_flight(&self) -> usize {
        self.entries
            .values()
            .filter(|e| matches!(e, Entry::InFlight { .. }))
            .count()
    }

    /// True once since something resolved — clears on read, so a UI can use it
    /// to decide whether a repaint is worth requesting.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Every name known so far, for handing to a view that wants a plain map.
    pub fn names(&self) -> HashMap<&str, &str> {
        self.entries
            .iter()
            .filter_map(|(k, v)| match v {
                Entry::Named(n) => Some((k.as_str(), n.as_str())),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn wanting_the_same_key_repeatedly_queues_it_once() {
        let mut r = HandleResolver::new();
        for _ in 0..100 {
            r.want("stake1a");
        }
        assert_eq!(r.queued(), 1, "an immediate-mode UI calls want every frame");
    }

    #[test]
    fn a_batch_leaves_the_queue_so_it_is_not_sent_twice() {
        let mut r = HandleResolver::new();
        r.want("stake1a");
        let batch = r.next_batch().expect("one queued");
        assert_eq!(batch, ["stake1a"]);
        assert_eq!(r.queued(), 0);
        assert_eq!(r.in_flight(), 1);
        assert!(r.next_batch().is_none());
    }

    #[test]
    fn batches_are_capped() {
        let mut r = HandleResolver::new().with_batch_size(10);
        for i in 0..25 {
            r.want(&format!("stake{i}"));
        }
        assert_eq!(r.next_batch().expect("first").len(), 10);
        assert_eq!(r.next_batch().expect("second").len(), 10);
        assert_eq!(r.next_batch().expect("third").len(), 5);
        assert!(r.next_batch().is_none());
    }

    /// The reason the queue drains. Most wallets have no handle; if "absent
    /// from the response" meant "still unknown", every frame would re-queue
    /// them and the lookup would never stop.
    #[test]
    fn keys_with_no_handle_are_remembered_as_having_none() {
        let mut r = HandleResolver::new();
        r.want("stake1a");
        r.want("stake1b");
        let batch = r.next_batch().expect("batch");
        r.apply(&batch, found(&[("stake1a", "$alice")]));

        assert_eq!(r.status("stake1a"), Status::Named);
        assert_eq!(r.status("stake1b"), Status::Nameless);

        r.want("stake1a");
        r.want("stake1b");
        assert_eq!(r.queued(), 0, "neither is asked about again");
    }

    #[test]
    fn a_failed_batch_retries_then_gives_up() {
        let mut r = HandleResolver::new().with_max_attempts(3);
        r.want("stake1a");
        for _ in 0..2 {
            let batch = r.next_batch().expect("retried");
            r.fail(&batch);
        }
        // Third failure exhausts the allowance.
        let batch = r.next_batch().expect("last attempt");
        r.fail(&batch);
        assert_eq!(r.status("stake1a"), Status::Abandoned);
        assert_eq!(r.queued(), 0);
        r.want("stake1a");
        assert!(
            r.next_batch().is_none(),
            "a dead service must not be hammered forever"
        );
    }

    #[test]
    fn abandoned_is_not_the_same_as_nameless() {
        // One says "there is no handle", the other says "we couldn't find
        // out". A caller may want to retry the second later; conflating them
        // would lose that.
        let mut r = HandleResolver::new().with_max_attempts(1);
        r.want("stake1a");
        let batch = r.next_batch().expect("batch");
        r.fail(&batch);
        assert_eq!(r.status("stake1a"), Status::Abandoned);
        assert_ne!(r.status("stake1a"), Status::Nameless);
    }

    #[test]
    fn a_seeded_name_is_never_looked_up() {
        let mut r = HandleResolver::new();
        r.insert_known("stake1a", "$alice");
        r.want("stake1a");
        assert_eq!(r.queued(), 0);
        assert_eq!(r.name("stake1a"), Some("$alice"));
    }

    /// A late answer must not overwrite a fresher state.
    #[test]
    fn a_stale_response_does_not_clobber_a_seeded_name() {
        let mut r = HandleResolver::new();
        r.want("stake1a");
        let batch = r.next_batch().expect("batch");
        r.insert_known("stake1a", "$alice");
        r.apply(&batch, HashMap::new()); // the old batch reports "no handle"
        assert_eq!(
            r.name("stake1a"),
            Some("$alice"),
            "the newer fact wins over a settled in-flight request"
        );
    }

    #[test]
    fn dirty_reports_once_per_change() {
        let mut r = HandleResolver::new();
        assert!(!r.take_dirty());
        r.want("stake1a");
        assert!(!r.take_dirty(), "queueing is not a visible change");
        let batch = r.next_batch().expect("batch");
        r.apply(&batch, found(&[("stake1a", "$alice")]));
        assert!(r.take_dirty(), "a resolution is worth a repaint");
        assert!(!r.take_dirty(), "and only once");
    }

    #[test]
    fn names_hands_back_only_what_resolved() {
        let mut r = HandleResolver::new();
        r.want("stake1a");
        r.want("stake1b");
        let batch = r.next_batch().expect("batch");
        r.apply(&batch, found(&[("stake1a", "$alice")]));
        let names = r.names();
        assert_eq!(names.len(), 1);
        assert_eq!(names.get("stake1a"), Some(&"$alice"));
    }
}
