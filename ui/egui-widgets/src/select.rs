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

/// Transient UI state — is the menu open, what has been typed, where the
/// keyboard cursor is.
///
/// **Held in egui temp memory keyed by the salt, not by the caller.** This is
/// the same call `ComboBox` makes, and it is what lets a select appear inside
/// a loop — a row of a group list, a tier in a table — without the host
/// carrying a `HashMap` of picker state whose only purpose is to be threaded
/// back in. None of it is model state: losing it on a reload costs an open
/// menu and a half-typed filter, which is the correct amount to lose.
///
/// Public because the transitions are worth testing on their own.
#[derive(Debug, Clone, Default)]
pub struct SelectState {
    pub open: bool,
    /// The filter text while open. Cleared on open so the menu starts whole.
    pub query: String,
    /// Keyboard cursor into the FILTERED list.
    pub highlight: usize,
}

impl SelectState {
    pub(crate) fn load(ui: &Ui, id: egui::Id) -> Self {
        ui.data(|d| d.get_temp::<Self>(id)).unwrap_or_default()
    }

    pub(crate) fn store(self, ui: &Ui, id: egui::Id) {
        ui.data_mut(|d| d.insert_temp(id, self));
    }

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
    id_salt: egui::Id,
    options: &'a [SelectOption],
    value: Option<SelectValue>,
    placeholder: &'a str,
    empty_text: &'a str,
    clearable: bool,
    width: f32,
}

impl<'a> Select<'a> {
    /// `id_salt` is `impl Hash`, egui's own convention — so a row in a list
    /// passes `("tier", index)` rather than a constant.
    ///
    /// It matters more here than for most widgets: this one owns a persistent
    /// open/query/highlight state AND a floating `Area`. Two selects sharing a
    /// salt collide on both, and egui paints its "first/second use of widget
    /// ID" banner across the layout. Taking `impl Hash` instead of `&str`
    /// makes the unique-per-instance case free rather than a `format!`.
    pub fn new(id_salt: impl std::hash::Hash, options: &'a [SelectOption]) -> Self {
        Self {
            id_salt: egui::Id::new(id_salt),
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

        // `Id::new(salt)` directly, NOT `ui.make_persistent_id` — the latter
        // mixes in the parent `Ui`'s id, and sibling cells of a `Grid` share
        // one, so two rows with different salts could still collide.
        let widget_id = self.id_salt;
        let edit_id = widget_id.with("edit");
        // Loaded here, stored at the end — the caller never sees it.
        let mut state = SelectState::load(ui, widget_id);

        // Options matching the current filter. Filtering lives here rather
        // than in the caller because a select that does not narrow as you type
        // is the thing this widget exists to stop being.
        let filtered: Vec<&SelectOption> = match state.query.trim() {
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
        if state.highlight >= filtered.len() {
            state.highlight = filtered.len().saturating_sub(1);
        }

        // ── Control ───────────────────────────────────────────────────────
        let open = state.open;
        // Whether the pointer is over the clear `×` — see the routing below.
        let mut hovered_clear = false;
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
                                let edit = egui::TextEdit::singleline(&mut state.query)
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
                                        // `truncate`, not wrap: a long value
                                        // wrapping to a second line would push
                                        // the control off its fixed height and
                                        // make a row of selects ragged.
                                        let label =
                                            ui.add(egui::Label::new(text).truncate());
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
                            // Hover, not click — the control's `interact`
                            // below covers this and is registered after it,
                            // so in egui's top-most-wins ordering it takes
                            // the click. Same routing as MultiSelect's chips.
                            let clear = ui
                                .add(egui::Label::new(PhosphorIcon::X.rich_text(
                                    11.0,
                                    theme::TEXT_MUTED,
                                )))
                                .on_hover_text("clear");
                            hovered_clear = clear.hovered();
                            if hovered_clear {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                        }
                    });
                });
            });

        // The whole box opens it — a select you must hit the chevron of is a
        // select that feels broken. So the control takes every click inside
        // itself and routes it: over the clear `×` it clears, anywhere else
        // it toggles the menu.
        let control_rect = frame.response.rect;
        let control = ui.interact(control_rect, widget_id, Sense::click());
        if control.hovered() && !open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        if control.clicked() {
            if hovered_clear {
                out.cleared = true;
            } else if open {
                state.close();
            } else {
                state.open();
                ui.memory_mut(|m| m.request_focus(edit_id));
            }
        }

        // ── Keyboard ──────────────────────────────────────────────────────
        // `lost_focus()` too: a single-line TextEdit surrenders focus ON Enter,
        // which is the very keystroke that chooses a row.
        if state.open {
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
                    state.highlight = (state.highlight + 1).min(filtered.len() - 1);
                }
                if up {
                    state.highlight = state.highlight.saturating_sub(1);
                }
                if enter {
                    if let Some(option) = filtered.get(state.highlight) {
                        out.chosen = Some(option.id.clone());
                    }
                    state.close();
                }
                if esc {
                    state.close();
                }
            }
        }

        // ── Menu ──────────────────────────────────────────────────────────
        if state.open {
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
                            let chosen_label =
                                self.value.as_ref().map(|v| v.label.as_str());
                            egui::ScrollArea::vertical()
                                .max_height(MENU_MAX_HEIGHT)
                                .show(ui, |ui| {
                                    for (index, option) in filtered.iter().enumerate() {
                                        let selected =
                                            chosen_label == Some(option.label.as_str());
                                        if menu_row(
                                            ui,
                                            option,
                                            index == state.highlight,
                                            selected,
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
                state.close();
            } else if ui.input(|i| i.pointer.any_click())
                && !control.clicked()
                && !menu.response.hovered()
            {
                // Click-outside closes, the one behaviour every select has and
                // whose absence makes a menu feel stuck.
                state.close();
            }
        }

        state.store(ui, widget_id);
        out
    }
}

// ============================================================================
// MultiSelect
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct MultiSelectResponse {
    /// An option id was added this frame.
    pub added: Option<String>,
    /// Index into `selected` whose chip was removed this frame.
    pub removed: Option<usize>,
    /// The clear-all `×` was pressed.
    pub cleared: bool,
}

/// Many values, as removable chips inside one control.
///
/// react-select's multi mode, and the same box as [`Select`] — chips where the
/// single value would be, the filter input trailing them, indicators on the
/// right. The distinction that matters: **the menu only offers what is not
/// already chosen**, so the option list shrinks as you pick, and picking
/// something twice is not a state that exists.
///
/// The host owns the selection and applies the reported add/remove; this
/// widget owns only the open/filter state, in temp memory like [`Select`].
pub struct MultiSelect<'a> {
    id_salt: egui::Id,
    /// Ids currently selected, in the host's order.
    selected: &'a [String],
    options: &'a [SelectOption],
    placeholder: &'a str,
    empty_text: &'a str,
    clearable: bool,
    creatable: bool,
    width: f32,
}

impl<'a> MultiSelect<'a> {
    pub fn new(
        id_salt: impl std::hash::Hash,
        selected: &'a [String],
        options: &'a [SelectOption],
    ) -> Self {
        Self {
            id_salt: egui::Id::new(id_salt),
            selected,
            options,
            placeholder: "Select…",
            empty_text: "Nothing left to add",
            clearable: true,
            creatable: false,
            width: 320.0,
        }
    }

    /// Allow values that are not in `options` — react-select's "creatable".
    ///
    /// For fields where the option list is a *record of what has been used*
    /// rather than a closed vocabulary: tags, labels, categories. Typing
    /// something new offers a "Create …" row at the top of the menu, and the
    /// created value comes back through `added` like any other.
    ///
    /// Off by default: for a closed set — roles, providers, group members —
    /// inventing a value produces config that references nothing.
    pub fn creatable(mut self, creatable: bool) -> Self {
        self.creatable = creatable;
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

    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show(self, ui: &mut Ui) -> MultiSelectResponse {
        install_phosphor_font(ui.ctx());
        let mut out = MultiSelectResponse::default();
        ui.style_mut().interaction.selectable_labels = false;

        let widget_id = self.id_salt;
        let edit_id = widget_id.with("edit");
        let mut state = SelectState::load(ui, widget_id);

        // Only what is NOT already selected, then narrowed by the filter.
        let available: Vec<&SelectOption> = self
            .options
            .iter()
            .filter(|o| !self.selected.contains(&o.id))
            .filter(|o| match state.query.trim() {
                "" => true,
                q => o.label.to_lowercase().contains(&q.to_lowercase()),
            })
            .collect();
        // The "Create …" row, when the typed value is genuinely new. Offered
        // only for a value that matches no option and nothing already picked
        // — otherwise it invites making a duplicate of something one row down.
        let typed = state.query.trim().to_string();
        let create: Option<String> = (self.creatable
            && !typed.is_empty()
            && !self
                .options
                .iter()
                .any(|o| o.label.eq_ignore_ascii_case(&typed))
            && !self.selected.iter().any(|s| s.eq_ignore_ascii_case(&typed)))
        .then_some(typed);

        // The menu is [create?] ++ available, and the keyboard cursor indexes
        // that whole list — so Enter on a fresh value creates it.
        let create_rows = usize::from(create.is_some());
        let row_count = available.len() + create_rows;
        if state.highlight >= row_count {
            state.highlight = row_count.saturating_sub(1);
        }

        let open = state.open;
        let mut edit_response = None;
        // Which chip's `×` the pointer is over, if any — the control's click
        // is routed here rather than to opening the menu.
        let mut hovered_remove: Option<usize> = None;

        let frame = egui::Frame::new()
            .fill(theme::BG_SECONDARY)
            .corner_radius(6.0)
            .stroke(theme::hairline(if open {
                theme::ACCENT
            } else {
                theme::BORDER
            }))
            .inner_margin(Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.set_width(self.width);
                // `horizontal_wrapped`, not `horizontal`: a control holding
                // eight chips has to grow downwards rather than off the edge.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    ui.set_min_height(CONTROL_HEIGHT - 8.0);

                    for (index, id) in self.selected.iter().enumerate() {
                        // A chip shows the option's LABEL where one resolves,
                        // and the raw id where it does not — the same honesty
                        // as `Select`'s unresolved value, chip-sized.
                        let option = self.options.iter().find(|o| o.id == *id);
                        let label = option.map(|o| o.label.as_str()).unwrap_or(id.as_str());
                        let known = option.is_some();
                        if chip(ui, label, option.and_then(|o| o.swatch), known) {
                            hovered_remove = Some(index);
                        }
                    }

                    if open {
                        let edit = egui::TextEdit::singleline(&mut state.query)
                            .id(edit_id)
                            .frame(egui::Frame::default())
                            .desired_width(90.0)
                            .hint_text(if self.selected.is_empty() {
                                self.placeholder
                            } else {
                                ""
                            })
                            .text_color(theme::TEXT_PRIMARY);
                        edit_response = Some(ui.add(edit));
                    } else if self.selected.is_empty() {
                        ui.colored_label(theme::TEXT_MUTED, self.placeholder);
                    }
                });
            });

        let control_rect = frame.response.rect;
        let control = ui.interact(control_rect, widget_id, Sense::click());
        if control.hovered() && !open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if control.clicked() {
            // A click over a chip's `×` removes it and does NOT also toggle
            // the menu — otherwise deleting three chips is a fight with a
            // dropdown that reopens under the cursor each time.
            if let Some(index) = hovered_remove {
                out.removed = Some(index);
            } else if open {
                state.close();
            } else {
                state.open();
                ui.memory_mut(|m| m.request_focus(edit_id));
            }
        }

        if state.open {
            let focused = edit_response
                .as_ref()
                .is_some_and(|r| r.has_focus() || r.lost_focus());
            if focused {
                let (down, up, enter, esc, backspace) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
                    )
                });
                if down && row_count > 0 {
                    state.highlight = (state.highlight + 1).min(row_count - 1);
                }
                if up {
                    state.highlight = state.highlight.saturating_sub(1);
                }
                if enter {
                    let picked = match (&create, state.highlight) {
                        // Row 0 is the create row when there is one.
                        (Some(new_value), 0) => Some(new_value.clone()),
                        _ => available
                            .get(state.highlight - create_rows)
                            .map(|o| o.id.clone()),
                    };
                    if let Some(id) = picked {
                        out.added = Some(id);
                        // Stay open and clear the filter: adding members is a
                        // run of picks, not one. Closing after each would make
                        // choosing five options five round trips.
                        state.query.clear();
                        state.highlight = 0;
                    }
                }
                if esc {
                    state.close();
                }
                // Backspace on an empty filter removes the last chip — the
                // behaviour every tag input has, and the reason people type
                // rather than reach for the mouse.
                if backspace && state.query.is_empty() && !self.selected.is_empty() {
                    out.removed = Some(self.selected.len() - 1);
                }
            }

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
                            if available.is_empty() && create.is_none() {
                                ui.add_space(4.0);
                                ui.colored_label(theme::TEXT_MUTED, self.empty_text);
                                ui.add_space(4.0);
                                return None;
                            }
                            let mut picked = None;
                            egui::ScrollArea::vertical()
                                .max_height(MENU_MAX_HEIGHT)
                                .show(ui, |ui| {
                                    if let Some(new_value) = &create {
                                        let row = SelectOption::new(
                                            new_value.clone(),
                                            format!("Create “{new_value}”"),
                                        );
                                        if menu_row(ui, &row, state.highlight == 0, false) {
                                            picked = Some(new_value.clone());
                                        }
                                    }
                                    for (index, option) in available.iter().enumerate() {
                                        // Nothing in this menu is "selected" —
                                        // selected options are not in it.
                                        if menu_row(
                                            ui,
                                            option,
                                            index + create_rows == state.highlight,
                                            false,
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
                out.added = Some(id);
                state.query.clear();
                state.highlight = 0;
            } else if ui.input(|i| i.pointer.any_click())
                && !control.clicked()
                && !menu.response.hovered()
            {
                state.close();
            }
        }

        // Clear-all sits OUTSIDE the box, after it: inside, among a row of
        // per-chip `×`s, a control that discards all of them is a mis-click
        // waiting to happen.
        if self.clearable && !self.selected.is_empty() {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("clear all")
                                .small()
                                .color(theme::TEXT_MUTED),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    out.cleared = true;
                }
            });
        }

        state.store(ui, widget_id);
        out
    }
}

/// One removable chip inside a [`MultiSelect`].
///
/// Returns whether the pointer is over its `×` **right now** — not whether it
/// was clicked. The control's own `interact` covers the whole box and is
/// registered after these, so in egui's top-most-wins ordering it takes every
/// click inside it; a chip that tested its own `clicked()` never fired, and
/// pressing `×` merely reopened the menu. So hover decides *where* a click
/// goes and the control decides *that* it happened.
///
/// `known` is false for a selected id with no matching option, which renders
/// warning-tinted rather than vanishing.
fn chip(ui: &mut Ui, label: &str, swatch: Option<Color32>, known: bool) -> bool {
    let font = egui::FontId::proportional(12.0);
    let fg = if known {
        theme::TEXT_PRIMARY
    } else {
        theme::ACCENT_YELLOW
    };
    // Chips are elided too, at a generous cap: one pathological tag should
    // not make a chip wider than the control it sits in.
    let galley = elided_line(ui, label, font, fg, 180.0);
    let swatch_w = if swatch.is_some() { 14.0 } else { 0.0 };
    let size = vec2(galley.size().x + swatch_w + 30.0, 20.0);
    // `hover`, not `click` — see the doc comment: the control takes the click.
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    ui.painter().rect_filled(rect, 3.0, theme::BG_HIGHLIGHT);
    // A hairline so a chip reads as an object against the control's own fill;
    // without it the two greys blur into one another and the chips look like
    // text that happens to be shaded.
    ui.painter().rect_stroke(
        rect,
        3.0,
        theme::hairline(if known { theme::BORDER } else { theme::ACCENT_YELLOW }),
        egui::StrokeKind::Inside,
    );
    let mut cursor = rect.min.x + 6.0;
    if let Some(color) = swatch {
        ui.painter()
            .circle_filled(egui::pos2(cursor + 4.0, rect.center().y), 4.0, color);
        cursor += swatch_w;
    }
    ui.painter().galley(
        egui::pos2(cursor, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );

    // The `×` is its own hit target inside the chip, so clicking the label
    // does not remove it — a chip is a value, not a button.
    let x_rect =
        egui::Rect::from_center_size(egui::pos2(rect.right() - 11.0, rect.center().y), vec2(16.0, 16.0));
    let x_hovered = ui.rect_contains_pointer(x_rect);
    ui.painter().text(
        x_rect.center(),
        egui::Align2::CENTER_CENTER,
        PhosphorIcon::X.as_str(),
        egui::FontId::new(10.0, crate::icons::phosphor_family()),
        if x_hovered { theme::ERROR } else { theme::TEXT_MUTED },
    );
    if x_hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    x_hovered
}

/// Lay out one line, ellipsised at `max_width`.
///
/// `Painter::text` neither wraps nor clips — it just draws, so a subtitle
/// longer than the menu ran out past its own border and over whatever was
/// behind it. Menu rows are painted rather than laid out (they are fixed-
/// height hit targets), so the elision has to be asked for explicitly.
fn elided_line(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 1,
        overflow_character: Some('…'),
        ..Default::default()
    };
    ui.painter().layout_job(job)
}

/// One menu row. Returns true when chosen.
///
/// `highlighted` is the keyboard cursor; `selected` is the value already held.
/// They are different things and look different: the cursor is a background,
/// the selection is a tick. Conflating them means arrowing past the current
/// value makes it appear to change.
fn menu_row(ui: &mut Ui, option: &SelectOption, highlighted: bool, selected: bool) -> bool {
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

    // A tick reserves its column whether or not it is drawn, so labels do not
    // shift sideways as the selection moves.
    if selected {
        ui.painter().text(
            egui::pos2(rect.min.x + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            PhosphorIcon::Check.as_str(),
            egui::FontId::new(12.0, crate::icons::phosphor_family()),
            theme::ACCENT,
        );
    }
    let mut cursor = rect.min.x + 26.0;
    if let Some(color) = option.swatch {
        ui.painter().circle_filled(
            egui::pos2(cursor + 4.0, rect.center().y),
            4.0,
            color,
        );
        cursor += 16.0;
    }

    // What is left of the row after the tick column, any swatch, and a right
    // margin — everything painted below is elided to fit inside it.
    let text_width = (rect.right() - cursor - 8.0).max(24.0);
    let text_top = match option.subtitle {
        Some(_) => rect.min.y + 6.0,
        None => rect.center().y - 7.0,
    };
    let label = elided_line(
        ui,
        &option.label,
        egui::FontId::proportional(13.0),
        theme::TEXT_PRIMARY,
        text_width,
    );
    ui.painter()
        .galley(egui::pos2(cursor, text_top), label, theme::TEXT_PRIMARY);
    if let Some(subtitle) = &option.subtitle {
        let subtitle = elided_line(
            ui,
            subtitle,
            egui::FontId::proportional(11.0),
            theme::TEXT_MUTED,
            text_width,
        );
        ui.painter().galley(
            egui::pos2(cursor, rect.min.y + 22.0),
            subtitle,
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
        let opts = options();
        let select = Select::new("s", &opts).value_from_id("999", "role is gone");
        let value = select.value.expect("a set id must render something");
        assert_eq!(value.label, "999", "the raw id, so the rule is still legible");
        assert_eq!(value.warning.as_deref(), Some("role is gone"));
    }

    #[test]
    fn a_resolved_value_renders_as_its_label() {
        let opts = options();
        let select = Select::new("s", &opts).value_from_id("2", "gone");
        let value = select.value.expect("set");
        assert_eq!(value.label, "deckhand");
        assert!(value.warning.is_none());
    }

    /// Empty is empty — not a warning about the empty string.
    #[test]
    fn an_empty_id_is_a_placeholder_not_a_warning() {
        let opts = options();
        assert!(
            Select::new("s", &opts)
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
