//! `PaneNavBar` — the shell nav for an app made of capability panes.
//!
//! A horizontal strip of destinations with a persistent selection, where an
//! entry the reader is not entitled to appears **locked with its reason**
//! rather than vanishing.
//!
//! ## Why not `ButtonGroup`
//!
//! That is an action bar: fire-and-forget clicks, response is "which one was
//! pressed". A nav has state — something is *currently* selected, and the
//! strip has to render that every frame. Reaching for `ButtonGroup` and
//! tracking selection alongside it puts the selected-ness in the caller,
//! which is how two navs in one app end up highlighting differently.
//!
//! ## Why horizontal, and not a side panel
//!
//! Because `egui::Panel` claims a full region, and a shell whose content sits
//! in a centred, width-constrained scroll column has no full region to give
//! it — the panel gets squeezed and clips. A nav that lives *in* the content
//! column composes with whatever the app already does.
//!
//! Use a real `Panel` when the shell genuinely owns the viewport. This widget
//! is for the much more common case where it does not.
//!
//! ## It hides itself
//!
//! A nav offering one destination is furniture: it costs vertical space and
//! tells the reader nothing they can act on. [`PaneNavBar`] draws nothing
//! when fewer than two entries are *visible*, and says so in its response so
//! the caller can skip its own spacing too.
//!
//! ```ignore
//! use egui_widgets::{PaneNavBar, PaneNavEntry};
//!
//! let resp = PaneNavBar::new(selected_id)
//!     .add(PaneNavEntry::new(0, "Clients"))
//!     .add(PaneNavEntry::new(1, "Gateway").locked("Operator access — ask Augminted"))
//!     .show(ui);
//! if let Some(id) = resp.selected {
//!     selected_id = id;
//! }
//! ```

use egui::{RichText, Ui};

use crate::icons::{PhosphorIcon, install_phosphor_font, phosphor_label};

/// One destination in the nav.
pub struct PaneNavEntry<'a> {
    id: u64,
    label: &'a str,
    icon: Option<PhosphorIcon>,
    /// `Some(reason)` = present but refused.
    locked: Option<&'a str>,
}

impl<'a> PaneNavEntry<'a> {
    /// An available destination. `id` is the caller's own discriminant.
    pub fn new(id: u64, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            locked: None,
        }
    }

    /// Leading glyph. A [`PhosphorIcon`], never a bare unicode literal —
    /// most decorative codepoints are not in the font stack and render as
    /// tofu with no error anywhere.
    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Present, but refused — greyed, unselectable, with `reason` on hover.
    ///
    /// **Pass the entitlement's own `locked_hint`**, not a sentence written
    /// here: the id, the display name and the explanation should come from
    /// one place, so what a reader is told matches what the backend enforces.
    ///
    /// Locking is not the control. A locked entry must still be refused by
    /// its backend — this only decides what a reader is shown.
    pub fn locked(mut self, reason: &'a str) -> Self {
        self.locked = Some(reason);
        self
    }
}

/// Outcome of one [`PaneNavBar::show`].
#[derive(Default, Debug)]
pub struct PaneNavResponse {
    /// The entry the reader chose this frame. `None` if they did not choose.
    pub selected: Option<u64>,
    /// Whether the strip drew anything — false when it hid itself. Callers
    /// use this to skip their own surrounding spacing.
    pub shown: bool,
}

/// Builder.
pub struct PaneNavBar<'a> {
    entries: Vec<PaneNavEntry<'a>>,
    current: u64,
    hide_when_single: bool,
}

impl<'a> PaneNavBar<'a> {
    /// New nav, with the currently-selected entry's id.
    pub fn new(current: u64) -> Self {
        Self {
            entries: Vec::new(),
            current,
            hide_when_single: true,
        }
    }

    #[allow(clippy::should_implement_trait)] // builder verb, not arithmetic
    pub fn add(mut self, entry: PaneNavEntry<'a>) -> Self {
        self.entries.push(entry);
        self
    }

    /// Draw even when there is only one destination. Default `false` —
    /// see the module docs on why a one-entry nav is furniture.
    pub fn always_show(mut self, b: bool) -> Self {
        self.hide_when_single = !b;
        self
    }

    pub fn show(self, ui: &mut Ui) -> PaneNavResponse {
        let mut response = PaneNavResponse::default();
        if self.hide_when_single && self.entries.len() < 2 {
            return response;
        }
        // Unconditional: a locked entry gets a padlock whether or not it
        // carries an icon of its own, so "any entry has an icon" is not the
        // right question. Idempotent.
        install_phosphor_font(ui.ctx());
        response.shown = true;

        // `horizontal_wrapped`, not `horizontal`: a narrow viewport should
        // spill the nav onto a second row rather than push destinations off
        // the edge where they cannot be reached at all.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for entry in &self.entries {
                match entry.locked {
                    None => {
                        let selected = entry.id == self.current;
                        let text: egui::WidgetText = match entry.icon {
                            // A LayoutJob, never `format!("{icon} {label}")` —
                            // a single-family RichText looks the glyph up in
                            // the proportional font, misses, and draws tofu
                            // with no error anywhere.
                            Some(icon) => phosphor_label(ui, icon, entry.label),
                            None => RichText::new(entry.label).into(),
                        };
                        if ui.selectable_label(selected, text).clicked() && !selected {
                            response.selected = Some(entry.id);
                        }
                    }
                    Some(reason) => {
                        // Disabled rather than absent: "you may not, and here
                        // is why" is a different message from "this does not
                        // exist", and a reader who could qualify deserves the
                        // first one.
                        //
                        // A padlock replaces the entry's own icon rather than
                        // joining it: the reason it is greyed is the only
                        // thing worth a glyph here, and hover is too cheap a
                        // place to hide the whole explanation.
                        let text = phosphor_label(ui, PhosphorIcon::Lock, entry.label);
                        // A DISABLED selectable BUTTON, not a bare `Label`:
                        // the two have different padding, so a mixed row sits
                        // the locked entries below the baseline of the
                        // selectable ones. Same widget, same metrics.
                        // (`Button::selectable` — `SelectableLabel` is gone.)
                        ui.add_enabled(false, egui::Button::selectable(false, text))
                            .on_disabled_hover_text(reason);
                    }
                }
            }
        });
        ui.add_space(4.0);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-destination nav draws nothing — and says so, so the caller can
    /// skip its own spacing rather than leaving a gap where a strip isn't.
    #[test]
    fn a_single_destination_hides_itself() {
        let bar = PaneNavBar::new(0).add(PaneNavEntry::new(0, "Only"));
        assert!(bar.hide_when_single);
        assert_eq!(bar.entries.len(), 1);

        // Opt out for a shell that wants the strip regardless.
        let bar = PaneNavBar::new(0)
            .add(PaneNavEntry::new(0, "Only"))
            .always_show(true);
        assert!(!bar.hide_when_single);
    }

    /// Locked is a state of a PRESENT entry. A caller that wants an entry
    /// gone omits it; `locked` never means hidden, because the two carry
    /// different messages to a reader who could qualify.
    #[test]
    fn locked_entries_are_present_not_absent() {
        let bar = PaneNavBar::new(0)
            .add(PaneNavEntry::new(0, "Clients"))
            .add(PaneNavEntry::new(1, "Gateway").locked("hold the role"));
        assert_eq!(bar.entries.len(), 2);
        assert_eq!(bar.entries[1].locked, Some("hold the role"));
        // Two entries, one locked — still shown, because the reader has a
        // real choice to see even if one arm is refused.
        assert!(bar.entries.len() >= 2);
    }
}
