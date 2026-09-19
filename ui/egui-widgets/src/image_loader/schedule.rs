//! Which image fetches run, which wait, and which are abandoned.
//!
//! The pure half of `fetch::BrowserHttpLoader` (wasm only): no `web_sys`, no
//! clock, no I/O, so it builds and is tested natively. The adapter feeds it
//! requests, completions and the time, and carries out what it decides —
//! start these fetches, abort those.
//!
//! Two decisions live here.
//!
//! - **Budget.** At most `budget` fetches are in flight; the rest queue. The
//!   queue serves what was asked for in the latest pass ahead of anything
//!   left over, and among those the oldest request first. So after switching
//!   collection the new page's thumbnails go ahead of whatever the old page
//!   left behind, and they fill in top-left first rather than bottom-right.
//! - **Demand.** egui asks for an image on every pass it is painted, and the
//!   grids only paint what is on screen. A pending load nobody has asked for
//!   lately is therefore one nobody can see. Under [`Demand::Visible`] it is
//!   cancelled after a grace period: dropped if queued, aborted if in flight.
//!   Asked for again, it starts again from scratch.
//!
//! Completed results are cached until forgotten, as every egui loader does.
//! Cancellation only ever touches loads that are still pending.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Names one fetch attempt. A load cancelled and then asked for again gets a
/// new ticket, so the first attempt's late completion cannot land as the
/// second's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// The body of a completed fetch.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub bytes: Arc<[u8]>,
    pub mime: Option<String>,
}

/// What happens to a pending load that stops being asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Demand {
    /// Nothing: every load runs to completion once started. This is how
    /// `egui_extras`' http loader behaves.
    Sticky,
    /// Cancel a pending load that has not been asked for within `grace`. The
    /// grace stops a quick scroll past and back from throwing away a fetch
    /// that was nearly done.
    Visible { grace: Duration },
}

/// How a loader spends its fetches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadPolicy {
    /// Fetches in flight at once. Anything beyond waits in the queue, where
    /// its order can still be decided and it can be dropped for free.
    pub budget: usize,
    pub demand: Demand,
}

impl Default for LoadPolicy {
    /// Sixteen in flight — well above the six-per-host HTTP/1.1 limit, so it
    /// never slows a page down, but low enough that the queue stays on this
    /// side of the network, where it can be reordered and dropped. One second
    /// of grace.
    fn default() -> Self {
        Self {
            budget: 16,
            demand: Demand::Visible {
                grace: Duration::from_secs(1),
            },
        }
    }
}

/// A snapshot of the loader, for display and for tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadCounts {
    pub queued: usize,
    pub in_flight: usize,
    pub ready: usize,
    pub failed: usize,
    /// Pending loads abandoned since the loader was created.
    pub cancelled: u64,
}

/// The answer to asking for a URI.
#[derive(Debug, Clone)]
pub enum Want {
    Ready(Fetched),
    Failed(String),
    Pending,
}

/// A fetch the adapter should begin now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub uri: String,
    pub ticket: Ticket,
}

/// Whether a completion was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Stored,
    /// The load had been cancelled (or forgotten) while the fetch ran.
    Stale,
}

/// When a pending load was last asked for.
#[derive(Debug, Clone, Copy)]
struct Asked {
    /// Seconds, on whatever clock the adapter uses.
    at: f64,
    pass: u64,
}

#[derive(Debug, Clone, Copy)]
enum Stage {
    /// `order` is when it was first asked for; lower goes first.
    Queued {
        order: u64,
    },
    InFlight {
        ticket: Ticket,
    },
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    stage: Stage,
    asked: Asked,
}

/// The loader's bookkeeping. See the module docs.
#[derive(Debug)]
pub struct Schedule {
    policy: LoadPolicy,
    pending: HashMap<String, Pending>,
    done: HashMap<String, Result<Fetched, String>>,
    /// The pass currently being drawn; advanced by [`Schedule::end_pass`].
    pass: u64,
    /// Source of both queue order and tickets.
    next: u64,
    in_flight: usize,
    cancelled: u64,
}

impl Schedule {
    pub fn new(policy: LoadPolicy) -> Self {
        Self {
            policy,
            pending: HashMap::new(),
            done: HashMap::new(),
            pass: 0,
            next: 0,
            in_flight: 0,
            cancelled: 0,
        }
    }

    pub fn policy(&self) -> LoadPolicy {
        self.policy
    }

    /// Takes effect from the next [`Schedule::starts`] and
    /// [`Schedule::end_pass`]. Lowering the budget aborts nothing; the extra
    /// fetches are allowed to finish.
    pub fn set_policy(&mut self, policy: LoadPolicy) {
        self.policy = policy;
    }

    /// Ask for `uri` at time `now`. Queues it if it is new; either way it
    /// counts as wanted this pass.
    pub fn want(&mut self, uri: &str, now: f64) -> Want {
        match self.done.get(uri) {
            Some(Ok(fetched)) => return Want::Ready(fetched.clone()),
            Some(Err(err)) => return Want::Failed(err.clone()),
            None => {}
        }
        let asked = Asked {
            at: now,
            pass: self.pass,
        };
        match self.pending.get_mut(uri) {
            Some(pending) => pending.asked = asked,
            None => {
                let order = self.bump();
                self.pending.insert(
                    uri.to_owned(),
                    Pending {
                        stage: Stage::Queued { order },
                        asked,
                    },
                );
            }
        }
        Want::Pending
    }

    /// Move queued loads into flight while the budget allows, best first.
    pub fn starts(&mut self) -> Vec<Start> {
        let free = self.policy.budget.saturating_sub(self.in_flight);
        if free == 0 || self.pending.len() == self.in_flight {
            return Vec::new();
        }
        // Asked for in this pass or the one just ended — the pass in progress
        // may not have reached every image yet.
        let recent = self.pass.saturating_sub(1);
        let mut queued: Vec<(bool, u64, &str)> = self
            .pending
            .iter()
            .filter_map(|(uri, pending)| match pending.stage {
                Stage::Queued { order } => Some((pending.asked.pass < recent, order, uri.as_str())),
                Stage::InFlight { .. } => None,
            })
            .collect();
        // `false` (recent) sorts first, then oldest request.
        queued.sort_unstable_by_key(|&(stale, order, _)| (stale, order));
        let chosen: Vec<String> = queued
            .into_iter()
            .take(free)
            .map(|(_, _, uri)| uri.to_owned())
            .collect();

        chosen
            .into_iter()
            .map(|uri| {
                let ticket = Ticket(self.bump());
                if let Some(pending) = self.pending.get_mut(&uri) {
                    pending.stage = Stage::InFlight { ticket };
                }
                self.in_flight += 1;
                Start { uri, ticket }
            })
            .collect()
    }

    /// Record a fetch's outcome. Kept only if that attempt is still the live one.
    pub fn finish(&mut self, uri: &str, ticket: Ticket, result: Result<Fetched, String>) -> Finish {
        match self.pending.get(uri).map(|p| p.stage) {
            Some(Stage::InFlight { ticket: live }) if live == ticket => {
                self.pending.remove(uri);
                self.in_flight -= 1;
                self.done.insert(uri.to_owned(), result);
                Finish::Stored
            }
            Some(Stage::InFlight { .. } | Stage::Queued { .. }) | None => Finish::Stale,
        }
    }

    /// The pass has ended at `now`: advance, and under [`Demand::Visible`]
    /// cancel whatever has gone unasked for longer than the grace. Returns
    /// the tickets to abort.
    pub fn end_pass(&mut self, now: f64) -> Vec<Ticket> {
        self.pass += 1;
        match self.policy.demand {
            Demand::Sticky => Vec::new(),
            Demand::Visible { grace } => {
                let cutoff = now - grace.as_secs_f64();
                self.cancel_where_asked(|_, asked| asked.at < cutoff)
            }
        }
    }

    /// Abandon every pending load. Returns the tickets to abort.
    pub fn cancel_pending(&mut self) -> Vec<Ticket> {
        self.cancel_where_asked(|_, _| true)
    }

    /// Abandon the pending loads whose URI matches. Returns the tickets to abort.
    pub fn cancel_where(&mut self, mut matches: impl FnMut(&str) -> bool) -> Vec<Ticket> {
        self.cancel_where_asked(|uri, _| matches(uri))
    }

    /// Drop `uri` entirely, cached or pending, so the next ask refetches it.
    /// Returns the ticket to abort if it was in flight.
    pub fn forget(&mut self, uri: &str) -> Option<Ticket> {
        self.done.remove(uri);
        let ticket = self.cancel_where_asked(|u, _| u == uri);
        ticket.into_iter().next()
    }

    /// Drop everything. Returns the tickets to abort.
    pub fn forget_all(&mut self) -> Vec<Ticket> {
        self.done.clear();
        self.cancel_pending()
    }

    pub fn counts(&self) -> LoadCounts {
        let failed = self.done.values().filter(|r| r.is_err()).count();
        LoadCounts {
            queued: self.pending.len() - self.in_flight,
            in_flight: self.in_flight,
            ready: self.done.len() - failed,
            failed,
            cancelled: self.cancelled,
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn byte_size(&self) -> usize {
        self.done
            .values()
            .map(|r| match r {
                Ok(fetched) => fetched.bytes.len(),
                Err(err) => err.len(),
            })
            .sum()
    }

    fn cancel_where_asked(&mut self, mut cancel: impl FnMut(&str, Asked) -> bool) -> Vec<Ticket> {
        let mut tickets = Vec::new();
        let before = self.pending.len();
        self.pending.retain(|uri, pending| {
            if !cancel(uri, pending.asked) {
                return true;
            }
            match pending.stage {
                Stage::InFlight { ticket } => tickets.push(ticket),
                Stage::Queued { .. } => {}
            }
            false
        });
        self.in_flight -= tickets.len();
        self.cancelled += (before - self.pending.len()) as u64;
        tickets
    }

    fn bump(&mut self) -> u64 {
        let n = self.next;
        self.next += 1;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(1);

    fn policy(budget: usize, demand: Demand) -> LoadPolicy {
        LoadPolicy { budget, demand }
    }

    fn visible(budget: usize) -> Schedule {
        Schedule::new(policy(budget, Demand::Visible { grace: GRACE }))
    }

    fn body(text: &str) -> Result<Fetched, String> {
        Ok(Fetched {
            bytes: Arc::from(text.as_bytes()),
            mime: Some("image/png".into()),
        })
    }

    fn uris(starts: &[Start]) -> Vec<&str> {
        starts.iter().map(|s| s.uri.as_str()).collect()
    }

    #[test]
    fn the_budget_caps_what_is_in_flight_and_a_completion_frees_a_place() {
        let mut s = visible(2);
        for uri in ["a", "b", "c"] {
            assert!(matches!(s.want(uri, 0.0), Want::Pending));
        }
        let first = s.starts();
        assert_eq!(uris(&first), ["a", "b"]);
        assert!(s.starts().is_empty(), "no place left in the budget");

        assert_eq!(s.finish("a", first[0].ticket, body("a")), Finish::Stored);
        assert_eq!(uris(&s.starts()), ["c"]);
        assert!(matches!(s.want("a", 0.0), Want::Ready(_)));
    }

    #[test]
    fn a_new_page_goes_ahead_of_what_the_old_page_left_queued() {
        let mut s = visible(1);
        // The old page asks for three; one gets the only place.
        for uri in ["old-1", "old-2", "old-3"] {
            s.want(uri, 0.0);
        }
        let running = s.starts();
        s.end_pass(0.1);
        s.end_pass(0.2);

        // Navigated: only the new page is painted now.
        for uri in ["new-1", "new-2"] {
            s.want(uri, 0.3);
        }
        s.finish("old-1", running[0].ticket, body("old"));
        assert_eq!(uris(&s.starts()), ["new-1"]);
    }

    #[test]
    fn among_recent_asks_the_first_asked_goes_first() {
        let mut s = visible(3);
        // Painted top-left to bottom-right, the same order every pass.
        for pass in 0..3 {
            for uri in ["top-left", "middle", "bottom-right"] {
                s.want(uri, pass as f64 * 0.016);
            }
            s.end_pass(pass as f64 * 0.016);
        }
        assert_eq!(uris(&s.starts()), ["top-left", "middle", "bottom-right"]);
    }

    #[test]
    fn a_load_nobody_asks_for_is_cancelled_after_the_grace_and_not_before() {
        let mut s = visible(1);
        s.want("in-flight", 0.0);
        s.want("queued", 0.0);
        let running = s.starts();

        assert!(s.end_pass(0.9).is_empty(), "within the grace");
        assert_eq!(s.counts().queued + s.counts().in_flight, 2);

        assert_eq!(s.end_pass(1.1), vec![running[0].ticket]);
        let counts = s.counts();
        assert_eq!(
            (counts.queued, counts.in_flight, counts.cancelled),
            (0, 0, 2)
        );
    }

    #[test]
    fn a_load_still_being_asked_for_survives_the_sweep() {
        let mut s = visible(4);
        s.want("on-screen", 0.0);
        s.want("scrolled-away", 0.0);
        s.starts();
        for t in 1..=20 {
            let now = t as f64 * 0.1;
            s.want("on-screen", now);
            s.end_pass(now);
        }
        let counts = s.counts();
        assert_eq!((counts.in_flight, counts.cancelled), (1, 1));
    }

    #[test]
    fn sticky_demand_never_cancels() {
        let mut s = Schedule::new(policy(1, Demand::Sticky));
        s.want("a", 0.0);
        s.want("b", 0.0);
        s.starts();
        assert!(s.end_pass(1_000.0).is_empty());
        assert_eq!(s.counts().cancelled, 0);
    }

    #[test]
    fn a_cancelled_fetch_finishing_late_does_not_land_on_its_retry() {
        let mut s = visible(1);
        s.want("a", 0.0);
        let first = s.starts().remove(0);
        assert_eq!(s.cancel_pending(), vec![first.ticket]);

        s.want("a", 5.0);
        let retry = s.starts().remove(0);
        assert_ne!(first.ticket, retry.ticket);

        assert_eq!(
            s.finish("a", first.ticket, Err("aborted".into())),
            Finish::Stale
        );
        assert!(matches!(s.want("a", 5.0), Want::Pending));
        assert_eq!(s.finish("a", retry.ticket, body("a")), Finish::Stored);
        assert!(matches!(s.want("a", 5.0), Want::Ready(_)));
    }

    #[test]
    fn cancel_where_touches_only_matching_pending_loads() {
        let mut s = visible(8);
        s.want("iiif/policy-a:01", 0.0);
        s.want("iiif/policy-b:01", 0.0);
        s.want("iiif/policy-a:02", 0.0);
        let started = s.starts();
        let done = started
            .iter()
            .find(|st| st.uri == "iiif/policy-a:02")
            .unwrap();
        s.finish("iiif/policy-a:02", done.ticket, body("kept"));

        assert_eq!(s.cancel_where(|uri| uri.contains("policy-a")).len(), 1);
        assert!(matches!(s.want("iiif/policy-a:02", 0.0), Want::Ready(_)));
        let counts = s.counts();
        assert_eq!(
            (counts.in_flight, counts.ready, counts.cancelled),
            (1, 1, 1)
        );
    }

    #[test]
    fn a_failure_is_cached_until_forgotten() {
        let mut s = visible(1);
        s.want("a", 0.0);
        let start = s.starts().remove(0);
        s.finish("a", start.ticket, Err("404".into()));
        assert!(matches!(s.want("a", 0.0), Want::Failed(e) if e == "404"));
        assert_eq!(s.counts().failed, 1);

        assert_eq!(s.forget("a"), None, "nothing was in flight");
        assert!(matches!(s.want("a", 0.0), Want::Pending));
        assert_eq!(s.starts().len(), 1);
    }

    #[test]
    fn forgetting_an_in_flight_load_hands_back_its_ticket() {
        let mut s = visible(1);
        s.want("a", 0.0);
        let start = s.starts().remove(0);
        assert_eq!(s.forget("a"), Some(start.ticket));
        assert!(!s.has_pending());
    }
}
