//! `Select` — a single-select control with the anatomy of a real select.
//!
//! Modelled on [react-select](https://react-select.com), because that is the
//! bar people actually have in mind when they say "a select". The thing that
//! makes those feel solid is not the dropdown — it is that the **control is
//! one box**:
//!
//! ```text
//! ┌──────────────────────────────────┐
//! │ ● @crew                 ×  │  ⌄  │
//! └──────────────────────────────────┘
//! ```
//!
//! One bordered container, a fixed height, the value on the left, and the
//! affordances gathered on the right behind a hairline separator. Emitting
//! those as three sibling egui buttons — which is what this crate did before —
//! gives a row where the clear `×` is as visually heavy as the value it
//! discards, nothing says "this opens", and two rows of different name lengths
//! put their controls in different places.
//!
//! ## What it does that a `ComboBox` does not
//!
//! - **Type to filter, in place.** Opening turns the value area into a
//!   frameless input; the closed value is the placeholder behind it.
//! - **The menu floats.** An `Area` above the content, so opening a select at
//!   the bottom of a form does not shove the form around.
//! - **Rows carry more than a string** — a colour swatch, a subtitle.
//! - **A value that is not in the options still renders.** Config outlives the
//!   things it references (a Discord role gets deleted while an id sits in a
//!   rule), and blanking the field would hide a rule that still exists.
//!   [`SelectValue::warning`] renders it flagged instead.
//!
//! ## Not multi-select
//!
//! One value. Multi-select is [`crate::token_multiselect`], whose chips-in-a-
//! box shape is a different control with different affordances — react-select
//! ships both for the same reason.

use egui::{Align, Color32, Layout, Margin, Sense, Ui, vec2};

use crate::icons::install_phosphor_font;
use crate::{PhosphorIcon, theme};

/// One row in the menu.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
    /// Leading dot — a Discord role colour, a status, a series tint.
    pub swatch: Option<Color32>,
    /// Second line, muted. Kept short; this is a menu, not a table.
    pub subtitle: Option<String>,
}

impl SelectOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            swatch: None,
            subtitle: None,
        }
    }

    pub fn swatch(mut self, color: Option<Color32>) -> Self {
        self.swatch = color;
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

/// What the control shows when closed.
///
/// Separate from [`SelectOption`] so a caller can display a value that is not
/// in the option list — see the module docs on why blanking it is wrong.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectValue {
    pub label: String,
    pub swatch: Option<Color32>,
    /// Present when the value is suspect: renders warning-tinted with this
    /// text as the tooltip. `None` for an ordinary resolved value.
    pub warning: Option<String>,
}

impl SelectValue {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }

    pub fn swatch(mut self, color: Option<Color32>) -> Self {
        self.swatch = color;
        self
    }

    pub fn warning(mut self, why: impl Into<String>) -> Self {
        self.warning = Some(why.into());
        self
    }
}

impl From<&SelectOption> for SelectValue {
    fn from(option: &SelectOption) -> Self {
        Self {
            label: option.label.clone(),
            swatch: option.swatch,
            warning: None,
        }
    }
}

/// Caller-owned, persists across frames.
#[derive(Debug, Clone, Default)]
pub struct SelectState {
    pub open: bool,
    /// The filter text while open. Cleared on open so the menu starts whole.
    pub query: String,
    /// Keyboard cursor into the FILTERED list.
    pub highlight: usize,
}

impl SelectState {
    fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.highlight = 0;
    }

    fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.highlight = 0;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectResponse {
    /// An option id was chosen this frame.
    pub chosen: Option<String>,
    /// The clear `×` was pressed. The caller decides what empty means —
    /// blanking a field and deleting its row are different things.
    pub cleared: bool,
}

/// Control height. React-select's is 38px; this sits a little tighter to match
/// the density of the rest of the crate, while still being a real target.
const CONTROL_HEIGHT: f32 = 30.0;
/// Room for `× │ ⌄` — reserved up front so the value column is the same width
/// on every row regardless of how long the value is.
const INDICATORS_WIDTH: f32 = 48.0;
const MENU_ROW_HEIGHT: f32 = 28.0;
const MENU_MAX_HEIGHT: f32 = 240.0;

pub struct Select<'a> {
    id_salt: &'a str,
    state: &'a mut SelectState,
    options: &'a [SelectOption],
    value: Option<SelectValue>,
    placeholder: &'a str,
    empty_text: &'a str,
    clearable: bool,
    width: f32,
}

impl<'a> Select<'a> {
    pub fn new(
        id_salt: &'a str,
        state: &'a mut SelectState,
        options: &'a [SelectOption],
    ) -> Self {
        Self {
            id_salt,
            state,
            options,
            value: None,
            placeholder: "Select…",
            empty_text: "No matches",
            clearable: true,
            width: 220.0,
        }
    }

    /// The current value. `None` renders the placeholder.
    pub fn value(mut self, value: Option<SelectValue>) -> Self {
        self.value = value;
        self
    }

    /// Resolve `id` against the options, or render it flagged when it matches
    /// nothing — the honest handling of a value whose referent is gone.
    pub fn value_from_id(mut self, id: &str, unresolved_hint: &str) -> Self {
        self.value = if id.trim().is_empty() {
            None
        } else {
            Some(match self.options.iter().find(|o| o.id == id) {
                Some(option) => SelectValue::from(option),
                None => SelectValue::new(id).warning(unresolved_hint),
            })
        };
        self
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn empty_text(mut self, empty_text: &'a str) -> Self {
        self.empty_text = empty_text;
        self
    }

    /// Whether the `×` appears when a value is set.
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show(self, ui: &mut Ui) -> SelectResponse {
        install_phosphor_font(ui.ctx());
        let mut out = SelectResponse::default();
        // The control's parts are affordances, not copyable text — selectable
        // labels put an I-beam over the click target and fight it.
        ui.style_mut().interaction.selectable_labels = false;

        let widget_id = ui.make_persistent_id(self.id_salt);
        let edit_id = widget_id.with("edit");

        // Options matching the current filter. Filtering lives here rather
        // than in the caller because a select that does not narrow as you type
        // is the thing this widget exists to stop being.
        let filtered: Vec<&SelectOption> = match self.state.query.trim() {
            "" => self.options.iter().collect(),
            query => {
                let q = query.to_lowercase();
                self.options
                    .iter()
                    .filter(|o| {
                        o.label.to_lowercase().contains(&q)
                            || o.subtitle
                                .as_deref()
                                .is_some_and(|s| s.to_lowercase().contains(&q))
                    })
                    .collect()
            }
        };
        if self.state.highlight >= filtered.len() {
            self.state.highlight = filtered.len().saturating_sub(1);
        }

        // ── Control ───────────────────────────────────────────────────────
        let open = self.state.open;
        let mut clicked_control = false;
        let mut edit_response = None;

        let frame = egui::Frame::new()
            .fill(theme::BG_SECONDARY)
            .corner_radius(6.0)
            // The border IS the state indicator, as a focus ring is on the web.
            .stroke(theme::hairline(if open { theme::ACCENT } else { theme::BORDER }))
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(self.width);
                ui.horizontal(|ui| {
                    ui.set_min_height(CONTROL_HEIGHT - 8.0);
                    let value_width = (ui.available_width() - INDICATORS_WIDTH).max(40.0);

                    // Value column — fixed width, so two rows with different
                    // value lengths still put their indicators in one column.
                    ui.allocate_ui_with_layout(
                        vec2(value_width, CONTROL_HEIGHT - 8.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            if open {
                                let edit = egui::TextEdit::singleline(&mut self.state.query)
                                    .id(edit_id)
                                    // An empty `Frame` draws nothing — this
                                    // fork's `frame()` takes a Frame, not a
                                    // bool. The control's border is the frame.
                                    .frame(egui::Frame::default())
                                    .desired_width(f32::INFINITY)
                                    .hint_text(
                                        self.value
                                            .as_ref()
                                            .map(|v| v.label.clone())
                                            .unwrap_or_else(|| self.placeholder.to_string()),
                                    )
                                    .text_color(theme::TEXT_PRIMARY);
                                edit_response = Some(ui.add(edit));
                            } else {
                                match &self.value {
                                    Some(value) => {
                                        if let Some(color) = value.swatch {
                                            ui.label(
                                                egui::RichText::new("●").color(color).size(11.0),
                                            );
                                        }
                                        if value.warning.is_some() {
                                            ui.label(PhosphorIcon::Warning.rich_text(
                                                12.0,
                                                theme::ACCENT_YELLOW,
                                            ));
                                        }
                                        let text = egui::RichText::new(&value.label).color(
                                            match value.warning {
                                                Some(_) => theme::ACCENT_YELLOW,
                                                None => theme::TEXT_PRIMARY,
                                            },
                                        );
                                        let label = ui.label(text);
                                        if let Some(why) = &value.warning {
                                            label.on_hover_text(why);
                                        }
                                    }
                                    // Muted, like every placeholder — the one
                                    // reliable signal that a field is empty
                                    // rather than holding a short value.
                                    None => {
                                        ui.colored_label(theme::TEXT_MUTED, self.placeholder);
                                    }
                                }
                            }
                        },
                    );

                    // Indicators, packed from the right.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(PhosphorIcon::CaretDown.rich_text(12.0, theme::TEXT_MUTED));
                        // The hairline that makes the chevron read as part of
                        // the control rather than a button parked next to it.
                        let (sep, _) = ui.allocate_exact_size(
                            vec2(9.0, CONTROL_HEIGHT - 14.0),
                            Sense::hover(),
                        );
                        ui.painter().line_segment(
                            [sep.center_top(), sep.center_bottom()],
                            theme::hairline(theme::BORDER),
                        );
                        if self.clearable && self.value.is_some() {
                            let clear = ui
                                .add(
                                    egui::Label::new(
                                        PhosphorIcon::X.rich_text(11.0, theme::TEXT_MUTED),
                                    )
                                    .sense(Sense::click()),
                                )
                                .on_hover_text("clear");
                            if clear.clicked() {
                                out.cleared = true;
                            }
                        }
                    });
                });
            });

        // The whole box opens it — a select you must hit the chevron of is a
        // select that feels broken. Interact AFTER the contents so the clear
        // `×` and the text field win their own clicks.
        let control_rect = frame.response.rect;
        let control = ui.interact(control_rect, widget_id, Sense::click());
        if control.clicked() && !out.cleared {
            clicked_control = true;
        }
        if control.hovered() && !open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        if clicked_control {
            if open {
                self.state.close();
            } else {
                self.state.open();
                ui.memory_mut(|m| m.request_focus(edit_id));
            }
        }

        // ── Keyboard ──────────────────────────────────────────────────────
        // `lost_focus()` too: a single-line TextEdit surrenders focus ON Enter,
        // which is the very keystroke that chooses a row.
        if self.state.open {
            let focused = edit_response
                .as_ref()
                .is_some_and(|r| r.has_focus() || r.lost_focus());
            if focused {
                let (down, up, enter, esc) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                    )
                });
                if down && !filtered.is_empty() {
                    self.state.highlight = (self.state.highlight + 1).min(filtered.len() - 1);
                }
                if up {
                    self.state.highlight = self.state.highlight.saturating_sub(1);
                }
                if enter {
                    if let Some(option) = filtered.get(self.state.highlight) {
                        out.chosen = Some(option.id.clone());
                    }
                    self.state.close();
                }
                if esc {
                    self.state.close();
                }
            }
        }

        // ── Menu ──────────────────────────────────────────────────────────
        if self.state.open {
            let menu_id = widget_id.with("menu");
            let menu = egui::Area::new(menu_id)
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(control_rect.min.x, control_rect.max.y + 4.0))
                .show(ui.ctx(), |ui| {
                    ui.style_mut().interaction.selectable_labels = false;
                    egui::Frame::new()
                        .fill(theme::BG_SECONDARY)
                        .corner_radius(6.0)
                        .stroke(theme::hairline(theme::BORDER))
                        .inner_margin(Margin::same(4))
                        .show(ui, |ui| {
                            ui.set_width(control_rect.width());
                            if filtered.is_empty() {
                                ui.add_space(4.0);
                                ui.colored_label(theme::TEXT_MUTED, self.empty_text);
                                ui.add_space(4.0);
                                return None;
                            }
                            let mut picked = None;
                            egui::ScrollArea::vertical()
                                .max_height(MENU_MAX_HEIGHT)
                                .show(ui, |ui| {
                                    for (index, option) in filtered.iter().enumerate() {
                                        if menu_row(
                                            ui,
                                            option,
                                            index == self.state.highlight,
                                        ) {
                                            picked = Some(option.id.clone());
                                        }
                                    }
                                });
                            picked
                        })
                        .inner
                });

            if let Some(id) = menu.inner {
                out.chosen = Some(id);
                self.state.close();
            } else if ui.input(|i| i.pointer.any_click())
                && !control.clicked()
                && !menu.response.hovered()
            {
                // Click-outside closes, the one behaviour every select has and
                // whose absence makes a menu feel stuck.
                self.state.close();
            }
        }

        out
    }
}

/// One menu row. Returns true when chosen.
fn menu_row(ui: &mut Ui, option: &SelectOption, highlighted: bool) -> bool {
    let height = match option.subtitle {
        Some(_) => MENU_ROW_HEIGHT + 12.0,
        None => MENU_ROW_HEIGHT,
    };
    // `Sense::CLICK` — the non-focusable const, so arrowing through the menu
    // does not fight egui's focus manager for the text field.
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::CLICK);

    let hovered = response.hovered();
    if hovered || highlighted {
        ui.painter()
            .rect_filled(rect, 4.0, theme::BG_HIGHLIGHT);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let mut cursor = rect.min.x + 8.0;
    if let Some(color) = option.swatch {
        ui.painter().circle_filled(
            egui::pos2(cursor + 4.0, rect.center().y),
            4.0,
            color,
        );
        cursor += 16.0;
    }

    let text_top = match option.subtitle {
        Some(_) => rect.min.y + 6.0,
        None => rect.center().y - 7.0,
    };
    ui.painter().text(
        egui::pos2(cursor, text_top),
        egui::Align2::LEFT_TOP,
        &option.label,
        egui::FontId::proportional(13.0),
        theme::TEXT_PRIMARY,
    );
    if let Some(subtitle) = &option.subtitle {
        ui.painter().text(
            egui::pos2(cursor, rect.min.y + 22.0),
            egui::Align2::LEFT_TOP,
            subtitle,
            egui::FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
    }

    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<SelectOption> {
        vec![
            SelectOption::new("1", "crew"),
            SelectOption::new("2", "deckhand"),
        ]
    }

    /// A value whose option is gone still renders, flagged — config outlives
    /// the things it points at, and blanking the field would hide a rule that
    /// still exists and still matches nobody.
    #[test]
    fn an_unresolved_value_renders_flagged_rather_than_empty() {
        let mut state = SelectState::default();
        let opts = options();
        let select = Select::new("s", &mut state, &opts).value_from_id("999", "role is gone");
        let value = select.value.expect("a set id must render something");
        assert_eq!(value.label, "999", "the raw id, so the rule is still legible");
        assert_eq!(value.warning.as_deref(), Some("role is gone"));
    }

    #[test]
    fn a_resolved_value_renders_as_its_label() {
        let mut state = SelectState::default();
        let opts = options();
        let select = Select::new("s", &mut state, &opts).value_from_id("2", "gone");
        let value = select.value.expect("set");
        assert_eq!(value.label, "deckhand");
        assert!(value.warning.is_none());
    }

    /// Empty is empty — not a warning about the empty string.
    #[test]
    fn an_empty_id_is_a_placeholder_not_a_warning() {
        let mut state = SelectState::default();
        let opts = options();
        assert!(
            Select::new("s", &mut state, &opts)
                .value_from_id("  ", "gone")
                .value
                .is_none()
        );
    }

    /// Opening clears the filter so the menu starts whole; closing forgets it
    /// so reopening is not haunted by the last search.
    #[test]
    fn opening_and_closing_reset_the_filter() {
        let mut state = SelectState {
            open: false,
            query: "stale".into(),
            highlight: 4,
        };
        state.open();
        assert!(state.open);
        assert_eq!(state.query, "");
        assert_eq!(state.highlight, 0);

        state.query = "typed".into();
        state.close();
        assert!(!state.open);
        assert_eq!(state.query, "");
    }
}
