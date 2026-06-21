//! `TypeaheadSearch` — a search box with a keyboard-navigable result dropdown.
//!
//! The recurring "type a few characters, pick from a live-filtered list"
//! pattern: token search, command palettes, entity pickers. The widget is
//! **presentational** — the caller supplies the already-ranked `options` to
//! show (filtered server-side, or locally via [`filter_options`]) and owns the
//! query string and highlight index as state. It renders the input + dropdown,
//! handles up/down/enter navigation and click selection, and reports back what
//! changed.
//!
//! Splitting filtering out of the widget keeps it reusable for both
//! server-driven search (results arrive from an endpoint as the query changes)
//! and purely client-side filtering (precompute a flat option list once, then
//! [`filter_options`] per keystroke).
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{TypeaheadSearch, TypeaheadOption};
//!
//! // `query` and `highlight` are persisted by the caller across frames.
//! let shown = egui_widgets::filter_options(&all_options, query, 25);
//! let resp = TypeaheadSearch::new("token_search", query, &shown, highlight)
//!     .placeholder("Search tokens by name or ticker…")
//!     .autofocus(true)
//!     .show(ui);
//! if let Some(id) = resp.chosen {
//!     // navigate to the selected option
//! }
//! if resp.query_changed {
//!     // refetch server results for the new `query`
//! }
//! ```

use egui::{Color32, RichText, Ui};

use crate::theme;
use crate::{Chip, ChipVariant, PhosphorIcon};

/// One selectable row in the dropdown. All display strings are caller-formatted.
#[derive(Clone)]
pub struct TypeaheadOption {
    /// Opaque value returned when this row is chosen (e.g. a policy id).
    pub id: String,
    /// Primary label (token name).
    pub title: String,
    /// Optional secondary line (ticker, truncated policy, …).
    pub subtitle: Option<String>,
    /// Optional leading icon URL (rendered via the active image loader).
    pub icon_url: Option<String>,
    /// Optional trailing semantic badges (verified / rug / …).
    pub badges: Vec<(String, ChipVariant)>,
}

impl TypeaheadOption {
    /// Construct a minimal option (id + title).
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            icon_url: None,
            badges: Vec::new(),
        }
    }

    /// Set the secondary line.
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }

    /// Set the leading icon URL.
    pub fn icon(mut self, url: impl Into<String>) -> Self {
        self.icon_url = Some(url.into());
        self
    }

    /// Add a trailing badge.
    pub fn badge(mut self, label: impl Into<String>, variant: ChipVariant) -> Self {
        self.badges.push((label.into(), variant));
        self
    }
}

/// Outcome of one [`TypeaheadSearch::show`] frame.
#[derive(Default)]
pub struct TypeaheadResponse {
    /// The query text was edited this frame (refetch / refilter).
    pub query_changed: bool,
    /// An option was chosen (enter on the highlight, or a click) — its `id`.
    pub chosen: Option<String>,
}

/// Builder for the typeahead search box.
pub struct TypeaheadSearch<'a> {
    id_salt: &'a str,
    query: &'a mut String,
    options: &'a [TypeaheadOption],
    highlight: &'a mut usize,
    placeholder: &'a str,
    empty_text: &'a str,
    max_visible_rows: usize,
    autofocus: bool,
    accent: Color32,
}

impl<'a> TypeaheadSearch<'a> {
    /// Construct over caller-owned `query` and `highlight` state and the
    /// already-ranked `options` to display.
    pub fn new(
        id_salt: &'a str,
        query: &'a mut String,
        options: &'a [TypeaheadOption],
        highlight: &'a mut usize,
    ) -> Self {
        Self {
            id_salt,
            query,
            options,
            highlight,
            placeholder: "Search…",
            empty_text: "No matches",
            max_visible_rows: 8,
            autofocus: false,
            accent: theme::ACCENT_CYAN,
        }
    }

    /// Placeholder / hint text shown in the empty input.
    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = text;
        self
    }

    /// Text shown when the query is non-empty but no options match.
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty_text = text;
        self
    }

    /// Max rows visible before the dropdown scrolls (default 8).
    pub fn max_visible_rows(mut self, n: usize) -> Self {
        self.max_visible_rows = n.max(1);
        self
    }

    /// Focus the input the first time it's shown (once per widget id).
    pub fn autofocus(mut self, yes: bool) -> Self {
        self.autofocus = yes;
        self
    }

    /// Accent color for the highlighted row and focus ring (default cyan).
    pub fn accent(mut self, color: Color32) -> Self {
        self.accent = color;
        self
    }

    /// Render the search box and dropdown.
    pub fn show(self, ui: &mut Ui) -> TypeaheadResponse {
        let mut out = TypeaheadResponse::default();
        let row_height = 34.0;

        // ── Input row: magnifier + single-line edit ──────────────────────
        let edit_id = ui.make_persistent_id((self.id_salt, "edit"));
        let te_response = egui::Frame::new()
            .fill(theme::BG_SECONDARY)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(PhosphorIcon::MagnifyingGlass.rich_text(16.0, theme::TEXT_SECONDARY));
                    ui.add_space(4.0);
                    // Frameless edit — the surrounding rounded frame is the
                    // visible affordance. (This egui fork's `frame()` takes a
                    // `Frame`, not a bool; an empty frame draws nothing.)
                    let edit = egui::TextEdit::singleline(self.query)
                        .id(edit_id)
                        .frame(egui::Frame::default())
                        .desired_width(f32::INFINITY)
                        .hint_text(self.placeholder)
                        .text_color(theme::TEXT_PRIMARY);
                    ui.add(edit)
                })
                .inner
            })
            .inner;

        if te_response.changed() {
            out.query_changed = true;
            // A fresh query invalidates the previous highlight.
            *self.highlight = 0;
        }

        // Focus once on first appearance, if requested.
        if self.autofocus {
            let focused_once = ui.make_persistent_id((self.id_salt, "focused_once"));
            let already = ui
                .data_mut(|d| d.get_temp::<bool>(focused_once))
                .unwrap_or(false);
            if !already {
                ui.memory_mut(|m| m.request_focus(edit_id));
                ui.data_mut(|d| d.insert_temp(focused_once, true));
            }
        }

        let len = self.options.len();
        if len == 0 {
            // Non-empty query with no results → a quiet empty state.
            if !self.query.trim().is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(self.empty_text)
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            }
            return out;
        }

        // Keep the highlight in range as the option set shrinks/grows.
        if *self.highlight >= len {
            *self.highlight = len - 1;
        }

        // ── Keyboard navigation (only while the input is focused) ─────────
        if te_response.has_focus() {
            let (down, up, enter) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Enter),
                )
            });
            if down {
                *self.highlight = (*self.highlight + 1).min(len - 1);
            }
            if up {
                *self.highlight = self.highlight.saturating_sub(1);
            }
            if enter {
                out.chosen = Some(self.options[*self.highlight].id.clone());
            }
        }

        // ── Dropdown ──────────────────────────────────────────────────────
        // Pull Copy fields into locals so the row loop only borrows
        // `self.highlight` (the one mutated on hover) — `self.row(&mut self)`
        // would otherwise clash with iterating `self.options`.
        let options = self.options;
        let accent = self.accent;
        let id_salt = self.id_salt;
        let max_visible = self.max_visible_rows;
        let highlight = self.highlight;

        ui.add_space(6.0);
        egui::Frame::new()
            .fill(theme::BG_PRIMARY)
            .corner_radius(8.0)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .inner_margin(4.0)
            .show(ui, |ui| {
                let max_h = row_height * max_visible as f32;
                egui::ScrollArea::vertical()
                    .id_salt((id_salt, "dropdown"))
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for (i, opt) in options.iter().enumerate() {
                            let resp = row(ui, i == *highlight, opt, row_height, accent);
                            // Hovering moves the highlight so mouse + keyboard
                            // selection stay in sync.
                            if resp.hovered() {
                                *highlight = i;
                            }
                            if resp.clicked() {
                                out.chosen = Some(opt.id.clone());
                            }
                        }
                    });
            });

        out
    }
}

/// Render a single dropdown row, returning its interaction response (the
/// caller reads `.hovered()` / `.clicked()`).
fn row(
    ui: &mut Ui,
    highlighted: bool,
    opt: &TypeaheadOption,
    height: f32,
    accent: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click());

    if highlighted || response.hovered() {
        ui.painter().rect_filled(rect, 6.0, theme::BG_HIGHLIGHT);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // Lay out content within the row rect.
    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(8.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    content.spacing_mut().item_spacing.x = 8.0;

    if let Some(url) = &opt.icon_url {
        content.add(
            egui::Image::new(url)
                .fit_to_exact_size(egui::vec2(24.0, 24.0))
                .corner_radius(4.0),
        );
    }

    content.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let title_color = if highlighted {
            accent
        } else {
            theme::TEXT_PRIMARY
        };
        ui.label(RichText::new(&opt.title).color(title_color).strong());
        if let Some(sub) = &opt.subtitle {
            ui.label(RichText::new(sub).small().color(theme::TEXT_MUTED));
        }
    });

    if !opt.badges.is_empty() {
        content.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (label, variant) in &opt.badges {
                Chip::new(label).variant(*variant).show(ui);
            }
        });
    }

    response
}

/// Case-insensitive prefix/substring filter + rank over a precomputed flat
/// option list — the client-side counterpart to a server search endpoint.
///
/// Tiers (best first): exact title, title prefix, word-start prefix, title
/// substring, subtitle substring. Returns borrowed references to the matching
/// options (cap `limit`), preserving the original order within a tier.
pub fn filter_options<'a>(
    options: &'a [TypeaheadOption],
    query: &str,
    limit: usize,
) -> Vec<&'a TypeaheadOption> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(u8, usize, &TypeaheadOption)> = options
        .iter()
        .enumerate()
        .filter_map(|(i, opt)| {
            let title = opt.title.to_lowercase();
            let score = if title == q {
                0
            } else if title.starts_with(&q) {
                1
            } else if title.split_whitespace().any(|w| w.starts_with(&q)) {
                2
            } else if title.contains(&q) {
                3
            } else if opt
                .subtitle
                .as_deref()
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false)
            {
                4
            } else {
                return None;
            };
            Some((score, i, opt))
        })
        .collect();

    // Sort by tier, then original index (stable) for a predictable order.
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(limit).map(|(_, _, o)| o).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(title: &str) -> TypeaheadOption {
        TypeaheadOption::new(title.to_lowercase(), title)
    }

    #[test]
    fn empty_query_returns_nothing() {
        let options = vec![opt("Snek")];
        assert!(filter_options(&options, "  ", 10).is_empty());
    }

    #[test]
    fn ranks_exact_then_prefix_then_substring() {
        let options = vec![opt("Megasnek"), opt("Snekkidoo"), opt("Snek")];
        let got: Vec<&str> = filter_options(&options, "snek", 10)
            .iter()
            .map(|o| o.title.as_str())
            .collect();
        assert_eq!(got, vec!["Snek", "Snekkidoo", "Megasnek"]);
    }

    #[test]
    fn matches_subtitle_as_last_resort() {
        let options = vec![TypeaheadOption::new("id", "Token").subtitle("policy279c909f")];
        assert_eq!(filter_options(&options, "279c909f", 10).len(), 1);
    }

    #[test]
    fn respects_limit() {
        let options: Vec<_> = (0..30).map(|i| opt(&format!("Snek{i}"))).collect();
        assert_eq!(filter_options(&options, "snek", 5).len(), 5);
    }
}
