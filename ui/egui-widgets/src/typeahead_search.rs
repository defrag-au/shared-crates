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

use crate::theme::{Ink, Radius, Space, SpaceExt, ThemeExt, Token};
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
    accent: Ink,
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
            accent: Ink::Token(Token::AccentCyan),
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
    pub fn accent(mut self, color: impl Into<Ink>) -> Self {
        self.accent = color.into();
        self
    }

    /// Render the search box and dropdown.
    ///
    /// Forces a vertical layout internally so the input row and the results
    /// dropdown always stack top-to-bottom, even when the widget is placed
    /// inside a horizontal parent layout.
    pub fn show(self, ui: &mut Ui) -> TypeaheadResponse {
        let mut result = TypeaheadResponse::default();
        ui.vertical(|ui| {
            result = self.show_impl(ui);
        });
        result
    }

    fn show_impl(self, ui: &mut Ui) -> TypeaheadResponse {
        let mut out = TypeaheadResponse::default();
        // Row labels are click targets, not copyable data — selectable labels
        // would put the cursor into text-select (I-beam, drag-highlights) and
        // fight row clicks.
        ui.style_mut().interaction.selectable_labels = false;
        // Tall enough for an icon + a two-line title/subtitle without the
        // subtitle clipping into the next row.
        let row_height = 46.0;

        // ── Input row: magnifier + single-line edit ──────────────────────
        let edit_id = ui.make_persistent_id((self.id_salt, "edit"));
        let te_response = egui::Frame::new()
            .fill(ui.tokens().color.bg_secondary)
            .corner_radius(ui.tokens().corner(Radius::Lg))
            .inner_margin(ui.tokens().margin_xy(Space::Lg, Space::Md))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        PhosphorIcon::MagnifyingGlass
                            .rich_text(16.0, ui.tokens().color.text_secondary),
                    );
                    ui.gap(Space::Sm);
                    // Frameless edit — the surrounding rounded frame is the
                    // visible affordance. (This egui fork's `frame()` takes a
                    // `Frame`, not a bool; an empty frame draws nothing.)
                    let edit = egui::TextEdit::singleline(self.query)
                        .id(edit_id)
                        .frame(egui::Frame::default())
                        .desired_width(f32::INFINITY)
                        .hint_text(self.placeholder)
                        .text_color(ui.tokens().color.text_primary);
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

        // Focus on every APPEARANCE, if requested.
        //
        // This was a `focused_once` bool that was set and never cleared, so the
        // field focused the first time it was ever drawn in the session and
        // never again — a command palette autofocused on its first open and
        // then made you click into it for the rest of the session.
        //
        // Remembering the pass it was last drawn in answers the real question
        // instead. Drawn last pass means it is still open and the caret belongs
        // wherever the reader put it; a gap means it went away and came back,
        // which is an appearance. No coordination with whatever owns the
        // open/closed flag, so nothing has to remember to reset anything.
        if self.autofocus {
            let seen = ui.make_persistent_id((self.id_salt, "last_drawn_pass"));
            let now = ui.ctx().cumulative_pass_nr();
            let last = ui.data(|d| d.get_temp::<u64>(seen));
            if reappeared(last, now) {
                ui.memory_mut(|m| m.request_focus(edit_id));
            }
            ui.data_mut(|d| d.insert_temp(seen, now));
        }

        let len = self.options.len();
        if len == 0 {
            // Non-empty query with no results → a quiet empty state.
            if !self.query.trim().is_empty() {
                ui.gap(Space::Md);
                ui.label(
                    RichText::new(self.empty_text)
                        .small()
                        .color(ui.tokens().color.text_muted),
                );
            }
            return out;
        }

        // Keep the highlight in range as the option set shrinks/grows.
        if *self.highlight >= len {
            *self.highlight = len - 1;
        }

        // ── Keyboard navigation ───────────────────────────────────────────
        // `lost_focus()` as well as `has_focus()`: a single-line `TextEdit`
        // SURRENDERS FOCUS on Enter, so a check gated on `has_focus()` alone
        // misses the very keystroke that is meant to choose a row.
        //
        // `consume_key`, not `key_pressed`: taking the event stops egui's focus
        // manager also acting on it and moving focus off the box mid-search.
        if te_response.has_focus() || te_response.lost_focus() {
            let (down, up, enter) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
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
                // Keep the caret in the box so the next search can just be
                // typed — choosing a result is not the end of the task.
                ui.memory_mut(|m| m.request_focus(edit_id));
            }
        }

        // ── Dropdown ──────────────────────────────────────────────────────
        // Pull Copy fields into locals so the row loop only borrows
        // `self.highlight` (the one mutated on hover) — `self.row(&mut self)`
        // would otherwise clash with iterating `self.options`.
        let options = self.options;
        let accent = self.accent.of(ui);
        let id_salt = self.id_salt;
        let max_visible = self.max_visible_rows;
        let highlight = self.highlight;

        ui.gap(Space::Base);
        egui::Frame::new()
            .fill(ui.tokens().color.bg_primary)
            .corner_radius(ui.tokens().corner(Radius::Lg))
            .stroke(egui::Stroke::new(1.0_f32, ui.tokens().color.border))
            .inner_margin(ui.tokens().margin(Space::Sm))
            .show(ui, |ui| {
                let max_h = row_height * max_visible as f32;
                egui::ScrollArea::vertical()
                    .id_salt((id_salt, "dropdown"))
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // No gap between rows: the viewport height is an exact
                        // multiple of `row_height`, so whole rows always show
                        // (no half-clipped last row), and there are no dead
                        // strips between rows where a click hits nothing.
                        // Rows carry their own inner padding.
                        ui.set_item_gap_y(Space::None);
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
    // `Sense::CLICK`, NOT `Sense::click()` — the const is the non-focusable
    // variant. A focusable row joins egui's keyboard focus order, so ArrowDown
    // moves FOCUS into the list instead of moving our highlight, and Enter then
    // synthesises a click on whichever row holds focus rather than the
    // highlighted one. The dropdown is driven by `highlight`; rows are a mouse
    // target only.
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::CLICK);

    if highlighted || response.hovered() {
        ui.painter()
            .rect_filled(rect, 6.0, ui.tokens().color.bg_highlight);
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
    content.set_item_gap_x(Space::Md);

    if let Some(url) = &opt.icon_url {
        content.add(
            egui::Image::new(url)
                .fit_to_exact_size(egui::vec2(24.0, 24.0))
                .corner_radius(ui.tokens().corner(Radius::Base)),
        );
    }

    content.vertical(|ui| {
        ui.set_item_gap_y(Space::Xs);
        let title_color = if highlighted {
            accent
        } else {
            ui.tokens().color.text_primary
        };
        ui.label(RichText::new(&opt.title).color(title_color).strong());
        if let Some(sub) = &opt.subtitle {
            ui.label(
                RichText::new(sub)
                    .small()
                    .color(ui.tokens().color.text_muted),
            );
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

/// Whether this is a fresh appearance rather than a continuation.
///
/// `last_drawn` is the pass the widget was last drawn in. Adjacent passes mean
/// it never went away; a gap — or nothing at all — means it has just appeared.
fn reappeared(last_drawn: Option<u64>, now: u64) -> bool {
    match last_drawn {
        Some(last) => now.saturating_sub(last) > 1,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pass::TestPass as _;
    use egui::pos2;

    #[test]
    fn autofocus_fires_on_every_appearance_not_just_the_first() {
        // The bug: a `focused_once` bool, set and never cleared. A command
        // palette autofocused on its first open of the session and then made
        // you click into the field every time after.
        assert!(reappeared(None, 0), "never drawn — an appearance");
        assert!(reappeared(None, 900), "and still one much later");
        assert!(reappeared(Some(3), 40), "closed and reopened");
        assert!(reappeared(Some(0), 2), "even a single missed pass counts");
    }

    #[test]
    fn autofocus_leaves_the_caret_alone_while_it_stays_open() {
        // The other half, and the reason this is not just "focus every pass":
        // stealing focus on a pass where the reader is already typing would put
        // the caret back to where egui wants it rather than where they left it.
        assert!(!reappeared(Some(7), 7), "drawn twice in one pass");
        assert!(!reappeared(Some(7), 8), "and on consecutive passes");
    }

    fn opt(title: &str) -> TypeaheadOption {
        TypeaheadOption::new(title.to_lowercase(), title)
    }

    // ── interaction harness ───────────────────────────────────────────────
    // Rendering tests are not enough for a picker: "the rows are drawn" and
    // "clicking a row selects it" are different claims, and it was the second
    // one that was broken in the app.

    use egui::{Event, PointerButton, Pos2, RawInput, Rect, vec2};

    struct Harness {
        ctx: egui::Context,
        query: String,
        highlight: usize,
    }

    impl Harness {
        fn new(query: &str) -> Self {
            let ctx = egui::Context::default();
            // The input row draws a phosphor magnifier; without the font bound,
            // layout panics before any interaction happens.
            crate::icons::install_fonts(&ctx);
            Self {
                ctx,
                query: query.into(),
                highlight: 0,
            }
        }

        /// One frame with the given events; returns what the widget reported.
        ///
        /// `Context::run_ui` supplies the root `Ui` the panel shows inside —
        /// see the note in `party_finder`'s harness.
        fn frame(&mut self, options: &[TypeaheadOption], events: Vec<Event>) -> TypeaheadResponse {
            let mut out = TypeaheadResponse::default();
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 500.0))),
                events,
                ..Default::default()
            };
            let query = &mut self.query;
            let highlight = &mut self.highlight;
            let _ = self.ctx.test_pass(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    // The app puts the finder inside a horizontal row (next to
                    // the pinned chip), so the harness does too.
                    ui.horizontal(|ui| {
                        out = TypeaheadSearch::new("h", query, options, highlight).show(ui);
                    });
                });
            });
            out
        }

        fn click_at(&mut self, options: &[TypeaheadOption], pos: Pos2) -> TypeaheadResponse {
            // Hover first: egui needs the pointer to be over the widget on a
            // frame before the press is attributed to it.
            self.frame(options, vec![Event::PointerMoved(pos)]);
            self.frame(
                options,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            self.frame(
                options,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            )
        }
    }

    fn three() -> Vec<TypeaheadOption> {
        vec![
            TypeaheadOption::new("a", "$alpha").subtitle("stake1a…"),
            TypeaheadOption::new("b", "$beta").subtitle("stake1b…"),
            TypeaheadOption::new("c", "$gamma").subtitle("stake1c…"),
        ]
    }

    /// Clicking a row chooses THAT row.
    #[test]
    fn clicking_a_row_chooses_it() {
        let opts = three();
        let mut h = Harness::new("a");
        // Frame once to establish layout, then click into the second row.
        h.frame(&opts, vec![]);
        // Input row ~46px tall incl. frame; rows are 46px each below it.
        let r = h.click_at(&opts, pos2(120.0, 46.0 + 6.0 + 4.0 + 46.0 + 23.0));
        assert_eq!(
            r.chosen.as_deref(),
            Some("b"),
            "click on row 2 must choose it"
        );
    }

    /// Enter chooses the highlighted row. A single-line `TextEdit` SURRENDERS
    /// FOCUS on Enter, so anything gated on `has_focus()` never sees the key.
    #[test]
    fn enter_chooses_the_highlighted_row() {
        let opts = three();
        let mut h = Harness::new("a");
        h.frame(&opts, vec![]);
        // Focus the input by clicking it.
        h.click_at(&opts, pos2(120.0, 20.0));
        // Arrow down to the second row, then Enter.
        h.frame(
            &opts,
            vec![Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        let r = h.frame(
            &opts,
            vec![Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(
            r.chosen.as_deref(),
            Some("b"),
            "enter must choose the highlight"
        );
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
