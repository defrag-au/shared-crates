//! Story: `AccessGate` — the app-level access screen (sign-in prompt +
//! requirements screen), driven by an `authorizations::Feature` and a list
//! of communities that grant access.

use egui_widgets::access_gate::{AccessGate, GateProvider, GateStatus};

use crate::{ACCENT, TEXT_MUTED};

authorizations::features! {
    pub const DEMO_ACCESS = {
        id: "story.app-access",
        name: "Collection Explorer",
        locked_hint: "Access is granted through partner communities — hold a qualifying role",
    };
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Access Gate").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "The front door for a gated tool: a sign-in prompt for anonymous \
             visitors, and a requirements screen (what to join) for signed-in \
             but unqualified users. Returns an action the app maps to \
             navigation / logout.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let providers = vec![
        GateProvider {
            label: "BlackFlag".into(),
            invite_url: Some("https://discord.gg/example".into()),
            requirements: vec!["Hold the Deckhand role — verify an OG NFT in #verify".into()],
        },
        GateProvider {
            label: "Skulliance".into(),
            invite_url: None,
            requirements: vec!["Complete member verification to earn the Member role".into()],
        },
    ];

    ui.label(
        egui::RichText::new("Anonymous (sign-in prompt)")
            .small()
            .strong(),
    );
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(160.0);
        let _ = AccessGate::new(&DEMO_ACCESS, GateStatus::Anonymous).show(ui);
    });
    ui.add_space(12.0);

    ui.label(
        egui::RichText::new("Signed in but unqualified (requirements)")
            .small()
            .strong(),
    );
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(240.0);
        let _ = AccessGate::new(&DEMO_ACCESS, GateStatus::Unqualified)
            .providers(&providers)
            .show(ui);
    });
}
