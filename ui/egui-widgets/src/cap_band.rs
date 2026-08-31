//! `CapBand` — a notional valuation against what it would actually fetch.
//!
//! Two series in one unit on one scale: a headline number everybody quotes,
//! and the smaller number that survives contact with the order book. **The gap
//! between them is the widget's whole reason to exist** — a thin market's
//! quoted capitalisation is fiction, and drawing the two together makes that
//! legible without a caveat nobody reads.
//!
//! Built for a token's market cap (all supply × spot, against what the
//! sellable float would realise sold into the AMM curve), but nothing here
//! knows about tokens: it is any "headline vs realisable" pair sharing a unit.
//!
//! ## Conventions this follows
//!
//! - **x is the spine's own [`TimeScale`]**, passed in from the spine's
//!   response, so a point sits under the ruler date it happened on and
//!   brushing the spine moves this with every other face.
//! - **The realisable figure is a RANGE, not a point.** Where a caller cannot
//!   say whether some quantity is liquid, it passes a low and a high and the
//!   width of the band is the uncertainty. Collapsing that to one number would
//!   be a claim the caller has not made.
//!
//! ## Two failure modes it guards, both found by looking at it
//!
//! - **A zero-height band is four collinear points.** The tessellator cannot
//!   derive a normal for it and emits long diagonal rays across the panel.
//!   Where low == high — no uncertainty at that instant — the band is drawn as
//!   a line instead, which is both correct-looking and the honest rendering.
//! - **[`TimeScale`] extrapolates past both ends of its view.** A sample
//!   outside the visible window maps to an x far off-canvas and drags a
//!   polygon edge across the whole chart. Samples are clipped rather than
//!   clamped: clamping would pile every off-screen sample onto one edge and
//!   draw a spike that never happened.

use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2, pos2};

use crate::theme;
use crate::time_spine::TimeScale;

/// One sample of the band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapSample {
    /// Unix seconds — the same unit the spine's axis speaks.
    pub at: i64,
    /// The headline figure.
    pub notional: f64,
    /// Conservative realisable figure.
    pub low: f64,
    /// Optimistic realisable figure. Equal to `low` when there is no
    /// uncertainty, which is drawn as a line rather than a zero-height band.
    pub high: f64,
}

/// Below this many pixels a band is drawn as a line — see the module header.
const MIN_BAND_PX: f32 = 0.75;

pub struct CapBandResponse {
    pub response: egui::Response,
    /// Sample nearest the pointer, if it is over the plot.
    pub hovered: Option<CapSample>,
}

pub struct CapBand<'a> {
    samples: &'a [CapSample],
    scale: &'a TimeScale,
    playhead: Option<i64>,
    height: f32,
    notional_label: &'a str,
    realisable_label: &'a str,
    format_value: &'a dyn Fn(f64) -> String,
    notional_color: Color32,
    realisable_color: Color32,
    log_y: bool,
}

impl<'a> CapBand<'a> {
    pub fn new(samples: &'a [CapSample], scale: &'a TimeScale) -> Self {
        Self {
            samples,
            scale,
            playhead: None,
            height: 140.0,
            notional_label: "notional",
            realisable_label: "realisable",
            format_value: &|v| format!("{v:.0}"),
            notional_color: Color32::from_rgb(226, 196, 120),
            realisable_color: Color32::from_rgb(64, 154, 148),
            log_y: false,
        }
    }

    /// Plot y on a log scale.
    ///
    /// Off by default, but the right choice for anything launch-shaped. A
    /// token's first week routinely peaks at 50–100× where it settles, and on
    /// a linear axis that spike claims the whole plot while the months after —
    /// which is the part with the holders in it — collapse onto the floor as a
    /// flat line. The chart then answers "was there a spike" and nothing else.
    ///
    /// It suits this widget particularly: the gap between the two series is a
    /// RATIO, and constant ratios keep a constant vertical distance under a
    /// log scale, so a band that stays 20% of notional looks the same width
    /// whether the token is worth 500k or 5k.
    pub fn log_y(mut self, on: bool) -> Self {
        self.log_y = on;
        self
    }

    /// Draw a marker at this instant — normally the spine's playhead.
    pub fn playhead(mut self, at: i64) -> Self {
        self.playhead = Some(at);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn labels(mut self, notional: &'a str, realisable: &'a str) -> Self {
        self.notional_label = notional;
        self.realisable_label = realisable;
        self
    }

    pub fn format_value(mut self, f: &'a dyn Fn(f64) -> String) -> Self {
        self.format_value = f;
        self
    }

    pub fn colors(mut self, notional: Color32, realisable: Color32) -> Self {
        self.notional_color = notional;
        self.realisable_color = realisable;
        self
    }

    pub fn show(self, ui: &mut Ui) -> CapBandResponse {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), self.height), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);

        if self.samples.is_empty() {
            // Absence is a finding: a caller with no samples has no market to
            // draw, and an empty frame would read as a bug.
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "no data in range",
                FontId::proportional(12.0),
                theme::TEXT_SECONDARY,
            );
            return CapBandResponse {
                response,
                hovered: None,
            };
        }

        let max = self
            .samples
            .iter()
            .fold(0.0f64, |m, s| m.max(s.notional))
            .max(f64::MIN_POSITIVE);
        // Floor for the log scale: the smallest positive value present, so the
        // axis spans the data rather than reaching for zero, which log cannot
        // represent. Non-positive samples are pinned to the floor rather than
        // dropped — a period worth nothing is a fact about the token.
        let min = self
            .samples
            .iter()
            .flat_map(|s| [s.notional, s.low])
            .filter(|v| *v > 0.0)
            .fold(f64::INFINITY, f64::min);
        let (lo, hi) = (min.max(f64::MIN_POSITIVE).ln(), max.ln());
        let span = (hi - lo).max(f64::MIN_POSITIVE);

        // Headroom so the peak does not touch the frame and the legend has
        // somewhere to sit that `painter_at` will not clip away.
        let plot_h = rect.height() - 26.0;
        let log_y = self.log_y && min.is_finite();
        let y_of = |v: f64| {
            let frac = if log_y {
                ((v.max(f64::MIN_POSITIVE).ln() - lo) / span).clamp(0.0, 1.0)
            } else {
                v / max
            };
            rect.bottom() - frac as f32 * plot_h
        };
        let x_of = |at: i64| {
            self.scale
                .x_from_time_f32(at as f64)
                .filter(|x| rect.x_range().contains(*x))
        };

        for w in self.samples.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let (Some(xa), Some(xb)) = (x_of(a.at), x_of(b.at)) else {
                continue;
            };
            let (la, ha) = (y_of(a.low), y_of(a.high));
            let (lb, hb) = (y_of(b.low), y_of(b.high));
            if (la - ha).max(lb - hb) < MIN_BAND_PX {
                p.line_segment(
                    [pos2(xa, la), pos2(xb, lb)],
                    Stroke::new(1.0_f32, self.realisable_color),
                );
            } else {
                p.add(egui::Shape::convex_polygon(
                    vec![pos2(xa, la), pos2(xb, lb), pos2(xb, hb), pos2(xa, ha)],
                    self.realisable_color.gamma_multiply(0.45),
                    Stroke::NONE,
                ));
            }
        }

        let pts: Vec<Pos2> = self
            .samples
            .iter()
            .filter_map(|s| x_of(s.at).map(|x| pos2(x, y_of(s.notional))))
            .collect();
        if pts.len() > 1 {
            p.add(egui::Shape::line(
                pts,
                Stroke::new(1.4_f32, self.notional_color),
            ));
        }

        if let Some(at) = self.playhead
            && let Some(x) = x_of(at)
        {
            p.vline(
                x,
                rect.y_range(),
                Stroke::new(1.0_f32, theme::TEXT_SECONDARY),
            );
        }

        // Legend, and one labelled magnitude reference. Without a scale mark
        // the chart makes a quantitative argument the reader cannot check.
        p.text(
            rect.left_top() + Vec2::new(6.0, 4.0),
            Align2::LEFT_TOP,
            format!(
                "{} · peak {}",
                self.notional_label,
                (self.format_value)(max)
            ),
            FontId::proportional(11.0),
            self.notional_color,
        );
        p.text(
            rect.left_top() + Vec2::new(6.0, 17.0),
            Align2::LEFT_TOP,
            self.realisable_label,
            FontId::proportional(11.0),
            self.realisable_color,
        );

        let hovered = response.hover_pos().and_then(|pos| {
            rect.contains(pos)
                .then(|| nearest(self.samples, self.scale, pos))
                .flatten()
        });
        if let Some(h) = hovered
            && let Some(x) = x_of(h.at)
        {
            p.circle_filled(pos2(x, y_of(h.notional)), 2.5, self.notional_color);
        }

        CapBandResponse { response, hovered }
    }
}

/// Sample whose x is closest to the pointer.
fn nearest(samples: &[CapSample], scale: &TimeScale, pos: Pos2) -> Option<CapSample> {
    let t = scale.time_from_x_f32(pos.x)?;
    samples
        .iter()
        .min_by_key(|s| ((s.at as f64 - t).abs() * 1000.0) as i64)
        .copied()
}

/// Ratio of realisable to notional, as a percentage range.
///
/// Exposed because it is the number the band exists to communicate, and every
/// caller would otherwise recompute it — differently.
pub fn honesty_ratio(sample: &CapSample) -> (f64, f64) {
    if sample.notional <= 0.0 {
        return (0.0, 0.0);
    }
    (
        100.0 * sample.low / sample.notional,
        100.0 * sample.high / sample.notional,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(at: i64, notional: f64, low: f64, high: f64) -> CapSample {
        CapSample {
            at,
            notional,
            low,
            high,
        }
    }

    #[test]
    fn honesty_ratio_is_a_range_and_survives_a_zero_notional() {
        let (lo, hi) = honesty_ratio(&s(0, 1000.0, 200.0, 300.0));
        assert!((lo - 20.0).abs() < 1e-9);
        assert!((hi - 30.0).abs() < 1e-9);
        // A period with no market must not divide by zero.
        assert_eq!(honesty_ratio(&s(0, 0.0, 0.0, 0.0)), (0.0, 0.0));
    }

    #[test]
    fn nearest_picks_the_closest_sample_not_the_first() {
        let samples = [s(100, 1.0, 1.0, 1.0), s(200, 1.0, 1.0, 1.0)];
        let scale = TimeScale::continuous(
            egui::Rangef::new(0.0, 100.0),
            crate::time_spine::TimeView::covering(100, 200),
            100,
            200,
        );
        let near_end = nearest(&samples, &scale, pos2(95.0, 0.0)).unwrap();
        assert_eq!(near_end.at, 200);
        let near_start = nearest(&samples, &scale, pos2(5.0, 0.0)).unwrap();
        assert_eq!(near_start.at, 100);
    }
}
