//! Themed progress bar widget with optional label, percentage, and countdown.
//!
//! Supports two modes:
//! - **Determinate**: known progress fraction (0.0–1.0)
//! - **Countdown**: shows remaining time, fills from right to left as time elapses
//!
//! Uses the shared Tokyo Night palette by default but accepts custom colors.

use egui::{CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::{Ink, Space, SpaceExt, ThemeExt, Token};

/// What the bar's outline is drawn with.
///
/// Three states, so an enum rather than an `Option<Stroke>`: the border can take
/// the theme's weight and colour (the default, which a `new` cannot resolve), an
/// explicit stroke, or nothing. The public `border(Option<Stroke>)` setter keeps
/// its signature and maps onto the latter two.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BarBorder {
    /// The active theme's border.
    #[default]
    Theme,
    Custom(Stroke),
    None,
}

impl BarBorder {
    fn resolve(self, ui: &Ui) -> Option<Stroke> {
        match self {
            Self::Theme => {
                let t = ui.tokens();
                Some(t.geometry.border(t.color.border))
            }
            Self::Custom(s) => Some(s),
            Self::None => None,
        }
    }
}

/// Configuration for a progress bar.
pub struct ProgressBar {
    /// Progress fraction (0.0 = empty, 1.0 = full).
    fraction: f32,
    /// Optional label shown to the left of the bar.
    label: Option<String>,
    /// Optional right-aligned text (e.g. "2d 3h remaining").
    detail: Option<String>,
    /// Whether to show the percentage text centered on the bar.
    show_percentage: bool,
    /// Fill color for the completed portion.
    fill_color: Ink,
    /// Background color for the track.
    track_color: Ink,
    border: BarBorder,
    /// Bar height in pixels.
    height: f32,
    /// Corner rounding radius.
    rounding: u8,
}

impl ProgressBar {
    /// Create a new progress bar with the given fraction (clamped to 0.0–1.0).
    pub fn new(fraction: f32) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            label: None,
            detail: None,
            show_percentage: false,
            fill_color: Ink::Token(Token::Accent),
            track_color: Ink::Token(Token::BgSecondary),
            border: BarBorder::Theme,
            height: 16.0,
            rounding: 4,
        }
    }

    /// Create a countdown-style progress bar.
    ///
    /// `elapsed` and `total` are in the same unit (e.g. seconds).
    /// The bar fills from full → empty as time elapses.
    pub fn countdown(elapsed: f64, total: f64) -> Self {
        let remaining = if total > 0.0 {
            ((total - elapsed) / total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        Self::new(remaining)
    }

    /// Set the label shown to the left of the bar.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the right-aligned detail text (e.g. remaining time).
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Show the percentage text centered on the bar.
    pub fn show_percentage(mut self) -> Self {
        self.show_percentage = true;
        self
    }

    /// Set the fill color.
    pub fn fill_color(mut self, color: impl Into<Ink>) -> Self {
        self.fill_color = color.into();
        self
    }

    /// Set the track (background) color.
    pub fn track_color(mut self, color: impl Into<Ink>) -> Self {
        self.track_color = color.into();
        self
    }

    /// Set the bar height in pixels.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set the corner rounding radius.
    pub fn rounding(mut self, rounding: u8) -> Self {
        self.rounding = rounding;
        self
    }

    /// Set the border stroke (or `None` for no border).
    pub fn border(mut self, stroke: Option<Stroke>) -> Self {
        self.border = match stroke {
            Some(s) => BarBorder::Custom(s),
            None => BarBorder::None,
        };
        self
    }

    /// Render the progress bar and return the response.
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        // Label + detail row above the bar
        if self.label.is_some() || self.detail.is_some() {
            ui.horizontal(|ui| {
                if let Some(label) = &self.label {
                    ui.label(
                        RichText::new(label)
                            .color(ui.tokens().color.text_secondary)
                            .small(),
                    );
                }
                if let Some(detail) = &self.detail {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(detail)
                                .color(ui.tokens().color.text_muted)
                                .small(),
                        );
                    });
                }
            });
            ui.gap(Space::Xs);
        }

        // Allocate space for the bar
        let available_width = ui.available_width();
        let desired_size = Vec2::new(available_width, self.height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        let border = self.border.resolve(ui);
        let t = ui.tokens();
        let fill_color = self.fill_color.resolve(&t);
        let track_color = self.track_color.resolve(&t);

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let rounding = CornerRadius::same(self.rounding);

            // Track background
            painter.rect_filled(rect, rounding, track_color);

            // Fill
            if self.fraction > 0.0 {
                let fill_width = rect.width() * self.fraction;
                let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));
                painter.rect_filled(fill_rect, rounding, fill_color);

                // If partially filled, clip the right corners of the fill
                // to avoid rounding artefacts when the fill doesn't reach the end
                if self.fraction < 0.98 {
                    let clip_rect =
                        Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));
                    painter.rect_filled(
                        clip_rect,
                        CornerRadius {
                            nw: self.rounding,
                            sw: self.rounding,
                            ne: 0,
                            se: 0,
                        },
                        fill_color,
                    );
                }
            }

            // Border
            if let Some(stroke) = border {
                painter.rect_stroke(rect, rounding, stroke, StrokeKind::Outside);
            }

            // Percentage text centered on bar
            if self.show_percentage {
                let pct_text = format!("{}%", (self.fraction * 100.0).round() as u32);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    pct_text,
                    egui::FontId::proportional(self.height * 0.65),
                    ui.tokens().color.text_primary,
                );
            }
        }

        // Tooltip on hover
        if response.hovered() {
            let pct = (self.fraction * 100.0).round() as u32;
            response.clone().on_hover_text(format!("{pct}%"));
        }

        response
    }
}
