//! Compound-key prefix trie tag filter widget.
//!
//! A text input with instant prefix search across hierarchical entries
//! (category + value). Selected entries display as removable tag chips.
//! The compound-key trie indexes each entry under both its full label
//! ("background: red") and its value alone ("red"), so typing either
//! a category prefix or a value prefix finds matching entries.

use crate::theme;
use egui::{Color32, Rect, RichText, Vec2};
use std::collections::HashSet;

// ============================================================================
// Public types
// ============================================================================

/// A single filterable entry.
pub struct FilterEntry {
    /// Full display label, e.g. "Background: Red".
    pub label: String,
    /// Category name, e.g. "Background".
    pub category: String,
    /// Value name, e.g. "Red".
    pub value: String,
    /// Optional tag chip color (e.g. green = owned, muted = missing).
    pub color: Option<Color32>,
}

/// Configuration for the filter widget.
pub struct TraitFilterConfig {
    /// Max suggestions to show in dropdown.
    pub max_suggestions: usize,
    /// Placeholder text when input is empty.
    pub placeholder: &'static str,
    /// Tag chip corner rounding.
    pub tag_rounding: f32,
    /// Whether to group suggestions by category in the dropdown.
    pub group_by_category: bool,
    /// Unique ID salt for this widget instance.
    pub id: &'static str,
}

impl Default for TraitFilterConfig {
    fn default() -> Self {
        Self {
            max_suggestions: 12,
            placeholder: "Filter by trait...",
            tag_rounding: 4.0,
            group_by_category: true,
            id: "trait_filter",
        }
    }
}

/// Persistent state for the filter widget.
pub struct TraitFilterState {
    /// Current text input.
    input: String,
    /// Indices of selected entries (into the entries slice passed to `show`).
    pub selected: Vec<usize>,
    /// Compound-key prefix trie.
    trie: CompoundTrie,
    /// Length of entries slice when trie was last built.
    entries_len: usize,
    /// Keyboard-navigated cursor index within the current suggestions list.
    cursor: Option<usize>,
}

impl Default for TraitFilterState {
    fn default() -> Self {
        Self {
            input: String::new(),
            selected: Vec::new(),
            trie: CompoundTrie::empty(),
            entries_len: 0,
            cursor: None,
        }
    }
}

/// Result from a `show()` call.
pub struct TraitFilterResponse {
    /// Whether the selected set changed this frame.
    pub changed: bool,
}

// ============================================================================
// Compound-key prefix trie
// ============================================================================

/// Sorted-vec prefix trie with dual keys per entry.
///
/// Each entry is indexed under two lowercase keys:
/// 1. Full label: `"background: red"` — matches category-first search
/// 2. Value only: `"red"` — matches value-first search
///
/// Prefix lookup via `partition_point` (binary search) for O(log n) start,
/// then linear scan forward while prefix matches.
struct CompoundTrie {
    sorted: Vec<(String, usize)>,
}

impl CompoundTrie {
    fn empty() -> Self {
        Self { sorted: Vec::new() }
    }

    fn build(entries: &[FilterEntry]) -> Self {
        let mut keys = Vec::with_capacity(entries.len() * 2);
        for (i, entry) in entries.iter().enumerate() {
            keys.push((entry.label.to_lowercase(), i));
            keys.push((entry.value.to_lowercase(), i));
        }
        keys.sort();
        keys.dedup();
        Self { sorted: keys }
    }

    /// Find entry indices whose keys start with `query`.
    /// Excludes indices in `exclude` and deduplicates (an entry may match
    /// on both its label key and value key).
    fn prefix_search(&self, query: &str, max: usize, exclude: &[usize]) -> Vec<usize> {
        let q = query.to_lowercase();
        let start = self
            .sorted
            .partition_point(|(k, _)| k.as_str() < q.as_str());
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        for (key, idx) in &self.sorted[start..] {
            if !key.starts_with(&q) {
                break;
            }
            if exclude.contains(idx) {
                continue;
            }
            if seen.insert(*idx) {
                results.push(*idx);
                if results.len() >= max {
                    break;
                }
            }
        }
        results
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Show the trait filter widget: tag chips + text input + dropdown suggestions.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut TraitFilterState,
    entries: &[FilterEntry],
    config: &TraitFilterConfig,
) -> TraitFilterResponse {
    // Rebuild trie if entries changed
    if entries.len() != state.entries_len {
        state.trie = CompoundTrie::build(entries);
        state.entries_len = entries.len();
        state.selected.retain(|&idx| idx < entries.len());
    }

    let mut changed = false;
    let widget_id = ui.make_persistent_id(config.id);
    let input_id = widget_id.with("input");

    // Dropdown visibility is driven entirely by focus — no separate flag needed.
    // Escape defocuses the input (egui default), which hides the dropdown.
    let has_focus = ui.ctx().memory(|m| m.has_focus(input_id));

    // Compute suggestions when focused.
    let suggestions = if has_focus {
        state
            .trie
            .prefix_search(&state.input, config.max_suggestions, &state.selected)
    } else {
        Vec::new()
    };

    // Auto-highlight the best match whenever suggestions exist.
    if !suggestions.is_empty() {
        match state.cursor {
            None => state.cursor = Some(0),
            Some(c) if c >= suggestions.len() => state.cursor = Some(0),
            _ => {}
        }
    } else {
        state.cursor = None;
    }

    // ── Pre-emptive key consumption ─────────────────────────────────
    // Consume keys BEFORE TextEdit renders so it never sees them.
    let mut key_enter = false;
    let mut key_down = false;
    let mut key_up = false;

    if has_focus && !suggestions.is_empty() {
        let mods = egui::Modifiers::NONE;
        key_enter = ui.input_mut(|i| i.consume_key(mods, egui::Key::Enter));
        key_down = ui.input_mut(|i| i.consume_key(mods, egui::Key::ArrowDown));
        key_up = ui.input_mut(|i| i.consume_key(mods, egui::Key::ArrowUp));
    }

    // ── Tag chips + text input row ──────────────────────────────────
    let row_resp = ui
        .horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

            // Paint selected tags
            let mut remove_idx: Option<usize> = None;
            for (pos, &entry_idx) in state.selected.iter().enumerate() {
                if entry_idx >= entries.len() {
                    continue;
                }
                let entry = &entries[entry_idx];
                if paint_tag(ui, entry, config.tag_rounding) {
                    remove_idx = Some(pos);
                }
            }
            if let Some(pos) = remove_idx {
                state.selected.remove(pos);
                changed = true;
            }

            // Text input — Enter/ArrowUp/ArrowDown already consumed above.
            let remaining_width = ui.available_width().max(80.0);
            ui.add_sized(
                Vec2::new(remaining_width, 18.0),
                egui::TextEdit::singleline(&mut state.input)
                    .hint_text(config.placeholder)
                    .desired_width(remaining_width)
                    .font(egui::FontId::proportional(11.0))
                    .margin(Vec2::new(4.0, 2.0))
                    .id(input_id),
            )
        })
        .inner;

    // ── Process consumed keys ───────────────────────────────────────
    // Backspace on empty input removes last tag (TextEdit ignores
    // backspace when empty, so no need to consume it).
    if has_focus
        && state.input.is_empty()
        && !state.selected.is_empty()
        && ui.input(|i| i.key_pressed(egui::Key::Backspace))
    {
        state.selected.pop();
        changed = true;
    }

    if !suggestions.is_empty() {
        if key_down {
            state.cursor = Some(match state.cursor {
                Some(c) => (c + 1).min(suggestions.len() - 1),
                None => 0,
            });
        }
        if key_up {
            state.cursor = Some(match state.cursor {
                Some(c) if c > 0 => c - 1,
                _ => 0,
            });
        }
        if key_enter {
            let idx = state.cursor.unwrap_or(0);
            if let Some(&entry_idx) = suggestions.get(idx) {
                state.selected.push(entry_idx);
                state.input.clear();
                changed = true;
            }
        }
    }

    // Re-query suggestions after selection changes this frame
    let suggestions = if changed {
        let fresh = state
            .trie
            .prefix_search(&state.input, config.max_suggestions, &state.selected);
        state.cursor = if fresh.is_empty() { None } else { Some(0) };
        fresh
    } else {
        suggestions
    };

    // ── Dropdown suggestions (visible only when input is focused) ───
    if has_focus && !suggestions.is_empty() {
        let input_rect = row_resp.rect;
        let popup_id = widget_id.with("popup");

        let dropdown_resp = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(input_rect.min.x, input_rect.max.y + 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme::BG_SECONDARY)
                    .corner_radius(4.0)
                    .stroke(egui::Stroke::new(1.0, theme::BG_HIGHLIGHT))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.set_max_width(input_rect.width().max(200.0));
                        paint_suggestions(
                            ui,
                            entries,
                            &suggestions,
                            state.cursor,
                            config.group_by_category,
                        )
                    })
                    .inner
            });

        if let Some(clicked_idx) = dropdown_resp.inner {
            state.selected.push(clicked_idx);
            state.input.clear();
            state.cursor = Some(0);
            changed = true;
            // Return focus to input so user can keep typing
            ui.ctx().memory_mut(|m| m.request_focus(input_id));
        }
    } else if has_focus && suggestions.is_empty() && !state.input.is_empty() {
        let input_rect = row_resp.rect;
        let popup_id = widget_id.with("popup");
        egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(input_rect.min.x, input_rect.max.y + 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theme::BG_SECONDARY)
                    .corner_radius(4.0)
                    .stroke(egui::Stroke::new(1.0, theme::BG_HIGHLIGHT))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("No matching traits")
                                .color(theme::TEXT_MUTED)
                                .size(10.0),
                        );
                    });
            });
    }

    TraitFilterResponse { changed }
}

// ============================================================================
// Painting helpers
// ============================================================================

/// Paint a single tag chip. Returns `true` if the remove button was clicked.
fn paint_tag(ui: &mut egui::Ui, entry: &FilterEntry, rounding: f32) -> bool {
    let chip_color = entry
        .color
        .unwrap_or(Color32::from_rgba_premultiplied(86, 95, 137, 160));
    let text_color = if is_light(chip_color) {
        Color32::from_rgb(26, 27, 38)
    } else {
        Color32::from_rgb(220, 220, 230)
    };

    let label_text = &entry.label;
    let font = egui::FontId::proportional(10.0);
    let close_font = egui::FontId::proportional(9.0);

    let galley = ui
        .painter()
        .layout_no_wrap(label_text.to_string(), font.clone(), text_color);
    let text_width = galley.size().x;

    let close_width = 12.0;
    let pad_h = 6.0;
    let pad_v = 2.0;
    let chip_width = pad_h + text_width + 4.0 + close_width + pad_h;
    let chip_height = galley.size().y + pad_v * 2.0;

    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(chip_width, chip_height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, rounding, chip_color);

        let text_pos = egui::pos2(rect.min.x + pad_h, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, text_color);

        let close_rect = Rect::from_min_size(
            egui::pos2(rect.max.x - pad_h - close_width, rect.min.y),
            Vec2::new(close_width, chip_height),
        );
        let close_color = if resp.hovered() {
            Color32::from_rgb(247, 118, 142)
        } else {
            text_color.gamma_multiply(0.6)
        };
        ui.painter().text(
            close_rect.center(),
            egui::Align2::CENTER_CENTER,
            "\u{00d7}",
            close_font,
            close_color,
        );
    }

    resp.clicked()
}

/// Paint the grouped suggestion list. Returns the entry index if one was clicked.
fn paint_suggestions(
    ui: &mut egui::Ui,
    entries: &[FilterEntry],
    suggestions: &[usize],
    cursor: Option<usize>,
    group_by_category: bool,
) -> Option<usize> {
    let mut clicked = None;

    if group_by_category {
        let mut groups: Vec<(&str, Vec<(usize, usize)>)> = Vec::new();
        for (suggestion_pos, &entry_idx) in suggestions.iter().enumerate() {
            let cat = entries[entry_idx].category.as_str();
            if let Some(group) = groups.iter_mut().find(|(c, _)| *c == cat) {
                group.1.push((suggestion_pos, entry_idx));
            } else {
                groups.push((cat, vec![(suggestion_pos, entry_idx)]));
            }
        }

        for (category, items) in &groups {
            ui.label(
                RichText::new(*category)
                    .color(theme::TEXT_MUTED)
                    .size(9.0)
                    .strong(),
            );

            for &(suggestion_pos, entry_idx) in items {
                let entry = &entries[entry_idx];
                let is_cursor = cursor == Some(suggestion_pos);

                let resp = ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let bg = if is_cursor {
                        theme::BG_HIGHLIGHT
                    } else {
                        Color32::TRANSPARENT
                    };
                    let text_color = if is_cursor {
                        theme::ACCENT_CYAN
                    } else {
                        theme::TEXT_PRIMARY
                    };

                    let resp =
                        ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::click());
                    if is_cursor {
                        ui.painter().rect_filled(resp.rect, 2.0, bg);
                    }
                    if let Some(color) = entry.color {
                        let dot_center = egui::pos2(resp.rect.min.x + 12.0, resp.rect.center().y);
                        ui.painter().circle_filled(dot_center, 3.0, color);
                    }
                    let text_x = if entry.color.is_some() {
                        resp.rect.min.x + 20.0
                    } else {
                        resp.rect.min.x + 4.0
                    };
                    ui.painter().text(
                        egui::pos2(text_x, resp.rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &entry.value,
                        egui::FontId::proportional(11.0),
                        text_color,
                    );

                    resp
                });

                if resp.inner.clicked()
                    || resp.inner.hovered() && ui.input(|i| i.pointer.any_released())
                {
                    clicked = Some(entry_idx);
                }
            }

            ui.add_space(2.0);
        }
    } else {
        for (suggestion_pos, &entry_idx) in suggestions.iter().enumerate() {
            let entry = &entries[entry_idx];
            let is_cursor = cursor == Some(suggestion_pos);
            let text_color = if is_cursor {
                theme::ACCENT_CYAN
            } else {
                theme::TEXT_PRIMARY
            };
            let bg = if is_cursor {
                theme::BG_HIGHLIGHT
            } else {
                Color32::TRANSPARENT
            };

            let resp = ui.horizontal(|ui| {
                let resp = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::click());
                if is_cursor {
                    ui.painter().rect_filled(resp.rect, 2.0, bg);
                }
                ui.painter().text(
                    egui::pos2(resp.rect.min.x + 4.0, resp.rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &entry.label,
                    egui::FontId::proportional(11.0),
                    text_color,
                );
                resp
            });

            if resp.inner.clicked() {
                clicked = Some(entry_idx);
            }
        }
    }

    clicked
}

/// Rough check: is this color "light" enough to need dark text?
fn is_light(c: Color32) -> bool {
    let luma = c.r() as f32 * 0.299 + c.g() as f32 * 0.587 + c.b() as f32 * 0.114;
    luma > 160.0
}
