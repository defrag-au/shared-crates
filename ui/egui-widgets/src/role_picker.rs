//! `RolePicker` — choose a Discord role by name instead of pasting a snowflake.
//!
//! Wraps [`crate::TypeaheadSearch`] with the two things a role field needs that
//! a general search box does not:
//!
//! - **A resolved role renders as itself**, not as a search box. The common
//!   state of this control is "already chosen", and a picker that stayed a text
//!   input would make the chosen value the least visible thing on the row.
//! - **An unresolvable id still renders.** Roles are guild state that changes
//!   without us: a role can be deleted while an id referencing it sits in
//!   config. Showing the raw id, flagged, is the only honest option — dropping
//!   it would hide a rule that still exists and still matches nobody, and
//!   silently rewriting it would discard the operator's intent.
//!
//! ```ignore
//! let resp = RolePicker::new("tier-0", &mut state, &roles, tier.role.as_str()).show(ui);
//! if let Some(id) = resp.chosen {
//!     tier.role = id;
//! }
//! ```

use egui::{Color32, Ui};

use crate::theme;
use crate::{PhosphorIcon, TypeaheadOption, TypeaheadSearch};

/// One role offered by the picker.
///
/// Deliberately not a Discord library type: this crate is shared with wasm
/// frontends that must not pull a gateway model in, and the caller already has
/// to cross a wire to get these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleOption {
    pub id: String,
    pub name: String,
    /// Discord's role colour as `0xRRGGBB`. Zero means "no colour", which
    /// Discord renders as the default text colour rather than as black.
    pub color: u32,
}

impl RoleOption {
    pub fn new(id: impl Into<String>, name: impl Into<String>, color: u32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            color,
        }
    }

    /// The swatch colour, or `None` for Discord's uncoloured default.
    fn swatch(&self) -> Option<Color32> {
        (self.color != 0).then(|| {
            Color32::from_rgb(
                ((self.color >> 16) & 0xff) as u8,
                ((self.color >> 8) & 0xff) as u8,
                (self.color & 0xff) as u8,
            )
        })
    }
}

/// Caller-owned state, so the picker itself stays a builder.
#[derive(Debug, Clone, Default)]
pub struct RolePickerState {
    /// True while the dropdown is open. Starts closed: a field with a value
    /// shows the value.
    pub searching: bool,
    pub query: String,
    pub highlight: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RolePickerResponse {
    /// A role id was chosen.
    pub chosen: Option<String>,
    /// The clear button was pressed — the caller decides whether that means an
    /// empty id or removing the row.
    pub cleared: bool,
}

pub struct RolePicker<'a> {
    id_salt: &'a str,
    state: &'a mut RolePickerState,
    roles: &'a [RoleOption],
    selected: &'a str,
    placeholder: &'a str,
    width: f32,
}

impl<'a> RolePicker<'a> {
    pub fn new(
        id_salt: &'a str,
        state: &'a mut RolePickerState,
        roles: &'a [RoleOption],
        selected: &'a str,
    ) -> Self {
        Self {
            id_salt,
            state,
            roles,
            selected,
            placeholder: "Search roles…",
            width: 220.0,
        }
    }

    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = text;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show(self, ui: &mut Ui) -> RolePickerResponse {
        let mut out = RolePickerResponse::default();
        let resolved = self.roles.iter().find(|role| role.id == self.selected);

        if !self.state.searching {
            ui.horizontal(|ui| {
                match (self.selected.is_empty(), resolved) {
                    // Nothing chosen yet.
                    (true, _) => {
                        if ui.button("choose role…").clicked() {
                            self.state.searching = true;
                            self.state.query.clear();
                            self.state.highlight = 0;
                        }
                    }
                    // Chosen and known.
                    (false, Some(role)) => {
                        if let Some(color) = role.swatch() {
                            ui.colored_label(color, "●");
                        }
                        if ui.button(format!("@{}", role.name)).clicked() {
                            self.state.searching = true;
                            self.state.query.clear();
                            self.state.highlight = 0;
                        }
                    }
                    // Chosen, but no longer in the guild — or the roster has
                    // not arrived yet. Both render the id rather than pretend
                    // the field is empty; the tooltip separates them, because
                    // "still loading" and "this role is gone" want completely
                    // different reactions from the reader.
                    (false, None) => {
                        ui.label(PhosphorIcon::Warning.rich_text(14.0, theme::ACCENT_YELLOW));
                        let response = ui.button(self.selected);
                        if response.clicked() {
                            self.state.searching = true;
                            self.state.query.clear();
                            self.state.highlight = 0;
                        }
                        response.on_hover_text(match self.roles.is_empty() {
                            true => "role list not loaded yet — this id is unverified",
                            false => "no such role in this guild — it may have been deleted",
                        });
                    }
                }
                // `rich_text`, not `as_str`: Phosphor is installed as its OWN
                // family, so a bare codepoint in the default family is tofu —
                // which is exactly what `as_str` here produced on screen. The
                // glyph test catches a `✖` literal but cannot catch this,
                // because the string is legitimate and only the family is
                // wrong.
                if !self.selected.is_empty()
                    && ui
                        .button(PhosphorIcon::X.rich_text(14.0, theme::TEXT_MUTED))
                        .clicked()
                {
                    out.cleared = true;
                }
            });
            return out;
        }

        // Ranked here rather than by the caller: every consumer would otherwise
        // reimplement the same case-insensitive contains, and inconsistently.
        let needle = self.state.query.trim().to_lowercase();
        let options: Vec<TypeaheadOption> = self
            .roles
            .iter()
            .filter(|role| needle.is_empty() || role.name.to_lowercase().contains(&needle))
            .map(|role| {
                // The id as the subtitle, always. Two roles can share a name,
                // and the id is what actually gets stored — a picker that never
                // showed it would make that collision unresolvable.
                TypeaheadOption::new(role.id.clone(), format!("@{}", role.name))
                    .subtitle(role.id.clone())
            })
            .collect();

        ui.vertical(|ui| {
            ui.set_width(self.width);
            let resp = TypeaheadSearch::new(
                self.id_salt,
                &mut self.state.query,
                &options,
                &mut self.state.highlight,
            )
            .placeholder(self.placeholder)
            .empty_text(match self.roles.is_empty() {
                true => "no roles loaded — refresh the roster",
                false => "no role matches",
            })
            .autofocus(true)
            .show(ui);

            if let Some(id) = resp.chosen {
                out.chosen = Some(id);
                self.state.searching = false;
            }
            if ui.button("cancel").clicked() {
                self.state.searching = false;
            }
        });

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_of_zero_is_no_swatch() {
        // Discord uses 0 for "no colour", which renders as default text — not
        // as black, which is what a naive conversion would paint.
        assert!(RoleOption::new("1", "member", 0).swatch().is_none());
        assert_eq!(
            RoleOption::new("1", "member", 0x5865F2).swatch(),
            Some(Color32::from_rgb(0x58, 0x65, 0xF2))
        );
    }
}
