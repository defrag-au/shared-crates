//! Bullet bar — a value fill against a track with a **target marker**.
//!
//! The classic "bullet graph" measure: a horizontal track, a fill for the current
//! value, and a vertical tick for the target. At a glance you see whether the
//! value is short of, at, or past its target — ideal for rarity tuning ("actual
//! share vs target share"), coverage, budgets, progress-to-goal, etc.
//!
//! Builder style, matching the other measure widgets (e.g. `progress_bar`).

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;

pub struct BulletBar {
    value: f32,
    target: f32,
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
        let target = self.target.clamp(0.0, self.max);

        // Optional label row: label (left) + value/target % (center) + detail (right).
        if self.label.is_some() || self.detail.is_some() || self.show_percent {
            ui.horizontal(|ui| {
                if let Some(lbl) = &self.label {
                    ui.label(RichText::new(lbl).color(theme::TEXT_SECONDARY).size(11.0));
                }
                if self.show_percent {
                    let met = (value - target).abs() <= self.tolerance;
                    let vc = match self.good_color {
                        Some(c) if met => c,
                        _ => theme::TEXT_PRIMARY,
                    };
                    ui.label(
                        RichText::new(format!("{:.0}%", value / self.max * 100.0))
                            .color(vc)
                            .size(11.0),
                    );
                    // Phosphor arrow (the default font has no U+2192 glyph).
                    crate::PhosphorIcon::ArrowRight.show(ui, 11.0, theme::TEXT_MUTED);
                    ui.label(
                        RichText::new(format!("{:.0}%", target / self.max * 100.0))
                            .color(theme::TEXT_MUTED)
                            .size(11.0),
                    );
                }
                if let Some(detail) = &self.detail {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(detail).color(theme::TEXT_MUTED).size(11.0));
                    });
                }
            });
            ui.add_space(2.0);
        }

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        let rounding = CornerRadius::same(self.rounding);

        // Track.
        painter.rect_filled(rect, rounding, self.track_color);

        // Value fill.
        let met = (value - target).abs() <= self.tolerance;
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
        let tx = rect.min.x + rect.width() * (target / self.max);
        painter.line_segment(
            [
                egui::pos2(tx, rect.min.y - 2.0),
                egui::pos2(tx, rect.max.y + 2.0),
            ],
            Stroke::new(2.0, self.target_color),
        );

        response.on_hover_text(format!(
            "{:.1} / target {:.1}",
            value, target
        ))
    }
}
