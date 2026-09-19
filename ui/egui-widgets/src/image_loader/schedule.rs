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
//! - **Retention.** Completed images are held so a scroll back does not
//!   refetch, but not forever. Past [`Retain`] the coldest are released —
//!   see [`Schedule::release_cold`], and `fetch.rs` for both the part that
//!   makes releasing worth anything and the reason it runs as a begin-pass
//!   plugin rather than from the loader's own `end_pass`.
//!
//! Cancellation only ever touches loads that are still pending; retention only
//! ever touches loads that have completed.
//!
//! # Why retention is not just "drop the bytes"
//!
//! Three caches key on one URI: the fetched bytes here, egui's decoded
//! `ColorImage`, and the texture on the GPU. This one is the SMALLEST of them
//! — a decoded RGBA texture runs an order of magnitude or more above the
//! compressed bytes it came from — so dropping bytes alone would free almost
//! nothing. What makes it matter is that the adapter turns a release into
//! `ctx.forget_image(uri)`, which drops all three together.
//!
//! This used to retain every completed image for the life of the page,
//! because that is what egui's own loaders do. On a long browsing session
//! through thumbnail grids that is unbounded, and it was measured in
//! gigabytes.

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

/// What happens to a COMPLETED image nobody is looking at any more.
///
/// Deliberately not folded into [`Demand`], even though both are "nobody can
/// see this". The grace that is right for a pending fetch is wrong here by
/// orders of magnitude: abandoning a fetch costs a restart of something that
/// had not finished anyway, whereas releasing a completed image costs a
/// refetch AND a re-decode of something already paid for. A one-second
/// visibility grace applied to completions would make an ordinary scroll
/// thrash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Retain {
    /// Keep every completed image until something forgets it explicitly.
    ///
    /// How egui's own loaders behave, and unbounded: a session that browses
    /// enough thumbnails will exhaust the tab. Reasonable only when the set of
    /// images is small and known.
    Everything,
    /// Keep decoded texture memory under `bytes`, releasing least-recently-
    /// asked first.
    ///
    /// BYTES, not a count of images, and the difference is the whole point. A
    /// count is a proxy for memory only if images are all the same size, and
    /// art is not: a grid of 256px thumbnails and a wall of 2048px pieces are
    /// four hundred times apart per image. A cap of 256 images sounds
    /// conservative and is 2 GB of the latter — which is how a tab that had
    /// "bounded" retention still sat at gigabytes.
    ///
    /// The figure has to come from outside — see [`Schedule::release_cold`] —
    /// because this half deliberately cannot see a texture.
    UnderBytes { bytes: usize },
}

/// How a loader spends its fetches, and what it keeps afterwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadPolicy {
    /// Fetches in flight at once. Anything beyond waits in the queue, where
    /// its order can still be decided and it can be dropped for free.
    pub budget: usize,
    pub demand: Demand,
    pub retain: Retain,
}

impl Default for LoadPolicy {
    /// Sixteen in flight — well above the six-per-host HTTP/1.1 limit, so it
    /// never slows a page down, but low enough that the queue stays on this
    /// side of the network, where it can be reordered and dropped. One second
    /// of grace.
    ///
    /// 256 MB of decoded texture: several screens of any plausible grid, so
    /// scrolling back over what was just looked at does not refetch, while a
    /// session that works through a whole collection settles instead of
    /// growing. Scales itself — a wall of large art retains fewer pieces than
    /// a grid of thumbnails, which is the behaviour a count could never give.
    fn default() -> Self {
        Self {
            budget: 16,
            demand: Demand::Visible {
                grace: Duration::from_secs(1),
            },
            retain: Retain::UnderBytes {
                bytes: 256 * 1024 * 1024,
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

/// A finished load, and when it was last wanted.
///
/// `asked` is what makes retention possible: it is refreshed every time
/// [`Schedule::want`] serves this entry, so the oldest timestamp is the image
/// nobody has looked at for longest. Without it a completion carries no
/// evidence of whether anyone still cares about it.
#[derive(Debug, Clone)]
struct Done {
    result: Result<Fetched, String>,
    asked: Asked,
}

/// The loader's bookkeeping. See the module docs.
#[derive(Debug)]
pub struct Schedule {
    policy: LoadPolicy,
    pending: HashMap<String, Pending>,
    done: HashMap<String, Done>,
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
        let asked = Asked {
            at: now,
            pass: self.pass,
        };
        // Serving a completion is also the only signal that anyone still wants
        // it, so record it here. This is the LRU touch that `release_cold`
        // reads; without it every completion looks equally cold.
        if let Some(done) = self.done.get_mut(uri) {
            done.asked = asked;
            return match &done.result {
                Ok(fetched) => Want::Ready(fetched.clone()),
                Err(err) => Want::Failed(err.clone()),
            };
        }
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
                // Inherits the pending load's `asked`, so an image completing
                // long after anyone stopped looking at it is cold the moment
                // it lands rather than counting as freshly wanted.
                let asked = self
                    .pending
                    .remove(uri)
                    .map(|p| p.asked)
                    .unwrap_or(Asked { at: 0.0, pass: 0 });
                self.in_flight -= 1;
                self.done.insert(uri.to_owned(), Done { result, asked });
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

    /// Drop the completed images held beyond [`Retain`], coldest first.
    ///
    /// Returns what was dropped so the caller can forget it through the
    /// CONTEXT as well. That second step is the one that frees real memory —
    /// on its own this only releases compressed bytes, while the decoded image
    /// and its texture, which are much larger, stay in egui's caches.
    ///
    /// Failures are never released. A cached error is what stops a broken URL
    /// being refetched every time it scrolls into view, and it costs a string
    /// rather than a texture, so it is not what anyone came here to reclaim.
    /// `texture_bytes` is what the host measures as currently decoded — for
    /// egui, `Context::tex_manager().read().bytes_used()`. It is an AGGREGATE,
    /// so this works in averages: how many to drop is `over budget ÷ mean
    /// size`. Approximate on any one pass and self-correcting across them,
    /// since the next pass measures again.
    ///
    /// That aggregate also counts textures this loader never fetched — the
    /// font atlas, icons — which act as a floor it cannot evict below. Holding
    /// no images is the stopping condition, so a budget set under that floor
    /// releases everything once and then does nothing, rather than spinning.
    pub fn release_cold(&mut self, texture_bytes: usize) -> Vec<String> {
        let Retain::UnderBytes { bytes } = self.policy.retain else {
            return Vec::new();
        };
        let Some(over) = texture_bytes.checked_sub(bytes).filter(|n| *n > 0) else {
            return Vec::new();
        };
        let mut held: Vec<(f64, &str)> = self
            .done
            .iter()
            .filter(|(_, done)| done.result.is_ok())
            .map(|(uri, done)| (done.asked.at, uri.as_str()))
            .collect();
        if held.is_empty() {
            return Vec::new();
        }
        // Round UP, so being over budget always releases at least one image.
        // Rounding down stalls exactly when the overage is smaller than one
        // average image, which is the steady state — it would sit permanently
        // just over the cap and never act.
        let mean = (texture_bytes / held.len()).max(1);
        let excess = over.div_ceil(mean).min(held.len());
        // `total_cmp` rather than `partial_cmp().unwrap()` — the clock is an
        // `f64` from the host and a sort that panics on an unexpected NaN
        // would take the whole frame with it.
        held.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let cold: Vec<String> = held
            .into_iter()
            .take(excess)
            .map(|(_, uri)| uri.to_owned())
            .collect();
        for uri in &cold {
            self.done.remove(uri);
        }
        cold
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
        let failed = self.done.values().filter(|d| d.result.is_err()).count();
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
            .map(|done| match &done.result {
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
        LoadPolicy {
            budget,
            demand,
            // Retention off unless a test is about retention, so the existing
            // cancellation tests keep measuring only what they were written to
            // measure.
            retain: Retain::Everything,
        }
    }

    fn visible(budget: usize) -> Schedule {
        Schedule::new(policy(budget, Demand::Visible { grace: GRACE }))
    }

    /// One notional decoded image, for the arithmetic below. Held constant so
    /// a test can say "three images' worth" and mean it.
    const TEXTURE: usize = 4 * 1024 * 1024;

    /// A schedule holding texture memory under `images` notional images'
    /// worth, with a fetch budget big enough that nothing queues.
    fn retaining(images: usize) -> Schedule {
        Schedule::new(LoadPolicy {
            budget: 64,
            demand: Demand::Visible { grace: GRACE },
            retain: Retain::UnderBytes {
                bytes: images * TEXTURE,
            },
        })
    }

    /// Fetch `uri` to completion, last wanted at `at`.
    fn loaded(s: &mut Schedule, uri: &str, at: f64) {
        s.want(uri, at);
        let start = s
            .starts()
            .into_iter()
            .find(|st| st.uri == uri)
            .expect("the budget has room");
        assert_eq!(s.finish(uri, start.ticket, body(uri)), Finish::Stored);
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
    fn nothing_is_released_while_under_the_retention_cap() {
        let mut s = retaining(4);
        for uri in ["a", "b", "c"] {
            loaded(&mut s, uri, 0.0);
        }
        assert!(s.release_cold(3 * TEXTURE).is_empty());
        assert_eq!(s.counts().ready, 3);
    }

    #[test]
    fn over_the_cap_the_least_recently_asked_for_is_released_first() {
        let mut s = retaining(2);
        loaded(&mut s, "coldest", 0.0);
        loaded(&mut s, "middle", 1.0);
        loaded(&mut s, "newest", 2.0);

        assert_eq!(s.release_cold(3 * TEXTURE), ["coldest"]);
        // Released means GONE, not merely reported: the next ask has to fetch
        // it again rather than being served a cached copy.
        assert!(matches!(s.want("coldest", 3.0), Want::Pending));
        assert!(matches!(s.want("middle", 3.0), Want::Ready(_)));
        assert!(matches!(s.want("newest", 3.0), Want::Ready(_)));
    }

    /// The point of tracking `asked` on completions. An image fetched long ago
    /// but still on screen is the LAST thing that should be released, and
    /// without the touch in `want` it would look like the first.
    #[test]
    fn being_asked_for_again_rescues_the_oldest_fetch() {
        let mut s = retaining(2);
        loaded(&mut s, "old-but-on-screen", 0.0);
        loaded(&mut s, "b", 1.0);
        loaded(&mut s, "c", 2.0);

        // Still painted, so still asked for.
        assert!(matches!(s.want("old-but-on-screen", 3.0), Want::Ready(_)));

        assert_eq!(s.release_cold(3 * TEXTURE), ["b"]);
        assert!(matches!(s.want("old-but-on-screen", 4.0), Want::Ready(_)));
    }

    #[test]
    fn a_failure_is_never_released_and_does_not_use_up_the_cap() {
        let mut s = retaining(1);
        s.want("broken", 0.0);
        let start = s.starts().remove(0);
        s.finish("broken", start.ticket, Err("404".into()));
        loaded(&mut s, "good", 1.0);

        assert!(
            s.release_cold(TEXTURE).is_empty(),
            "one success against a cap of one image's worth"
        );
        // The cached error is what stops a broken URL being refetched every
        // time it scrolls past.
        assert!(matches!(s.want("broken", 2.0), Want::Failed(e) if e == "404"));
    }

    #[test]
    fn retain_everything_releases_nothing_however_many_land() {
        let mut s = Schedule::new(LoadPolicy {
            budget: 64,
            demand: Demand::Visible { grace: GRACE },
            retain: Retain::Everything,
        });
        for n in 0..50 {
            loaded(&mut s, &format!("img-{n}"), n as f64);
        }
        assert!(s.release_cold(50 * TEXTURE).is_empty());
        assert_eq!(s.counts().ready, 50);
    }

    #[test]
    fn releasing_frees_the_bytes_it_was_holding() {
        let mut s = retaining(1);
        loaded(&mut s, "coldest", 0.0);
        loaded(&mut s, "newest", 1.0);
        let full = s.byte_size();

        assert_eq!(s.release_cold(2 * TEXTURE).len(), 1);
        assert!(
            s.byte_size() < full,
            "releasing has to actually drop the bytes, not just the bookkeeping"
        );
    }

    /// Why the cap is bytes and not a count. The same three images against the
    /// same budget release nothing when they are thumbnails and most of
    /// themselves when they are large art — which a count of images cannot
    /// express, and is how "bounded" retention still sat at gigabytes.
    #[test]
    fn the_same_images_release_differently_by_how_large_they_actually_are() {
        let budget = 8 * 1024 * 1024;
        let policy = |bytes| LoadPolicy {
            budget: 64,
            demand: Demand::Visible { grace: GRACE },
            retain: Retain::UnderBytes { bytes },
        };

        let mut thumbnails = Schedule::new(policy(budget));
        let mut art = Schedule::new(policy(budget));
        for (n, uri) in ["a", "b", "c"].iter().enumerate() {
            loaded(&mut thumbnails, uri, n as f64);
            loaded(&mut art, uri, n as f64);
        }

        // 3 × 256² RGBA — well inside the budget.
        assert!(thumbnails.release_cold(3 * 256 * 256 * 4).is_empty());
        // 3 × 2048² RGBA — 48 MB against an 8 MB budget.
        assert_eq!(art.release_cold(3 * 2048 * 2048 * 4).len(), 3);
    }

    /// Being over by less than one average image still has to act. Rounding
    /// down would park the cache permanently just above its cap, which is the
    /// steady state rather than an edge case.
    #[test]
    fn an_overage_smaller_than_one_image_still_releases_one() {
        let mut s = retaining(2);
        loaded(&mut s, "coldest", 0.0);
        loaded(&mut s, "newest", 1.0);

        assert_eq!(s.release_cold(2 * TEXTURE + 1), ["coldest"]);
    }

    /// The font atlas and icons are in the host's figure and cannot be evicted
    /// by this loader. A budget below that floor must release what it has and
    /// then stop, not spin reporting work it cannot do.
    #[test]
    fn a_budget_under_the_unevictable_floor_stops_instead_of_spinning() {
        let mut s = retaining(0);
        loaded(&mut s, "a", 0.0);

        assert_eq!(s.release_cold(50 * TEXTURE), ["a"]);
        // Nothing left that this loader owns, however far over the figure is.
        assert!(s.release_cold(50 * TEXTURE).is_empty());
    }

    /// Releasing is a feedback loop — measure, evict, measure again — and it
    /// carries no latch against acting on a stale reading, because there is no
    /// staleness to guard: `TextureManager::free` removes the entry from the
    /// map `allocated()` iterates SYNCHRONOUSLY, so the host's next figure
    /// already reflects what we dropped. (The GPU-side free lands a frame
    /// later via the texture delta; that is not what is measured here.)
    ///
    /// A latch was tried and removed. Damping this loop on the aggregate is
    /// actively wrong: the total also rises when NEW images decode, so a
    /// "wait until the figure falls" guard switches retention off during a
    /// scroll — precisely when it is needed.
    ///
    /// So the contract is simply: each call answers the figure it was given.
    #[test]
    fn each_call_answers_the_figure_it_was_given() {
        let mut s = retaining(1);
        for (n, uri) in ["coldest", "middle", "newest"].iter().enumerate() {
            loaded(&mut s, uri, n as f64);
        }

        // Three images' worth against a one-image budget: two must go.
        assert_eq!(s.release_cold(3 * TEXTURE), ["coldest", "middle"]);
        // The figure has moved, and one image is within budget.
        assert!(s.release_cold(TEXTURE).is_empty());
        assert_eq!(s.counts().ready, 1);
    }

    /// A rising total mid-scroll keeps being acted on rather than stalling —
    /// new art arriving is exactly when releasing matters.
    #[test]
    fn a_total_that_climbs_while_releasing_still_gets_acted_on() {
        let mut s = retaining(2);
        for (n, uri) in ["a", "b", "c", "d"].iter().enumerate() {
            loaded(&mut s, uri, n as f64);
        }

        assert_eq!(s.release_cold(4 * TEXTURE), ["a", "b"]);
        // Two released, but two more decoded while that happened, so the
        // figure is no lower. It must still release.
        assert!(!s.release_cold(4 * TEXTURE).is_empty());
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
