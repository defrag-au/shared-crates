//! `UserBadge` — a compact "logged in as" pill (avatar + name) with a
//! click-to-open popup carrying a sign-out action. Data-only inputs (name +
//! optional avatar URL), so it's reusable by any app regardless of how the
//! session was obtained.
//!
//! ```no_run
//! # use egui_widgets::user_badge::{UserBadge, UserBadgeAction};
//! # fn demo(ui: &mut egui::Ui) {
//! if UserBadge::new("damo").avatar_url(Some("https://…/a.png")).show(ui)
//!     == UserBadgeAction::SignOut
//! {
//!     // clear the session
//! }
//! # }
//! ```

use egui::{Color32, RichText, Sense, Ui, Vec2};

use crate::icons::{install_phosphor_font, PhosphorIcon};

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
    id_salt: &'a str,
}

impl<'a> UserBadge<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            avatar_url: None,
            id_salt: "user_badge",
        }
    }

    pub fn avatar_url(mut self, url: Option<&'a str>) -> Self {
        self.avatar_url = url;
        self
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
                            ui.label(
                                PhosphorIcon::User.rich_text(16.0, ui.visuals().weak_text_color()),
                            );
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
                ui.set_min_width(160.0);
                ui.label(RichText::new(self.name).strong());
                ui.label(
                    RichText::new("Signed in via Discord")
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
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
