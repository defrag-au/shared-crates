//! TimeSpine — ONE time axis for a surface of many faces.
//!
//! The thing the project-ledger view was missing: every face scrubbed its own
//! playhead and none of them could talk to each other. This is the shared
//! spine — a [`TimeScale`] (time ↔ x), a playhead, a brush range, play/pause —
//! and a [`SpineState`] every face reads. Brushing a week on the spine filters
//! every face below it at once; that linked question is the whole point.
//!
//! ## Attribution
//!
//! [`TimeScale`] and [`paint_ticks`] are ported from Rerun's `re_time_ruler`
//! crate (`crates/viewer/re_time_ruler/src/{time_ranges_ui,paint_ticks}.rs`
//! at rev `504c060f68`, <https://github.com/rerun-io/rerun>), Copyright (c)
//! Rerun Technologies AB, dual-licensed MIT / Apache-2.0. Ported here rather
//! than depended upon because that crate pulls in Rerun's whole viewer data
//! model (`re_viewer_context`, `re_log_types`) and pins a different egui. What
//! carried over: the piecewise-linear segment mapping with gap-collapsing,
//! `f64` for the mapping (screen coords are `f32`, but a zoomed-in segment's
//! ends are far off-screen), extrapolation past both ends, the three-tier tick
//! painter that fades tiers in as you zoom, and the tick-spacing ladder. What
//! changed: Rerun's `TimeReal`/`AbsoluteTimeRange`/`TimeView` become plain
//! `f64` seconds and `(i64, i64)` ranges; snap-to-segment during playback is
//! kept; pan/zoom return a new [`TimeView`].
//!
//! ## Conventions carried from the egui week
//!
//! - the playhead **opens at the end**; play rewinds
//! - the brush **filters**, the playhead **reveals** — two different verbs
//! - time is unix seconds throughout; formatting is the caller's
//!
//! ## The naked spine and its layers
//!
//! The spine itself owns only what every time axis needs: the time ↔ x
//! mapping, zoom and pan, the ticks, the playhead, the brush and the play
//! button. **What appears in the lane under the ruler is not its business.**
//! That is drawn by [`SpineLayer`]s, handed in by the caller and painted in
//! the order given, behind the ticks and the playhead, each on the same
//! [`SpineCanvas`] — so a density silhouette, the marks for a watched wallet
//! and a coverage band all sit on one axis without the spine knowing what
//! any of them are, and a surface can stack as many as it needs.
//!
//! Three layers ship here — [`CoverageLayer`], [`MarksLayer`],
//! [`DensityLayer`] — and the `coverage`/`marks`/`density` builders are sugar
//! for pushing them. Anything else implements the trait.

use egui::emath::Rangef;
use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Response, Rgba, Sense, Shape, Stroke, Ui, Vec2,
    lerp, pos2, remap, remap_clamp, vec2,
};

use crate::motion::{Easing, tween, tween_bool};

/// Colours for [`MarkKind`]. Two categories, so two hues from the catalog's
/// categorical order — never a red/green pair, which reads as good/bad rather
/// than in/out.
const MARK_IN: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
const MARK_OUT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x2e);
/// Directionless events — deliberately UNSATURATED rather than a third hue.
///
/// A hue would read as a third category competing with in and out; these are
/// the events that decline the question, and they are usually the majority. A
/// grey lets a burst of them still show as density while leaving the two
/// directional colours as the thing the eye picks out.
const MARK_NEUTRAL: Color32 = Color32::from_rgb(0x7a, 0x82, 0x94);
/// Half-height of a neutral mark, in points. Small: it straddles the midline
/// instead of rising from it, so it must not read as a short In and a short
/// Out drawn on top of each other.
const NEUTRAL_HALF: f32 = 2.0;

/// The pin — deliberately a THIRD hue, neither mark colour.
///
/// It has to be findable among a few thousand marks, and picking either
/// in/out colour would make the one event the reader was sent to look at
/// indistinguishable from the crowd it sits in.
const PIN: Color32 = Color32::from_rgb(0x9b, 0x7c, 0xf5);

/// Width of one density column, in points. Two: one is a hairline that
/// flickers between adjacent pixels as the view pans, and wider starts to read
/// as bars with edges rather than as a silhouette.
const COLUMN_W: f32 = 2.0;
/// The silhouette is the SAME grey as a neutral mark, lightened — it is the
/// same claim ("this happened, no direction") made about a count instead of
/// an event, and the directional marks drawn over it need to win.
const DENSITY_FILL: Color32 = Color32::from_rgb(0x5e, 0x66, 0x78);
/// Least half-height of a column that has anything in it. One transaction
/// against ten thousand rounds to nothing on a log scale too, and "one" versus
/// "none" is the distinction a mark lane shows for free, so it must not be
/// lost when the lane changes form.
const COLUMN_FLOOR: f32 = 1.0;

// ---------------------------------------------------------------------------
// TimeScale — the mapping (ported from re_time_ruler::TimeRangesUi)
// ---------------------------------------------------------------------------

/// The ideal gap between time segments (ui points). Shrunk if there are many.
const MAX_GAP: f64 = 40.0;
/// How much of the gap segments may expand into. Strictly < 0.5 or they overlap.
const GAP_EXPANSION_FRACTION: f64 = 1.0 / 4.0;

/// The window of time being viewed: `[min, min + spanned]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeView {
    pub min: f64,
    pub spanned: f64,
}

impl TimeView {
    pub fn covering(min: i64, max: i64) -> Self {
        let min = min as f64;
        let max = (max as f64).max(min + 1.0);
        Self {
            min,
            spanned: max - min,
        }
    }

    pub fn max(&self) -> f64 {
        self.min + self.spanned
    }
}

/// One linear stretch of time on screen.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    /// Screen x-range (`f64` — can be far off-screen when zoomed).
    pub x: (f64, f64),
    /// Time range that maps linearly onto `x` (expanded slightly past `tight`).
    pub time: (f64, f64),
    /// The tight time bound of the data in this segment.
    pub tight: (i64, i64),
}

/// A piecewise-linear time ↔ x mapping with collapsible gaps between segments.
///
/// One segment covering the whole domain is the common case (a continuous
/// project timeline). Several segments let the spine collapse dead time — the
/// year between a mint window and its first royalty, say — without lying about
/// the axis: the gap is drawn as a gap.
#[derive(Debug, Clone)]
pub struct TimeScale {
    x_range: (f64, f64),
    view: TimeView,
    pub segments: Vec<Segment>,
    /// x per second inside segments (and when extrapolating).
    pub points_per_sec: f64,
}

/// Width of the gap between segments in ui points.
pub fn gap_width(x_range: Rangef, segments: &[(i64, i64)]) -> f64 {
    let gaps = segments.len().saturating_sub(1);
    if gaps == 0 {
        MAX_GAP
    } else {
        (x_range.span() as f64 / gaps as f64).min(MAX_GAP)
    }
}

impl TimeScale {
    /// Build the mapping. `ranges` are ascending, non-overlapping tight
    /// `(min, max)` seconds; usually one.
    pub fn new(x_range: Rangef, view: TimeView, ranges: &[(i64, i64)]) -> Self {
        debug_assert!(x_range.min < x_range.max);
        let gap_ui = gap_width(x_range, ranges);
        let xr = (x_range.min as f64, x_range.max as f64);
        let width_ui = xr.1 - xr.0;
        let pps = width_ui / view.spanned;
        let points_per_sec = if pps > 0.0 && pps.is_finite() {
            pps
        } else {
            1.0
        };

        // Expand each segment slightly into its gaps so a drag that overshoots
        // a segment's end doesn't immediately fall into the non-linear realm.
        let shortest_gap = ranges
            .windows(2)
            .map(|w| (w[1].0 - w[0].1) as f64)
            .fold(f64::INFINITY, f64::min);
        let expansion_secs = (GAP_EXPANSION_FRACTION * gap_ui / points_per_sec)
            .min(shortest_gap * GAP_EXPANSION_FRACTION);
        let expansion_ui = points_per_sec * expansion_secs;

        let mut left = 0.0;
        let mut segments: Vec<Segment> = ranges
            .iter()
            .map(|&(t0, t1)| {
                let range_w = (t1 - t0).max(0) as f64 * points_per_sec;
                let right = left + range_w;
                let seg = Segment {
                    x: (left - expansion_ui, right + expansion_ui),
                    time: (t0 as f64 - expansion_secs, t1 as f64 + expansion_secs),
                    tight: (t0, t1),
                };
                left = right + gap_ui;
                seg
            })
            .collect();

        let mut scale = Self {
            x_range: xr,
            view,
            segments: Vec::new(),
            points_per_sec,
        };
        // Align x_range.start with view.min by translating everything.
        scale.segments = std::mem::take(&mut segments);
        if let Some(x0) = scale.x_from_time(view.min) {
            let dx = xr.0 - x0;
            for s in &mut scale.segments {
                s.x.0 += dx;
                s.x.1 += dx;
            }
        }
        scale
    }

    /// Continuous single-segment scale over `[min, max]` — the common case.
    pub fn continuous(x_range: Rangef, view: TimeView, min: i64, max: i64) -> Self {
        Self::new(x_range, view, &[(min, max.max(min))])
    }

    pub fn view(&self) -> TimeView {
        self.view
    }

    pub fn x_range(&self) -> Rangef {
        Rangef::new(self.x_range.0 as f32, self.x_range.1 as f32)
    }

    pub fn x_from_time(&self, t: f64) -> Option<f64> {
        let first = self.segments.first()?;
        let (mut last_x, mut last_t) = (first.x.0, first.time.0);
        if t < last_t {
            return Some(last_x - self.points_per_sec * (last_t - t));
        }
        for s in &self.segments {
            if t < s.time.0 {
                let f = inv_lerp(last_t, s.time.0, t);
                return Some(lerp(last_x..=s.x.0, f));
            } else if t <= s.time.1 {
                let f = inv_lerp(s.time.0, s.time.1, t);
                return Some(lerp(s.x.0..=s.x.1, f));
            }
            last_x = s.x.1;
            last_t = s.time.1;
        }
        Some(last_x + self.points_per_sec * (t - last_t))
    }

    pub fn x_from_time_f32(&self, t: f64) -> Option<f32> {
        self.x_from_time(t).map(|x| x as f32)
    }

    pub fn time_from_x(&self, x: f64) -> Option<f64> {
        let first = self.segments.first()?;
        let (mut last_x, mut last_t) = (first.x.0, first.time.0);
        if x < last_x {
            return Some(last_t + (x - last_x) / self.points_per_sec);
        }
        for s in &self.segments {
            if x < s.x.0 {
                let f = remap(x, last_x..=s.x.0, 0.0..=1.0);
                return Some(lerp(last_t..=s.time.0, f));
            } else if x <= s.x.1 {
                let f = remap(x, s.x.0..=s.x.1, 0.0..=1.0);
                return Some(lerp(s.time.0..=s.time.1, f));
            }
            last_x = s.x.1;
            last_t = s.time.1;
        }
        Some(last_t + (x - last_x) / self.points_per_sec)
    }

    pub fn time_from_x_f32(&self, x: f32) -> Option<f64> {
        self.time_from_x(x as f64)
    }

    /// Keep a playing playhead out of the gaps between segments.
    pub fn snap_to_segments(&self, t: f64) -> f64 {
        for s in &self.segments {
            if t < s.time.0 {
                return s.time.0;
            } else if t <= s.time.1 {
                return t;
            }
        }
        t
    }

    /// Pan by `dx` ui points, returning the new view.
    pub fn pan(&self, dx: f32) -> Option<TimeView> {
        Some(TimeView {
            min: self.time_from_x(self.x_range.0 + dx as f64)?,
            spanned: self.view.spanned,
        })
    }

    /// Zoom about screen `x` by `factor` (>1 zooms in), returning the new view.
    /// Never zooms out past `max_span` seconds.
    pub fn zoom_at(&self, x: f32, factor: f32, max_span: f64) -> Option<TimeView> {
        let min_factor = self.view.spanned / max_span;
        let factor = (factor as f64).max(min_factor);
        let (mut min_x, max_x) = self.x_range;
        let t = remap(x as f64, min_x..=max_x, 0.0..=1.0);
        let width = max_x - min_x;
        let new_width = width / factor;
        min_x -= t * (new_width - width);
        Some(TimeView {
            min: self.time_from_x(min_x)?,
            spanned: self.view.spanned / factor,
        })
    }
}

fn inv_lerp(a: f64, b: f64, v: f64) -> f64 {
    if (b - a).abs() < f64::EPSILON {
        0.5
    } else {
        (v - a) / (b - a)
    }
}

// ---------------------------------------------------------------------------
// Ticks (ported from re_time_ruler::paint_ticks)
// ---------------------------------------------------------------------------

/// The next coarser "nice" tick spacing, in seconds. Ported from Rerun's
/// `next_grid_tick_magnitude_nanos`, rescaled to seconds:
/// ×10 up to 10 s → minute → 10 min → hour → 12 h → day → ×10 days.
pub fn next_tick_step_secs(spacing: i64) -> i64 {
    const MIN: i64 = 60;
    const HOUR: i64 = 3600;
    const DAY: i64 = 86_400;
    if spacing < 10 {
        spacing * 10
    } else if spacing == 10 {
        60
    } else if spacing == MIN {
        10 * MIN
    } else if spacing == 10 * MIN {
        HOUR
    } else if spacing == HOUR {
        12 * HOUR
    } else if spacing == 12 * HOUR {
        DAY
    } else {
        spacing.checked_mul(10).unwrap_or(spacing)
    }
}

/// Paint tick marks + labels for every segment of `scale` across `line_y`.
/// Three tiers fade in as you zoom; `format_tick(secs, spacing_secs)` labels
/// the tier that has room (spacing tells it how much precision is meaningful).
pub fn paint_ticks(
    scale: &TimeScale,
    ui: &Ui,
    painter: &egui::Painter,
    line_y: Rangef,
    format_tick: &dyn Fn(i64, i64) -> String,
) {
    let clip = ui.clip_rect();
    let (clip_l, clip_r) = (clip.left() as f64, clip.right() as f64);
    for seg in &scale.segments {
        let (mut x0, mut x1) = seg.x;
        let (mut t0, mut t1) = seg.time;
        if x1 < clip_l || clip_r < x0 {
            continue;
        }
        // Clamp to the visible part so zoomed-in segments don't cost CPU.
        let lf = inv_lerp(x0, x1, clip_l);
        if 0.0 < lf && lf < 1.0 {
            x0 = clip_l;
            t0 = lerp(t0..=t1, lf);
        }
        let rf = inv_lerp(seg.x.0, seg.x.1, clip_r);
        if 0.0 < rf && rf < 1.0 {
            x1 = clip_r;
            t1 = lerp(seg.time.0..=seg.time.1, rf);
        }
        let rect = Rect::from_x_y_ranges(Rangef::new(x0 as f32, x1 as f32), line_y);
        painter
            .with_clip_rect(rect)
            .extend(tick_shapes(ui, &rect, (t0, t1), format_tick));
    }
}

fn tick_shapes(
    ui: &Ui,
    canvas: &Rect,
    time_range: (f64, f64),
    format_tick: &dyn Fn(i64, i64) -> String,
) -> Vec<Shape> {
    let dark = ui.visuals().dark_mode;
    let font_id: FontId = egui::TextStyle::Small.resolve(ui.style());
    let color_from_alpha = |a: f32| -> Color32 {
        if dark {
            Rgba::from_white_alpha(a * a).into()
        } else {
            Rgba::from_black_alpha(a).into()
        }
    };
    let (tmin, tmax) = time_range;
    let width_t = (tmax - tmin) as f32;
    if width_t <= 0.0 || !canvas.is_positive() {
        return Vec::new();
    }
    let x_from_time = |t: i64| -> f32 {
        let f = (t as f64 - tmin) as f32 / width_t;
        lerp(canvas.x_range(), f)
    };
    let pps = canvas.width() / width_t;
    let min_small_spacing = 4.0;
    let expected_text_w = 60.0;
    let line_strength = |spacing: i64| -> f32 {
        let next_mag = next_tick_step_secs(spacing) / spacing;
        remap_clamp(
            spacing as f32 * pps,
            min_small_spacing..=(next_mag as f32 * min_small_spacing),
            0.0..=1.0,
        )
    };
    let text_color = |spacing: i64| -> Color32 {
        let a = remap_clamp(
            spacing as f32 * pps,
            expected_text_w..=(3.0 * expected_text_w),
            0.0..=0.7,
        );
        color_from_alpha(a)
    };

    let max_small = canvas.width() / min_small_spacing;
    let mut small = 1i64;
    while width_t / small as f32 > max_small {
        small = next_tick_step_secs(small);
    }
    let medium = next_tick_step_secs(small);
    let big = next_tick_step_secs(medium);

    let base = 0.7;
    let (big_l, med_l, small_l) = (
        line_strength(big),
        line_strength(medium),
        line_strength(small),
    );
    let (big_c, med_c, small_c) = (
        color_from_alpha(base * big_l),
        color_from_alpha(base * med_l),
        color_from_alpha(base * small_l),
    );
    let (big_t, med_t, small_t) = (text_color(big), text_color(medium), text_color(small));

    let mut shapes = Vec::new();
    let mut t = (tmin.floor() as i64).div_euclid(small) * small;
    let end = tmax.ceil() as i64 + 1;
    let visible = ui.clip_rect().intersect(*canvas);
    while t < end {
        let x = x_from_time(t);
        if visible.min.x <= x && x <= visible.max.x {
            let is_med = t.rem_euclid(medium) == 0;
            let is_big = t.rem_euclid(big) == 0;
            let (h, lc, tc, spacing) = if is_big {
                (med_l, big_c, big_t, big)
            } else if is_med {
                (small_l, med_c, med_t, medium)
            } else {
                (0.0, small_c, small_t, small)
            };
            let top = lerp(canvas.y_range(), lerp(0.75..=0.5, h));
            shapes.push(Shape::line_segment(
                [pos2(x, top), pos2(x, canvas.max.y)],
                Stroke::new(1.0_f32, lc),
            ));
            if tc != Color32::TRANSPARENT {
                let text = format_tick(t, spacing);
                ui.ctx().fonts_mut(|f| {
                    // MEASURE BEFORE DRAWING. The `visible` test above bounds
                    // the tick, not the label that hangs to the right of it,
                    // so a tick near the edge painted its text past the canvas
                    // — invisible inside a desktop margin, and straight off
                    // the viewport on a phone. The tick line still draws; only
                    // its label is dropped, which is the right thing to lose.
                    let galley = f.layout_no_wrap(text, font_id.clone(), tc);
                    let size = galley.size();
                    if x + 4.0 + size.x <= canvas.max.x {
                        let y = lerp(canvas.y_range(), 0.5) - size.y * 0.5;
                        shapes.push(Shape::galley(pos2(x + 4.0, y), galley, tc));
                    }
                });
            }
        }
        t = t.saturating_add(small);
    }
    shapes
}

// ---------------------------------------------------------------------------
// Time formatting (no chrono — one civil-date algorithm)
// ---------------------------------------------------------------------------

/// `(year, month, day)` from unix seconds (UTC), Howard Hinnant's algorithm.
pub fn civil_from_unix(unix: i64) -> (i64, u32, u32) {
    let days = unix.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DD`.
pub fn format_date(unix: i64) -> String {
    let (y, m, d) = civil_from_unix(unix);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Half the width of the playhead's arrow head. The playhead is inset by this
/// at both ends so the marker never draws past the widget's clip rect.
const PLAYHEAD_HALF_W: f32 = 5.0;

/// A tick label with only as much precision as `spacing` seconds warrants:
/// day-or-coarser → date; sub-day → `HH:MM`; sub-minute → `HH:MM:SS`.
pub fn compact_tick_label(unix: i64, spacing: i64) -> String {
    if spacing >= 86_400 {
        format_date(unix)
    } else {
        let s = unix.rem_euclid(86_400);
        let (h, m, sec) = (s / 3600, (s / 60) % 60, s % 60);
        if spacing >= 60 {
            format!("{h:02}:{m:02}")
        } else {
            format!("{h:02}:{m:02}:{sec:02}")
        }
    }
}

// ---------------------------------------------------------------------------
// SpineState — the shared store every face reads
// ---------------------------------------------------------------------------

/// The one piece of state a whole surface shares. Faces READ it; only the
/// spine (and explicit face scrubs) write it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpineState {
    /// Full data extent `[min, max]` (unix seconds). The scale's domain.
    pub domain: (i64, i64),
    /// The playhead — what has been REVEALED so far.
    pub playhead: i64,
    /// Optional brush — what is being FILTERED to. `None` = everything.
    pub brush: Option<(i64, i64)>,
    pub playing: bool,
    /// Wall-clock seconds for one full domain sweep when playing.
    pub play_duration: f32,
    /// The visible time window (pan/zoom); defaults to the whole domain.
    pub view: TimeView,
    last_tick: Option<f64>,
}

impl SpineState {
    /// Opens at the END — a scrubber that opens at zero shows a ghost of a
    /// chart; opening finished shows the whole story, and play rewinds.
    pub fn new(domain: (i64, i64)) -> Self {
        let domain = (domain.0, domain.1.max(domain.0 + 1));
        Self {
            domain,
            playhead: domain.1,
            brush: None,
            playing: false,
            play_duration: 12.0,
            view: TimeView::covering(domain.0, domain.1),
            last_tick: None,
        }
    }

    /// The range faces should FILTER to: the brush, else the domain.
    pub fn filter_range(&self) -> (i64, i64) {
        self.brush.unwrap_or(self.domain)
    }

    /// Whether `t` passes both the brush filter and the playhead reveal.
    pub fn shows(&self, t: i64) -> bool {
        let (a, b) = self.filter_range();
        t >= a && t <= b && t <= self.playhead
    }

    pub fn toggle_play(&mut self) {
        if !self.playing && self.playhead >= self.domain.1 {
            self.playhead = self.domain.0;
        }
        self.playing = !self.playing;
        self.last_tick = None;
    }

    /// Advance the playhead if playing. Call once per frame with `ctx`.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.playing {
            self.last_tick = None;
            return;
        }
        let now = ctx.input(|i| i.time);
        let dt = self.last_tick.map_or(0.0, |l| (now - l).max(0.0));
        self.last_tick = Some(now);
        let span = (self.domain.1 - self.domain.0) as f64;
        let advance = span * dt / self.play_duration.max(0.1) as f64;
        self.playhead = ((self.playhead as f64 + advance).round() as i64).min(self.domain.1);
        if self.playhead >= self.domain.1 {
            self.playing = false;
        }
        ctx.request_repaint();
    }

    pub fn set_playhead(&mut self, t: i64) {
        self.playhead = t.clamp(self.domain.0, self.domain.1);
        self.playing = false;
    }

    pub fn set_brush(&mut self, range: Option<(i64, i64)>) {
        self.brush = range.map(|(a, b)| {
            let (a, b) = (a.min(b), a.max(b));
            (
                a.clamp(self.domain.0, self.domain.1),
                b.clamp(self.domain.0, self.domain.1),
            )
        });
    }
}

// ---------------------------------------------------------------------------
// TimeSpine — the widget
// ---------------------------------------------------------------------------

/// What the spine did this frame.
pub struct TimeSpineResponse {
    pub response: Response,
    /// The scale, so faces below can align to the same x-mapping.
    pub scale: TimeScale,
    /// The playhead moved this frame (by drag, click or play).
    pub playhead_changed: bool,
    /// The brush changed this frame.
    pub brush_changed: bool,
}

/// The shared time spine: ruler + ticks, playhead, brush lane, play button.
/// Which way a marked event went for the thing being watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// Something arrived (an asset acquired, funds received).
    In,
    /// Something left.
    Out,
    /// It happened, and it had no direction for the thing being watched.
    ///
    /// **Not a fallback for "we didn't work it out" — a real third answer.**
    /// In/Out are relative to a subject, and some subjects have events that
    /// are genuinely neither. Watching a POLICY, a mint is supply arriving and
    /// a burn is supply leaving, but an ordinary transfer is circulation
    /// between two holders: nothing enters or leaves. Transfers are also most
    /// of the feed, so forcing them into `In` would paint almost every mark in
    /// the "arrived" hue and make the lane's colour meaningless — while
    /// dropping them would empty the density strip on any collection that has
    /// finished minting, which is exactly the history a reader came to see.
    ///
    /// Drawn straddling the midline rather than rising or falling from it, so
    /// the shape says "no direction" before the colour does.
    Neutral,
}

/// A count of events over a stretch of time — one bar of a histogram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DensityBin {
    /// Start, unix seconds.
    pub start: i64,
    /// Width, seconds.
    pub span: i64,
    /// How many events fell inside it.
    pub count: u64,
}

/// What a [`SpineLayer`] gets to draw with: the spine's own mapping and the
/// rectangles it has already decided on. A layer never allocates space or
/// handles input — it paints, and may name what is under the pointer.
pub struct SpineCanvas<'c> {
    pub painter: &'c egui::Painter,
    /// The spine's time ↔ x mapping for this frame. Use it, never your own:
    /// a layer that maps time itself will drift from the ticks the moment the
    /// reader zooms.
    pub scale: &'c TimeScale,
    /// The whole ruler — tick lane above the baseline, event lane below.
    pub ruler: Rect,
    /// The lane under the baseline, where layers ordinarily draw.
    pub lane: Rect,
    pub visuals: &'c egui::Visuals,
    /// The caller's tick formatter, so a layer's tooltip dates match the
    /// ruler's.
    pub format_tick: &'c dyn Fn(i64, i64) -> String,
    /// The pointer, when it is over the lane.
    pub hover: Option<Pos2>,
}

/// Something drawn along the spine, behind the ticks.
///
/// Layers are painted in the order the caller adds them — first at the
/// bottom. They share one canvas and one mapping, which is the point: a
/// coverage band, a density silhouette and a watched wallet's marks compose
/// on a single axis without any of them knowing about the others.
pub trait SpineLayer {
    /// Paint, and if the pointer is over something worth naming, return one
    /// line for the tooltip. The topmost layer that answers wins.
    fn paint(&self, canvas: &SpineCanvas<'_>) -> Option<String>;
}

/// Faint `(from, to)` bands: nobody was looking outside them.
pub struct CoverageLayer<'a>(pub &'a [(i64, i64)]);

impl SpineLayer for CoverageLayer<'_> {
    fn paint(&self, c: &SpineCanvas<'_>) -> Option<String> {
        for &(a, b) in self.0 {
            if let (Some(xa), Some(xb)) = (
                c.scale.x_from_time_f32(a as f64),
                c.scale.x_from_time_f32(b as f64),
            ) {
                let r = Rect::from_x_y_ranges(
                    Rangef::new(xa.max(c.ruler.left()), xb.min(c.ruler.right())),
                    Rangef::new(c.lane.top() + 2.0, c.lane.bottom() - 2.0),
                );
                c.painter
                    .rect_filled(r, CornerRadius::ZERO, c.visuals.faint_bg_color);
            }
        }
        None
    }
}

/// Discrete events, one hairline each: in rises from the midline, out falls
/// from it, neutral straddles it.
///
/// Right for as long as the events can be told apart. Past a few per pixel
/// the lane saturates: a day with eight hundred and a day with thirty both
/// paint the same solid bar, and the only thing left legible is the gaps —
/// the lane ends up showing when NOTHING happened and saying nothing about
/// how much did. That is when to reach for [`DensityLayer`] underneath and
/// keep this one for the events that are genuinely rare.
pub struct MarksLayer<'a>(pub &'a [(i64, MarkKind)]);

impl SpineLayer for MarksLayer<'_> {
    fn paint(&self, c: &SpineCanvas<'_>) -> Option<String> {
        paint_marks(c.painter, c.scale, &c.ruler, &c.lane, self.0);
        None
    }
}

/// A RATE — how much happened when — as a waveform of columns.
///
/// The silhouette grows from the midline in both directions, the way a
/// neutral mark does: it is the same "happened, no direction" claim made
/// about a count. Height is square-root scaled up to a ROBUST knee (see
/// [`column_ceiling`]) and log-compressed above it — see
/// [`column_half_height`] for why neither a plain scale nor clipping
/// survived being looked at. Any column with anything in it gets at least
/// [`COLUMN_FLOOR`], so one-versus-none stays visible.
///
/// Carries no directional events of its own: put a [`MarksLayer`] with the
/// mints and burns ON TOP. They keep the mark language exactly, and the two
/// layers stay ignorant of each other.
///
/// Hover names the column: the caller's tick format for its start, and the
/// count.
pub struct DensityLayer<'a>(pub &'a [DensityBin]);

impl SpineLayer for DensityLayer<'_> {
    fn paint(&self, c: &SpineCanvas<'_>) -> Option<String> {
        let columns = bin_columns(c.ruler.x_range(), COLUMN_W, self.0, |t| {
            c.scale.x_from_time(t as f64)
        });
        let knee = column_ceiling(&columns);
        let max = columns.iter().copied().fold(0.0_f64, f64::max);
        let mid = c.lane.center().y;
        let max_half = c.lane.height() * 0.5 - 1.0;
        for (i, &count) in columns.iter().enumerate() {
            let half = column_half_height(count, knee, max, max_half);
            if half <= 0.0 {
                continue;
            }
            let x0 = c.ruler.left() + i as f32 * COLUMN_W;
            let r = Rect::from_min_max(
                pos2(x0, mid - half),
                pos2((x0 + COLUMN_W).min(c.ruler.right()), mid + half),
            );
            c.painter.rect_filled(r, CornerRadius::ZERO, DENSITY_FILL);
        }

        // The column under the pointer, with its own span in seconds so the
        // formatter can pick a precision that matches the zoom.
        let p = c.hover?;
        let i = ((p.x - c.ruler.left()) / COLUMN_W).floor().max(0.0) as usize;
        let count = *columns.get(i).filter(|n| **n > 0.0)?;
        let t0 = c
            .scale
            .time_from_x_f32(c.ruler.left() + i as f32 * COLUMN_W)?;
        let t1 = c
            .scale
            .time_from_x_f32(c.ruler.left() + (i + 1) as f32 * COLUMN_W)?;
        let span = ((t1 - t0).round() as i64).max(1);
        Some(format!(
            "{}  ·  {}",
            (c.format_tick)(t0.round() as i64, span),
            count.round() as u64
        ))
    }
}

pub struct TimeSpine<'a> {
    state: &'a mut SpineState,
    format_tick: &'a dyn Fn(i64, i64) -> String,
    /// What is drawn along the spine, bottom first — see the module header.
    /// The lane is where "when did this act" and "which interval do I want to
    /// brush" meet, so whatever is being WATCHED belongs here.
    layers: Vec<Box<dyn SpineLayer + 'a>>,
    /// The one moment this view is ABOUT, if it is about one — see
    /// [`TimeSpine::pin`].
    pin: Option<i64>,
    height: f32,
    show_play: bool,
    /// May the reader drag out a range to FILTER by? See
    /// [`TimeSpine::brushing`].
    brushing: bool,
    /// Extra space reserved left of the ruler, for a caller that draws a row
    /// of labels beneath (see [`crate::activity_lanes::ActivityLanes`]).
    left_inset: f32,
}

impl<'a> TimeSpine<'a> {
    pub fn new(state: &'a mut SpineState) -> Self {
        Self {
            state,
            format_tick: &compact_tick_label,
            layers: Vec::new(),
            pin: None,
            height: 56.0,
            show_play: true,
            brushing: true,
            left_inset: 0.0,
        }
    }

    /// Allow dragging out a range to filter by. Default on.
    ///
    /// Turn it off where the spine is a NAVIGATOR rather than a filter — a
    /// surface with one long list and no linked faces to narrow together. The
    /// brush is a second, modal way to hide rows, and where nothing else
    /// responds to it, it mostly reads as the list mysteriously going short.
    ///
    /// Disabling also clears any brush already set, so a range cannot outlive
    /// the affordance that made it and leave rows filtered with no visible
    /// control to undo it.
    pub fn brushing(mut self, on: bool) -> Self {
        self.brushing = on;
        self
    }

    /// Reserve space left of the ruler so a label column beneath can line up.
    pub fn left_inset(mut self, px: f32) -> Self {
        self.left_inset = px.max(0.0);
        self
    }

    pub fn format_tick(mut self, f: &'a dyn Fn(i64, i64) -> String) -> Self {
        self.format_tick = f;
        self
    }

    /// Add a layer on top of those already added. Layers paint bottom-first
    /// in the order given, behind the ticks and the playhead.
    pub fn layer(mut self, layer: impl SpineLayer + 'a) -> Self {
        self.layers.push(Box::new(layer));
        self
    }

    /// Sugar for a [`CoverageLayer`]: faint `(from, to)` bands — e.g. each
    /// party's `watched_from` … cursor. Add it FIRST; it is a ground, and
    /// anything added before it will be painted over.
    pub fn coverage(self, bands: &'a [(i64, i64)]) -> Self {
        self.layer(CoverageLayer(bands))
    }

    /// Sugar for a [`MarksLayer`]: WHEN the watched thing acted, one hairline
    /// per event, so the answer to "when did they trade" sits on the axis you
    /// drag to isolate it. See the layer for where this form stops working.
    pub fn marks(self, marks: &'a [(i64, MarkKind)]) -> Self {
        self.layer(MarksLayer(marks))
    }

    /// Sugar for a [`DensityLayer`]: HOW MUCH happened when, as a waveform.
    /// Put the rare directional events on top with [`TimeSpine::marks`].
    pub fn density(self, bins: &'a [DensityBin]) -> Self {
        self.layer(DensityLayer(bins))
    }

    /// The one moment this view is ABOUT — a deep-linked event, a selected
    /// row, the thing somebody was sent here to look at.
    ///
    /// # Why this is not a [`MarkKind`]
    ///
    /// Marks answer "when did this thing act, and which way did the value
    /// go" — they are a direction. A pin answers "which of these is the one",
    /// which is not a direction at all, and folding it in as a third variant
    /// would make `MarkKind` mean two unrelated things.
    ///
    /// # Why not just move the playhead
    ///
    /// Because the playhead REVEALS: everything after it is hidden. Sending
    /// it back to a linked event would erase all the later history the reader
    /// can see, which is the opposite of helping them place it. The pin says
    /// "here" without changing what is shown — the two verbs stay separate,
    /// as the module header insists.
    ///
    /// Drawn on the ruler rather than in the brush lane, so it reads against
    /// the DATES rather than among the events.
    pub fn pin(mut self, at: Option<i64>) -> Self {
        self.pin = at;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn show_play(mut self, on: bool) -> Self {
        self.show_play = on;
        self
    }

    pub fn show(self, ui: &mut Ui) -> TimeSpineResponse {
        let Self {
            state,
            format_tick,
            layers,
            pin,
            height,
            show_play,
            brushing,
            left_inset,
        } = self;
        state.tick(ui.ctx());
        let mut playhead_changed = state.playing;
        let mut brush_changed = false;

        // A brush set before brushing was turned off would keep filtering with
        // no control left to clear it — rows missing, and nothing on screen to
        // explain why. Clearing here means the flag alone is enough.
        if !brushing && state.brush.is_some() {
            state.set_brush(None);
            brush_changed = true;
        }

        let full_w = ui.available_width();
        // The ruler starts after the play button AND after any caller-reserved
        // gutter. `ActivityLanes` needs a label column to the left of the
        // ruler, and its ticks must sit under the ruler's dates — so BOTH have
        // to agree on where x starts, which means the spine has to be told.
        let play_w = if show_play { 30.0 } else { 0.0 } + left_inset;
        let (rect, response) =
            ui.allocate_exact_size(vec2(full_w, height), Sense::click_and_drag());
        let id = response.id;
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();

        // ── play button ────────────────────────────────────────────────────
        if show_play {
            // 24 = the button's own 30 minus its 6px gap. NOT `play_w`, which
            // now also carries the caller's label gutter.
            let brect = Rect::from_min_size(rect.min, vec2(24.0, height));
            let bresp = ui.interact(brect, id.with("play"), Sense::click());
            if bresp.clicked() {
                state.toggle_play();
                playhead_changed = true;
            }
            let c = brect.center();
            let col = if bresp.hovered() {
                visuals.strong_text_color()
            } else {
                visuals.text_color()
            };
            if state.playing {
                let w = 3.0;
                painter.rect_filled(
                    Rect::from_center_size(c - vec2(3.5, 0.0), vec2(w, 12.0)),
                    CornerRadius::ZERO,
                    col,
                );
                painter.rect_filled(
                    Rect::from_center_size(c + vec2(3.5, 0.0), vec2(w, 12.0)),
                    CornerRadius::ZERO,
                    col,
                );
            } else {
                painter.add(Shape::convex_polygon(
                    vec![
                        c + vec2(-4.5, -6.5),
                        c + vec2(6.0, 0.0),
                        c + vec2(-4.5, 6.5),
                    ],
                    col,
                    Stroke::NONE,
                ));
            }
        }

        // ── the ruler ──────────────────────────────────────────────────────
        let ruler = Rect::from_min_max(pos2(rect.min.x + play_w, rect.min.y), rect.max);
        let tick_lane =
            Rect::from_min_max(ruler.min, pos2(ruler.max.x, ruler.min.y + height * 0.5));
        let brush_lane = Rect::from_min_max(pos2(ruler.min.x, tick_lane.max.y), ruler.max);
        let scale =
            TimeScale::continuous(ruler.x_range(), state.view, state.domain.0, state.domain.1);

        // ── the layers ─────────────────────────────────────────────────────
        // Bottom first, in the caller's order, before the ticks so the ruler
        // and the playhead stay on top. The topmost layer with something
        // under the pointer names it.
        let canvas = SpineCanvas {
            painter: &painter,
            scale: &scale,
            ruler,
            lane: brush_lane,
            visuals,
            format_tick,
            hover: ui
                .input(|i| i.pointer.hover_pos())
                .filter(|p| brush_lane.contains(*p)),
        };
        let mut tooltip: Option<String> = None;
        for layer in &layers {
            if let Some(line) = layer.paint(&canvas) {
                tooltip = Some(line);
            }
        }

        // Baseline + ticks.
        painter.line_segment(
            [
                pos2(ruler.left(), tick_lane.bottom()),
                pos2(ruler.right(), tick_lane.bottom()),
            ],
            Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color),
        );
        paint_ticks(&scale, ui, &painter, tick_lane.y_range(), format_tick);

        // THE PIN — the one moment this view is about.
        //
        // On the ruler, not among the marks: it answers "which of these", not
        // "which way did it go". Drawn after the ticks so a date label cannot
        // sit on top of the one thing the reader was sent to find, and as a
        // full-height stem with a head so it is legible against a dense mark
        // lane — a wallet with four thousand events makes any one-pixel
        // tick invisible.
        if let Some(t) = pin
            && let Some(x) = scale.x_from_time_f32(t as f64)
            && x >= ruler.left()
            && x <= ruler.right()
        {
            let stem = Stroke::new(1.5_f32, PIN);
            painter.line_segment([pos2(x, ruler.top()), pos2(x, ruler.bottom())], stem);
            // A downward head at the top, so it reads as pointing AT the axis
            // rather than as a second playhead (which points up from below).
            let head = 4.0_f32;
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(x - head, ruler.top()),
                    pos2(x + head, ruler.top()),
                    pos2(x, ruler.top() + head * 1.6),
                ],
                PIN,
                Stroke::NONE,
            ));
        }

        // ── interaction ────────────────────────────────────────────────────
        // Tick lane: drag/click = playhead. Brush lane: drag = brush range;
        // double-click = clear brush. The lanes are the two verbs.
        let ptr = response.interact_pointer_pos();
        let press_origin: Option<Pos2> = ui.data(|d| d.get_temp(id.with("press")));
        if (response.drag_started() || (response.clicked() && press_origin.is_none()))
            && let Some(p) = ptr
        {
            ui.data_mut(|d| d.insert_temp(id.with("press"), p));
        }
        let origin = press_origin.or(ptr);
        if let (Some(p), Some(o)) = (ptr, origin) {
            // With brushing off, a drag anywhere — including the mark lane —
            // moves the PLAYHEAD. Otherwise dragging across the marks would
            // land in a dead zone that silently does nothing, which reads as
            // the widget being broken rather than as a disabled feature.
            let in_brush_lane = brushing && brush_lane.contains(o);
            if response.dragged() || response.clicked() {
                if in_brush_lane && response.dragged() {
                    if let (Some(ta), Some(tb)) =
                        (scale.time_from_x_f32(o.x), scale.time_from_x_f32(p.x))
                    {
                        state.set_brush(Some((ta.round() as i64, tb.round() as i64)));
                        brush_changed = true;
                    }
                } else if let Some(t) = scale.time_from_x_f32(p.x) {
                    state.set_playhead(t.round() as i64);
                    playhead_changed = true;
                }
            }
        }
        if response.drag_stopped() {
            ui.data_mut(|d| d.remove::<Pos2>(id.with("press")));
            // A brush narrower than a couple of pixels is a click, not a range.
            if let Some((a, b)) = state.brush
                && ((b - a) as f64 * scale.points_per_sec) < 2.0
            {
                state.set_brush(None);
                brush_changed = true;
            }
        }
        if brushing && response.double_clicked() && brush_lane.contains(ptr.unwrap_or(rect.min)) {
            state.set_brush(None);
            brush_changed = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            let zoom = ui.input(|i| i.zoom_delta());
            if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                if zoom != 1.0
                    && let Some(v) =
                        scale.zoom_at(p.x, zoom, (state.domain.1 - state.domain.0) as f64)
                {
                    state.view = clamp_view(v, state.domain);
                }
                if scroll.x != 0.0
                    && let Some(v) = scale.pan(-scroll.x)
                {
                    state.view = clamp_view(v, state.domain);
                }
            }
        }

        // ── brush + playhead marks ─────────────────────────────────────────
        if let Some((a, b)) = state.brush
            && let (Some(xa), Some(xb)) = (
                scale.x_from_time_f32(a as f64),
                scale.x_from_time_f32(b as f64),
            )
        {
            let r = Rect::from_x_y_ranges(
                Rangef::new(xa.max(ruler.left()), xb.min(ruler.right())),
                brush_lane.y_range(),
            );
            let mut fill = visuals.selection.bg_fill;
            fill = fill.linear_multiply(0.55);
            painter.rect_filled(r, CornerRadius::same(2), fill);
            let edge = Stroke::new(1.5_f32, visuals.selection.stroke.color);
            painter.line_segment([r.left_top(), r.left_bottom()], edge);
            painter.line_segment([r.right_top(), r.right_bottom()], edge);
        }

        // Playhead — tweened so a click glides rather than teleports (and a
        // playing head is smooth even at coarse tick rates).
        // Inset by the marker's own half-width. Clamping to the bare ruler
        // edge drew the head's arrow centred ON the edge, so `painter_at`
        // sliced half of it off — and the playhead sits at the right-hand end
        // by default, which made the clipped state the FIRST thing a reader
        // sees on a narrow screen.
        let head_x = Rangef::new(
            ruler.left() + PLAYHEAD_HALF_W,
            (ruler.right() - PLAYHEAD_HALF_W).max(ruler.left() + PLAYHEAD_HALF_W),
        );
        let target_x = scale
            .x_from_time_f32(state.playhead as f64)
            .unwrap_or(ruler.right())
            .clamp(head_x.min, head_x.max);
        let x = tween(
            ui.ctx(),
            id.with("playhead"),
            target_x,
            0.18,
            Easing::OutCubic,
        );
        let ph_col = visuals.strong_text_color();
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.5_f32, ph_col),
        );
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x - PLAYHEAD_HALF_W, rect.top()),
                pos2(x + PLAYHEAD_HALF_W, rect.top()),
                pos2(x, rect.top() + 7.0),
            ],
            ph_col,
            Stroke::NONE,
        ));
        // Time label as a filled BADGE, clamped inside the ruler so it never
        // clips at the ends.
        //
        // Bare text here sat directly on top of the event marks and was
        // unreadable against them — the one number on the widget that says
        // WHERE YOU ARE, illegible exactly when the history is dense enough
        // to need a scrubber. A pill gives it its own background so it reads
        // over marks, brush fill and ticks alike.
        let label = format_tick(state.playhead, 86_400);
        let galley = ui.painter().layout_no_wrap(
            label,
            egui::TextStyle::Small.resolve(ui.style()),
            // The chip carries the contrast, so the text takes the colour that
            // reads against the FILL rather than against the widget.
            visuals.strong_text_color(),
        );
        let pad = vec2(5.0, 2.0);
        let badge_size = galley.size() + pad * 2.0;
        // Sits beside the playhead, flipping to its left when it would
        // otherwise run off the right-hand end.
        let bx = if x + 6.0 + badge_size.x <= ruler.right() - 2.0 {
            x + 6.0
        } else {
            (x - 6.0 - badge_size.x).max(ruler.left() + 2.0)
        };
        let badge = Rect::from_min_size(pos2(bx, rect.bottom() - badge_size.y - 1.0), badge_size);
        painter.rect_filled(
            badge,
            CornerRadius::same(3),
            // Opaque, not a tint: a translucent chip over dense marks is the
            // same illegibility with extra steps.
            visuals.extreme_bg_color,
        );
        painter.rect_stroke(
            badge,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, ph_col.linear_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        painter.galley(badge.min + pad, galley, visuals.strong_text_color());

        // What the topmost layer says is under the pointer. A silhouette can
        // be READ for shape but not for numbers, and "how many, exactly" is
        // the one question a mark lane could never answer either.
        if let Some(line) = tooltip {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                id.with("layer_tip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(egui::RichText::new(line).small());
            });
        }

        // Playing indicator glow on the head — eases in/out.
        let glow = tween_bool(
            ui.ctx(),
            id.with("glow"),
            state.playing,
            0.25,
            Easing::InOutCubic,
        );
        if glow > 0.0 {
            painter.circle_filled(
                pos2(x, rect.top() + 3.0),
                3.0 + 3.0 * glow,
                ph_col.linear_multiply(0.35 * glow),
            );
        }

        TimeSpineResponse {
            response,
            scale,
            playhead_changed,
            brush_changed,
        }
    }
}

/// One hairline per event in the brush lane: in rises from the midline, out
/// falls from it, neutral straddles it.
fn paint_marks(
    painter: &egui::Painter,
    scale: &TimeScale,
    ruler: &Rect,
    brush_lane: &Rect,
    marks: &[(i64, MarkKind)],
) {
    let mid = brush_lane.center().y;
    for &(t, kind) in marks {
        let Some(x) = scale.x_from_time_f32(t as f64) else {
            continue;
        };
        if x < ruler.left() || x > ruler.right() {
            continue;
        }
        let (y0, y1, col) = match kind {
            MarkKind::In => (mid, brush_lane.top() + 1.0, MARK_IN),
            MarkKind::Out => (mid, brush_lane.bottom() - 1.0, MARK_OUT),
            // Straddles the midline: the SHAPE says "no direction" before the
            // colour does, which matters because these are usually the
            // majority of the lane.
            MarkKind::Neutral => (mid - NEUTRAL_HALF, mid + NEUTRAL_HALF, MARK_NEUTRAL),
        };
        painter.line_segment(
            [pos2(x, y0), pos2(x, y1)],
            Stroke::new(1.0_f32, col.gamma_multiply(0.85)),
        );
    }
}

/// Re-bin `bins` into screen columns `column_w` wide across `x_range`.
///
/// Each bin's count is shared among the columns it overlaps in proportion to
/// the overlap, so a day bucket zoomed to two hundred pixels spreads evenly
/// and a hundred hour buckets zoomed out to one pixel SUM into it. Nothing on
/// screen is dropped — the same rule `CoverageLanes` follows for sub-pixel
/// runs — and the total across the columns equals the total of the bins that
/// fall inside the range.
///
/// `x_of` is the spine's time → x mapping. Bins wholly off-screen contribute
/// nothing; bins straddling an edge contribute the part that is on it.
pub fn bin_columns(
    x_range: Rangef,
    column_w: f32,
    bins: &[DensityBin],
    x_of: impl Fn(i64) -> Option<f64>,
) -> Vec<f64> {
    let width = x_range.span();
    if width <= 0.0 || column_w <= 0.0 {
        return Vec::new();
    }
    let n = (width / column_w).ceil() as usize;
    let mut columns = vec![0.0_f64; n];
    let left = x_range.min as f64;
    let cw = column_w as f64;
    for bin in bins {
        if bin.count == 0 {
            continue;
        }
        let (Some(x0), Some(x1)) = (x_of(bin.start), x_of(bin.start + bin.span.max(1))) else {
            continue;
        };
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        // A bin narrower than a hair still has to land somewhere: give it a
        // minimal width so the overlap arithmetic below has something to share.
        let x1 = x1.max(x0 + 1e-6);
        let bin_w = x1 - x0;
        let first = (((x0 - left) / cw).floor().max(0.0)) as usize;
        let last = (((x1 - left) / cw).ceil().max(0.0)) as usize;
        for (i, slot) in columns
            .iter_mut()
            .enumerate()
            .take(last.min(n))
            .skip(first.min(n))
        {
            let c0 = left + i as f64 * cw;
            let c1 = c0 + cw;
            let overlap = (x1.min(c1) - x0.max(c0)).max(0.0);
            if overlap > 0.0 {
                *slot += bin.count as f64 * overlap / bin_w;
            }
        }
    }
    columns
}

/// The share of non-empty columns that sit below the knee; the rest are
/// compressed into the top of the lane.
///
/// One in twenty. A three-year policy at two pixels a column has ~700
/// columns, so ~35 of them — a four-day mint plus two or three trading
/// frenzies — live above the knee, while every ordinary week keeps its own
/// height.
const CEILING_QUANTILE: f64 = 0.95;

/// How much of the lane the body of the history gets. The top quarter is for
/// everything above the knee.
const KNEE_HEIGHT: f32 = 0.75;

/// The knee: the count at which the square-root body hands over to the
/// compressed top — the [`CEILING_QUANTILE`] of the NON-EMPTY columns, so a
/// handful of outliers do not set the scale for everything else.
///
/// Against the true maximum a mint burst flattens three years of aftermarket
/// into a hairline; this lets the body breathe. Empty columns are excluded
/// because a quiet history is mostly zeros, and a quantile over them would put
/// the knee at zero.
pub fn column_ceiling(columns: &[f64]) -> f64 {
    let mut filled: Vec<f64> = columns.iter().copied().filter(|c| *c > 0.0).collect();
    if filled.is_empty() {
        return 0.0;
    }
    filled.sort_by(|a, b| a.total_cmp(b));
    let i = ((filled.len() as f64 - 1.0) * CEILING_QUANTILE).round() as usize;
    filled[i.min(filled.len() - 1)]
}

/// Half-height of a column holding `count`, given the `knee` (see
/// [`column_ceiling`]), the tallest column `max`, and `max_half` points.
///
/// A SOFT KNEE, the way a compressor treats a loud passage. Up to the knee the
/// body is square-root scaled into the lower [`KNEE_HEIGHT`] of the lane —
/// linear crushes the aftermarket under the mint, log flattens everything
/// into a slab, and the root sits between, with a quarter of the knee's
/// traffic at half the body's height. Above the knee the excess is
/// log-compressed into the remaining top, so the tallest column reaches full
/// height and nothing in between is flat.
///
/// This replaced CLIPPING at the knee, which was drawn and looked at: on a
/// five-year collection the busiest five percent of columns are not a few
/// one-day spikes but a weeks-long frenzy, and clipping turned it into
/// flat-topped slabs that read as a different chart glued in. With no column
/// above the knee the body takes the whole lane.
///
/// Floored at [`COLUMN_FLOOR`] for any non-empty column, so one-versus-none
/// survives the scale.
pub fn column_half_height(count: f64, knee: f64, max: f64, max_half: f32) -> f32 {
    if count <= 0.0 || knee <= 0.0 || max_half <= 0.0 {
        return 0.0;
    }
    let compressed_top = max > knee;
    let body_top = if compressed_top { KNEE_HEIGHT } else { 1.0 };
    let f = if count <= knee || !compressed_top {
        (count / knee).min(1.0).sqrt() as f32 * body_top
    } else {
        let excess = ((count / knee).ln() / (max / knee).ln()).clamp(0.0, 1.0) as f32;
        body_top + (1.0 - body_top) * excess
    };
    (f * max_half).clamp(COLUMN_FLOOR.min(max_half), max_half)
}

fn clamp_view(v: TimeView, domain: (i64, i64)) -> TimeView {
    let (d0, d1) = (domain.0 as f64, domain.1 as f64);
    let spanned = v.spanned.min(d1 - d0).max(1.0);
    let min = v.min.clamp(d0, d1 - spanned);
    TimeView { min, spanned }
}

/// Convenience for faces: the x of `t` on the spine's scale, or `None` if the
/// scale is empty.
pub fn x_of(scale: &TimeScale, t: i64) -> Option<f32> {
    scale.x_from_time_f32(t as f64)
}

#[allow(dead_code)]
fn _size_hint() -> Vec2 {
    Vec2::ZERO
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from re_time_ruler `test_time_ranges_ui`: segment ends round-trip.
    #[test]
    fn segments_round_trip() {
        let s = TimeScale::new(
            Rangef::new(100.0, 1000.0),
            TimeView {
                min: 0.5,
                spanned: 14.2,
            },
            &[(0, 0), (1, 5), (10, 100)],
        );
        for seg in &s.segments {
            assert!((s.time_from_x(seg.x.0).unwrap() - seg.time.0).abs() < 1e-9);
            assert!((s.time_from_x(seg.x.1).unwrap() - seg.time.1).abs() < 1e-9);
            let min_x = s.x_from_time(seg.time.0).unwrap();
            assert!((min_x - seg.x.0).abs() < 0.5);
            let max_x = s.x_from_time(seg.time.1).unwrap();
            assert!((max_x - seg.x.1).abs() < 0.5);
        }
    }

    /// Ported from re_time_ruler `test_time_ranges_ui_2`: every pixel and every
    /// second round-trip within tolerance across a gap.
    #[test]
    fn pixels_and_seconds_round_trip_across_a_gap() {
        let s = TimeScale::new(
            Rangef::new(0.0, 500.0),
            TimeView {
                min: 0.0,
                spanned: 50.0,
            },
            &[(10, 20), (30, 40)],
        );
        for x_in in 0..=500 {
            let x_in = x_in as f64;
            let t = s.time_from_x(x_in).unwrap();
            let x_out = s.x_from_time(t).unwrap();
            assert!((x_in - x_out).abs() < 0.5, "x {x_in} -> {t} -> {x_out}");
        }
        for t_in in 0..=50 {
            let t_in = t_in as f64;
            let x = s.x_from_time(t_in).unwrap();
            let t_out = s.time_from_x(x).unwrap();
            assert!((t_in - t_out).abs() < 0.1, "t {t_in} -> {x} -> {t_out}");
        }
    }

    #[test]
    fn continuous_scale_is_linear_and_aligned() {
        let s = TimeScale::continuous(
            Rangef::new(0.0, 1000.0),
            TimeView::covering(1000, 2000),
            1000,
            2000,
        );
        assert!((s.x_from_time(1000.0).unwrap() - 0.0).abs() < 1e-6);
        assert!((s.x_from_time(1500.0).unwrap() - 500.0).abs() < 1e-6);
        assert!((s.x_from_time(2000.0).unwrap() - 1000.0).abs() < 1e-6);
        assert!((s.time_from_x(250.0).unwrap() - 1250.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_and_pan_return_sane_views() {
        let s = TimeScale::continuous(
            Rangef::new(0.0, 1000.0),
            TimeView::covering(0, 1000),
            0,
            1000,
        );
        let z = s.zoom_at(500.0, 2.0, 1e9).unwrap();
        assert!((z.spanned - 500.0).abs() < 1e-6);
        assert!(
            (z.min - 250.0).abs() < 1e-6,
            "zoom about centre keeps centre"
        );
        let p = s.pan(100.0).unwrap();
        assert!((p.min - 100.0).abs() < 1e-6);
        assert!((p.spanned - 1000.0).abs() < 1e-6);
        // never zoom out past the max span
        let z = s.zoom_at(0.0, 0.1, 2000.0).unwrap();
        assert!((z.spanned - 2000.0).abs() < 1e-6);
    }

    #[test]
    fn tick_ladder_matches_rerun_scaled_to_seconds() {
        assert_eq!(next_tick_step_secs(1), 10);
        assert_eq!(next_tick_step_secs(10), 60);
        assert_eq!(next_tick_step_secs(60), 600);
        assert_eq!(next_tick_step_secs(600), 3600);
        assert_eq!(next_tick_step_secs(3600), 43_200);
        assert_eq!(next_tick_step_secs(43_200), 86_400);
        assert_eq!(next_tick_step_secs(86_400), 864_000);
    }

    #[test]
    fn dates_and_labels() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_596_059_091), "2020-07-29");
        // Mekka S1 floor.
        assert_eq!(
            format_date(1_596_059_091 + (160_419_671 - 4_492_800)),
            "2025-07-08"
        );
        assert_eq!(compact_tick_label(86_400 * 3, 86_400), "1970-01-04");
        assert_eq!(compact_tick_label(3661, 60), "01:01");
        assert_eq!(compact_tick_label(3661, 1), "01:01:01");
    }

    // ── the density lane ───────────────────────────────────────────────────

    /// A linear mapping over `[0, 1000]` seconds onto `[0, 1000]` points.
    fn unit_x(t: i64) -> Option<f64> {
        Some(t as f64)
    }

    fn bin(start: i64, span: i64, count: u64) -> DensityBin {
        DensityBin { start, span, count }
    }

    /// THE INVARIANT: nothing on screen is dropped. Whatever the bin and
    /// column widths, the columns sum to the bins that fall on the axis.
    #[test]
    fn rebinning_conserves_the_count() {
        let bins = [
            bin(0, 100, 7),
            bin(100, 100, 3000),
            bin(250, 1, 1),
            bin(500, 500, 42),
        ];
        for cw in [1.0, 2.0, 3.0, 17.0] {
            let cols = bin_columns(Rangef::new(0.0, 1000.0), cw, &bins, unit_x);
            let total: f64 = cols.iter().sum();
            assert!((total - 3050.0).abs() < 1e-6, "cw {cw}: {total}");
        }
    }

    /// A day bucket zoomed to two hundred pixels spreads evenly across them
    /// rather than piling into its first column.
    #[test]
    fn a_wide_bin_spreads_across_its_columns() {
        let cols = bin_columns(Rangef::new(0.0, 1000.0), 2.0, &[bin(0, 200, 100)], unit_x);
        for c in &cols[..100] {
            assert!((c - 1.0).abs() < 1e-9, "{c}");
        }
        assert!(cols[100..].iter().all(|c| *c == 0.0));
    }

    /// Many narrow bins under one column SUM, rather than the last one winning.
    #[test]
    fn narrow_bins_sum_into_their_column() {
        let bins: Vec<DensityBin> = (0..10).map(|i| bin(i, 1, 5)).collect();
        let cols = bin_columns(Rangef::new(0.0, 1000.0), 10.0, &bins, unit_x);
        assert!((cols[0] - 50.0).abs() < 1e-9, "{}", cols[0]);
    }

    /// Bins straddling the edge contribute the part on screen; bins wholly
    /// off it contribute nothing and do not panic.
    #[test]
    fn off_screen_bins_are_clipped_not_dropped_into_the_edge() {
        let bins = [bin(-100, 200, 100), bin(-500, 100, 999), bin(950, 100, 100)];
        let cols = bin_columns(Rangef::new(0.0, 1000.0), 10.0, &bins, unit_x);
        let total: f64 = cols.iter().sum();
        assert!((total - 100.0).abs() < 1e-6, "{total}");
        // Half of each straddling bin is on screen — 50 apiece — but the
        // first is 200 px wide and the last 100, so they land at different
        // densities: 50 over ten columns, then 50 over five.
        assert!((cols[0] - 5.0).abs() < 1e-9, "{}", cols[0]);
        assert!((cols[99] - 10.0).abs() < 1e-9, "{}", cols[99]);
    }

    #[test]
    fn a_degenerate_range_yields_no_columns() {
        assert!(bin_columns(Rangef::new(5.0, 5.0), 2.0, &[bin(0, 1, 1)], unit_x).is_empty());
        assert!(bin_columns(Rangef::new(0.0, 10.0), 0.0, &[bin(0, 1, 1)], unit_x).is_empty());
    }

    /// The body is root-scaled into the lower part of the lane, the knee hands
    /// over to a compressed top, the TALLEST column fills the lane, nothing
    /// flat-tops, and any column with something in it is at least the floor —
    /// one-versus-none is the distinction the mark lane showed for free.
    #[test]
    fn column_heights_have_a_soft_knee_and_a_floor() {
        let (knee, max, max_half) = (100.0, 5000.0, 12.0);
        assert_eq!(column_half_height(0.0, knee, max, max_half), 0.0);
        let one = column_half_height(0.2, knee, max, max_half);
        assert_eq!(one, COLUMN_FLOOR, "a single event is visible: {one}");
        // The body: a quarter of the knee stands at half the body's height.
        let body_top = max_half * KNEE_HEIGHT;
        let at_knee = column_half_height(knee, knee, max, max_half);
        assert!((at_knee - body_top).abs() < 1e-5, "{at_knee}");
        let quarter = column_half_height(25.0, knee, max, max_half);
        assert!((quarter - body_top * 0.5).abs() < 1e-5, "{quarter}");
        // Above the knee: still rising, NOT flat — a frenzy keeps its shape.
        let (a, b, c) = (
            column_half_height(200.0, knee, max, max_half),
            column_half_height(1000.0, knee, max, max_half),
            column_half_height(4000.0, knee, max, max_half),
        );
        assert!(at_knee < a && a < b && b < c && c < max_half, "{a} {b} {c}");
        // The tallest column fills the lane; nothing exceeds it.
        assert_eq!(column_half_height(max, knee, max, max_half), max_half);
        assert!(column_half_height(max * 2.0, knee, max, max_half) <= max_half);
    }

    /// With nothing above the knee there is nothing to compress, so the body
    /// takes the WHOLE lane rather than leaving the top quarter empty.
    #[test]
    fn without_outliers_the_body_uses_the_whole_lane() {
        let h = column_half_height(100.0, 100.0, 100.0, 12.0);
        assert_eq!(h, 12.0);
        let h = column_half_height(25.0, 100.0, 100.0, 12.0);
        assert!((h - 6.0).abs() < 1e-5, "{h}");
    }

    /// THE CEILING IS ROBUST. A four-day mint of thousands against three years
    /// of tens must not set the scale — against the true max the aftermarket
    /// is a hairline and the lane is a flat line with one tower.
    #[test]
    fn the_ceiling_ignores_the_outliers_and_the_zeros() {
        let mut columns = vec![0.0; 300];
        for (i, c) in columns.iter_mut().enumerate() {
            *c = match i {
                0..=3 => 3000.0,
                100..=160 => 0.0, // the dead stretch
                _ => 10.0 + (i % 7) as f64,
            };
        }
        let ceiling = column_ceiling(&columns);
        assert!(
            ceiling < 100.0,
            "the mint must not set the ceiling: {ceiling}"
        );
        assert!(ceiling >= 10.0, "nor may the zeros drag it down: {ceiling}");
        // The body of the history now has a height of its own.
        let quiet = column_half_height(10.0, ceiling, 3000.0, 11.0);
        assert!(
            quiet > 5.0,
            "a quiet day is visible, not a hairline: {quiet}"
        );
        assert_eq!(column_ceiling(&[]), 0.0);
        assert_eq!(column_ceiling(&[0.0, 0.0]), 0.0);
        assert_eq!(column_ceiling(&[7.0]), 7.0);
    }

    #[test]
    fn spine_state_opens_at_end_and_filters() {
        let mut s = SpineState::new((100, 200));
        assert_eq!(s.playhead, 200, "opens at the end");
        assert!(s.shows(150) && s.shows(200) && !s.shows(201));
        s.set_brush(Some((180, 120))); // reversed, out-of-order input
        assert_eq!(s.brush, Some((120, 180)));
        assert!(!s.shows(110), "outside brush");
        assert!(s.shows(150));
        s.set_playhead(130);
        assert!(!s.shows(150), "beyond playhead");
        assert!(!s.playing);
        s.toggle_play();
        assert!(s.playing);
        assert_eq!(s.playhead, 130, "play from mid does not rewind");
        s.set_playhead(200);
        s.toggle_play();
        assert_eq!(s.playhead, 100, "play from the end rewinds");
    }

    #[test]
    fn play_advances_with_wall_clock_and_stops_at_end() {
        let ctx = egui::Context::default();
        let step = |t: f64| crate::motion::tests::step(&ctx, t);
        let mut s = SpineState::new((0, 1000));
        s.play_duration = 10.0; // 100 s of data per wall second
        s.set_playhead(0);
        s.toggle_play();
        step(0.0);
        s.tick(&ctx);
        let _ = ctx.end_pass();
        step(1.0);
        s.tick(&ctx);
        let _ = ctx.end_pass();
        assert!((s.playhead - 100).abs() <= 1, "got {}", s.playhead);
        step(30.0);
        s.tick(&ctx);
        let _ = ctx.end_pass();
        assert_eq!(s.playhead, 1000);
        assert!(!s.playing, "stops at the end");
    }
}
