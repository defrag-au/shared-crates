//! Story: `UserBadge` — the "logged in as" pill (avatar/icon + name) with a
//! click-to-open popup carrying the identity block and sign-out.

use egui_widgets::icons::PhosphorIcon;
use egui_widgets::user_badge::{UserBadge, UserBadgeAction};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("User Badge").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A compact logged-in-as pill. Click it for a popup with the session's \
             identity block and sign-out. Data-only inputs — reusable by any app, \
             whether the session came from an OAuth login or a signed wallet.",
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
    ui.add_space(12.0);

    // The wallet-session shape: the handle is what a human recognises, so it
    // leads; the address it resolves to and the allocation it carries sit
    // behind the click, where they can be read carefully and copied.
    ui.label(egui::RichText::new("Wallet session (handle + entitlement)").small());
    ui.add_space(2.0);
    let _ = UserBadge::new("$boef")
        .icon(PhosphorIcon::Wallet)
        .subtitle("Signed in with wallet")
        .identifier(
            "stake",
            "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9",
        )
        .detail("Aliens", "2,410,000")
        .detail("History", "12 months")
        .id_salt("story_badge_3")
        .show(ui);
    ui.add_space(12.0);

    // …and the same wallet before a handle resolves, which is the state a
    // reader actually arrives in. The pill must still read as an identity.
    ui.label(egui::RichText::new("Wallet session, no handle").small());
    ui.add_space(2.0);
    let _ = UserBadge::new("stake1u8962…c5rhq9")
        .icon(PhosphorIcon::Wallet)
        .subtitle("Signed in with wallet")
        .identifier(
            "stake",
            "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9",
        )
        .detail("Aliens", "0")
        .detail("History", "90 days")
        .id_salt("story_badge_4")
        .show(ui);
}
