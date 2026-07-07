//! Story: entitlement-gated rendering (`gated` / `locked_card` /
//! `locked_chip`) — the frontend half of the `authorizations` framework.

use egui_widgets::gated::{gated, GateState, LockedStyle};

use crate::{ACCENT, TEXT_MUTED};

authorizations::features! {
    pub const DEMO_FEATURE = {
        id: "story.demo-feature",
        name: "Visual Search",
        locked_hint: "Collector-gated — run /collector in a partner Discord to unlock",
    };
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Gated").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Entitlement-gated rendering. The same `Feature` const drives backend \
             enforcement and these locked affordances, so the id, name, and unlock \
             copy never drift. Immediate mode: the grant decision re-runs each frame.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let granted = GateState::from_scope_string("story.demo-feature");
    let wrong_session = GateState::from_scope_string("other.entitlement");
    let anonymous = GateState::Anonymous;

    let cases: [(&str, &GateState, LockedStyle); 4] = [
        ("Granted — content renders", &granted, LockedStyle::Card),
        ("Anonymous — locked card", &anonymous, LockedStyle::Card),
        (
            "Authenticated but not entitled — locked card + nudge",
            &wrong_session,
            LockedStyle::Card,
        ),
        ("Anonymous — inline chip", &anonymous, LockedStyle::Chip),
    ];

    for (caption, gate, style) in cases {
        ui.label(egui::RichText::new(caption).small().strong());
        ui.add_space(2.0);
        gated(ui, gate, &DEMO_FEATURE, style, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("gated content — drop an image to search the collection");
            });
        });
        ui.add_space(12.0);
    }
}
