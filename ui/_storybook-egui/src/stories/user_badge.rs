//! Story: `UserBadge` — the "logged in as" pill (avatar + name) with a
//! click-to-open sign-out popup.

use egui_widgets::user_badge::{UserBadge, UserBadgeAction};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("User Badge").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A compact logged-in-as pill. Click it for a popup with sign-out. \
             Data-only inputs (name + optional avatar URL) — reusable by any app.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    ui.label(egui::RichText::new("With no avatar (glyph fallback)").small());
    ui.add_space(2.0);
    if UserBadge::new("damo").id_salt("story_badge_1").show(ui) == UserBadgeAction::SignOut {
        // (story: no-op)
    }
    ui.add_space(12.0);

    ui.label(egui::RichText::new("With avatar URL").small());
    ui.add_space(2.0);
    let _ = UserBadge::new("Skulliance Member")
        .avatar_url(Some("https://cdn.discordapp.com/embed/avatars/0.png"))
        .id_salt("story_badge_2")
        .show(ui);
}
