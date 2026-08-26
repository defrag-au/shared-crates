//! `CommandPalette` — a modal, keyboard-first action launcher over a caller-supplied command set.
//!
//! The ⌘K/Ctrl-K pattern: press the chord (or click an affordance), a centred
//! overlay opens with an autofocused fuzzy search over everything the app can
//! do right now, enter dispatches, escape dismisses. The widget owns only the
//! overlay chrome and the open/close keybinding; search rendering delegates to
//! [`TypeaheadSearch`] so the palette looks like every other picker, and
//! ranking stays with the caller (or [`crate::filter_options`]) so the same
//! palette serves static command lists and context-dependent ones.
//!
//! The caller owns [`PaletteState`] (open flag + query + highlight) and the
//! command list for THIS frame — rebuild it per frame so items can appear and
//! disappear with app context (e.g. "add action to selected binding" only
//! when a binding is selected). Commands are [`TypeaheadOption`]s: the `id`
//! is dispatched back verbatim via [`PaletteAction::Invoke`].

use egui::{Align2, Key, KeyboardShortcut, Modifiers, Ui, Vec2};

use crate::machine::Machine;
use crate::theme;
use crate::typeahead_search::{filter_options, TypeaheadOption, TypeaheadSearch};

enum PalettePhase {
    Closed,
    Open,
}

/// Caller-owned palette state, persisted across frames. Open/closed rides a
/// [`Machine`], whose entry framing suppresses dismissal on the frame the
/// palette opened — the click that OPENED it is still in that frame's input
/// and lands outside the not-yet-rendered window, which otherwise read as an
/// instant self-dismiss ("the palette never appeared").
pub struct PaletteState {
    phase: Machine<PalettePhase>,
    pub query: String,
    pub highlight: usize,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self {
            phase: Machine::new(PalettePhase::Closed),
            query: String::new(),
            highlight: 0,
        }
    }
}

impl PaletteState {
    pub fn open(&mut self) {
        self.phase.transition(PalettePhase::Open);
        self.query.clear();
        self.highlight = 0;
    }

    pub fn close(&mut self) {
        self.phase.transition(PalettePhase::Closed);
    }

    pub fn is_open(&self) -> bool {
        matches!(self.phase.get(), PalettePhase::Open)
    }
}

/// What the palette did this frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PaletteAction {
    None,
    /// A command was chosen — its `id`. The palette has already closed.
    Invoke(String),
}

/// The command palette overlay.
pub struct CommandPalette<'a> {
    id_salt: &'a str,
    commands: &'a [TypeaheadOption],
    placeholder: &'a str,
    /// Max results shown for a query (and for the empty-query browse list).
    limit: usize,
    /// Listen for ⌘K / Ctrl-K to open. On by default; disable when an app
    /// binds the chord itself.
    keybinding: bool,
}

impl<'a> CommandPalette<'a> {
    pub fn new(id_salt: &'a str, commands: &'a [TypeaheadOption]) -> Self {
        Self {
            id_salt,
            commands,
            placeholder: "Type a command…",
            limit: 12,
            keybinding: true,
        }
    }

    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = text;
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    pub fn keybinding(mut self, enabled: bool) -> Self {
        self.keybinding = enabled;
        self
    }

    /// Handle the open/close keys and render the overlay when open. Call once
    /// per frame from the app root (not inside a panel whose focus matters —
    /// the overlay floats above everything via an egui `Window`).
    pub fn show(self, ui: &mut Ui, state: &mut PaletteState) -> PaletteAction {
        let ctx = ui.ctx().clone();

        if self.keybinding {
            let chord = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
            if ctx.input_mut(|i| i.consume_shortcut(&chord)) {
                if state.is_open() {
                    state.close();
                } else {
                    state.open();
                }
            }
        }
        if !state.is_open() {
            state.phase.tick();
            return PaletteAction::None;
        }
        // Entry frame = the frame `open()` was called (show ticks at exit,
        // so this holds however early or late in the frame the open happened).
        let just_opened = state.phase.entered();
        if !just_opened && ctx.input(|i| i.key_pressed(Key::Escape)) {
            state.close();
            state.phase.tick();
            return PaletteAction::None;
        }

        // Empty query browses the full list (up to the limit) — discovery is
        // half of what a palette is for.
        let shown: Vec<TypeaheadOption> = if state.query.trim().is_empty() {
            self.commands.iter().take(self.limit).cloned().collect()
        } else {
            filter_options(self.commands, &state.query, self.limit)
                .into_iter()
                .cloned()
                .collect()
        };

        let mut action = PaletteAction::None;
        let mut dismissed = false;

        egui::Window::new("command_palette")
            .id(egui::Id::new((self.id_salt, "palette_window")))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 80.0))
            .fixed_size(Vec2::new(460.0, 0.0))
            .frame(
                egui::Frame::window(&ctx.global_style())
                    .fill(theme::BG_SECONDARY)
                    .stroke(egui::Stroke::new(1.0_f32, theme::ACCENT)),
            )
            .show(&ctx, |ui| {
                let resp = TypeaheadSearch::new(
                    self.id_salt,
                    &mut state.query,
                    &shown,
                    &mut state.highlight,
                )
                .placeholder(self.placeholder)
                .empty_text("Nothing matches")
                .max_visible_rows(self.limit)
                .autofocus(true)
                .show(ui);

                if let Some(id) = resp.chosen {
                    action = PaletteAction::Invoke(id);
                }
                if resp.query_changed {
                    state.highlight = 0;
                }

                // Click-away dismissal: the window consumes its own clicks, so
                // a primary click anywhere else while open closes the palette.
                // Suppressed on the opening frame — see `just_opened`.
                if !just_opened
                    && ui.input(|i| i.pointer.any_click())
                    && !ui.rect_contains_pointer(ui.min_rect().expand(8.0))
                {
                    dismissed = true;
                }
            });

        if let PaletteAction::Invoke(_) = &action {
            state.close();
        } else if dismissed {
            state.close();
        }
        state.phase.tick();
        action
    }
}
