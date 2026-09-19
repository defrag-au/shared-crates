//! What a frontend is costing, while it runs.
//!
//! Four questions, four shapes:
//!
//! - **What does a frame cost?** [`Frames`] — a ring of the last
//!   [`FRAME_WINDOW`] frames, recording *two* numbers per frame that are
//!   routinely confused. See "build time is not frame interval" below.
//! - **What work is outstanding?** [`Gauge`] — a named counter with an RAII
//!   [`Guard`], incremented at the one chokepoint the work passes through.
//! - **Where did one operation spend its time?** [`Timeline`] — the stages of
//!   a multi-step flow, so "the cart took four seconds" becomes "the cart
//!   spent 3.4 of them waiting on the wallet".
//! - **How much memory are we holding?** [`mem`] — honest on wasm, and honest
//!   about being unavailable elsewhere.
//!
//! Rendering is somebody else's job. `egui-widgets`' `perf_strip` draws this;
//! nothing here knows what a pixel is.
//!
//! # Build time is not frame interval
//!
//! The obvious metric is FPS, and on a reactive UI it is close to meaningless.
//! egui only repaints when something asks it to, so an idle view sits at
//! whatever cadence its slowest timer sets — 4 fps if a panel is polling at
//! 250 ms — and a naive counter reports that as catastrophic when in fact the
//! app is doing nothing because there is nothing to do.
//!
//! So a frame records both:
//!
//! - **build ms** — how long the frame's work actually took. Cadence-independent.
//!   This is the number that says whether a widget is too expensive, and the
//!   one to watch when adding work to a view.
//! - **interval ms** — wall time since the previous frame, from which FPS
//!   derives. This says how often we are *choosing* to render, which is a
//!   battery question, not a cost question.
//!
//! A healthy idle frontend reads "build 0.9 ms · 4 fps" and both halves are
//! good news. Reporting only the second would have you optimising an app that
//! is already asleep.
//!
//! # Cost of the probe itself
//!
//! [`Gauge::enter`] is three relaxed atomics; against the `fetch()` it is
//! wrapping, that is not measurable. [`Frames::record`] is an uncontended mutex
//! and two array writes, once per frame. Neither allocates.
//!
//! The reads DO allocate ([`gauges`] returns a `Vec`, [`Frames::stats`] sorts a
//! copy), which is why they are built for a **sampling** caller — a few times a
//! second, not once a frame. The panel that renders them enforces that; a new
//! caller has to choose to.
//!
//! Nothing here is behind a feature flag, because there is nothing to switch
//! off. The one probe that would genuinely cost — a counting `GlobalAlloc`, two
//! atomics on *every allocation in the process* — is deliberately not in this
//! crate.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::Relaxed};

// ─── Gauges ──────────────────────────────────────────────────────────────────

/// A named count of work that is outstanding right now.
///
/// Declare one as a `static` at the chokepoint the work passes through, and
/// bracket the work with [`enter`](Gauge::enter):
///
/// ```
/// use perf_probe::Gauge;
///
/// static REQUESTS: Gauge = Gauge::new("http requests");
///
/// # fn send() {
/// let _guard = REQUESTS.enter();
/// // ... the work ...
/// # }
/// ```
///
/// **`let _guard`, not `let _`.** `let _ = REQUESTS.enter()` drops the guard
/// immediately and silently records a gauge that is always zero — it compiles,
/// it runs, and the panel shows nothing wrong.
///
/// Tracks three numbers because they answer different questions: `current` is
/// "is something stuck", `peak` is "how deep did the queue get" (which a
/// sampling reader will otherwise miss entirely), and `total` is throughput.
pub struct Gauge {
    name: &'static str,
    current: AtomicI64,
    peak: AtomicI64,
    total: AtomicU64,
    registered: AtomicBool,
}

impl Gauge {
    /// Declare a gauge. `const`, so it can be a `static`.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            current: AtomicI64::new(0),
            peak: AtomicI64::new(0),
            total: AtomicU64::new(0),
            registered: AtomicBool::new(false),
        }
    }

    /// The gauge's display name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Count one unit of work as started; the returned guard counts it as
    /// finished when it drops.
    ///
    /// Drop-based rather than a matching `leave()` because the work being
    /// counted is usually fallible and usually `async`: every `?` and every
    /// early return is a path that a hand-written decrement eventually stops
    /// covering, and a gauge that leaks upward is worse than no gauge — it
    /// reads as a stuck queue forever.
    pub fn enter(&'static self) -> Guard {
        self.ensure_registered();
        let now = self.current.fetch_add(1, Relaxed) + 1;
        self.peak.fetch_max(now, Relaxed);
        self.total.fetch_add(1, Relaxed);
        Guard { gauge: self }
    }

    /// Make the gauge visible before its first use.
    ///
    /// A gauge registers itself on first [`enter`](Gauge::enter), which means a
    /// panel opened before the first request has ever been sent shows no card
    /// for it at all — indistinguishable from "this build has no instrumentation".
    /// Call this at startup for anything whose *absence* is itself information.
    pub fn register(&'static self) {
        self.ensure_registered();
    }

    /// Outstanding right now.
    pub fn current(&self) -> i64 {
        self.current.load(Relaxed)
    }

    /// The highest `current` ever observed.
    pub fn peak(&self) -> i64 {
        self.peak.load(Relaxed)
    }

    /// Total units started since the process began.
    pub fn total(&self) -> u64 {
        self.total.load(Relaxed)
    }

    fn ensure_registered(&'static self) {
        // `swap` rather than load-then-store: two threads reaching a cold gauge
        // together would otherwise both see `false` and register it twice, and
        // the panel would grow a duplicate card that never goes away.
        if !self.registered.swap(true, Relaxed)
            && let Ok(mut reg) = REGISTRY.lock()
        {
            reg.push(self);
        }
    }

    fn snapshot(&self) -> GaugeSnapshot {
        GaugeSnapshot {
            name: self.name,
            current: self.current(),
            peak: self.peak(),
            total: self.total(),
        }
    }
}

/// Holds one unit of a [`Gauge`] open. Decrements on drop.
#[must_use = "the gauge only counts the work while this guard is alive; \
              binding it to `_` drops it immediately and always reads zero"]
pub struct Guard {
    gauge: &'static Gauge,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.gauge.current.fetch_sub(1, Relaxed);
    }
}

/// One gauge's numbers, detached from the gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GaugeSnapshot {
    pub name: &'static str,
    pub current: i64,
    pub peak: i64,
    pub total: u64,
}

static REGISTRY: Mutex<Vec<&'static Gauge>> = Mutex::new(Vec::new());

/// Every gauge that has been used or [registered](Gauge::register), in
/// declaration-touch order.
///
/// Allocates — call it on a sampling tick, not per frame.
pub fn gauges() -> Vec<GaugeSnapshot> {
    match REGISTRY.lock() {
        Ok(reg) => reg.iter().map(|g| g.snapshot()).collect(),
        Err(_) => Vec::new(),
    }
}

// ─── Frames ──────────────────────────────────────────────────────────────────

/// How many frames the ring keeps. Two seconds at 60 Hz — long enough to see a
/// stutter, short enough that the panel reflects what is happening *now*
/// rather than averaging a hitch away.
pub const FRAME_WINDOW: usize = 120;

/// One frame's budget at 60 Hz. Everything — build, tessellate, paint — has to
/// fit inside this, so a build time approaching it is already too slow.
pub const BUDGET_60HZ_MS: f32 = 1000.0 / 60.0;

/// A ring of recent frame costs.
///
/// See the crate docs for why this records two numbers and not just FPS.
pub struct Frames {
    build_ms: [f32; FRAME_WINDOW],
    interval_ms: [f32; FRAME_WINDOW],
    next: usize,
    len: usize,
}

impl Frames {
    const fn new() -> Self {
        Self {
            build_ms: [0.0; FRAME_WINDOW],
            interval_ms: [0.0; FRAME_WINDOW],
            next: 0,
            len: 0,
        }
    }

    /// Record one frame.
    ///
    /// `build_ms` is how long the frame's work took; `interval_ms` is wall time
    /// since the previous frame. The caller supplies both because this crate
    /// has no clock — see the note in `Cargo.toml` about `Instant` on wasm.
    pub fn record(&mut self, build_ms: f32, interval_ms: f32) {
        self.build_ms[self.next] = build_ms;
        self.interval_ms[self.next] = interval_ms;
        self.next = (self.next + 1) % FRAME_WINDOW;
        self.len = (self.len + 1).min(FRAME_WINDOW);
    }

    /// Summarise the window. Allocates and sorts — sampling callers only.
    pub fn stats(&self) -> FrameStats {
        if self.len == 0 {
            return FrameStats::default();
        }
        let mut build: Vec<f32> = self.chronological(&self.build_ms);
        let interval_mean =
            self.chronological(&self.interval_ms).iter().sum::<f32>() / self.len as f32;

        build.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f32| build[(((build.len() - 1) as f32) * q).round() as usize];

        FrameStats {
            samples: self.len,
            build_p50_ms: at(0.50),
            build_p95_ms: at(0.95),
            build_max_ms: build[build.len() - 1],
            interval_mean_ms: interval_mean,
            // A zero interval is what a single recorded frame looks like, and
            // dividing by it would put `inf` on screen.
            fps: if interval_mean > 0.0 {
                1000.0 / interval_mean
            } else {
                0.0
            },
        }
    }

    /// Build times oldest-first, for a sparkline.
    pub fn build_history(&self) -> Vec<f64> {
        self.chronological(&self.build_ms)
            .into_iter()
            .map(f64::from)
            .collect()
    }

    /// Unwrap the ring into oldest-first order.
    fn chronological(&self, buf: &[f32; FRAME_WINDOW]) -> Vec<f32> {
        if self.len < FRAME_WINDOW {
            buf[..self.len].to_vec()
        } else {
            let (tail, head) = buf.split_at(self.next);
            head.iter().chain(tail.iter()).copied().collect()
        }
    }
}

/// A summary of the frame window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// How many frames the summary is over. Zero means nothing recorded yet —
    /// distinct from "recorded, and free", which the other fields cannot say.
    pub samples: usize,
    /// Typical frame build cost.
    pub build_p50_ms: f32,
    /// The slow tail. This is the number a reader feels as jank; the median
    /// stays flat while it climbs.
    pub build_p95_ms: f32,
    /// The single worst frame in the window.
    pub build_max_ms: f32,
    /// Mean wall time between frames.
    pub interval_mean_ms: f32,
    /// Frames per second, derived from `interval_mean_ms`. On a reactive UI
    /// this is a cadence, not a score — see the crate docs.
    pub fps: f32,
}

impl FrameStats {
    /// The slow tail as a fraction of the 60 Hz budget. `1.0` means p95 frames
    /// alone consume the entire budget, leaving nothing for paint.
    pub fn budget_frac(&self) -> f32 {
        self.build_p95_ms / BUDGET_60HZ_MS
    }
}

static FRAMES: Mutex<Frames> = Mutex::new(Frames::new());

/// Record a frame into the global ring.
///
/// A poisoned lock is dropped silently: instrumentation must never be the
/// reason a frontend stops drawing.
pub fn record_frame(build_ms: f32, interval_ms: f32) {
    if let Ok(mut f) = FRAMES.lock() {
        f.record(build_ms, interval_ms);
    }
}

/// Read the global frame ring. Sampling callers only — this allocates.
pub fn frame_stats() -> FrameStats {
    FRAMES
        .lock()
        .map(|f| f.stats())
        .unwrap_or_else(|_| FrameStats::default())
}

/// Build-time history from the global ring, oldest first. Allocates.
pub fn frame_history() -> Vec<f64> {
    FRAMES.lock().map(|f| f.build_history()).unwrap_or_default()
}

// ─── Timelines ───────────────────────────────────────────────────────────────

/// One stage of a multi-step operation, and what it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// What the stage was. Borrowed from the caller's own vocabulary — a cart
    /// phase, a request leg — because this crate has no idea what the stages
    /// of anybody's operation are.
    pub label: String,
    pub ms: f64,
}

/// How long each stage of ONE operation took.
///
/// The third question after "what does a frame cost" and "what work is
/// outstanding": **where did a multi-step operation spend its time**. A cart
/// build that takes four seconds is only actionable once you know which of
/// gather / build / sign / submit the four seconds were in.
///
/// Like [`Frames`], the caller passes the milliseconds in — this crate holds no
/// clock (see the note in `Cargo.toml` about `Instant` on wasm). A caller that
/// reads a real clock gets real spans; a test that passes the numbers it chose
/// gets exactly those, which is the point: a timing assertion should not depend
/// on how fast the machine running it happens to be.
///
/// Monotonic in, monotonic out: [`Timeline::mark`] saturates at zero rather
/// than recording a negative span, so a clock that jumps backwards produces a
/// useless-but-harmless `0.0` instead of a duration that cannot exist.
#[derive(Debug, Clone)]
pub struct Timeline {
    started_ms: f64,
    last_ms: f64,
    spans: Vec<Span>,
}

impl Timeline {
    /// Start a timeline at the caller's current reading.
    pub fn start(now_ms: f64) -> Self {
        Self {
            started_ms: now_ms,
            last_ms: now_ms,
            spans: Vec::new(),
        }
    }

    /// Close the stage that was running and label it.
    ///
    /// The span recorded is from the previous mark (or the start) to `now_ms`.
    pub fn mark(&mut self, label: impl Into<String>, now_ms: f64) {
        let ms = (now_ms - self.last_ms).max(0.0);
        self.last_ms = now_ms;
        self.spans.push(Span {
            label: label.into(),
            ms,
        });
    }

    /// The stages closed so far, oldest first.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Start to the last mark. Excludes any stage still running — an operation
    /// reports what it has finished, not what it hopes to.
    pub fn total_ms(&self) -> f64 {
        (self.last_ms - self.started_ms).max(0.0)
    }

    /// The most expensive stage, when there is one. What to read first.
    pub fn slowest(&self) -> Option<&Span> {
        self.spans
            .iter()
            .max_by(|a, b| a.ms.partial_cmp(&b.ms).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// One line for a log: `total=1520.0ms gather=120.0 build=1200.0 sign=200.0`.
    pub fn summary(&self) -> String {
        let mut out = format!("total={:.1}ms", self.total_ms());
        for s in &self.spans {
            out.push_str(&format!(" {}={:.1}", s.label, s.ms));
        }
        out
    }
}

// ─── Memory ──────────────────────────────────────────────────────────────────

/// How much memory the process is holding, where that can be answered honestly.
pub mod mem {
    /// wasm grows its linear memory in 64 KiB pages.
    pub const WASM_PAGE_BYTES: u64 = 64 * 1024;

    /// Bytes of wasm linear memory currently allocated to the module, or `None`
    /// off wasm.
    ///
    /// # This is a high-water mark, not live usage
    ///
    /// wasm linear memory **never shrinks**. Freeing a 200 MB artifact returns
    /// it to the Rust allocator, which keeps it for the next allocation; the
    /// module's page count stays where it was. So this number only ever goes
    /// up, and a flat line after a big load means "we are reusing what we
    /// took", not "we leaked".
    ///
    /// That still makes it the right number to watch, because it is the one the
    /// browser charges us for and the one that gets a tab killed on a phone.
    /// It just cannot be read as a leak detector.
    ///
    /// Deliberately not `performance.memory`: that is the *JavaScript* heap,
    /// which is not where a Rust frontend's data lives, and it is Chrome-only
    /// and quantised into buckets on top.
    pub fn linear_memory_bytes() -> Option<u64> {
        #[cfg(target_arch = "wasm32")]
        {
            Some(core::arch::wasm32::memory_size(0) as u64 * WASM_PAGE_BYTES)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard is the whole contract: work in flight counts, work that
    /// returned does not — including down a path that returned early.
    #[test]
    fn a_gauge_counts_only_what_is_still_open() {
        static G: Gauge = Gauge::new("test/open");
        assert_eq!(G.current(), 0);
        let a = G.enter();
        let b = G.enter();
        assert_eq!(G.current(), 2);
        drop(a);
        assert_eq!(G.current(), 1);
        drop(b);
        assert_eq!(G.current(), 0);
    }

    /// `total` and `peak` survive the work finishing — which is the point of
    /// keeping them. A sampling reader looking only at `current` sees a system
    /// that was never busy.
    #[test]
    fn peak_and_total_outlive_the_work() {
        static G: Gauge = Gauge::new("test/peak");
        {
            let _a = G.enter();
            let _b = G.enter();
            let _c = G.enter();
        }
        assert_eq!(G.current(), 0, "all three closed");
        assert_eq!(G.peak(), 3, "but it got three deep");
        assert_eq!(G.total(), 3, "and three went through");
    }

    /// Registration is idempotent, so a hot chokepoint does not grow the
    /// registry once per call.
    #[test]
    fn a_gauge_registers_exactly_once() {
        static G: Gauge = Gauge::new("test/register-once");
        G.register();
        drop(G.enter());
        drop(G.enter());
        let n = gauges()
            .iter()
            .filter(|g| g.name == "test/register-once")
            .count();
        assert_eq!(n, 1);
    }

    /// An empty window must be distinguishable from a fast one. `samples: 0`
    /// is the only field that can say "nothing recorded".
    #[test]
    fn an_empty_window_reports_no_samples_rather_than_zero_cost() {
        let f = Frames::new();
        let s = f.stats();
        assert_eq!(s.samples, 0);
        assert_eq!(s.fps, 0.0, "and not an infinity from dividing by no time");
    }

    /// The ring must unwrap in time order once it has wrapped, or the sparkline
    /// draws a sawtooth that is an artefact of the buffer rather than the app.
    #[test]
    fn the_ring_unwraps_oldest_first_after_wrapping() {
        let mut f = Frames::new();
        for i in 0..(FRAME_WINDOW + 5) {
            f.record(i as f32, 16.0);
        }
        let h = f.build_history();
        assert_eq!(h.len(), FRAME_WINDOW);
        assert_eq!(h[0], 5.0, "the oldest surviving frame leads");
        assert_eq!(
            h[FRAME_WINDOW - 1],
            (FRAME_WINDOW + 4) as f64,
            "the newest trails"
        );
    }

    /// The tail is the metric that matters, so it must actually track the tail
    /// and not be smoothed into the median.
    #[test]
    fn p95_follows_the_slow_tail_while_the_median_stays_flat() {
        let mut f = Frames::new();
        for _ in 0..FRAME_WINDOW {
            f.record(2.0, 16.0);
        }
        let calm = f.stats();
        for _ in 0..10 {
            f.record(40.0, 16.0);
        }
        let janky = f.stats();

        assert_eq!(calm.build_p50_ms, 2.0);
        assert_eq!(janky.build_p50_ms, 2.0, "the median has not moved");
        assert!(
            janky.build_p95_ms > calm.build_p95_ms,
            "but the tail has: {} -> {}",
            calm.build_p95_ms,
            janky.build_p95_ms
        );
        assert_eq!(janky.build_max_ms, 40.0);
    }

    /// FPS comes from the interval, and build time from the build — a frontend
    /// idling at 4 Hz on cheap frames is healthy, and must not read as jank.
    #[test]
    fn a_cheap_frame_at_a_slow_cadence_is_not_jank() {
        let mut f = Frames::new();
        for _ in 0..FRAME_WINDOW {
            f.record(0.9, 250.0);
        }
        let s = f.stats();
        assert_eq!(s.fps, 4.0, "rendering four times a second");
        assert_eq!(s.build_p95_ms, 0.9, "and each one is nearly free");
        assert!(s.budget_frac() < 0.1, "nowhere near the frame budget");
    }

    /// Off wasm there is no honest answer, and the type says so rather than
    /// inventing a zero.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn memory_is_absent_rather_than_wrong_off_wasm() {
        assert_eq!(mem::linear_memory_bytes(), None);
    }

    /// The whole reason the caller passes the milliseconds in: a timing test
    /// asserts the numbers it chose, not whatever the machine did.
    #[test]
    fn spans_are_the_gaps_between_marks() {
        let mut t = Timeline::start(1_000.0);
        t.mark("gather", 1_120.0);
        t.mark("build", 2_320.0);
        t.mark("sign", 2_520.0);

        assert_eq!(
            t.spans(),
            &[
                Span {
                    label: "gather".into(),
                    ms: 120.0
                },
                Span {
                    label: "build".into(),
                    ms: 1_200.0
                },
                Span {
                    label: "sign".into(),
                    ms: 200.0
                },
            ]
        );
        assert_eq!(t.total_ms(), 1_520.0);
        assert_eq!(t.slowest().map(|s| s.label.as_str()), Some("build"));
        assert_eq!(
            t.summary(),
            "total=1520.0ms gather=120.0 build=1200.0 sign=200.0"
        );
    }

    /// An unfinished stage is not counted. An operation reports what it has
    /// completed, so a timeline read mid-flight cannot claim time it is still
    /// spending.
    #[test]
    fn a_running_stage_is_not_in_the_total() {
        let mut t = Timeline::start(0.0);
        t.mark("gather", 100.0);
        assert_eq!(t.total_ms(), 100.0, "the open stage contributes nothing");
        assert_eq!(t.spans().len(), 1);
    }

    /// `performance.now()` is monotonic; `Date.now()` is not, and a host may
    /// pass either. A backwards jump must not produce a negative duration.
    #[test]
    fn a_backwards_clock_yields_zero_not_a_negative_span() {
        let mut t = Timeline::start(500.0);
        t.mark("gather", 400.0);
        assert_eq!(t.spans()[0].ms, 0.0);
        assert_eq!(t.total_ms(), 0.0);
    }
}
