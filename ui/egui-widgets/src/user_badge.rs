//! `UserBadge` — a compact "logged in as" pill (avatar/icon + name) whose
//! click-to-open popup carries the session's identity block and a sign-out
//! action.
//!
//! Data-only inputs, so it's reusable by any app regardless of how the session
//! was obtained. That matters because the two things a session badge shows —
//! *who you are* and *what that entitles you to* — are named differently per
//! app: a Discord login shows an avatar and a username, a wallet login shows a
//! handle, a stake address and a holding. Both are the same widget with
//! different slots filled.
//!
//! The popup composes [`crate::id_pill::IdPill`] for the identifier (so a stake
//! address is middle-elided, copyable, and links to pool.pm for free) and
//! [`crate::property_list::PropertyList`] for the detail rows, rather than
//! hand-building either.
//!
//! ```no_run
//! # use egui_widgets::user_badge::{UserBadge, UserBadgeAction};
//! # use egui_widgets::icons::PhosphorIcon;
//! # fn demo(ui: &mut egui::Ui) {
//! // A Discord session: avatar + the default provenance line.
//! if UserBadge::new("damo").avatar_url(Some("https://…/a.png")).show(ui)
//!     == UserBadgeAction::SignOut
//! {
//!     // clear the session
//! }
//!
//! // A wallet session: handle up front, the proof and the entitlement inside.
//! let _ = UserBadge::new("$boef")
//!     .icon(PhosphorIcon::Wallet)
//!     .subtitle("Signed in with wallet")
//!     .identifier("stake", "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9")
//!     .detail("Aliens", "1,234,567")
//!     .detail("History", "12 months")
//!     .id_salt("wallet_badge")
//!     .show(ui);
//! # }
//! ```

use egui::{Color32, RichText, Sense, Ui, Vec2};

use crate::icons::PhosphorIcon;
use crate::id_pill::{IdPill, IdPillLayout};
use crate::property_list::PropertyList;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};

/// What the user did with the badge this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserBadgeAction {
    None,
    SignOut,
}

/// A logged-in-user pill. Construct with a display name; optionally attach
/// an avatar URL (egui image loaders must be installed by the app).
pub struct UserBadge<'a> {
    name: &'a str,
    avatar_url: Option<&'a str>,
    icon: PhosphorIcon,
    subtitle: &'a str,
    identifier: Option<(&'a str, &'a str)>,
    details: Vec<(&'a str, String)>,
    id_salt: &'a str,
}

impl<'a> UserBadge<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            avatar_url: None,
            icon: PhosphorIcon::User,
            // Kept as the default because it is what the original callers
            // (Discord OAuth sessions) actually do. Apps whose sessions are
            // proved some other way must say so via [`Self::subtitle`] — a
            // provenance line is the one thing on this widget a reader will
            // take literally.
            subtitle: "Signed in via Discord",
            identifier: None,
            details: Vec::new(),
            id_salt: "user_badge",
        }
    }

    pub fn avatar_url(mut self, url: Option<&'a str>) -> Self {
        self.avatar_url = url;
        self
    }

    /// Glyph shown in the pill when there is no avatar. Default
    /// [`PhosphorIcon::User`].
    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = icon;
        self
    }

    /// The provenance line under the name — how this session was established.
    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = subtitle;
        self
    }

    /// The identifier behind the display name, shown in the popup as an
    /// [`IdPill`].
    ///
    /// Separate from the name on purpose: the name is what a human recognises
    /// (`$boef`), the identifier is what it resolves to and what they'd paste
    /// somewhere else. A truncated address is *not* safe to eyeball, so it
    /// belongs behind the click with a copy button, never in the pill.
    pub fn identifier(mut self, label: &'a str, value: &'a str) -> Self {
        self.identifier = Some((label, value));
        self
    }

    /// Append a label/value row to the popup — what this session is entitled
    /// to, what it holds, when it expires.
    pub fn detail(mut self, label: &'a str, value: impl Into<String>) -> Self {
        self.details.push((label, value.into()));
        self
    }

    /// Append a row only when there is one, so an optional field doesn't
    /// force a `Vec` at the call site.
    pub fn detail_optional<S: Into<String>>(self, label: &'a str, value: Option<S>) -> Self {
        match value {
            Some(v) => self.detail(label, v),
            None => self,
        }
    }

    /// Override the popup id salt (needed if more than one badge renders).
    pub fn id_salt(mut self, salt: &'a str) -> Self {
        self.id_salt = salt;
        self
    }

    /// Whether the display name is just the identifier, shortened.
    ///
    /// Split on the ELLIPSIS rather than compared for equality or halved: the
    /// two strings are elided by different code to different widths — the
    /// caller picks the pill's, [`IdPill`] picks its own — so neither the
    /// whole string nor its midpoint lines up. The ellipsis is the actual
    /// boundary, and the pieces either side of it are exactly what the reader
    /// is being shown twice.
    fn name_repeats_identifier(&self) -> bool {
        let Some((_, value)) = self.identifier else {
            return false;
        };
        if self.name == value {
            return true;
        }
        let Some((head, tail)) = self.name.split_once('…') else {
            return false;
        };
        // Enough of a prefix to mean something: every Cardano stake address
        // starts "stake1", so a two-character match is not a match.
        head.chars().count() >= 7
            && !tail.is_empty()
            && value.starts_with(head)
            && value.ends_with(tail)
    }

    pub fn show(self, ui: &mut Ui) -> UserBadgeAction {
        crate::icons::ensure_fonts(ui);

        // The pill: avatar (or fallback glyph) + name, laid out as one
        // clickable group.
        let pill = egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(ui.tokens().margin_xy(Space::Md, Space::Sm))
            // Exactly half the pill's height, so this is a pill rather than a
            // rounded box — `Full` says that, and stays true if a theme changes
            // the ramp or the padding changes the height.
            .corner_radius(ui.tokens().corner(Radius::Full))
            .show(ui, |ui| {
                // `ui.horizontal` and not an explicit layout: it is the only
                // container that shrinks to its content, which a pill in a
                // header must do — `with_layout` takes `available_rect` and
                // the pill then spans the bar and clips its own name.
                //
                // The cost is that it takes its DIRECTION from the parent, so
                // in a right-aligned header the row runs right-to-left and
                // this comes out caret, name, avatar. Adding the pieces
                // pre-reversed cancels that out.
                ui.horizontal(|ui| {
                    ui.set_item_gap_x(Space::Base);
                    let avatar = |ui: &mut Ui| match self.avatar_url {
                        Some(url) => {
                            ui.add(
                                egui::Image::new(url)
                                    .fit_to_exact_size(Vec2::splat(20.0))
                                    // Half of the 20pt avatar — a circle.
                                    .corner_radius(ui.tokens().corner(Radius::Full)),
                            );
                        }
                        None => {
                            ui.label(self.icon.rich_text(16.0, ui.visuals().weak_text_color()));
                        }
                    };
                    let name = |ui: &mut Ui| {
                        ui.label(RichText::new(self.name).size(ui.text_size(TextSize::Md)));
                    };
                    let caret = |ui: &mut Ui| {
                        ui.label(
                            PhosphorIcon::CaretDown.rich_text(10.0, ui.visuals().weak_text_color()),
                        );
                    };
                    if ui.layout().prefer_right_to_left() {
                        caret(ui);
                        name(ui);
                        avatar(ui);
                    } else {
                        avatar(ui);
                        name(ui);
                        caret(ui);
                    }
                });
            })
            .response
            .interact(Sense::click());

        if pill.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // A sizing pass measures; it must not touch popup state. The pill
        // above is what has a size, so measuring is already done — and
        // `Popup::menu` below closes on a click outside itself, which a
        // measuring pass reports for every click there is. A caller that
        // measures before placing would otherwise close this menu in the same
        // frame the real pass opened it, and it would never appear.
        if ui.is_sizing_pass() {
            return UserBadgeAction::None;
        }

        // Keyed to THIS pill, not just to the salt.
        //
        // `ui.make_persistent_id(salt)` gives two badges with the same salt
        // the same popup, and `Popup::menu` closes on a click outside itself
        // — so the second badge closes the first one's menu in the same frame
        // it opened, and neither ever appears. That is not hypothetical: an
        // `AccountBar` uses one fixed salt, so a page showing several of them
        // had a menu that could not be opened at all. `pill.id` is unique per
        // widget instance, which is the grain a popup actually belongs to.
        let popup_id = pill.id.with(self.id_salt);
        let mut action = UserBadgeAction::None;
        egui::Popup::menu(&pill)
            .id(popup_id)
            // `Popup::menu` sets `gap(0.0)`, which butts the panel against the
            // pill so the two read as one shape that has been cut in half.
            // A gap makes the pill the thing and the panel its consequence.
            .gap(ui.tokens().space(Space::Sm))
            // Hung from the pill's TRAILING edge, because that is the edge the
            // pill is aligned to: this sits in a right-aligned header, so a
            // panel growing leftward from the pill's left edge hangs off into
            // the middle of the page with nothing under it.
            .align(egui::RectAlign::BOTTOM_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                // An identifier pill and a detail grid both need room the bare
                // name never did, and a popup that resizes as the session
                // resolves reads as a glitch.
                let roomy = self.identifier.is_some() || !self.details.is_empty();
                ui.set_min_width(if roomy { 240.0 } else { 160.0 });

                // The name again ONLY when it says something the pill did not.
                //
                // A wallet with no handle takes its elided address as its
                // name, and the identifier row below is that same address
                // elided slightly differently — so the panel opened with the
                // reader's own key three times in a row, twice at nearly the
                // same width, which reads as a rendering fault rather than as
                // detail. A handle is a different matter: it IS the thing the
                // address resolves to, and repeating it heads the panel.
                if !self.name_repeats_identifier() {
                    ui.label(RichText::new(self.name).strong());
                }
                ui.label(
                    RichText::new(self.subtitle)
                        .size(ui.text_size(TextSize::Sm))
                        .color(ui.visuals().weak_text_color()),
                );

                if let Some((label, value)) = self.identifier {
                    ui.gap(Space::Sm);
                    IdPill::new(label, value)
                        .layout(IdPillLayout::Inline)
                        .with_widths(10, 6)
                        .show(ui);
                }

                if !self.details.is_empty() {
                    ui.gap(Space::Sm);
                    let mut list = PropertyList::new();
                    for (label, value) in &self.details {
                        list = list.add(label, value.clone());
                    }
                    list.show(ui);
                }

                ui.separator();
                if ui
                    .button(RichText::new("Sign out").color(Color32::from_rgb(224, 120, 120)))
                    .clicked()
                {
                    action = UserBadgeAction::SignOut;
                }
            });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";

    /// A wallet with no handle names itself with its own elided address, and
    /// the panel must not then open with that address three times over.
    #[test]
    fn an_elided_identifier_is_not_repeated_as_a_heading() {
        // The pill elides 8/6; `IdPill` elides 10/6. Different strings, same
        // address — matching on the ends is what catches it.
        let badge = UserBadge::new("stake1u8…c5rhq9").identifier("stake", STAKE);
        assert!(badge.name_repeats_identifier());

        let wider = UserBadge::new("stake1u896…c5rhq9").identifier("stake", STAKE);
        assert!(wider.name_repeats_identifier());
    }

    /// A handle is not a repetition — it is what the address resolves to, and
    /// it heads the panel.
    #[test]
    fn a_handle_still_heads_the_panel() {
        let badge = UserBadge::new("$boef").identifier("stake", STAKE);
        assert!(!badge.name_repeats_identifier());

        // A display name that merely starts the same way is not the address.
        let named = UserBadge::new("stakeholders").identifier("stake", STAKE);
        assert!(!named.name_repeats_identifier());

        // Nor is a short name that happens to be elided.
        let short = UserBadge::new("st…9").identifier("stake", STAKE);
        assert!(!short.name_repeats_identifier());
    }

    /// A stake address short enough to show whole is still the same address.
    #[test]
    fn an_unelided_identifier_is_caught_too() {
        let badge = UserBadge::new(STAKE).identifier("stake", STAKE);
        assert!(badge.name_repeats_identifier());
    }

    /// With nothing to compare against there is nothing to suppress.
    #[test]
    fn a_badge_without_an_identifier_always_shows_its_name() {
        assert!(!UserBadge::new("damo").name_repeats_identifier());
    }
}
