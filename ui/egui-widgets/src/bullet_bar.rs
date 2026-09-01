//! Bullet bar — a value fill against a track with an **optional target marker**.
//!
//! The classic "bullet graph" measure: a horizontal track, a fill for the current
//! value, and a vertical tick for the target. At a glance you see whether the
//! value is short of, at, or past its target — ideal for rarity tuning ("actual
//! share vs target share"), coverage, budgets, progress-to-goal, etc.
//!
//! ## The target is OPTIONAL, and that is load-bearing
//!
//! Use [`BulletBar::untargeted`] when no target exists. Not `new(v, 0.0)` — a
//! zero target is a *claim* that zero was the goal, and it renders as a tick
//! pinned to the left edge with the fill overshooting it, which reads as
//! "massively over budget".
//!
//! The distinction came from measuring a project's spending against its
//! published commitments. Some categories had a published figure ("15% to
//! ops"); others had **nothing published at all** — and for those the finding
//! is that the spending sits *outside the published terms entirely*, which is
//! a different and stronger statement than overrunning a budget. Drawing a
//! marker anywhere would invent a commitment nobody made; drawing one at zero
//! would invent the most damning one available.
//!
//! So an untargeted bar shows the fill and no tick, and says so on hover.
//!
//! Builder style, matching the other measure widgets (e.g. `progress_bar`).

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;

pub struct BulletBar {
    value: f32,
    /// `None` = no target was ever set. See the module docs — this is not the
    /// same as a target of zero.
    target: Option<f32>,
    max: f32,
    label: Option<String>,
    detail: Option<String>,
    height: f32,
    rounding: u8,
    fill_color: Color32,
    track_color: Color32,
    target_color: Color32,
    /// If set, the fill switches to this color when the value is within
    /// [`tolerance`](Self::tolerance) of the target (a "met goal" cue).
    good_color: Option<Color32>,
    tolerance: f32,
    show_percent: bool,
}

impl BulletBar {
    /// `value` and `target` are on a 0..=`max` scale (`max` defaults to 1.0, i.e.
    /// fractions). Use [`max`](Self::max) for other domains.
    pub fn new(value: f32, target: f32) -> Self {
        Self::with_target(value, Some(target))
    }

    /// A measure with **no target** — nothing was ever set to compare against.
    ///
    /// Renders the fill and no marker. Prefer this over `new(value, 0.0)`,
    /// which asserts that zero was the goal and draws the value overshooting
    /// it. See the module docs.
    pub fn untargeted(value: f32) -> Self {
        Self::with_target(value, None)
    }

    /// The general form, for callers holding an `Option` already — e.g. a
    /// nullable `share` column where NULL means "nothing was published".
    pub fn with_target(value: f32, target: Option<f32>) -> Self {
        Self {
            value,
            target,
            max: 1.0,
            label: None,
            detail: None,
            height: 14.0,
            rounding: 3,
            fill_color: theme::ACCENT_BLUE,
            track_color: theme::BG_SECONDARY,
            target_color: theme::TEXT_PRIMARY,
            good_color: None,
            tolerance: 0.0,
            show_percent: false,
        }
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max.max(f32::EPSILON);
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn fill_color(mut self, color: Color32) -> Self {
        self.fill_color = color;
        self
    }

    pub fn target_color(mut self, color: Color32) -> Self {
        self.target_color = color;
        self
    }

    /// Turn the fill `good` when within `tolerance` (same units as the scale) of
    /// the target — e.g. `.good_within(theme::SUCCESS, 0.02)` for ±2%.
    pub fn good_within(mut self, color: Color32, tolerance: f32) -> Self {
        self.good_color = Some(color);
        self.tolerance = tolerance.abs();
        self
    }

    /// Show "value% → target%" in the label row (only meaningful when max == 1.0).
    pub fn show_percent(mut self, show: bool) -> Self {
        self.show_percent = show;
        self
    }

    pub fn show(self, ui: &mut Ui) -> egui::Response {
        crate::install_phosphor_font(ui.ctx());
        let value = self.value.clamp(0.0, self.max);
        let target = self.target.map(|t| t.clamp(0.0, self.max));
        // Without a target nothing can be "met", so the good colour never
        // applies — a bar with no goal must not render as a goal achieved.
        let met = target.is_some_and(|t| (value - t).abs() <= self.tolerance);

        // Optional label row: label (left) + value/target % (center) + detail (right).
        if self.label.is_some() || self.detail.is_some() || self.show_percent {
            ui.horizontal(|ui| {
                if let Some(lbl) = &self.label {
                    ui.label(RichText::new(lbl).color(theme::TEXT_SECONDARY).size(11.0));
                }
                if self.show_percent {
                    let vc = match self.good_color {
                        Some(c) if met => c,
                        _ => theme::TEXT_PRIMARY,
                    };
                    ui.label(
                        RichText::new(format!("{:.0}%", value / self.max * 100.0))
                            .color(vc)
                            .size(11.0),
                    );
                    // The "→ target" half is omitted entirely when there is no
                    // target. An arrow pointing at a blank, or at a 0%, would
                    // read as a goal.
                    if let Some(t) = target {
                        // Phosphor arrow (the default font has no U+2192 glyph).
                        crate::PhosphorIcon::ArrowRight.show(ui, 11.0, theme::TEXT_MUTED);
                        ui.label(
                            RichText::new(format!("{:.0}%", t / self.max * 100.0))
                                .color(theme::TEXT_MUTED)
                                .size(11.0),
                        );
                    }
                }
                if let Some(detail) = &self.detail {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(detail).color(theme::TEXT_MUTED).size(11.0));
                    });
                }
            });
            ui.add_space(2.0);
        }

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), self.height), Sense::hover());
        let painter = ui.painter_at(rect);
        let rounding = CornerRadius::same(self.rounding);

        // Track.
        painter.rect_filled(rect, rounding, self.track_color);

        // Value fill.
        let fill = match self.good_color {
            Some(c) if met => c,
            _ => self.fill_color,
        };
        let frac = value / self.max;
        if frac > 0.0 {
            let fill_rect = Rect::from_min_max(
                rect.min,
                egui::pos2(rect.min.x + rect.width() * frac, rect.max.y),
            );
            painter.rect_filled(fill_rect, rounding, fill);
        }

        // Target marker: a vertical tick that slightly overshoots the bar.
        // Drawn ONLY when a target exists — see the module docs.
        match target {
            Some(t) => {
                let tx = rect.min.x + rect.width() * (t / self.max);
                painter.line_segment(
                    [
                        egui::pos2(tx, rect.min.y - 2.0),
                        egui::pos2(tx, rect.max.y + 2.0),
                    ],
                    Stroke::new(2.0_f32, self.target_color),
                );
                response.on_hover_text(format!("{value:.1} / target {t:.1}"))
            }
            // Says the absence out loud. A bare value invites the reader to
            // supply their own target, and zero is the one they will assume.
            None => response.on_hover_text(format!("{value:.1} — no target set")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinction the `Option` exists for: "nothing was published" and
    /// "the goal was zero" are different claims, and only one of them draws a
    /// marker. Rendering is checked by the storybook screenshot; this pins the
    /// state that decides it.
    #[test]
    fn an_untargeted_bar_holds_no_target_and_a_zero_target_holds_one() {
        assert_eq!(BulletBar::untargeted(0.28).target, None);
        assert_eq!(BulletBar::new(0.28, 0.0).target, Some(0.0));
        assert_eq!(BulletBar::with_target(0.28, None).target, None);
        assert_eq!(BulletBar::with_target(0.28, Some(0.15)).target, Some(0.15));
    }

    /// `good_within` must never fire without a target. A bar with no goal that
    /// rendered in the "met" colour would report success against a commitment
    /// that was never made.
    #[test]
    fn no_target_can_never_be_met() {
        let b = BulletBar::untargeted(0.0).good_within(theme::SUCCESS, 0.02);
        let value: f32 = 0.0;
        let met = b.target.is_some_and(|t| (value - t).abs() <= b.tolerance);
        assert!(!met, "0.0 vs an absent target is not a met goal");

        let c = BulletBar::new(0.0, 0.0).good_within(theme::SUCCESS, 0.02);
        let met_zero = c.target.is_some_and(|t| (value - t).abs() <= c.tolerance);
        assert!(met_zero, "0.0 against a stated zero target IS met");
    }
}
