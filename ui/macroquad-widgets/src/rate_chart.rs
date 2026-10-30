//! `RateChart` — what a stake earned a day over a campaign, as a step line: a
//! flat run per rate, a riser where it changes, the stretch already earned
//! filled, and the mean as a hairline across it.
//!
//! A rate holds until something moves it, so the line steps rather than
//! interpolates. The axis is **time**, not a reading's index: a quiet fortnight
//! is as wide as a busy one, and the fill stops at the moment the figures were
//! priced to — past it the line is drawn dim, as a projection rather than a
//! fact. That is what makes it the sibling of the price chart's step line
//! rather than a second copy of it: no volume band, no trade marks, no
//! playhead.
//!
//! The pure half — [`runs`], [`value_at`], [`range`] — is separated from the
//! drawing so a host can test the arithmetic it feeds without a window.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::Painter;
use crate::theme::with_alpha;

/// One step of the series: from `at` until the next, the rate is `value`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateStep {
    pub at: u64,
    /// Display units a day: the host has applied the token's decimals.
    pub value: f64,
}

/// One drawn run: from `from` to `to`, the rate is `value`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub from: u64,
    pub to: u64,
    pub value: f64,
}

/// The runs inside `[t0, t1)`, clipped at both edges.
///
/// Each step runs until the next one. The first covers from `t0` — a rate in
/// force when the window opened is drawn from the left edge — and the last runs
/// on to `t1`, so the plot never trails off into nothing while a rate is in
/// force.
pub fn runs(steps: &[RateStep], t0: u64, t1: u64) -> Vec<Run> {
    if t1 <= t0 || steps.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let next = steps.get(i + 1).map_or(t1, |next| next.at);
        let from = step.at.max(t0);
        let to = next.min(t1);
        if to > from {
            out.push(Run {
                from,
                to,
                value: step.value,
            });
        }
    }
    out
}

/// The rate in force at `at`: the last step at or before it. Before the first
/// step nothing is held, so it is 0.
pub fn value_at(steps: &[RateStep], at: u64) -> f64 {
    steps
        .iter()
        .rev()
        .find(|step| step.at <= at)
        .map_or(0.0, |step| step.value)
}

/// The y range to draw: zero up to the highest reading, the mean included so
/// its hairline is never off the plot. A series with nothing in it is `(0, 0)`.
pub fn range(steps: &[RateStep], mean: Option<f64>) -> (f64, f64) {
    let peak = steps.iter().map(|step| step.value).fold(0.0f64, f64::max);
    let hi = peak.max(mean.unwrap_or(0.0)).max(0.0);
    (0.0, hi)
}

/// The chart's labels, preformatted by the host: the peak, the mean's own
/// reading, and the axis's ends.
#[derive(Clone, Copy, Debug, Default)]
pub struct RateChartLabels<'a> {
    pub peak: Option<&'a str>,
    pub mean: Option<&'a str>,
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
}

/// What the chart draws.
#[derive(Clone, Copy, Debug)]
pub struct RateChartVm<'a> {
    /// The steps, ascending. A step before `t0` carries into the window.
    pub steps: &'a [RateStep],
    /// The axis's left and right edges, unix seconds.
    pub t0: u64,
    pub t1: u64,
    /// The moment the figures are priced to: the fill stops here and the line
    /// past it is dimmed. `None` for a series earned all the way to `t1`.
    pub earned_to: Option<u64>,
    /// The average rate over `[t0, earned_to)`, drawn as a dashed hairline.
    pub mean: Option<f64>,
    /// The line and its labels.
    pub line: Color,
    /// The fill under the earned stretch. The host passes it already faded.
    pub fill: Color,
    pub labels: RateChartLabels<'a>,
}

/// Draw the chart in `rect`: the line in the upper part, the axis's labels in a
/// strip along the foot.
pub fn rate_chart(p: &Painter, vm: &RateChartVm, rect: Rect) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let t = p.theme.color;
    let s = p.size(TextSize::Xs);
    let axis_h = s + 6.0;
    // A band for the peak's reading, so it never sits on the line that set it.
    let peak_h = if vm.labels.peak.is_some() { s + 4.0 } else { 0.0 };
    let plot = Rect::new(
        rect.x,
        rect.y + peak_h,
        rect.w,
        (rect.h - axis_h - peak_h).max(1.0),
    );
    let axis = Rect::new(rect.x, rect.bottom() - axis_h, rect.w, axis_h);
    let peak_row = Rect::new(rect.x, rect.y, rect.w, peak_h.max(1.0));

    let floor = plot.bottom() - 0.5;
    draw_line(
        plot.x,
        floor,
        plot.right(),
        floor,
        1.0,
        with_alpha(vm.line, 0.25),
    );

    let (lo, hi) = range(vm.steps, vm.mean);
    if hi > lo {
        let span = vm.t1.saturating_sub(vm.t0).max(1) as f64;
        let x_of = |at: u64| {
            plot.x
                + plot.w
                    * ((at.saturating_sub(vm.t0) as f64 / span).clamp(0.0, 1.0) as f32)
        };
        let y_of = |value: f64| {
            plot.bottom() - plot.h * (((value - lo) / (hi - lo)).clamp(0.0, 1.0) as f32)
        };
        let earned = vm.earned_to.unwrap_or(vm.t1).clamp(vm.t0, vm.t1);
        let runs = runs(vm.steps, vm.t0, vm.t1);

        // The earned stretch is a filled area; what follows is the same line,
        // dimmed, because it is what the stake would earn rather than what it
        // has.
        for run in &runs {
            let (x0, x1) = (x_of(run.from), x_of(run.to));
            let y = y_of(run.value);
            let filled_to = x_of(run.to.min(earned).max(run.from));
            if filled_to > x0 {
                draw_rectangle(x0, y, filled_to - x0, floor - y, vm.fill);
            }
            let ink = if run.from < earned {
                vm.line
            } else {
                with_alpha(vm.line, 0.35)
            };
            draw_line(x0, y, x1, y, 1.5, ink);
        }
        // The riser at each change, drawn after the runs so it sits over both.
        for pair in runs.windows(2) {
            let (before, after) = (y_of(pair[0].value), y_of(pair[1].value));
            if (before - after).abs() > 0.5 {
                let ink = if pair[1].from < earned {
                    vm.line
                } else {
                    with_alpha(vm.line, 0.35)
                };
                let x = x_of(pair[1].from);
                draw_line(x, before, x, after, 1.5, ink);
            }
        }
        if let Some(mean) = vm.mean {
            let y = y_of(mean);
            let ink = with_alpha(vm.line, 0.5);
            let mut x = plot.x;
            while x < plot.right() {
                let seg = (x + 5.0).min(plot.right());
                draw_line(x, y, seg, y, 1.0, ink);
                x += 9.0;
            }
            if let Some(label) = vm.labels.mean {
                p.mono_in(
                    label,
                    Rect::new((plot.right() - 140.0).max(plot.x), y - s - 4.0, 140.0, s + 4.0),
                    s,
                    t.text_secondary,
                    crate::Align::End,
                );
            }
        }
        if let Some(peak) = vm.labels.peak {
            p.mono_in(peak, peak_row, s, t.text_muted, crate::Align::End);
        }
    }

    if let Some(start) = vm.labels.start {
        p.mono_in(start, axis, s, t.text_muted, crate::Align::Start);
    }
    if let Some(end) = vm.labels.end {
        p.mono_in(end, axis, s, t.text_muted, crate::Align::End);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(values: &[(u64, f64)]) -> Vec<RateStep> {
        values
            .iter()
            .map(|(at, value)| RateStep {
                at: *at,
                value: *value,
            })
            .collect()
    }

    #[test]
    fn a_run_ends_where_the_next_begins_and_the_last_reaches_the_edge() {
        let series = steps(&[(0, 10.0), (5, 30.0), (9, 20.0)]);
        assert_eq!(
            runs(&series, 2, 12),
            vec![
                Run {
                    from: 2,
                    to: 5,
                    value: 10.0
                },
                Run {
                    from: 5,
                    to: 9,
                    value: 30.0
                },
                Run {
                    from: 9,
                    to: 12,
                    value: 20.0
                },
            ],
            "clipped at the start, carried to the end"
        );
    }

    #[test]
    fn a_step_before_the_window_covers_from_the_left_edge() {
        let series = steps(&[(0, 10.0)]);
        assert_eq!(
            runs(&series, 100, 200),
            vec![Run {
                from: 100,
                to: 200,
                value: 10.0
            }]
        );
    }

    #[test]
    fn nothing_inside_the_axis_is_no_runs() {
        assert!(runs(&steps(&[(500, 10.0)]), 0, 100).is_empty());
        assert!(runs(&[], 0, 100).is_empty());
        assert!(runs(&steps(&[(0, 10.0)]), 100, 100).is_empty());
    }

    #[test]
    fn the_reading_at_a_moment_is_the_last_step_at_or_before_it() {
        let series = steps(&[(10, 10.0), (20, 30.0)]);
        assert_eq!(value_at(&series, 5), 0.0, "before anything is held");
        assert_eq!(value_at(&series, 10), 10.0);
        assert_eq!(value_at(&series, 19), 10.0);
        assert_eq!(value_at(&series, 20), 30.0);
    }

    #[test]
    fn the_range_runs_from_zero_to_the_highest_reading_and_holds_the_mean() {
        assert_eq!(range(&steps(&[(0, 10.0), (5, 40.0)]), None), (0.0, 40.0));
        assert_eq!(
            range(&steps(&[(0, 10.0)]), Some(25.0)),
            (0.0, 25.0),
            "a mean above every reading still gets a hairline on the plot"
        );
        assert_eq!(range(&[], None), (0.0, 0.0));
    }
}
