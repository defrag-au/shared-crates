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

use crate::icons::{PhosphorIcon, install_phosphor_font};
use crate::id_pill::{IdPill, IdPillLayout};
use crate::property_list::PropertyList;

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

    pub fn show(self, ui: &mut Ui) -> UserBadgeAction {
        install_phosphor_font(ui.ctx());

        // The pill: avatar (or fallback glyph) + name, laid out as one
        // clickable group.
        let pill = egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .corner_radius(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    match self.avatar_url {
                        Some(url) => {
                            ui.add(
                                egui::Image::new(url)
                                    .fit_to_exact_size(Vec2::splat(20.0))
                                    .corner_radius(10.0),
                            );
                        }
                        None => {
                            ui.label(self.icon.rich_text(16.0, ui.visuals().weak_text_color()));
                        }
                    }
                    ui.label(RichText::new(self.name).size(12.0));
                    ui.label(
                        PhosphorIcon::CaretDown.rich_text(10.0, ui.visuals().weak_text_color()),
                    );
                });
            })
            .response
            .interact(Sense::click());

        if pill.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let popup_id = ui.make_persistent_id(self.id_salt);
        let mut action = UserBadgeAction::None;
        egui::Popup::menu(&pill)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                // An identifier pill and a detail grid both need room the bare
                // name never did, and a popup that resizes as the session
                // resolves reads as a glitch.
                let roomy = self.identifier.is_some() || !self.details.is_empty();
                ui.set_min_width(if roomy { 240.0 } else { 160.0 });

                ui.label(RichText::new(self.name).strong());
                ui.label(
                    RichText::new(self.subtitle)
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );

                if let Some((label, value)) = self.identifier {
                    ui.add_space(4.0);
                    IdPill::new(label, value)
                        .layout(IdPillLayout::Inline)
                        .with_widths(10, 6)
                        .show(ui);
                }

                if !self.details.is_empty() {
                    ui.add_space(4.0);
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
