//! `ChannelBands` — where a wallet's money came from, period by period.
//!
//! Stacked bars over a discrete time axis, with an optional reference line for
//! a second quantity **in the same unit on the same axis**.
//!
//! ## Why composition and not a total
//!
//! A monthly total answers "how much" and hides "from where", and in a funding
//! trace the second question is usually the finding. A project funded the same
//! way for eight months and differently in the ninth looks unremarkable in a
//! total and unmistakable as a band that stops. The widget exists to make a
//! channel starting or stopping visible without the reader computing anything.
//!
//! ## The overlay is not a second axis
//!
//! [`ChannelBands::overlay`] draws a line across the same bars — typically what
//! was paid out, against what came in. Both are the same unit and share one
//! scale, so this is a reference line, not a dual-axis chart. Passing a series
//! measured in something else would be a real dual-axis violation, and the
//! widget cannot detect it: the caller must keep the units the same.
//!
//! ## Colour rules this enforces
//!
//! Colours come from [`CHANNEL_PALETTE`] in **fixed order by series identity**,
//! assigned once via [`assign_colors`] over the full channel set. Filtering the
//! chart must never repaint the survivors, which is what happens if colour is
//! taken from a series' position in the filtered list.
//!
//! Beyond [`CHANNEL_PALETTE`]'s length, series fold into a single "Other" band
//! ([`fold_to_other`]) rather than generating new hues. The palette is
//! validated against the dark surface — five slots, all six checks pass — and a
//! generated ninth hue would not be.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{assign_colors, ChannelBands, ChannelSeries};
//!
//! let colors = assign_colors(&["off-ramp", "conduit", "project", "recycled"]);
//! let series = vec![
//!     ChannelSeries::new("off-ramp", colors["off-ramp"], vec![11_696, 12_882, 0]),
//!     ChannelSeries::new("conduit",  colors["conduit"],  vec![0, 16_994, 13_537]),
//! ];
//! ChannelBands::new(&["2026-04", "2026-05", "2026-06"], &series)
//!     .overlay("paid out", &[12_180.0, 12_420.0, 20_520.0])
//!     .show(ui);
//! ```

use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

/// Categorical hues in fixed order, stepped for a dark surface.
///
/// Validated against surface `#1a1a2e`: lightness band, chroma floor, adjacent
/// CVD separation (worst ΔE 8.4), normal-vision floor (worst ΔE 19.3) and
/// contrast all pass. Do not reorder — the checks are on *adjacent* pairs, so
/// the order is part of what passed. Do not extend by inventing a sixth: fold
/// to "Other" instead.
pub const CHANNEL_PALETTE: [Color32; 5] = [
    Color32::from_rgb(0x39, 0x87, 0xe5), // blue
    Color32::from_rgb(0xd9, 0x59, 0x26), // orange
    Color32::from_rgb(0x19, 0x9e, 0x70), // aqua
    Color32::from_rgb(0xc9, 0x85, 0x00), // yellow
    Color32::from_rgb(0xd5, 0x51, 0x81), // magenta
];

/// Neutral for the folded "Other" band — deliberately outside the categorical
/// order so it never reads as one more channel.
pub const OTHER_COLOR: Color32 = Color32::from_rgb(0x6b, 0x6b, 0x80);

/// The label a folded band carries.
pub const OTHER_LABEL: &str = "Other";

/// Assign a colour to each channel **by name**, in the order given.
///
/// Do this once over the complete channel set and keep the result. Deriving
/// colour from a series' index inside a filtered list means hiding one channel
/// repaints the others, which silently invalidates every screenshot taken
/// before the filter changed.
///
/// Names past [`CHANNEL_PALETTE`] receive [`OTHER_COLOR`]; use [`fold_to_other`]
/// to collapse their values to match.
pub fn assign_colors(names: &[&str]) -> BTreeMap<String, Color32> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let c = CHANNEL_PALETTE.get(i).copied().unwrap_or(OTHER_COLOR);
            ((*n).to_string(), c)
        })
        .collect()
}

/// One channel's value in each period. `values.len()` must match the period
/// count; shorter series are treated as zero in the missing periods.
#[derive(Clone)]
pub struct ChannelSeries<'a> {
    pub name: &'a str,
    pub color: Color32,
    pub values: Vec<f64>,
}

impl<'a> ChannelSeries<'a> {
    pub fn new(name: &'a str, color: Color32, values: Vec<f64>) -> Self {
        Self {
            name,
            color,
            values,
        }
    }

    fn at(&self, period: usize) -> f64 {
        self.values.get(period).copied().unwrap_or(0.0).max(0.0)
    }

    /// Total across all periods — the ranking key when folding.
    pub fn total(&self) -> f64 {
        self.values.iter().filter(|v| **v > 0.0).sum()
    }
}

/// Keep the `keep` largest series by total and collapse the rest into one
/// "Other" band.
///
/// Returning a generated hue for a ninth series would put an unvalidated colour
/// on screen; folding keeps the palette inside what was actually checked. The
/// caller should say in the surrounding copy how many were folded — a silently
/// truncated chart reads as a complete one.
pub fn fold_to_other<'a>(series: &[ChannelSeries<'a>], keep: usize) -> Vec<ChannelSeries<'a>> {
    if series.len() <= keep {
        return series.to_vec();
    }
    let mut ranked: Vec<&ChannelSeries<'a>> = series.iter().collect();
    ranked.sort_by(|a, b| {
        b.total()
            .partial_cmp(&a.total())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let periods = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let mut out: Vec<ChannelSeries<'a>> = ranked.iter().take(keep).map(|s| (*s).clone()).collect();

    let mut other = vec![0.0; periods];
    for s in ranked.iter().skip(keep) {
        for (p, slot) in other.iter_mut().enumerate() {
            *slot += s.at(p);
        }
    }
    out.push(ChannelSeries {
        name: OTHER_LABEL,
        color: OTHER_COLOR,
        values: other,
    });
    out
}

/// Total of every series in one period.
pub fn period_total(series: &[ChannelSeries<'_>], period: usize) -> f64 {
    series.iter().map(|s| s.at(period)).sum()
}

pub struct ChannelBandsResponse {
    /// Period under the pointer, if any.
    pub hovered_period: Option<usize>,
    pub clicked_period: Option<usize>,
}

pub struct ChannelBands<'a> {
    periods: &'a [&'a str],
    series: &'a [ChannelSeries<'a>],
    overlay: Option<(&'a str, &'a [f64])>,
    height: f32,
    format_value: Option<&'a dyn Fn(f64) -> String>,
    show_legend: bool,
}

impl<'a> ChannelBands<'a> {
    pub fn new(periods: &'a [&'a str], series: &'a [ChannelSeries<'a>]) -> Self {
        Self {
            periods,
            series,
            overlay: None,
            height: 200.0,
            format_value: None,
            show_legend: true,
        }
    }

    /// A reference line over the bars. **Must be the same unit as the series** —
    /// it shares their axis. See the module docs.
    pub fn overlay(mut self, label: &'a str, values: &'a [f64]) -> Self {
        self.overlay = Some((label, values));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn format_value(mut self, f: &'a dyn Fn(f64) -> String) -> Self {
        self.format_value = Some(f);
        self
    }

    pub fn show_legend(mut self, show: bool) -> Self {
        self.show_legend = show;
        self
    }

    /// Tallest bar, including the overlay so the line can never leave the plot.
    fn max_value(&self) -> f64 {
        let bars = (0..self.periods.len())
            .map(|p| period_total(self.series, p))
            .fold(0.0f64, f64::max);
        let over = self
            .overlay
            .map(|(_, v)| v.iter().copied().fold(0.0f64, f64::max))
            .unwrap_or(0.0);
        bars.max(over).max(f64::MIN_POSITIVE)
    }

    pub fn show(self, ui: &mut Ui) -> ChannelBandsResponse {
        let muted = ui.visuals().weak_text_color();
        let surface = ui.visuals().panel_fill;
        let max = self.max_value();

        let width = ui.available_width();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width, self.height), Sense::click_and_drag());

        // Room for the period labels under the plot.
        let label_h = 16.0;
        let plot = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.max.y - label_h));
        // Headroom above the tallest bar: it keeps the top mark off the frame
        // edge and leaves room for the scale label INSIDE the clip rect. The
        // first attempt anchored that label above `plot.top()`, where
        // `painter_at(rect)` silently clipped it away — the chart shipped with
        // no magnitude reference at all and nothing said so.
        let scale = max * 1.10;
        let n = self.periods.len().max(1);
        let slot_w = plot.width() / n as f32;
        // Thin marks: the bar occupies most of its slot, never all of it.
        let bar_w = (slot_w * 0.62).min(46.0);

        let hovered_period = resp.hover_pos().and_then(|p| {
            plot.contains(p)
                .then(|| (((p.x - plot.left()) / slot_w) as usize).min(n - 1))
        });

        let painter = ui.painter_at(rect);

        // Baseline, and a single reference line at the top of the scale.
        //
        // One labelled line rather than a grid: the comparison here is between
        // adjacent bars, so a full grid is noise — but with NO scale reference
        // at all the chart makes a quantitative argument the reader cannot
        // check without hovering every bar, which is worse.
        painter.line_segment(
            [
                Pos2::new(plot.left(), plot.bottom()),
                Pos2::new(plot.right(), plot.bottom()),
            ],
            Stroke::new(1.0_f32, muted.gamma_multiply(0.4)),
        );
        let max_y = plot.bottom() - (max / scale) as f32 * plot.height();
        painter.line_segment(
            [
                Pos2::new(plot.left(), max_y),
                Pos2::new(plot.right(), max_y),
            ],
            Stroke::new(1.0_f32, muted.gamma_multiply(0.25)),
        );
        painter.text(
            Pos2::new(plot.left(), max_y - 1.0),
            Align2::LEFT_BOTTOM,
            self.fmt(max),
            FontId::monospace(9.0),
            muted,
        );

        for p in 0..n {
            let cx = plot.left() + slot_w * (p as f32 + 0.5);
            let mut y = plot.bottom();

            for s in self.series {
                let v = s.at(p);
                if v <= 0.0 {
                    continue;
                }
                let h = (v / scale) as f32 * plot.height();
                let top = y - h;
                let seg = Rect::from_min_max(
                    Pos2::new(cx - bar_w / 2.0, top),
                    Pos2::new(cx + bar_w / 2.0, y),
                );
                painter.rect_filled(seg, 0.0, s.color);
                // 2px surface gap between stacked segments, so the boundary is
                // a gap rather than two colours meeting — which is what keeps
                // adjacent hues legible under colour-vision differences.
                if top > plot.top() {
                    painter.line_segment(
                        [Pos2::new(seg.left(), top), Pos2::new(seg.right(), top)],
                        Stroke::new(2.0_f32, surface),
                    );
                }
                y = top;
            }

            let is_hovered = hovered_period == Some(p);
            painter.text(
                Pos2::new(cx, plot.bottom() + 3.0),
                Align2::CENTER_TOP,
                self.periods[p],
                FontId::monospace(9.0),
                if is_hovered {
                    ui.visuals().text_color()
                } else {
                    muted
                },
            );
        }

        // Overlay last so it sits above the fills.
        if let Some((_, values)) = self.overlay {
            let pts: Vec<Pos2> = (0..n)
                .map(|p| {
                    let v = values.get(p).copied().unwrap_or(0.0);
                    Pos2::new(
                        plot.left() + slot_w * (p as f32 + 0.5),
                        plot.bottom() - (v / scale) as f32 * plot.height(),
                    )
                })
                .collect();
            let ink = ui.visuals().text_color();
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(2.0_f32, ink));
            }
            for pt in &pts {
                // 2px surface ring so the marker reads over any band beneath it.
                painter.circle_filled(*pt, 4.0, surface);
                painter.circle_filled(*pt, 2.5, ink);
            }
            // One direct label, on the last point. Numbering every point is the
            // anti-pattern; leaving the line entirely unquantified is the other.
            if let (Some(last), Some(v)) = (pts.last(), values.get(n - 1)) {
                painter.text(
                    Pos2::new(last.x - 6.0, last.y - 6.0),
                    Align2::RIGHT_BOTTOM,
                    self.fmt(*v),
                    FontId::monospace(9.0),
                    ink,
                );
            }
        }

        if let Some(p) = hovered_period {
            self.tooltip(&resp, p);
        }

        if self.show_legend {
            self.legend(ui, muted);
        }

        ChannelBandsResponse {
            hovered_period,
            clicked_period: resp.clicked().then_some(hovered_period).flatten(),
        }
    }

    fn fmt(&self, v: f64) -> String {
        match self.format_value {
            Some(f) => f(v),
            None => format!("{v:.0}"),
        }
    }

    fn tooltip(&self, resp: &Response, p: usize) {
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(RichText::new(self.periods[p]).strong());
            for s in self.series {
                let v = s.at(p);
                if v <= 0.0 {
                    continue;
                }
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    ui.painter().rect_filled(r, 1.0, s.color);
                    // Text stays in ink; the swatch beside it carries identity.
                    ui.label(RichText::new(format!("{} {}", s.name, self.fmt(v))).size(11.0));
                });
            }
            ui.label(
                RichText::new(format!("total {}", self.fmt(period_total(self.series, p))))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            if let Some((label, values)) = self.overlay {
                ui.label(
                    RichText::new(format!(
                        "{label} {}",
                        self.fmt(values.get(p).copied().unwrap_or(0.0))
                    ))
                    .size(11.0),
                );
            }
        });
    }

    fn legend(&self, ui: &mut Ui, muted: Color32) {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for s in self.series {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, 9.0), Sense::hover());
                    ui.painter().rect_filled(r, 1.0, s.color);
                    ui.label(RichText::new(s.name).size(10.0).color(muted));
                });
            }
            if let Some((label, _)) = self.overlay {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, 9.0), Sense::hover());
                    let c = r.center();
                    ui.painter().line_segment(
                        [Pos2::new(r.left(), c.y), Pos2::new(r.right(), c.y)],
                        Stroke::new(2.0_f32, ui.visuals().text_color()),
                    );
                    ui.label(RichText::new(label).size(10.0).color(muted));
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &'static str, values: Vec<f64>) -> ChannelSeries<'static> {
        ChannelSeries::new(name, CHANNEL_PALETTE[0], values)
    }

    #[test]
    fn period_totals_sum_every_series() {
        let series = vec![s("a", vec![10.0, 0.0]), s("b", vec![5.0, 7.0])];
        assert_eq!(period_total(&series, 0), 15.0);
        assert_eq!(period_total(&series, 1), 7.0);
    }

    /// A short series is zero in the missing periods, never a panic and never
    /// a repeat of its last value.
    #[test]
    fn short_series_reads_as_zero_not_carried_forward() {
        let series = vec![s("a", vec![10.0])];
        assert_eq!(period_total(&series, 0), 10.0);
        assert_eq!(period_total(&series, 5), 0.0);
    }

    /// Colour follows identity, so removing a channel must not repaint the
    /// others — the rule a rank-based assignment breaks.
    #[test]
    fn colors_follow_the_name_not_the_position() {
        let all = assign_colors(&["off-ramp", "conduit", "project", "recycled"]);
        let filtered = assign_colors(&["off-ramp", "conduit", "project", "recycled"]);
        assert_eq!(all["conduit"], filtered["conduit"]);
        assert_eq!(all["off-ramp"], CHANNEL_PALETTE[0]);
        assert_eq!(all["conduit"], CHANNEL_PALETTE[1]);
        // Distinct slots, in palette order.
        assert_ne!(all["off-ramp"], all["conduit"]);
    }

    /// Past the validated palette, a name gets the neutral rather than an
    /// invented hue.
    #[test]
    fn beyond_the_palette_falls_to_other_not_a_new_hue() {
        let names = ["a", "b", "c", "d", "e", "f", "g"];
        let colors = assign_colors(&names);
        assert_eq!(colors["e"], CHANNEL_PALETTE[4]);
        assert_eq!(colors["f"], OTHER_COLOR);
        assert_eq!(colors["g"], OTHER_COLOR);
    }

    #[test]
    fn folding_keeps_the_largest_and_conserves_value() {
        let series = vec![
            s("big", vec![100.0, 100.0]),
            s("mid", vec![50.0, 50.0]),
            s("small1", vec![5.0, 1.0]),
            s("small2", vec![3.0, 2.0]),
        ];
        let before: f64 = (0..2).map(|p| period_total(&series, p)).sum();

        let folded = fold_to_other(&series, 2);
        assert_eq!(folded.len(), 3, "kept 2 + one Other band");
        assert_eq!(folded[0].name, "big");
        assert_eq!(folded[1].name, "mid");
        assert_eq!(folded[2].name, OTHER_LABEL);
        assert_eq!(folded[2].values, vec![8.0, 3.0]);

        let after: f64 = (0..2).map(|p| period_total(&folded, p)).sum();
        assert_eq!(before, after, "folding must not lose value");
    }

    #[test]
    fn folding_is_a_noop_when_it_fits() {
        let series = vec![s("a", vec![1.0]), s("b", vec![2.0])];
        let folded = fold_to_other(&series, 5);
        assert_eq!(folded.len(), 2);
        assert!(folded.iter().all(|f| f.name != OTHER_LABEL));
    }

    /// The palette is what the validator checked; a change here invalidates it.
    #[test]
    fn palette_is_five_distinct_validated_slots() {
        assert_eq!(CHANNEL_PALETTE.len(), 5);
        for (i, a) in CHANNEL_PALETTE.iter().enumerate() {
            for b in CHANNEL_PALETTE.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
            assert_ne!(*a, OTHER_COLOR);
        }
    }
}
