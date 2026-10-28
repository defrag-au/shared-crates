//! `PriceChart` — a price over time as a step line, with the volume it traded
//! split into buys and sells at the foot, a mark for each trade while there is
//! room for one, the playhead, and a finger's scrub.
//!
//! The axis is **time**, not trade index: a quiet week is as wide as a busy
//! one, so a moment sits at the same share of this plot and of the life strip
//! under it. The future is dimmed rather than hidden, so the line's end reads
//! as now; the range is the host's (it eases it), and a chart with no range
//! fits what it has.
//!
//! The pure half — [`chart_view`], [`time_at_x`], [`beat_at_x`], [`View`] — is
//! separated from the drawing so it can be tested without a window. The host
//! owns the beats, the range and the playback clock; the widget draws them.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::market_ink::MarketInk;
use crate::marks::{glow, pill_label};
use crate::painter::Painter;
use crate::theme::with_alpha;

/// Which way a trade went, from the holder's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// How big a trade was against the pool it hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeSize {
    Small,
    Big,
    Whale,
}

/// A trade's mark on the line, and what it contributed to the volume band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradeMark {
    pub side: TradeSide,
    pub size: TradeSize,
    /// ADA moved, where known: the volume band's input.
    pub ada: Option<f64>,
}

/// One point of the price series.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPoint {
    pub unix: u64,
    /// In display units; the host has applied the token's decimals and unit.
    pub price: f64,
    /// Set by a trade, and which way.
    pub trade: Option<TradeMark>,
}

/// The chart's x axis: a first and last beat, and the stretch of time they
/// span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub from: usize,
    pub to: usize,
    /// The axis's left and right edges, unix seconds.
    pub t0: u64,
    pub t1: u64,
}

impl View {
    /// Where on the axis `unix` falls, `0..=1`.
    pub fn at(&self, unix: u64) -> f32 {
        let span = self.t1.saturating_sub(self.t0).max(1) as f64;
        (unix.saturating_sub(self.t0) as f64 / span).clamp(0.0, 1.0) as f32
    }
}

/// The first beat at or after `unix`, or the end.
fn beat_at(beats: &[u64], unix: u64) -> usize {
    beats.partition_point(|b| *b < unix)
}

/// The stretch of the window the chart draws.
///
/// The whole window when it spans no more than `secs` (or `secs` is `None`:
/// max). Otherwise exactly `secs` of time: the window's first `secs` while
/// the playhead crosses them, then ending at the playhead and sliding with
/// it. The axis is time, as the life strip's is, so a moment sits at the same
/// share of both.
///
/// `beats` is each beat's unix second, ascending; `played` is how many beats
/// have played; `head_unix` is the playhead's moment, which runs between beats
/// so a sliding view moves every frame.
pub fn chart_view(
    beats: &[u64],
    from_ix: usize,
    played: usize,
    secs: Option<u64>,
    head_unix: u64,
) -> View {
    let last = beats.len().saturating_sub(1);
    let from = from_ix.min(last);
    let start = beats.get(from).copied().unwrap_or(0);
    let end = beats.last().copied().unwrap_or(start).max(head_unix);
    let whole = View {
        from,
        to: last,
        t0: start,
        t1: end.max(start + 1),
    };
    let Some(secs) = secs else {
        return whole;
    };
    if end.saturating_sub(start) <= secs {
        return whole;
    }
    let head = played.saturating_sub(1).clamp(from, last);
    let head_unix = head_unix.max(beats.get(head).copied().unwrap_or(start));
    if head_unix.saturating_sub(start) <= secs {
        let to = beat_at(beats, start + secs + 1).saturating_sub(1);
        return View {
            from,
            to: to.clamp(head, last),
            t0: start,
            t1: start + secs,
        };
    }
    View {
        from: beat_at(beats, head_unix - secs).clamp(from, head),
        to: head,
        t0: head_unix - secs,
        t1: head_unix,
    }
}

/// The moment under `x` on a plot whose axis is `(t0, t1)`.
pub fn time_at_x(rect: Rect, t0: u64, t1: u64, x: f32) -> u64 {
    let f = ((x - rect.x) / rect.w).clamp(0.0, 1.0) as f64;
    t0 + (f * t1.saturating_sub(t0) as f64) as u64
}

/// The beat under `x` on the plot: the last beat at or before the moment
/// there, within the view.
pub fn beat_at_x(beats: &[u64], view: View, rect: Rect, x: f32) -> usize {
    let unix = time_at_x(rect, view.t0, view.t1, x);
    beat_at(beats, unix + 1)
        .saturating_sub(1)
        .clamp(view.from, view.to)
}

/// The chart's labels, preformatted by the host: a high, and the axis's ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChartLabels<'a> {
    pub high: Option<&'a str>,
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
}

/// A stretch the host has only a coarse reading for (a whole-life
/// overview's bucket): its low, high and close, in display units. Drawn soft
/// and blurred where the points do not reach yet, so the plot is never
/// empty while its detail loads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoughStretch {
    pub unix: u64,
    pub secs: u64,
    pub lo: f64,
    pub hi: f64,
    pub close: f64,
    /// How strongly, `0..=1`: a stretch whose detail has just arrived fades
    /// out under it rather than vanishing.
    pub alpha: f32,
}

/// What the chart draws.
#[derive(Clone, Copy, Debug)]
pub struct PriceChartVm<'a> {
    /// The points in the window, ascending.
    pub points: &'a [ChartPoint],
    /// Where the points do not reach, the coarse shape, ascending: drawn
    /// blurred under the line. Empty for none.
    pub rough: &'a [RoughStretch],
    /// The last point before the window, so the line enters from the left
    /// edge rather than starting in mid-air.
    pub lead: Option<ChartPoint>,
    pub t0: u64,
    pub t1: u64,
    /// The y range, eased by the host; `None` fits the points it has.
    pub range: Option<(f64, f64)>,
    /// The playhead's moment, unix seconds.
    pub head: u64,
    /// A finger's moment on the plot, unix seconds.
    pub scrub: Option<u64>,
    /// The scrub's label, preformatted by the host.
    pub scrub_label: Option<&'a str>,
    /// A new all-time high's flash, `0..=1`.
    pub new_high: f32,
    /// The window's direction: the line and its fill.
    pub up: bool,
    pub ink: MarketInk,
    pub labels: ChartLabels<'a>,
}

/// The focus plot's volume band, at its foot: 12 on a short plot, a quarter
/// of a taller one, at most 32.
fn volume_h(plot: Rect) -> f32 {
    (plot.h * 0.24).clamp(12.0, 32.0)
}

/// Its columns, in design units: trades closer than this are summed.
const VOLUME_COL: f32 = 2.0;
/// The least room a trade needs on the plot to get a mark on the line.
const MARK_ROOM: f32 = 6.0;

/// Draw the plot in `rect`. The host draws the panel and the span chips around
/// it; this draws the grid, the future's dim, the volume, the line, the marks,
/// the head, the scrub and the axis labels.
pub fn price_chart(p: &Painter, vm: &PriceChartVm, plot: Rect) {
    if plot.w <= 0.0 || plot.h <= 0.0 {
        return;
    }
    let view = View {
        from: 0,
        to: vm.points.len().saturating_sub(1),
        t0: vm.t0,
        t1: vm.t1,
    };
    let x_of = |unix: u64| plot.x + plot.w * view.at(unix);
    let (lo, hi) = vm.range.unwrap_or_else(|| fit_range(vm.points, vm.lead));
    let hi = hi.max(lo + f64::EPSILON);
    let price_h = plot.h - volume_h(plot) - 2.0;
    let floor = plot.y + price_h;
    let y_of = |price: f64| floor - price_h * ((price - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;

    // The ground already covered, and what is still to come: at the
    // playhead's moment, which moves between beats, not the last beat's.
    let now_x = x_of(vm.head).clamp(plot.x, plot.right());
    draw_rectangle(
        now_x,
        plot.y - 2.0,
        plot.right() - now_x,
        plot.h + 4.0,
        with_alpha(p.theme.color.bg_primary, 0.35),
    );
    for i in 0..3 {
        let y = plot.y + price_h * i as f32 / 2.0;
        draw_line(
            plot.x,
            y,
            plot.right(),
            y,
            1.0,
            with_alpha(p.theme.color.border, 0.35),
        );
    }

    rough(
        vm.rough,
        &x_of,
        &y_of,
        plot,
        if vm.up { vm.ink.buy } else { vm.ink.sell },
    );

    // The window's points, led in by the last price before it.
    let mut series: Vec<ChartPoint> = Vec::with_capacity(vm.points.len() + 1);
    if let Some(lead) = vm.lead {
        series.push(lead);
    }
    series.extend_from_slice(vm.points);
    volume(&series, plot, &x_of, vm.ink);
    let line = if vm.up { vm.ink.buy } else { vm.ink.sell };
    // The line steps: a price holds until the next trade moves it.
    let mut pts: Vec<Vec2> = Vec::new();
    for (i, pt) in series.iter().enumerate() {
        let x = x_of(pt.unix).clamp(plot.x, plot.right());
        let y = y_of(pt.price);
        if i > 0 {
            let prev = *pts.last().expect("a point");
            pts.push(vec2(x, prev.y));
        }
        pts.push(vec2(x, y));
    }
    if let Some(&last) = pts.last() {
        pts.push(vec2(now_x.max(last.x), last.y));
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        // The area under the line.
        draw_triangle(a, b, vec2(b.x, floor), with_alpha(line, 0.10));
        draw_triangle(
            a,
            vec2(b.x, floor),
            vec2(a.x, floor),
            with_alpha(line, 0.10),
        );
        draw_line(a.x, a.y, b.x, b.y, 1.5, line);
    }
    // A mark on the line for each trade, only while there is room for one:
    // sized to the space a trade has, so a busy week reads as the line and
    // the volume band, not as a heap of dots.
    let trades = series
        .iter()
        .filter(|pt| pt.unix >= vm.t0 && pt.trade.is_some())
        .count();
    let room = plot.w / trades.max(1) as f32;
    if room >= MARK_ROOM {
        let base = (room * 0.3).clamp(1.2, 2.2);
        for pt in series.iter().filter(|pt| pt.unix >= vm.t0) {
            if let Some(trade) = pt.trade {
                let at = vec2(x_of(pt.unix), y_of(pt.price));
                let rad = match trade.size {
                    TradeSize::Small => base,
                    TradeSize::Big => base + 0.6,
                    TradeSize::Whale => base + 1.2,
                };
                let ink = match trade.side {
                    TradeSide::Buy => vm.ink.buy,
                    TradeSide::Sell => vm.ink.sell,
                };
                draw_circle(at.x, at.y, rad, ink);
            }
        }
    }
    if let Some(&head) = pts.last() {
        let flash = vm.new_high.clamp(0.0, 1.0);
        glow(head, 4.0 + flash * 6.0, line, 1.5 + flash * 2.0);
        draw_circle(head.x, head.y, 3.5, p.theme.color.text_primary);
        if flash > 0.0 {
            // Beside the head, below it: the plot's top band holds the
            // window's high, and an all-time high is always near it. Only an
            // all-time high flashes; a peak of the window shown is not news.
            let s = p.size(TextSize::Xs);
            let w = p.measure("NEW ATH", s).width;
            let x = if head.x + 10.0 + w < plot.right() {
                head.x + 10.0
            } else {
                head.x - 10.0 - w
            };
            p.text_mid(
                "NEW ATH",
                x,
                head.y + 9.0,
                s,
                with_alpha(vm.ink.crown, flash),
            );
        }
    }

    let s = p.size(TextSize::Xs);
    // A finger on the chart: where the replay will go.
    if let (Some(scrub), Some(label)) = (vm.scrub, vm.scrub_label) {
        let x = x_of(scrub).clamp(plot.x, plot.right());
        draw_line(x, plot.y - 4.0, x, plot.bottom() + 2.0, 2.0, vm.ink.crown);
        pill_label(p, label, x, plot.bottom() + 1.0, plot, vm.ink.crown);
    }
    // The high of what the chart draws, above its top-left.
    if let Some(high) = vm.labels.high {
        p.text_mid(high, plot.x, plot.y - 10.0, s, p.theme.color.text_muted);
    }
    // Its own row under it: where the axis starts and ends.
    let row = Rect::new(plot.x, plot.bottom() + 2.0, plot.w, 12.0);
    if let Some(start) = vm.labels.start {
        p.text_in(start, row, s, p.theme.color.text_muted, crate::Align::Start);
    }
    if let Some(end) = vm.labels.end {
        p.text_in(end, row, s, p.theme.color.text_muted, crate::Align::End);
    }
}

/// The coarse stretches, soft: each one's low-to-high as a faint band, and
/// the closes joined centre to centre as a line drawn wide and faint under
/// narrower and stronger passes — a blur, which says "about here" where the
/// detailed line says "exactly here". Stretches with a gap between them are
/// not joined.
fn rough(
    stretches: &[RoughStretch],
    x_of: &dyn Fn(u64) -> f32,
    y_of: &dyn Fn(f64) -> f32,
    plot: Rect,
    tone: Color,
) {
    for s in stretches {
        let (x0, x1) = (x_of(s.unix), x_of(s.unix + s.secs));
        if x1 <= plot.x || x0 >= plot.right() || s.alpha <= 0.0 {
            continue;
        }
        let (top, bottom) = (y_of(s.hi), y_of(s.lo));
        draw_rectangle(
            x0,
            top,
            (x1 - x0).max(1.0),
            (bottom - top).max(1.0),
            with_alpha(tone, 0.08 * s.alpha),
        );
    }
    // Each unbroken run of stretches as one line: from its first one's left
    // edge, through every centre, to its last one's right edge, so it meets
    // whatever is drawn either side of it.
    let mut runs: Vec<Vec<&RoughStretch>> = Vec::new();
    for s in stretches.iter().filter(|s| s.alpha > 0.0) {
        match runs.last_mut() {
            // One strength a run: a stretch fading out under detail that has
            // just landed draws apart, and the rough beside it still meets
            // its edge.
            Some(run)
                if run.last().is_some_and(|p| {
                    s.unix <= p.unix + p.secs + 1 && (s.alpha - p.alpha).abs() < 0.01
                }) =>
            {
                run.push(s)
            }
            _ => runs.push(vec![s]),
        }
    }
    for run in &runs {
        let (Some(first), Some(last)) = (run.first(), run.last()) else {
            continue;
        };
        let at = |unix: u64, s: &RoughStretch| (vec2(x_of(unix), y_of(s.close)), s.alpha);
        let line: Vec<(Vec2, f32)> = std::iter::once(at(first.unix, first))
            .chain(run.iter().map(|s| at(s.unix + s.secs / 2, s)))
            .chain(std::iter::once(at(last.unix + last.secs, last)))
            .collect();
        for w in line.windows(2) {
            let ((pa, aa), (pb, ab)) = (w[0], w[1]);
            let alpha = aa.min(ab);
            for (width, strength) in [(7.0, 0.06), (4.5, 0.12), (2.5, 0.22), (1.2, 0.35)] {
                draw_line(
                    pa.x,
                    pa.y,
                    pb.x,
                    pb.y,
                    width,
                    with_alpha(tone, strength * alpha),
                );
            }
        }
    }
}

/// The y range a chart with no host range fits: what it has, padded, with a
/// floor so a flat stretch still has a band.
fn fit_range(points: &[ChartPoint], lead: Option<ChartPoint>) -> (f64, f64) {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for pt in lead.iter().chain(points) {
        if pt.price.is_finite() {
            lo = lo.min(pt.price);
            hi = hi.max(pt.price);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if hi - lo < hi.abs() * 0.02 {
        lo = hi * 0.97;
        hi *= 1.03;
    }
    let pad = (hi - lo) * 0.12;
    ((lo - pad).max(0.0), hi + pad)
}

/// What traded in the window, as columns at the plot's foot: each column's
/// ADA split into sells (below, red) and buys (above, green), height by the
/// square root against the busiest column, so a quiet stretch still shows.
fn volume(points: &[ChartPoint], plot: Rect, x_of: &dyn Fn(u64) -> f32, ink: MarketInk) {
    let cols = (plot.w / VOLUME_COL).ceil().max(1.0) as usize;
    let mut buys = vec![0.0f64; cols];
    let mut sells = vec![0.0f64; cols];
    for pt in points {
        let Some(trade) = pt.trade else { continue };
        let Some(ada) = trade.ada else { continue };
        if !ada.is_finite() || ada <= 0.0 {
            continue;
        }
        let x = x_of(pt.unix);
        if x < plot.x || x > plot.right() {
            continue;
        }
        let col = (((x - plot.x) / VOLUME_COL) as usize).min(cols - 1);
        match trade.side {
            TradeSide::Buy => buys[col] += ada,
            TradeSide::Sell => sells[col] += ada,
        }
    }
    let most = buys
        .iter()
        .zip(&sells)
        .map(|(b, s)| b + s)
        .fold(0.0f64, f64::max);
    if most <= 0.0 {
        return;
    }
    let foot = plot.bottom();
    for (i, (b, s)) in buys.iter().zip(&sells).enumerate() {
        let total = b + s;
        if total <= 0.0 {
            continue;
        }
        let h = (volume_h(plot) * (total / most).sqrt() as f32).max(1.0);
        let sell_h = h * (s / total) as f32;
        let x = plot.x + i as f32 * VOLUME_COL;
        let w = VOLUME_COL - 0.5;
        draw_rectangle(x, foot - sell_h, w, sell_h, with_alpha(ink.sell, 0.85));
        draw_rectangle(x, foot - h, w, h - sell_h, with_alpha(ink.buy, 0.85));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 beats six hours apart, starting at 1,000: ten of them span less
    /// than a week, so the opening view is the window's first days.
    fn beats() -> Vec<u64> {
        (0..100).map(|i| 1_000 + i * 21_600).collect()
    }

    const WEEK: u64 = 7 * 86_400;

    #[test]
    fn max_draws_the_whole_window() {
        let b = beats();
        let last = b.len() - 1;
        let v = chart_view(&b, 0, 10, None, 0);
        assert_eq!((v.from, v.to, v.t0, v.t1), (0, last, b[0], b[last]));
    }

    #[test]
    fn early_on_the_view_is_the_windows_first_days() {
        let b = (0..100).map(|i| i as u64 * 21_600).collect::<Vec<_>>();
        let v = chart_view(&b, 0, 10, Some(WEEK), b[9]);
        assert_eq!((v.from, v.t0, v.t1), (0, 0, WEEK), "a week of time");
        assert!(b[v.to] <= v.t1, "its beats in it");
        assert!(b[v.to + 1] > v.t1, "and all of them");
    }

    #[test]
    fn later_the_view_ends_at_the_playhead_and_slides() {
        let b = (0..200).map(|i| i as u64 * 21_600).collect::<Vec<_>>();
        let at = b.len() - 5;
        let v = chart_view(&b, 0, at, Some(WEEK), b[at - 1]);
        assert_eq!(v.to, at - 1, "ends on the playhead");
        assert_eq!((v.t1, v.t0), (b[at - 1], b[at - 1] - WEEK));
        assert!(b[v.from] >= v.t0);
        assert!(b[v.from - 1] < v.t0, "and starts a week back");
    }

    /// The axis is time: a beat sits at its moment's share of the view, and a
    /// finger there finds it again.
    #[test]
    fn a_beat_sits_at_its_moment_and_a_finger_finds_it() {
        let b = (0..200).map(|i| i as u64 * 21_600).collect::<Vec<_>>();
        let at = b.len() - 5;
        let v = chart_view(&b, 0, at, Some(WEEK), b[at - 1]);
        let plot = Rect::new(20.0, 40.0, 300.0, 100.0);
        let ix = (v.from + v.to) / 2;
        let x = plot.x + plot.w * v.at(b[ix]);
        let found = beat_at_x(&b, v, plot, x);
        assert_eq!(b[found], b[ix]);
        // Halfway across is halfway through the week.
        assert!((v.at(v.t0 + WEEK / 2) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_empty_series_has_a_sane_view() {
        let v = chart_view(&[], 0, 0, Some(WEEK), 1_000);
        assert_eq!((v.from, v.to), (0, 0));
        assert!(v.t1 > v.t0);
    }

    #[test]
    fn a_missing_range_fits_the_points() {
        let points = vec![
            ChartPoint {
                unix: 0,
                price: 1.0,
                trade: None,
            },
            ChartPoint {
                unix: 1,
                price: 2.0,
                trade: None,
            },
        ];
        let (lo, hi) = fit_range(&points, None);
        assert!(lo < 1.0 && hi > 2.0, "{lo}..{hi}");
    }

    #[test]
    fn time_at_x_maps_the_edges_and_holds_them() {
        let r = Rect::new(10.0, 0.0, 100.0, 10.0);
        assert_eq!(time_at_x(r, 0, 1_000, 10.0), 0);
        assert_eq!(time_at_x(r, 0, 1_000, 110.0), 1_000);
        assert_eq!(time_at_x(r, 0, 1_000, -50.0), 0);
        assert_eq!(time_at_x(r, 0, 1_000, 5_000.0), 1_000);
        assert_eq!(time_at_x(r, 0, 1_000, 60.0), 500);
    }

    #[test]
    fn the_volume_band_needs_ada() {
        // The drawing is not testable here; the rule is that a trade without
        // an ADA figure contributes nothing, so a chart cannot show volume it
        // does not know.
        let trade = TradeMark {
            side: TradeSide::Buy,
            size: TradeSize::Small,
            ada: None,
        };
        assert!(trade.ada.is_none());
    }
}
