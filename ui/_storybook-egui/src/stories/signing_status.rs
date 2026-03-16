//! Storybook demo for the SigningStatus widget.

use egui_widgets::signing_status::{self, SigningPhase, SigningStatusConfig};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct SigningStatusStoryState {
    pub phase: SigningPhase,
    pub peer_signed: bool,
    pub last_action: String,
}

impl Default for SigningStatusStoryState {
    fn default() -> Self {
        Self {
            phase: SigningPhase::AwaitingSignatures,
            peer_signed: false,
            last_action: String::new(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SigningStatusStoryState) {
    ui.label(
        egui::RichText::new("SigningStatus Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Concurrent signing checklist for the trade desk. Shows each party's \
             signing progress and provides Sign/Cancel actions.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Phase selector
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Phase:")
                .color(egui_widgets::theme::TEXT_SECONDARY)
                .size(10.0),
        );

        let phases: &[(&str, SigningPhase, bool)] = &[
            ("Awaiting", SigningPhase::AwaitingSignatures, false),
            ("Waiting for Peer", SigningPhase::WaitingForPeer, false),
            ("Peer signed first", SigningPhase::AwaitingSignatures, true),
            ("Submitting", SigningPhase::Submitting, true),
            (
                "Submitted",
                SigningPhase::Submitted {
                    tx_hash: "abc123def456789012345678901234567890abcdef0123456789012345deadbeef"
                        .into(),
                },
                true,
            ),
            (
                "Confirmed",
                SigningPhase::Confirmed {
                    tx_hash: "abc123def456789012345678901234567890abcdef0123456789012345deadbeef"
                        .into(),
                },
                true,
            ),
        ];

        for (label, phase, peer) in phases {
            if ui
                .selectable_label(state.phase == *phase && state.peer_signed == *peer, *label)
                .clicked()
            {
                state.phase = phase.clone();
                state.peer_signed = *peer;
                state.last_action.clear();
            }
        }
    });

    ui.add_space(12.0);

    // Widget
    ui.allocate_ui(egui::vec2(320.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                let config = SigningStatusConfig {
                    peer_name: "$boef".into(),
                    ..SigningStatusConfig::default()
                };

                let resp = signing_status::show(ui, &state.phase, state.peer_signed, &config);

                if let Some(action) = resp.action {
                    match action {
                        signing_status::SigningAction::Sign => {
                            state.last_action = "Sign clicked!".into();
                            state.phase = SigningPhase::WaitingForPeer;
                        }
                        signing_status::SigningAction::Cancel => {
                            state.last_action = "Cancel clicked!".into();
                        }
                    }
                }
            });
    });

    ui.add_space(12.0);

    if !state.last_action.is_empty() {
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(egui_widgets::theme::ACCENT_CYAN)
                .size(11.0),
        );
    }

    ui.add_space(12.0);
    if ui.button("Reset").clicked() {
        *state = SigningStatusStoryState::default();
    }
}
