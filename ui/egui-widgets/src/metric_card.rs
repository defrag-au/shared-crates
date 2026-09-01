//! `MetricCard` — a dashboard stat card with label, value, optional trend and sparkline; `MetricRow` lays a set of them out sharing one width, height and baseline.
//!
//! Displays a key metric in a framed card. Supports:
//! - Large primary value with small label
//! - Optional trend indicator (up/down arrow with delta)
//! - Optional inline sparkline for recent history
//! - Configurable accent color for value/trend

use egui::{Color32, FontId, RichText, Ui, Vec2};

use crate::theme;

/// Font sizes the card paints with. Named because [`MetricCard::natural_size`]
/// has to measure with exactly the same ones — a measurement that drifts from
/// the painting is worse than no measurement, since it produces a row that is
/// confidently the wrong width.
const LABEL_SIZE: f32 = 12.0;
const VALUE_SIZE: f32 = 24.0;
const SUBTITLE_SIZE: f32 = 13.0;
const TREND_SIZE: f32 = 11.0;
const MARGIN: f32 = 12.0;

/// Trend direction for the metric.
#[derive(Clone, Copy, PartialEq)]
pub enum Trend {
    /// Value is increasing (shown in green with up arrow).
    Up,
    /// Value is decreasing (shown in red with down arrow).
    Down,
    /// Value is stable (shown in muted with dash).
    Flat,
}

/// A dashboard metric card.
pub struct MetricCard<'a> {
    /// Small label above the value (e.g. "Total Accrued").
    label: &'a str,
    /// Large primary value (e.g. "12,345").
    value: &'a str,
    /// Optional subtitle below the value (e.g. "5.2/hr").
    subtitle: Option<String>,
    /// Optional trend indicator.
    trend: Option<(Trend, String)>,
    /// Optional sparkline data points.
    sparkline_data: Option<&'a [f64]>,
    /// Accent color for the value text.
    value_color: Color32,
    /// Card width (None = available width).
    width: Option<f32>,
    /// Forced card height, so a row can share one baseline.
    height: Option<f32>,
    /// Card background color.
    bg_color: Color32,
    /// Border color.
    border_color: Color32,
}

impl<'a> MetricCard<'a> {
    /// Create a new metric card.
    pub fn new(label: &'a str, value: &'a str) -> Self {
        Self {
            label,
            value,
            subtitle: None,
            trend: None,
            sparkline_data: None,
            value_color: theme::TEXT_PRIMARY,
            width: None,
            height: None,
            bg_color: theme::BG_HIGHLIGHT,
            border_color: theme::BORDER,
        }
    }

    /// Set a subtitle shown below the value.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Set the trend indicator.
    pub fn trend(mut self, direction: Trend, delta: impl Into<String>) -> Self {
        self.trend = Some((direction, delta.into()));
        self
    }

    /// Set sparkline data to show a mini chart in the card.
    pub fn sparkline(mut self, data: &'a [f64]) -> Self {
        self.sparkline_data = Some(data);
        self
    }

    /// Set the value text color.
    pub fn value_color(mut self, color: Color32) -> Self {
        self.value_color = color;
        self
    }

    /// Set the card width.
    ///
    /// Pins the card's content box to exactly this width — which it did NOT
    /// used to: `allocate_ui` merely *offered* a size and the frame still grew
    /// to its content, so six cards all asking for 150 came out six different
    /// widths and every stat row in the codebase was ragged.
    ///
    /// ⚠️ **A value too wide to fit still overflows and widens the card.**
    /// egui does not clip a label, and truncating a number is worse than a
    /// wide card, so this is a floor you can rely on and a ceiling you cannot.
    /// Picking a width by hand and hoping the values fit is exactly the habit
    /// that produced the ragged rows — use [`MetricRow`], which measures the
    /// values first.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Force the card's height. Used by [`MetricRow`] to level a row.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// The size this card wants, measured with the fonts it will paint with.
    ///
    /// Exists so [`MetricRow`] can size a whole row before drawing any of it.
    /// egui lays out immediately, so there is no natural second pass to ask
    /// "how big did that turn out" — the measurement has to be predicted.
    pub fn natural_size(&self, ui: &Ui) -> Vec2 {
        // Via the painter, as `activity_feed` and `arrival_field` do — `Fonts`
        // itself wants `&mut`, which `Context::fonts` will not hand out.
        let measure = |text: &str, size: f32| -> Vec2 {
            ui.painter()
                .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE)
                .size()
        };

        let label = measure(self.label, LABEL_SIZE);
        let value = measure(self.value, VALUE_SIZE);

        // The value row is value + subtitle side by side.
        let mut value_row = value;
        if let Some(sub) = &self.subtitle {
            let s = measure(sub, SUBTITLE_SIZE);
            value_row.x += ui.spacing().item_spacing.x + s.x;
            value_row.y = value_row.y.max(s.y);
        }

        let mut w = label.x.max(value_row.x);
        let mut h = label.y + 4.0 + value_row.y;

        if let Some((_, delta)) = &self.trend {
            // 8px arrow + 4px gap, per `show`.
            let t = measure(delta, TREND_SIZE);
            w = w.max(8.0 + 4.0 + t.x);
            h += 2.0 + t.y.max(8.0);
        }
        if self.sparkline_data.is_some_and(|d| d.len() >= 2) {
            h += 8.0 + 32.0;
        }

        // Frame margins on both sides, plus the 1px stroke.
        Vec2::new(w + MARGIN * 2.0 + 2.0, h + MARGIN * 2.0 + 2.0)
    }

    /// Set the background color.
    pub fn bg_color(mut self, color: Color32) -> Self {
        self.bg_color = color;
        self
    }

    /// Render the metric card.
    pub fn show(self, ui: &mut Ui) {
        let frame = egui::Frame::NONE
            .fill(self.bg_color)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0_f32, self.border_color));

        let add_contents = |ui: &mut Ui| {
            // Force vertical layout — cards may be placed inside ui.horizontal()
            ui.vertical(|ui| {
                // Label — top of card, distinct from value
                ui.label(
                    RichText::new(self.label)
                        .color(theme::TEXT_SECONDARY)
                        .size(12.0),
                );

                ui.add_space(4.0);

                // Value row — value + optional subtitle on same line
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.value)
                            .color(self.value_color)
                            .size(24.0)
                            .strong(),
                    );
                    if let Some(subtitle) = &self.subtitle {
                        ui.label(RichText::new(subtitle).color(theme::TEXT_MUTED).size(13.0));
                    }
                });

                // Trend indicator on its own line
                if let Some((direction, delta)) = &self.trend {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let color = match direction {
                            Trend::Up => theme::SUCCESS,
                            Trend::Down => theme::ERROR,
                            Trend::Flat => theme::TEXT_MUTED,
                        };

                        // Paint a small triangle arrow instead of unicode
                        let size = 8.0;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                        let center = rect.center();
                        let painter = ui.painter();
                        match direction {
                            Trend::Up => {
                                let half = size / 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(center.x, center.y - half),
                                        egui::pos2(center.x + half, center.y + half),
                                        egui::pos2(center.x - half, center.y + half),
                                    ],
                                    color,
                                    egui::Stroke::NONE,
                                ));
                            }
                            Trend::Down => {
                                let half = size / 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(center.x - half, center.y - half),
                                        egui::pos2(center.x + half, center.y - half),
                                        egui::pos2(center.x, center.y + half),
                                    ],
                                    color,
                                    egui::Stroke::NONE,
                                ));
                            }
                            Trend::Flat => {
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - size / 2.0, center.y),
                                        egui::pos2(center.x + size / 2.0, center.y),
                                    ],
                                    egui::Stroke::new(1.5_f32, color),
                                );
                            }
                        }

                        ui.label(RichText::new(delta).color(color).size(11.0));
                    });
                }

                // Sparkline — own row with breathing room
                if let Some(data) = self.sparkline_data
                    && data.len() >= 2
                {
                    ui.add_space(8.0);
                    crate::Sparkline::new(data)
                        .height(32.0)
                        .line_width(1.5)
                        .line_color(self.value_color)
                        .fill(Color32::from_rgba_premultiplied(
                            self.value_color.r(),
                            self.value_color.g(),
                            self.value_color.b(),
                            25,
                        ))
                        .show_endpoint(false)
                        .bg_color(self.bg_color)
                        .show(ui);
                }
            });
        };

        let (width, height) = (self.width, self.height);
        if width.is_none() && height.is_none() {
            frame.show(ui, add_contents);
            return;
        }
        // `allocate_ui(vec2(w, 0))` — what this used to do — only offers a
        // hint; the frame still sizes to its content, so a row of cards all
        // asking for the same width came out six different widths. Setting the
        // minimum on the INNER ui is what actually pins the frame, because the
        // frame grows to whatever its contents claim.
        ui.scope(|ui| {
            frame.show(ui, |ui| {
                if let Some(w) = width {
                    // BOTH bounds. A minimum alone lets content grow the card,
                    // which is the original bug; a maximum alone lets it
                    // shrink. Dropping the max also silently un-constrains
                    // anything inside that sizes to `available_width()` — the
                    // sparkline stretched to the full window the moment this
                    // was min-only.
                    let inner = w - MARGIN * 2.0 - 2.0;
                    ui.set_min_width(inner);
                    ui.set_max_width(inner);
                }
                if let Some(h) = height {
                    ui.set_min_height(h - MARGIN * 2.0 - 2.0);
                }
                add_contents(ui);
            });
        });
    }
}

/// A row of [`MetricCard`]s that share one width, one height and one baseline.
///
/// ## Why this exists rather than a width on each card
///
/// A stat row reads as a table, and a table whose cells disagree about their
/// edges reads as clutter — the recurring note that rows must share a spine.
/// But a card cannot know the row's width from inside itself, and callers
/// setting the same `width(150)` on every card do NOT get one: the value is a
/// floor, so `"11796"` and `"278248 .. 333004 ADA"` still render 165px apart.
/// That is exactly how token-explorer's readout ended up ragged, and the
/// storybook asserted "fixed-width cards align in horizontal rows" while
/// demonstrating only values that happened to be the same size.
///
/// So the row measures every card with [`MetricCard::natural_size`], takes the
/// widest and tallest, and renders them all at that. Nothing is truncated — a
/// clipped number is a worse failure than a wide row — and no caller has to
/// guess a magic width.
#[derive(Default)]
pub struct MetricRow<'a> {
    cards: Vec<MetricCard<'a>>,
    spacing: Option<f32>,
    min_card_width: f32,
}

impl<'a> MetricRow<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, card: MetricCard<'a>) -> Self {
        self.cards.push(card);
        self
    }

    /// Gap between cards. Defaults to the ui's own item spacing.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// Floor for the shared width, so a row of very short values does not
    /// collapse into a huddle of tiny cards.
    pub fn min_card_width(mut self, width: f32) -> Self {
        self.min_card_width = width;
        self
    }

    pub fn show(self, ui: &mut Ui) {
        if self.cards.is_empty() {
            return;
        }
        let (mut w, h) = self
            .cards
            .iter()
            .fold((self.min_card_width, 0.0_f32), |(w, h), c| {
                let s = c.natural_size(ui);
                (w.max(s.x), h.max(s.y))
            });

        // Never wider than the row can actually show. Sizing purely to the
        // widest value put six cards at 305px into a 1310px panel and ran the
        // last one off the edge — a row that overflows its container is no
        // better aligned than a ragged one, it is just ragged off-screen.
        // Cards that then cannot fit their value overflow individually, which
        // is visible and local rather than silently clipping the row.
        let n = self.cards.len() as f32;
        let gap = self.spacing.unwrap_or(ui.spacing().item_spacing.x);
        // 2px of slack per card: `available_width()` is measured before the
        // row lays out and does not know about a scrollbar that may appear, so
        // sizing to it exactly leaves the last card clipped at the edge.
        let budget = (ui.available_width() - gap * (n - 1.0)) / n - 2.0;
        if budget > self.min_card_width {
            w = w.min(budget);
        }

        // `Align::TOP` is load-bearing: `ui.horizontal` centres by default, so
        // cards of differing height sit at differing tops and the row looks
        // staggered even once the widths agree.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            if let Some(s) = self.spacing {
                ui.spacing_mut().item_spacing.x = s;
            }
            for card in self.cards {
                card.width(w).height(h).show(ui);
            }
        });
    }
}
