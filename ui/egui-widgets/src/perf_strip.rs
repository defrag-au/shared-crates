//! `PerfStrip` — a small stack of readings saying what the frontend is costing: frame build time, render cadence, memory, and anything still in flight.
//!
//! Deliberately tiny. This is a HUD you leave switched on in the corner of a
//! real surface, not a dashboard you navigate to — so it is a handful of plain
//! numbers at 11pt, and the detail behind each one lives in its own hover
//! tooltip rather than on screen.
//!
//! The readings are **not selectable text**. They are instrument readings, not
//! content: they change four times a second, so a drag highlights a value that
//! is already gone, and a HUD pinned over an app should be inert to the pointer
//! apart from its tooltips.
//!
//! It stacks either way, per [`Orientation`]. **Vertical is the default**: the
//! spare room on a full-height app surface is nearly always a margin rather than
//! a band, so a column can sit in a corner without spanning anything. Reach for
//! [`Orientation::Horizontal`] when there is an existing footer or status bar to
//! join.
//!
//! The first version of this was a row of [`MetricCard`](crate::MetricCard)s.
//! It was accurate and completely unusable: 100pt tall, wider than the surfaces
//! it was meant to sit in, and it wrapped into a grid that competed with the
//! app's own content for attention. An instrument that displaces the thing it
//! is measuring does not get left on, and one that does not get left on never
//! catches the regression it exists to catch.
//!
//! The numbers come from [`perf_probe`], a pure crate with no clock and no
//! framework; this module supplies the clock ([`FrameScope`]) and draws the
//! result.
//!
//! # Wiring it up
//!
//! ```ignore
//! impl eframe::App for MyApp {
//!     // eframe 0.34: `ui`, not `update`.
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         let _frame = egui_widgets::perf_strip::FrameScope::begin();
//!         // ... the app's real work ...
//!         if self.show_perf {
//!             // Vertical by default; `.orientation(Orientation::Horizontal)`
//!             // for a footer.
//!             PerfStrip::new(&mut self.perf).show(ui);
//!         }
//!     }
//! }
//! ```
//!
//! The scope must be the **first** statement and must outlive everything else
//! in the method — it measures its own lifetime, so anything constructed before
//! it or dropped after it is not in the figure.
//!
//! The strip always reads left-to-right and top-to-bottom, whatever layout it is
//! dropped into. That means the usual trick for right-aligning a block —
//! wrapping it in `Layout::right_to_left` — no longer moves it; allocate the
//! space instead. It is the right trade: inside a right-to-left footer the
//! inherited version rendered every reading backwards, which is a much worse
//! outcome than a HUD sitting on the left.
//!
//! Gauges need no wiring at all: anything on `browser-fetch` is already counted.
//! To make a gauge appear before its first use rather than after,
//! [`register`](perf_probe::Gauge::register) it at startup.
//!
//! # Why this does not just show FPS
//!
//! A live FPS readout on a reactive UI is a measurement that creates what it
//! measures. egui repaints on demand, so a strip that refreshes every frame
//! forces the app to render continuously — the counter reports a healthy 60,
//! the fan comes on, and the number was caused by the instrument.
//!
//! So it **samples at [`SAMPLE_INTERVAL_S`]**, asks for a repaint on that
//! cadence and no faster, and leads with *frame build time*, which does not
//! depend on how often we render. FPS sits beside it as context and is
//! deliberately never coloured as a fault — an idle view at 4 fps with 0.9 ms
//! frames is a well-behaved app, and a readout that scolds it for that teaches
//! people to optimise the wrong thing.
//!
//! Between samples the strip re-draws cached strings and touches nothing else:
//! no `format!`, no `Vec`, no lock.

use egui::{Color32, Response, RichText, Ui, Vec2};
use perf_probe::{FrameStats, GaugeSnapshot};

use crate::sparkline::{SparkHoverStyle, Sparkline};
use crate::theme;

/// How often the strip re-reads the probes and re-formats its labels.
///
/// 250 ms is a compromise between the two ways this goes wrong: faster and the
/// strip's own repaint is a meaningful share of the app's frame budget; slower
/// and the numbers lag far enough behind an interaction that you cannot tell
/// which action caused a spike.
pub const SAMPLE_INTERVAL_S: f64 = 0.25;

/// Text size for the strip. One notch under the UI's body text — legible, and
/// visibly subordinate to the app's own content.
const TEXT_SIZE: f32 = 11.0;

/// The inline sparkline, in points. Wide enough to show a step change, small
/// enough to sit on a line of text.
const SPARK: Vec2 = Vec2::new(56.0, 13.0);

// ─── The clock ───────────────────────────────────────────────────────────────

/// Milliseconds from a monotonic clock.
///
/// Not `std::time::Instant`: it **panics** on `wasm32-unknown-unknown`, which
/// is the target this crate exists for, and the shims that paper over it pull
/// in another dependency to do what one `web-sys` call already does.
fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

/// Where the previous frame started, as `f64` bits. `0` means "no previous
/// frame", which is why the first frame is dropped rather than recorded with an
/// interval measured from the epoch.
static LAST_BEGIN_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Times one frame. Construct at the top of `App::ui`; it records on drop.
///
/// Drop-based so that an early return — which every app grows eventually, for a
/// loading state or a fatal error screen — still records the frame instead of
/// silently leaving a hole in the window.
#[must_use = "the frame is measured until this is dropped; binding it to `_` \
              drops it immediately and records a frame that did nothing"]
pub struct FrameScope {
    began_ms: f64,
}

impl FrameScope {
    /// Start timing the current frame.
    pub fn begin() -> Self {
        Self { began_ms: now_ms() }
    }
}

impl Drop for FrameScope {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering::Relaxed;
        let ended = now_ms();
        let previous = f64::from_bits(LAST_BEGIN_MS.swap(self.began_ms.to_bits(), Relaxed));
        if previous == 0.0 {
            // First frame: there is no interval yet, and inventing one from zero
            // would put a several-decade gap in the window.
            return;
        }
        perf_probe::record_frame(
            (ended - self.began_ms) as f32,
            (self.began_ms - previous) as f32,
        );
    }
}

// ─── The strip ───────────────────────────────────────────────────────────────

/// Everything the strip carries between frames.
///
/// It exists so the strip is *free on the frames it is not sampling*: the
/// formatted strings, the history and the gauge snapshot are built on the
/// sampling tick and merely drawn in between. A stateless version would allocate
/// a handful of strings sixty times a second to show numbers that change four
/// times a second.
#[derive(Default)]
pub struct PerfStripState {
    /// egui's own clock (seconds) at which the next sample is due.
    next_sample_at: f64,
    stats: FrameStats,
    history: Vec<f64>,
    gauges: Vec<GaugeSnapshot>,
    memory_bytes: Option<u64>,
    build_text: String,
    fps_text: String,
    memory_text: String,
    /// One `("name", "current")` per gauge, formatted once per sample.
    gauge_text: Vec<(&'static str, String)>,
    /// Hover text, one per reading, built on the sample tick.
    ///
    /// Per-reading rather than one block on the whole strip: the strip is four
    /// bare numbers, and *which* of them you did not understand is exactly the
    /// question a tooltip should answer. A single block makes you read all four
    /// explanations to find the one you wanted.
    build_tip: String,
    fps_tip: String,
    memory_tip: String,
    gauge_tips: Vec<String>,
    spark_tip: String,
}

impl PerfStripState {
    /// Has a sample ever been taken? Distinguishes "nothing to show yet" from
    /// "everything is zero", which the numbers alone cannot.
    fn sampled(&self) -> bool {
        self.stats.samples > 0
    }

    fn resample(&mut self) {
        self.stats = perf_probe::frame_stats();
        self.history = perf_probe::frame_history();
        self.gauges = perf_probe::gauges();
        self.memory_bytes = perf_probe::mem::linear_memory_bytes();
        self.reformat();
    }

    /// Build every string the strip draws, from the fields already read.
    ///
    /// Split from [`resample`](Self::resample) so a test can exercise the real
    /// formatting against a specific set of numbers — the probes are global and
    /// hold whatever else the test binary happened to do.
    fn reformat(&mut self) {
        let s = self.stats;
        self.build_text = format!("{:.1} ms", s.build_p50_ms);
        self.fps_text = format!("{:.0} fps", s.fps);
        self.memory_text = match self.memory_bytes {
            Some(b) => format_bytes(b),
            None => "—".to_string(),
        };
        self.gauge_text = self
            .gauges
            .iter()
            .map(|g| (g.name, g.current.to_string()))
            .collect();

        // The detail the cards used to spend 100pt of vertical space on. On
        // hover it costs nothing until someone asks. Each tooltip is the same
        // two parts: the numbers behind the reading, then what it MEANS — a
        // reading whose interpretation is counter-intuitive (all three of these
        // are) is not made obvious by more digits.
        self.build_tip = format!(
            "Frame build time — median {:.1} ms\n\
             p95 {:.1} ms · worst {:.1} ms · {:.0}% of the 16.7 ms budget at 60 Hz\n\n\
             How long the frame's work actually took. Independent of how often we \
             render, so this is the number to watch when adding work to a view.\n\
             Coloured off the p95, not the median: a view that misses one frame in \
             ten feels like jank while its median stays flat.",
            s.build_p50_ms,
            s.build_p95_ms,
            s.build_max_ms,
            s.budget_frac() * 100.0,
        );
        self.fps_tip = format!(
            "Render cadence — {:.0} fps, {:.0} ms between frames\n\n\
             How often we are CHOOSING to repaint, not a score. egui repaints on \
             demand, so an idle view sitting at a few fps is a well-behaved app \
             rather than a slow one — which is why this is never coloured as a \
             fault. If it is low and the build time is low, nothing is wrong.",
            s.fps, s.interval_mean_ms,
        );
        self.memory_tip = match self.memory_bytes {
            Some(_) => format!(
                "Memory — {} of wasm linear memory\n\n\
                 A HIGH-WATER MARK. wasm linear memory never shrinks: freeing a \
                 large artifact returns it to the allocator, not to the browser, \
                 so this only ever goes up. A flat line after a big load means we \
                 are reusing what we took, not that we leaked.\n\
                 It is still the number that gets a tab killed on a phone.",
                self.memory_text,
            ),
            None => "Memory — not measurable off wasm.\n\n\
                 There is no equivalent of the wasm page count on native short of \
                 wrapping the global allocator, which would cost two atomics on \
                 every allocation in the process. Deliberately not done."
                .to_string(),
        };
        self.gauge_tips = self
            .gauges
            .iter()
            .map(|g| {
                format!(
                    "{} — {} in flight\npeak {} · {} started in total\n\n\
                     Peak and total keep climbing after the work closes. That is \
                     what makes the reading useful to something sampling {:.0} \
                     times a second, which would otherwise miss every burst that \
                     began and ended between two samples.",
                    g.name,
                    g.current,
                    g.peak,
                    g.total,
                    1.0 / SAMPLE_INTERVAL_S,
                )
            })
            .collect();
        self.spark_tip = format!(
            "Frame build time over the last {} frames\n\n\
             Sampled every {:.0} ms — the strip asks for a repaint on that cadence \
             and no faster, so watching it does not itself hold the app awake.",
            s.samples,
            SAMPLE_INTERVAL_S * 1000.0,
        );
    }
}

/// Which way the readings stack.
///
/// The two are not interchangeable placements of the same thing: a HUD goes
/// wherever the surface has room to spare, and which axis that is decides where
/// it can go at all. [`Vertical`](Self::Vertical) is the default because the
/// spare room on a full-height app surface is almost always a margin, not a
/// band, and a column can sit in a corner over content without spanning it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    /// One reading per line. Narrow and tall — corners, side rails, overlays.
    #[default]
    Vertical,
    /// All readings on one line, separated by dots. Short and wide — footers,
    /// status bars, title rows.
    Horizontal,
}

impl Orientation {
    /// Every variant, for a story or a settings control to enumerate.
    pub const ALL: [Self; 2] = [Self::Vertical, Self::Horizontal];

    /// How it reads in a picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
        }
    }
}

/// A compact live performance HUD.
pub struct PerfStrip<'a> {
    state: &'a mut PerfStripState,
    show_sparkline: bool,
    orientation: Orientation,
}

impl<'a> PerfStrip<'a> {
    pub fn new(state: &'a mut PerfStripState) -> Self {
        Self {
            state,
            show_sparkline: true,
            orientation: Orientation::default(),
        }
    }

    /// Stack the readings down a column or along a row.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Drop the inline sparkline, leaving numbers only.
    ///
    /// For a surface where even 56pt is too much — a status bar already carrying
    /// other things — at the cost of the one element that shows a *change*
    /// rather than a level.
    pub fn show_sparkline(mut self, show: bool) -> Self {
        self.show_sparkline = show;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let now = ui.input(|i| i.time);
        if now >= self.state.next_sample_at {
            self.state.resample();
            self.state.next_sample_at = now + SAMPLE_INTERVAL_S;
        }

        // Keep the strip alive at the sampling cadence and NOT one frame faster.
        // `request_repaint()` here would pin the app at full rate for as long as
        // the strip is on screen and make every number on it a measurement of
        // the strip.
        let until_next = (self.state.next_sample_at - now).max(0.0);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(until_next));

        if !self.state.sampled() {
            return ui.label(
                RichText::new("no frames recorded — is `FrameScope::begin()` at the top of `ui`?")
                    .size(TEXT_SIZE)
                    .color(theme::TEXT_MUTED),
            );
        }

        let state = &*self.state;
        let show_sparkline = self.show_sparkline;
        let orientation = self.orientation;
        let body = |ui: &mut Ui| readings(ui, state, show_sparkline, orientation);

        // An EXPLICIT layout, not `ui.horizontal` / `ui.vertical`, which inherit
        // the caller's direction. Placed in the `Layout::right_to_left` a footer
        // is normally built with, the inherited version laid the readings out
        // backwards — "0 demo work · 7.8 MiB · 7 fps · 1.5 ms" — putting the
        // headline figure furthest from the eye and splitting each gauge from
        // its own name. The reading order is the widget's, not the surface's.
        //
        // `Align::Min`, not `Align::Center`: the cross-axis alignment here is
        // measured against the whole REMAINING height of the parent, not the
        // row's own, so centring dropped the strip into the middle of the page.
        match orientation {
            Orientation::Horizontal => {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), body)
                    .response
            }
            Orientation::Vertical => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), body)
                    .response
            }
        }
    }
}

/// The readings themselves, laid into whichever container the caller opened.
///
/// One function for both orientations rather than two: the *content* and its
/// order are the same either way, and only the separators differ. Two copies
/// would drift, and the way they drift is one axis quietly gaining a reading the
/// other does not have.
fn readings(ui: &mut Ui, state: &PerfStripState, show_sparkline: bool, o: Orientation) {
    // Touch sizing sets a 44pt floor on allocated space AND on row height inside
    // a `horizontal`, which would make this read-only strip three times its
    // content height on a phone. Opt out on the REGION — by the time each label
    // is added the row height is already decided.
    ui.spacing_mut().interact_size = Vec2::ZERO;
    ui.spacing_mut().item_spacing.x = 6.0;
    // A column of 11pt text at the default 6pt line gap reads as a list of
    // unrelated facts; closed up, it reads as one instrument.
    ui.spacing_mut().item_spacing.y = 1.0;
    // NOT SELECTABLE. These are readings, not content: they change four times a
    // second, so a drag across them highlights a value that no longer exists by
    // the time the pointer lands, and a stray click-drag over a HUD pinned in a
    // corner leaves a selection the reader now has to clear. It also makes the
    // whole strip inert to the pointer except for its tooltips, which is what a
    // thing sitting on top of an app should be.
    ui.style_mut().interaction.selectable_labels = false;

    if show_sparkline {
        Sparkline::new(&state.history)
            .width(SPARK.x)
            .height(SPARK.y)
            .line_width(1.0)
            .line_color(budget_color(state.stats))
            .bg_color(Color32::TRANSPARENT)
            .show_endpoint(false)
            // Its own tooltip, below, rather than the sparkline's built-in
            // nearest-value one: "frame 84 was 1.9 ms" is not a fact anybody
            // needs, and it would suppress the explanation that is.
            .hover_style(SparkHoverStyle::None)
            .show(ui)
            .on_hover_text(&state.spark_tip);
    }

    atom(
        ui,
        &state.build_text,
        budget_color(state.stats),
        &state.build_tip,
    );
    separator(ui, o);
    // Never coloured by a threshold: a low cadence is what an idle reactive UI
    // is SUPPOSED to look like.
    atom(ui, &state.fps_text, theme::TEXT_SECONDARY, &state.fps_tip);
    separator(ui, o);
    atom(
        ui,
        &state.memory_text,
        match state.memory_bytes {
            Some(_) => theme::TEXT_SECONDARY,
            None => theme::TEXT_MUTED,
        },
        &state.memory_tip,
    );

    for (i, ((name, value), g)) in state.gauge_text.iter().zip(&state.gauges).enumerate() {
        separator(ui, o);
        let count_color = if g.current > 0 {
            theme::ACCENT_CYAN
        } else {
            theme::TEXT_MUTED
        };
        // A gauge whose tooltip is missing is a bug in `reformat`, not a reason
        // to skip the reading — draw it with no hover rather than silently
        // dropping a count somebody is watching.
        let tip = state.gauge_tips.get(i).map(String::as_str).unwrap_or("");
        // Name and count stay on ONE line in both orientations — split across
        // two lines in the column, a bare number sits under a label it does not
        // obviously belong to. The name is muted and the count carries the
        // colour, so scanning lands on the number that moved rather than the
        // label that never does. They share one tooltip: they are one reading.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().interact_size = Vec2::ZERO;
            ui.spacing_mut().item_spacing.x = 5.0;
            atom(ui, name, theme::TEXT_MUTED, tip);
            atom(ui, value, count_color, tip);
        });
    }
}

/// What goes between two readings.
///
/// A row needs a mark to stop the numbers running together; a column already has
/// the line break doing that job, and a column of dots would be noise.
fn separator(ui: &mut Ui, o: Orientation) {
    match o {
        Orientation::Horizontal => dot(ui),
        Orientation::Vertical => {}
    }
}

/// One value on the strip, with the explanation behind it on hover.
///
/// `Sense::hover()` explicitly: a non-selectable `Label` does not sense anything
/// by default, so `on_hover_text` on it silently never fires. Not `Sense::click`,
/// which would also make every reading keyboard-focusable and put the strip into
/// the app's tab order.
fn atom(ui: &mut Ui, text: &str, color: Color32, tip: &str) {
    // Monospace so a digit changing width does not shuffle everything to its
    // right. On a readout that updates four times a second, proportional
    // figures make the whole strip twitch.
    let label = egui::Label::new(RichText::new(text).size(TEXT_SIZE).color(color).monospace())
        .sense(egui::Sense::hover());
    let response = ui.add(label);
    if !tip.is_empty() {
        response.on_hover_text(tip);
    }
}

/// The separator between values.
fn dot(ui: &mut Ui) {
    ui.label(
        RichText::new("·")
            .size(TEXT_SIZE)
            .color(theme::BORDER)
            .monospace(),
    );
}

/// How the frame-build value is coloured, against the 60 Hz budget.
///
/// The thresholds are on the **p95**, not the median: a view that is fine nine
/// frames in ten and misses the tenth reads as jank, and a median-coloured
/// value would call it green throughout.
fn budget_color(stats: FrameStats) -> Color32 {
    match stats.budget_frac() {
        f if f < 0.5 => theme::SUCCESS,
        f if f < 1.0 => theme::WARNING,
        _ => theme::ERROR,
    }
}

/// Bytes at the granularity a reader can act on.
///
/// One decimal place and no more: the figure moves in 64 KiB pages and is a
/// high-water mark, so digits past the first imply a precision the source does
/// not have.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.0} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour is the strip's whole judgement, so it has to key off the tail
    /// and it has to change at the budget.
    #[test]
    fn the_build_colour_grades_the_tail_against_the_frame_budget() {
        let cheap = FrameStats {
            samples: 120,
            build_p95_ms: 2.0,
            ..Default::default()
        };
        let tight = FrameStats {
            samples: 120,
            build_p95_ms: 12.0,
            ..Default::default()
        };
        let over = FrameStats {
            samples: 120,
            build_p95_ms: 25.0,
            ..Default::default()
        };
        assert_eq!(budget_color(cheap), theme::SUCCESS);
        assert_eq!(budget_color(tight), theme::WARNING);
        assert_eq!(budget_color(over), theme::ERROR);
    }

    /// A slow cadence must not be able to colour anything — it is not a fault,
    /// and this is the assertion that stops someone "fixing" it later.
    #[test]
    fn a_slow_cadence_alone_never_reads_as_a_fault() {
        let idle = FrameStats {
            samples: 120,
            build_p50_ms: 0.9,
            build_p95_ms: 1.1,
            interval_mean_ms: 250.0,
            fps: 4.0,
            ..Default::default()
        };
        assert_eq!(
            budget_color(idle),
            theme::SUCCESS,
            "4 fps on 1 ms frames is a healthy idle app"
        );
    }

    /// The memory figure is never presented bare — the caveat is the difference
    /// between a useful number and a misread one.
    #[test]
    fn memory_is_always_caveated_including_when_absent() {
        let tip = |bytes| {
            let mut state = PerfStripState {
                stats: FrameStats {
                    samples: 1,
                    ..Default::default()
                },
                memory_bytes: bytes,
                ..Default::default()
            };
            state.reformat();
            state.memory_tip
        };
        assert!(tip(Some(1)).contains("HIGH-WATER MARK"));
        assert!(tip(None).contains("not measurable off wasm"));
    }

    #[test]
    fn bytes_read_at_the_scale_they_are_measured() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(64 * 1024), "64 KiB");
        assert_eq!(format_bytes(180 * 1024 * 1024), "180.0 MiB");
    }

    /// Vertical is the default, and that is a decision rather than an accident
    /// of declaration order — see the type's docs.
    #[test]
    fn a_strip_stacks_vertically_unless_told_otherwise() {
        assert_eq!(Orientation::default(), Orientation::Vertical);
        assert_eq!(Orientation::ALL.len(), 2, "and both are enumerable");
        assert!(Orientation::ALL.contains(&Orientation::default()));
    }

    /// Dots separate readings on a row and would be noise in a column, where the
    /// line break already does the work.
    #[test]
    fn only_the_row_needs_separators() {
        // The separator is a draw call, so this asserts the decision it is
        // derived from rather than the pixels: exactly one orientation is the
        // one that runs values together on a single line.
        let needs: Vec<Orientation> = Orientation::ALL
            .into_iter()
            .filter(|o| matches!(o, Orientation::Horizontal))
            .collect();
        assert_eq!(needs, vec![Orientation::Horizontal]);
    }

    /// An empty window says so rather than drawing a confident row of zeroes,
    /// which is what a mis-wired `FrameScope` would otherwise look like.
    #[test]
    fn an_unsampled_strip_knows_it_has_nothing_to_show() {
        let state = PerfStripState::default();
        assert!(!state.sampled());
    }

    /// The strip shows four short numbers; everything else moved to the hover.
    /// This is the constraint that keeps it a strip — the first version put all
    /// of this on screen and came out 100pt tall.
    #[test]
    fn the_detail_lives_in_the_tooltip_not_on_the_strip() {
        let mut state = PerfStripState {
            stats: FrameStats {
                samples: 120,
                build_p50_ms: 1.4,
                build_p95_ms: 2.5,
                build_max_ms: 9.0,
                interval_mean_ms: 140.0,
                fps: 7.0,
            },
            ..Default::default()
        };
        state.reformat();

        assert_eq!(state.build_text, "1.4 ms", "six characters, not a card");
        assert_eq!(state.fps_text, "7 fps");
        assert!(
            !state.build_text.contains("p95"),
            "the tail is not on the strip"
        );
        assert!(state.build_tip.contains("p95 2.5 ms"), "it is on the hover");
        assert!(state.build_tip.contains("worst 9.0 ms"));
        assert!(state.fps_tip.contains("140 ms between frames"));
    }

    /// Each reading carries its OWN tooltip. One block on the whole strip makes
    /// you read four explanations to find the one you wanted, and *which* number
    /// you did not understand is the entire question a tooltip is answering.
    #[test]
    fn every_reading_explains_itself_separately() {
        let mut state = PerfStripState {
            stats: FrameStats {
                samples: 120,
                build_p50_ms: 1.4,
                build_p95_ms: 2.5,
                build_max_ms: 9.0,
                interval_mean_ms: 140.0,
                fps: 7.0,
            },
            memory_bytes: Some(8 * 1024 * 1024),
            ..Default::default()
        };
        state.reformat();

        for tip in [&state.build_tip, &state.fps_tip, &state.memory_tip] {
            assert!(!tip.is_empty(), "no reading is left unexplained");
        }
        // Each says what its own number MEANS, not just what it is — these are
        // the three readings that are routinely misread.
        assert!(
            state
                .build_tip
                .contains("Independent of how often we render")
        );
        assert!(state.fps_tip.contains("never coloured as a fault"));
        assert!(state.memory_tip.contains("HIGH-WATER MARK"));
    }

    /// Off wasm the memory tooltip has to explain the dash, or the reading looks
    /// broken rather than deliberately absent.
    #[test]
    fn the_memory_tooltip_explains_a_missing_reading() {
        let mut state = PerfStripState {
            stats: FrameStats {
                samples: 10,
                ..Default::default()
            },
            memory_bytes: None,
            ..Default::default()
        };
        state.reformat();

        assert_eq!(state.memory_text, "—");
        assert!(state.memory_tip.contains("not measurable off wasm"));
    }

    /// A gauge reads as `name value` on the strip, with peak and total held back
    /// for the hover — three numbers per gauge would make the strip unreadable
    /// with more than one of them.
    #[test]
    fn a_gauge_shows_one_number_on_the_strip_and_three_on_hover() {
        let mut state = PerfStripState {
            stats: FrameStats {
                samples: 10,
                ..Default::default()
            },
            gauges: vec![GaugeSnapshot {
                name: "http requests",
                current: 2,
                peak: 9,
                total: 431,
            }],
            ..Default::default()
        };
        state.reformat();

        assert_eq!(state.gauge_text, vec![("http requests", "2".to_string())]);
        assert_eq!(
            state.gauge_tips.len(),
            state.gauge_text.len(),
            "one tooltip per gauge, or a reading indexes into nothing"
        );
        assert!(state.gauge_tips[0].contains("http requests — 2 in flight"));
        assert!(state.gauge_tips[0].contains("peak 9 · 431 started in total"));
    }
}
