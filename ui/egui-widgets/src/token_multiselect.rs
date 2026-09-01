//! token_multiselect — pick a subset from a known set of options.
//!
//! A thin adapter over [`MultiSelect`](crate::select::MultiSelect) for the
//! common case where options are plain strings and their id IS their label:
//! group members, exclusive-set slots, a trait's required slots.
//!
//! ## Why it is a wrapper now
//!
//! It used to paint its own chips and hang the unselected options off a
//! `menu_button`. That worked, but it meant this crate had two answers to
//! "pick things from a list" that looked and behaved differently — one with a
//! typeahead and a bordered control, one with a menu you had to hit exactly.
//! The chips-in-a-box control is the shared one; this stays because collapsing
//! `&[String]` to `&[SelectOption]` at every call site is noise the callers
//! should not carry.
//!
//! The host still owns the selection and applies the reported add / remove /
//! clear.

use egui::Ui;

use crate::select::{MultiSelect, SelectOption};

#[derive(Default, Debug, Clone)]
pub struct TokenMultiselectResponse {
    /// An option picked this frame.
    pub added: Option<String>,
    /// Index into `selected` whose chip was removed this frame.
    pub removed: Option<usize>,
    /// The "clear" affordance was clicked this frame.
    pub cleared: bool,
}

pub struct TokenMultiselect<'a> {
    id_salt: egui::Id,
    selected: &'a [String],
    options: &'a [String],
    placeholder: &'a str,
    empty_text: &'a str,
    clearable: bool,
    width: f32,
}

impl<'a> TokenMultiselect<'a> {
    /// `id_salt` is `impl Hash` — pass the loop index when this appears in a
    /// list, or two instances share one open menu.
    pub fn new(
        id_salt: impl std::hash::Hash,
        selected: &'a [String],
        options: &'a [String],
    ) -> Self {
        Self {
            id_salt: egui::Id::new(id_salt),
            selected,
            options,
            placeholder: "Add…",
            empty_text: "Nothing left to add",
            clearable: false,
            width: 320.0,
        }
    }

    /// Placeholder shown while nothing is selected (default `"Add…"`).
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Copy for the menu when every option is already taken.
    pub fn empty_text(mut self, empty_text: &'a str) -> Self {
        self.empty_text = empty_text;
        self
    }

    /// Offer a "clear all" affordance under the control.
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show(self, ui: &mut Ui) -> TokenMultiselectResponse {
        // id == label: these options have no separate identity, which is the
        // whole reason this adapter exists.
        let options: Vec<SelectOption> = self
            .options
            .iter()
            .map(|o| SelectOption::new(o.clone(), o.clone()))
            .collect();

        let resp = MultiSelect::new(self.id_salt, self.selected, &options)
            .placeholder(self.placeholder)
            .empty_text(self.empty_text)
            .clearable(self.clearable)
            .width(self.width)
            .show(ui);

        TokenMultiselectResponse {
            added: resp.added,
            removed: resp.removed,
            cleared: resp.cleared,
        }
    }
}
