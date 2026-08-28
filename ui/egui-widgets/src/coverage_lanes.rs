//! `CoverageLanes` — was it up, was it down, or was nobody looking, per entity,
//! on the shared spine.
//!
//! `activity_lanes` answers "when did this act", where a lane is empty because
//! nothing happened. This one answers a question that has **three** answers, and
//! the third is the whole point: *observed producing*, *observed idle*, and
//! **unobserved**. A monitored fleet that reports nothing for an hour is a
//! different claim from an hour we failed to fetch, and a view that renders them
//! alike turns every ingest outage into recorded downtime.
//!
//! So the lane's ground state is **unobserved**, and knowledge paints over it.
//! A caller supplies runs only for spans it actually observed; whatever is left
//! stays grey. You cannot assert "idle" by forgetting to mention it — the same
//! reason `flow_ring`'s unexamined class is grey rather than a fourth colour.
//!
//! ## The marks
//!
//! - **Producing** — teal bar from the baseline, height ∝ [`Run::level`]. The
//!   taper as a fleet ramps down reads as a slope, not a cliff.
//! - **Idle** — a flat orange stub at the baseline. Observed zero needs a
//!   *visible* mark; a zero-height bar would be indistinguishable from ground.
//! - **Unobserved** — the ground itself. Recessive grey, no bar.
//!
//! ## What is fixed
//!
//! - **x is the spine's own `TimeScale`**, so a run sits under the ruler date it
//!   happened on and the playhead runs straight through.
//! - **Lane order is the caller's** and is stable for the session — same
//!   object-constancy rule as the ring's seats.
//! - Runs narrower than a pixel are **merged, not dropped**: the merged cell
//!   keeps duration-weighted observed/producing time, so a zoomed-out year does
//!   not silently lose the hour the fleet went dark.
//!
//! Generic over `(entity, runs)` on purpose — mining fleet uptime and indexer
//! block-capture gaps are the same question.

use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::selection::Selection;
use crate::time_spine::{SpineState, TimeScale};

/// What was true of an entity over a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Observed, and producing. Magnitude rides [`Run::level`].
    Producing,
    /// Observed, and producing nothing. **Not** the same as unobserved.
    Idle,
}

/// One observed span. Anything a lane does not cover is unobserved by
/// construction — that is the encoding, not an oversight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    /// Unix seconds, inclusive.
    pub start: i64,
    /// Unix seconds, exclusive.
    pub end: i64,
    pub state: Coverage,
    /// `0.0..=1.0`, normalised by the caller against whatever full output means
    /// for this entity. Ignored for [`Coverage::Idle`].
    pub level: f32,
}

impl Run {
    pub fn producing(start: i64, end: i64, level: f32) -> Self {
        Self {
            start,
            end,
            state: Coverage::Producing,
            level: level.clamp(0.0, 1.0),
        }
    }

    pub fn idle(start: i64, end: i64) -> Self {
        Self {
            start,
            end,
            state: Coverage::Idle,
            level: 0.0,
        }
    }

    fn secs(&self) -> f64 {
        (self.end - self.start).max(0) as f64
    }
}

/// One entity's row.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageLane<'a> {
    pub key: &'a str,
    /// Display name — the caller resolves aliases; the widget never guesses.
    pub name: String,
    /// Observed spans, ascending by `start` and non-overlapping. Gaps are
    /// unobserved.
    pub runs: &'a [Run],
    /// Drawn faint and excluded from the window tally when off.
    pub active: bool,
}

/// Time accounting over the spine's current filter window, across active lanes.
///
/// Summed here so a caller can render "uptime 86.4%" without walking the runs a
/// second time and reaching a different answer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WindowCoverage {
    pub producing_secs: i64,
    pub idle_secs: i64,
    /// Entity-seconds inside the window that nobody observed.
    ///
    /// All three fields are **entity-seconds** — an hour missing from ten lanes
    /// is ten missing hours, the same way ten lanes producing for an hour is ten
    /// producing hours. Mixing wall-clock seconds into this one field made
    /// `blind_spot` saturate to zero the moment there was more than one lane,
    /// which reported a fleet as fully observed while a whole day was missing
    /// from every machine.
    pub unobserved_secs: i64,
}

impl WindowCoverage {
    pub fn observed_secs(&self) -> i64 {
        self.producing_secs + self.idle_secs
    }

    pub fn total_secs(&self) -> i64 {
        self.observed_secs() + self.unobserved_secs
    }

    /// Producing time as a share of **observed** time — never of total, because
    /// dividing by unobserved time invents a denominator nobody measured.
    /// `None` when nothing was observed.
    pub fn uptime(&self) -> Option<f32> {
        let obs = self.observed_secs();
        (obs > 0).then(|| self.producing_secs as f32 / obs as f32)
    }

    /// Share of the window nobody observed — the honesty gauge for the number
    /// above it.
    pub fn blind_spot(&self) -> f32 {
        let total = self.total_secs();
        if total <= 0 {
            return 1.0;
        }
        self.unobserved_secs as f32 / total as f32
    }
}

pub struct CoverageLanesResponse {
    pub response: Response,
    pub hovered: Option<String>,
    /// A lane's on/off toggle was clicked.
    pub toggled: Option<String>,
    pub lanes_shown: usize,
    pub window: WindowCoverage,
}

/// Tints, validated against the Tokyo Night surface `#1a1b26` with the dataviz
/// validator rather than by eye — `#0d9384` / `#b06a1e` pass CVD separation
/// (ΔE 12.6 deutan), normal-vision (19.8), chroma and 3:1 contrast.
///
/// One function, so no call site re-derives a state colour — the rule
/// `flow_ring::ring_tint` already sets.
pub fn coverage_tint(state: Coverage) -> Color32 {
    match state {
        Coverage::Producing => Color32::from_rgb(0x0d, 0x93, 0x84),
        Coverage::Idle => Color32::from_rgb(0xb0, 0x6a, 0x1e),
    }
}

/// The emphasis step, for hover and selection. Passes every separation check but
/// sits above the lightness band, so it is never used for large areas — a wall
/// of hour cells at this lightness glares.
pub fn coverage_tint_emphasis(state: Coverage) -> Color32 {
    match state {
        Coverage::Producing => Color32::from_rgb(0x19, 0xc2, 0xad),
        Coverage::Idle => Color32::from_rgb(0xe0, 0x8a, 0x2e),
    }
}

/// The ground: absence of observation. Grey because colour means somebody
/// decided — the same call `flow_ring` makes for an unexamined party.
pub const UNOBSERVED: Color32 = Color32::from_rgb(0x4d, 0x54, 0x78);

pub struct CoverageLanes<'a> {
    lanes: &'a [CoverageLane<'a>],
    scale: &'a TimeScale,
    spine: &'a SpineState,
    selection: &'a mut Selection,
    gutter: f32,
    label_w: f32,
    lane_h: f32,
    max_lanes: usize,
}

/// An idle span still has to be visible. Fraction of the lane height.
///
/// Raised from 0.22 after looking at a 15-lane render: at 16px lanes the
/// thinner stub read as a baseline rule under the teal rather than as a state
/// of its own, which undersells exactly the comparison this widget exists for.
const IDLE_STUB: f32 = 0.34;
/// Floor for a producing bar, so a barely-hashing hour is not mistaken for idle.
const MIN_PRODUCING: f32 = 0.30;

impl<'a> CoverageLanes<'a> {
    pub fn new(
        lanes: &'a [CoverageLane<'a>],
        scale: &'a TimeScale,
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            lanes,
            scale,
            spine,
            selection,
            gutter: 30.0,
            label_w: 150.0,
            lane_h: 18.0,
            max_lanes: 40,
        }
    }

    /// Match the spine: `30.0` when it shows a play button, `0.0` otherwise.
    pub fn gutter(mut self, px: f32) -> Self {
        self.gutter = px;
        self
    }

    /// Width of the label column. **Pass the same value to
    /// `TimeSpine::left_inset`**, or the ruler starts left of the labels and the
    /// painter clips them away with no warning.
    pub fn label_width(mut self, px: f32) -> Self {
        self.label_w = px;
        self
    }

    pub fn lane_height(mut self, px: f32) -> Self {
        self.lane_h = px.clamp(8.0, 48.0);
        self
    }

    pub fn max_lanes(mut self, n: usize) -> Self {
        self.max_lanes = n.max(1);
        self
    }

    pub fn show(self, ui: &mut Ui) -> CoverageLanesResponse {
        let Self {
            lanes,
            scale,
            spine,
            selection,
            gutter,
            label_w,
            lane_h,
            max_lanes,
        } = self;
        let shown = lanes.len().min(max_lanes);
        let hidden = lanes.len() - shown;
        let extra = if hidden > 0 { 14.0 } else { 0.0 };
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), shown as f32 * lane_h + extra),
            Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let small = egui::TextStyle::Small.resolve(ui.style());

        let x_left = scale.x_range().min;
        let x_right = scale.x_range().max;
        let label_right = label_right_for(rect.left(), x_left, rect.width(), label_w);
        let (lo, hi) = spine.filter_range();
        let brushed = spine.brush.is_some();

        let mut hovered: Option<String> = None;
        let mut toggled: Option<String> = None;
        let mut window = WindowCoverage::default();
        let mut observed_in_window = 0i64;
        let ptr = response.hover_pos();

        for (i, lane) in lanes.iter().take(shown).enumerate() {
            let top = rect.top() + i as f32 * lane_h;
            let row = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), top + lane_h));
            let mid = top + lane_h * 0.5;
            let base = top + lane_h - 2.0;
            let emph = selection.emphasis(lane.key);
            let is_hover = ptr.is_some_and(|p| row.contains(p));
            if is_hover {
                hovered = Some(lane.key.to_string());
                painter.rect_filled(row, 0.0, ui.visuals().faint_bg_color);
            }

            let dot = pos2(rect.left() + gutter * 0.5, mid);
            painter.circle_filled(
                dot,
                3.0,
                if lane.active {
                    ink.gamma_multiply(emph)
                } else {
                    muted.gamma_multiply(0.35)
                },
            );
            if response.clicked() && ptr.is_some_and(|p| (p - dot).length() <= 8.0) {
                toggled = Some(lane.key.to_string());
            }
            painter.text(
                pos2(label_right, mid),
                Align2::RIGHT_CENTER,
                truncate(&lane.name, 20),
                small.clone(),
                if lane.active {
                    ink.gamma_multiply(emph)
                } else {
                    muted.gamma_multiply(0.5)
                },
            );

            // The ground is UNOBSERVED. Everything known paints over it, so a
            // lane with no data reads as "nobody looked" rather than "nothing
            // happened" — the distinction this widget exists for.
            let plot = Rect::from_min_max(pos2(x_left, top + 2.0), pos2(x_right, base));
            painter.rect_filled(plot, 1.0, UNOBSERVED.gamma_multiply(0.28));

            if !lane.active {
                continue;
            }

            let cells = merge_subpixel(lane.runs, scale, spine.playhead, x_left, x_right);
            for cell in &cells {
                let clipped_lo = cell.start.max(lo);
                let clipped_hi = cell.end.min(hi);
                if clipped_hi > clipped_lo {
                    let span = clipped_hi - clipped_lo;
                    let producing_share = if cell.observed_secs > 0.0 {
                        cell.producing_secs / cell.observed_secs
                    } else {
                        0.0
                    };
                    let p = (span as f64 * producing_share).round() as i64;
                    window.producing_secs += p;
                    window.idle_secs += span - p;
                    observed_in_window += span;
                }

                let in_window = cell.end > lo && cell.start < hi;
                // Outside the brush a cell recedes but stays: the brush filters,
                // it does not erase, and downtime needs its context.
                let a = if brushed && !in_window { 0.22 } else { 0.92 } * emph.max(0.3);

                let state = cell.state();
                let col = if is_hover {
                    coverage_tint_emphasis(state)
                } else {
                    coverage_tint(state)
                };
                let h = match state {
                    // A visible stub, because observed-zero must not look like
                    // ground.
                    Coverage::Idle => IDLE_STUB,
                    Coverage::Producing => {
                        (cell.level() * (1.0 - MIN_PRODUCING) + MIN_PRODUCING).clamp(0.0, 1.0)
                    }
                } * (lane_h - 4.0);

                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(cell.x0, base - h),
                        pos2(cell.x1.max(cell.x0 + 1.0), base),
                    ),
                    0.0,
                    col.gamma_multiply(a),
                );
            }
        }

        // Entity-seconds nobody observed: the window once per ACTIVE lane, less
        // what those lanes actually covered. Clamping a per-lane sum against a
        // single window instead saturates at zero for any fleet bigger than one
        // machine — the view would then claim full coverage while missing a day.
        let window_secs = (hi - lo).max(0);
        let active = lanes.iter().take(shown).filter(|l| l.active).count() as i64;
        window.unobserved_secs = (window_secs.saturating_mul(active) - observed_in_window).max(0);

        if let Some(px) = scale.x_from_time_f32(spine.playhead as f64)
            && (x_left..=x_right).contains(&px)
        {
            painter.line_segment(
                [
                    pos2(px, rect.top()),
                    pos2(px, rect.top() + shown as f32 * lane_h),
                ],
                Stroke::new(1.0_f32, ink.gamma_multiply(0.55)),
            );
        }

        if hidden > 0 {
            painter.text(
                pos2(x_left, rect.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                format!("{hidden} more lanes not shown"),
                small,
                muted,
            );
        }

        match &hovered {
            Some(h) => selection.hover(h.clone()),
            None => {
                if let Some(prev) = ui.data(|d| d.get_temp::<String>(response.id.with("hov"))) {
                    selection.clear_hover_if(&prev);
                }
            }
        }
        ui.data_mut(|d| match &hovered {
            Some(h) => {
                d.insert_temp(response.id.with("hov"), h.clone());
            }
            None => d.remove::<String>(response.id.with("hov")),
        });
        if response.clicked() && toggled.is_none() {
            match &hovered {
                Some(h) => selection.toggle_pin(h.clone()),
                None => selection.clear_pin(),
            }
        }

        CoverageLanesResponse {
            response,
            hovered,
            toggled,
            lanes_shown: shown,
            window,
        }
    }
}

/// A run, or several runs collapsed into one pixel column.
#[derive(Debug, Clone, Copy)]
struct Cell {
    x0: f32,
    x1: f32,
    start: i64,
    end: i64,
    observed_secs: f64,
    producing_secs: f64,
    /// `level x seconds`, so the merged bar height is duration-weighted.
    energy: f64,
}

impl Cell {
    fn state(&self) -> Coverage {
        if self.producing_secs > 0.0 {
            Coverage::Producing
        } else {
            Coverage::Idle
        }
    }

    fn level(&self) -> f32 {
        if self.observed_secs <= 0.0 {
            return 0.0;
        }
        (self.energy / self.observed_secs).clamp(0.0, 1.0) as f32
    }
}

/// Collapse runs that land inside the same pixel column.
///
/// Merging rather than dropping is the point: at a year's zoom an hour is far
/// under a pixel, and a widget that skipped it would lose exactly the outage the
/// reader came for. The merged cell carries duration-weighted observed and
/// producing time, so the tally stays exact however far out you zoom.
fn merge_subpixel(
    runs: &[Run],
    scale: &TimeScale,
    playhead: i64,
    x_left: f32,
    x_right: f32,
) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();

    for run in runs {
        // The playhead reveals: a run that has not happened yet is not drawn,
        // and a run straddling it is truncated rather than shown whole.
        let end = run.end.min(playhead);
        if end <= run.start {
            continue;
        }
        let visible_secs = (end - run.start) as f64;
        let full = run.secs();
        if full <= 0.0 {
            continue;
        }

        let (Some(x0), Some(x1)) = (
            scale.x_from_time_f32(run.start as f64),
            scale.x_from_time_f32(end as f64),
        ) else {
            continue;
        };
        if x1 < x_left || x0 > x_right {
            continue;
        }
        let x0 = x0.max(x_left);
        let x1 = x1.min(x_right);

        let producing = match run.state {
            Coverage::Producing => visible_secs,
            Coverage::Idle => 0.0,
        };
        let energy = match run.state {
            Coverage::Producing => visible_secs * run.level as f64,
            Coverage::Idle => 0.0,
        };

        match out.last_mut() {
            Some(prev) if x0 - prev.x0 < 1.0 => {
                prev.x1 = prev.x1.max(x1);
                prev.end = prev.end.max(end);
                prev.observed_secs += visible_secs;
                prev.producing_secs += producing;
                prev.energy += energy;
            }
            _ => out.push(Cell {
                x0,
                x1,
                start: run.start,
                end,
                observed_secs: visible_secs,
                producing_secs: producing,
                energy,
            }),
        }
    }

    out
}

/// Right edge of the label column, kept inside the clip rect. Same fallback as
/// `activity_lanes`: cramped beats invisible.
fn label_right_for(rect_left: f32, x_left: f32, rect_w: f32, label_w: f32) -> f32 {
    if x_left - rect_left >= 40.0 {
        x_left - 8.0
    } else {
        rect_left + label_w.min(rect_w * 0.3)
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!(
        "{}…",
        s.chars().take(n.saturating_sub(1)).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_spine::TimeView;
    use egui::{Id, Pos2, Rangef, vec2};

    fn run_ui(
        lanes: &[CoverageLane<'_>],
        spine: &SpineState,
        sel: &mut Selection,
    ) -> CoverageLanesResponse {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let scale = TimeScale::continuous(
            Rangef::new(200.0, 800.0),
            TimeView::covering(spine.domain.0, spine.domain.1),
            spine.domain.0,
            spine.domain.1,
        );
        let mut out = None;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 400.0))),
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("cl")).show(&ctx, |ui| {
            ui.set_min_size(vec2(900.0, 400.0));
            out = Some(CoverageLanes::new(lanes, &scale, spine, sel).show(ui));
        });
        let _ = ctx.end_pass();
        out.unwrap()
    }

    fn lane<'a>(key: &'a str, runs: &'a [Run]) -> CoverageLane<'a> {
        CoverageLane {
            key,
            name: key.into(),
            runs,
            active: true,
        }
    }

    /// **The reason this widget exists.** A gap is unobserved, never idle. If
    /// this ever flips, every ingest outage silently becomes recorded downtime.
    #[test]
    fn a_gap_is_unobserved_not_idle() {
        // Observed 0..300 producing, 700..1000 producing. 300..700 = nobody looked.
        let runs = [Run::producing(0, 300, 1.0), Run::producing(700, 1000, 1.0)];
        let lanes = [lane("a", &runs)];
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));

        let w = run_ui(&lanes, &spine, &mut sel).window;
        assert_eq!(w.producing_secs, 600);
        assert_eq!(w.idle_secs, 0, "a gap must NEVER be counted as idle");
        assert_eq!(w.unobserved_secs, 400, "the gap is unobserved time");
        assert_eq!(
            w.uptime(),
            Some(1.0),
            "uptime divides by OBSERVED time — the fleet hashed every hour anyone watched"
        );
        assert!((w.blind_spot() - 0.4).abs() < 1e-6);
    }

    /// Observed-idle is a positive claim and counts against uptime, unlike a gap.
    #[test]
    fn observed_idle_counts_against_uptime_but_a_gap_does_not() {
        let observed = [Run::producing(0, 600, 1.0), Run::idle(600, 1000)];
        let gapped = [Run::producing(0, 600, 1.0)];
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));

        let a = run_ui(&[lane("a", &observed)], &spine, &mut sel).window;
        assert_eq!(a.idle_secs, 400);
        assert_eq!(a.uptime(), Some(0.6));
        assert_eq!(a.blind_spot(), 0.0, "every hour was observed");

        let b = run_ui(&[lane("b", &gapped)], &spine, &mut sel).window;
        assert_eq!(b.uptime(), Some(1.0), "same producing time, no idle claim");
        assert!(
            b.blind_spot() > 0.0,
            "but the view admits it is partly blind"
        );
    }

    /// Sub-pixel runs must merge, not vanish — a lost run is lost downtime.
    #[test]
    fn subpixel_runs_merge_without_losing_time() {
        // 600px of plot over 1000s: ~1.67s per pixel. 1s runs are sub-pixel.
        let runs: Vec<Run> = (0..1000)
            .map(|i| {
                if i % 2 == 0 {
                    Run::producing(i, i + 1, 1.0)
                } else {
                    Run::idle(i, i + 1)
                }
            })
            .collect();
        let lanes = [lane("a", &runs)];
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));

        let w = run_ui(&lanes, &spine, &mut sel).window;
        assert_eq!(
            w.observed_secs(),
            1000,
            "every second is still accounted for after merging"
        );
        assert_eq!(w.unobserved_secs, 0);
        assert!(
            (w.uptime().unwrap() - 0.5).abs() < 0.01,
            "half producing, duration-weighted; got {:?}",
            w.uptime()
        );
    }

    /// The playhead reveals; the brush filters.
    #[test]
    fn the_playhead_truncates_and_the_brush_filters() {
        let runs = [Run::producing(0, 1000, 1.0)];
        let lanes = [lane("a", &runs)];
        let mut sel = Selection::default();

        let mut spine = SpineState::new((0, 1000));
        spine.set_playhead(400);
        let w = run_ui(&lanes, &spine, &mut sel).window;
        assert_eq!(
            w.producing_secs, 400,
            "nothing past the playhead exists yet"
        );

        spine.set_playhead(1000);
        spine.set_brush(Some((200, 600)));
        let w = run_ui(&lanes, &spine, &mut sel).window;
        assert_eq!(w.producing_secs, 400, "brushed to the middle");
    }

    /// An inactive lane keeps its row but contributes nothing.
    #[test]
    fn an_inactive_lane_keeps_its_row_but_counts_nothing() {
        let runs = [Run::producing(0, 1000, 1.0)];
        let lanes = [
            CoverageLane {
                key: "a",
                name: "a".into(),
                runs: &runs,
                active: false,
            },
            lane("b", &runs),
        ];
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));
        let r = run_ui(&lanes, &spine, &mut sel);
        assert_eq!(r.lanes_shown, 2, "the row is still there");
        assert_eq!(r.window.producing_secs, 1000, "only the active lane counts");
    }

    /// Every lane missing the same stretch is N missing lane-hours, not one.
    ///
    /// Regression: the first cut clamped a per-lane sum against a single window
    /// (`observed.min(window)`), which saturates for any fleet bigger than one
    /// machine — the storybook rendered a whole unobserved day across 15 lanes
    /// and still reported "blind 0.0%". Every single-lane test passed.
    #[test]
    fn a_gap_across_many_lanes_is_counted_once_per_lane() {
        // Three lanes, each observed for the first half only.
        let runs = [Run::producing(0, 500, 1.0)];
        let lanes: Vec<CoverageLane<'_>> = ["a", "b", "c"]
            .iter()
            .map(|k| CoverageLane {
                key: k,
                name: (*k).into(),
                runs: &runs,
                active: true,
            })
            .collect();
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));

        let w = run_ui(&lanes, &spine, &mut sel).window;
        assert_eq!(w.producing_secs, 1500, "3 lanes x 500s");
        assert_eq!(
            w.unobserved_secs, 1500,
            "3 lanes x the 500s nobody watched — NOT clamped to one window"
        );
        assert!(
            (w.blind_spot() - 0.5).abs() < 1e-6,
            "half of every lane's window was unobserved; got {}",
            w.blind_spot()
        );
    }

    /// Uptime must never divide by time nobody measured.
    #[test]
    fn uptime_is_none_when_nothing_was_observed() {
        let w = WindowCoverage {
            producing_secs: 0,
            idle_secs: 0,
            unobserved_secs: 3600,
        };
        assert_eq!(w.uptime(), None);
        assert_eq!(w.blind_spot(), 1.0);
    }

    /// Caps are stated, not swallowed.
    #[test]
    fn overflow_is_reported_not_swallowed() {
        let runs = [Run::producing(0, 100, 1.0)];
        let names: Vec<String> = (0..50).map(|i| format!("m{i}")).collect();
        let lanes: Vec<CoverageLane<'_>> = names
            .iter()
            .map(|n| CoverageLane {
                key: n,
                name: n.clone(),
                runs: &runs,
                active: true,
            })
            .collect();
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));
        assert_eq!(run_ui(&lanes, &spine, &mut sel).lanes_shown, 40);
    }

    /// Labels must land inside the clip rect — drawing outside vanishes silently.
    #[test]
    fn labels_stay_inside_the_clip_rect() {
        assert!(super::label_right_for(0.0, 160.0, 900.0, 150.0) < 160.0);
        assert!(super::label_right_for(0.0, 30.0, 900.0, 150.0) >= 30.0);
    }
}
