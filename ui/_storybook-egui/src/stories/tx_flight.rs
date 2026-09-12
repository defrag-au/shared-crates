//! Storybook demo for the TxFlight widget — the build / sign / submit loop
//! with a fake backend and a fake wallet so every phase can be reached.

use egui_widgets::tx_flight::{
    self, FlightAction, FlightPhase, FlightReview, FlightStage, TxFlightConfig,
};

use crate::{accent, bg, muted};

pub struct TxFlightStoryState {
    pub phase: FlightPhase,
    pub can_build: bool,
    pub wallet_declines: bool,
    pub submit_fails: bool,
    pub last_action: String,
    /// Frames left until the simulated async step resolves.
    countdown: u32,
}

impl Default for TxFlightStoryState {
    fn default() -> Self {
        Self {
            phase: FlightPhase::Idle,
            can_build: true,
            wallet_declines: false,
            submit_fails: false,
            last_action: String::new(),
            countdown: 0,
        }
    }
}

fn review() -> FlightReview {
    FlightReview {
        headline: "Park ask.spend as a reference script".into(),
        rows: vec![
            ("Network".into(), "preprod".into()),
            (
                "Script".into(),
                "fcf74fd0…1d6d94 (Plutus V2, 1,534 B)".into(),
            ),
            ("Parked at".into(), "addr_test1wr70wn7s…lsaqu9".into()),
            ("Locks".into(), "8.05 ADA".into()),
            ("Fee".into(), "0.23 ADA".into()),
        ],
    }
}

const TX_HASH: &str = "c15cd553aae2c18dbd869544cb6d2a2e50798b5f1ae0764bce6ed45a3a32a83a";
const SIMULATED_FRAMES: u32 = 90;

pub fn show(ui: &mut egui::Ui, state: &mut TxFlightStoryState) {
    ui.label(
        egui::RichText::new("TxFlight Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "One server-built, wallet-signed transaction as a checklist. The host \
             owns the async work; this story fakes it with a frame countdown.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    // Story controls.
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Jump to:")
                .color(crate::secondary(ui))
                .size(10.0),
        );
        let presets: &[(&str, FlightPhase)] = &[
            ("Idle", FlightPhase::Idle),
            ("Building", FlightPhase::Building),
            ("Review", FlightPhase::Review(review())),
            ("Signing", FlightPhase::Signing(review())),
            ("Submitting", FlightPhase::Submitting(review())),
            (
                "Landed",
                FlightPhase::Landed {
                    review: review(),
                    tx_hash: TX_HASH.into(),
                },
            ),
            (
                "Sign failed",
                FlightPhase::Failed {
                    stage: FlightStage::Sign,
                    error: "user declined to sign tx".into(),
                },
            ),
            (
                "Submit failed",
                FlightPhase::Failed {
                    stage: FlightStage::Submit,
                    error: "tx rejected: {\"contents\":{\"contents\":\"ConwayMempoolFailure \\\"All inputs are spent\\\"\"}}".into(),
                },
            ),
        ];
        for (label, phase) in presets {
            if ui.selectable_label(state.phase == *phase, *label).clicked() {
                state.phase = phase.clone();
                state.countdown = 0;
                state.last_action.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.can_build, "Blueprint uploaded (Build enabled)");
        ui.checkbox(&mut state.wallet_declines, "Wallet declines");
        ui.checkbox(&mut state.submit_fails, "Submit fails");
    });

    ui.add_space(12.0);

    // Simulated async: count frames down, then resolve the busy phase.
    if state.countdown > 0 {
        state.countdown -= 1;
        ui.ctx().request_repaint();
        if state.countdown == 0 {
            state.phase = match std::mem::replace(&mut state.phase, FlightPhase::Idle) {
                FlightPhase::Building => FlightPhase::Review(review()),
                FlightPhase::Signing(r) => {
                    if state.wallet_declines {
                        FlightPhase::Failed {
                            stage: FlightStage::Sign,
                            error: "user declined to sign tx".into(),
                        }
                    } else {
                        state.countdown = SIMULATED_FRAMES;
                        FlightPhase::Submitting(r)
                    }
                }
                FlightPhase::Submitting(r) => {
                    if state.submit_fails {
                        FlightPhase::Failed {
                            stage: FlightStage::Submit,
                            error: "provider unavailable: koios fetch: timeout".into(),
                        }
                    } else {
                        FlightPhase::Landed {
                            review: r,
                            tx_hash: TX_HASH.into(),
                        }
                    }
                }
                other => other,
            };
        }
    }

    ui.allocate_ui(egui::vec2(360.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                let config = TxFlightConfig {
                    build_label: "Build deployment",
                    can_build: state.can_build,
                    build_blocker: Some("Upload a blueprint first".into()),
                    ..TxFlightConfig::default()
                };
                let resp = tx_flight::show(ui, &state.phase, &config);
                if let Some(action) = resp.action {
                    state.last_action = format!("{action:?}");
                    match action {
                        FlightAction::Build => {
                            state.phase = FlightPhase::Building;
                            state.countdown = SIMULATED_FRAMES;
                        }
                        FlightAction::Sign => {
                            if let Some(r) = state.phase.review().cloned() {
                                state.phase = FlightPhase::Signing(r);
                                state.countdown = SIMULATED_FRAMES;
                            }
                        }
                        FlightAction::Submit => {
                            state.phase = FlightPhase::Submitting(review());
                            state.countdown = SIMULATED_FRAMES;
                        }
                        FlightAction::Discard | FlightAction::Reset => {
                            state.phase = FlightPhase::Idle;
                        }
                    }
                }
            });
    });

    ui.add_space(12.0);
    if !state.last_action.is_empty() {
        ui.label(
            egui::RichText::new(format!("Last action: {}", state.last_action))
                .color(crate::tok(ui, egui_widgets::theme::Token::AccentCyan))
                .size(11.0),
        );
    }
    ui.add_space(12.0);
    if ui.button("Reset").clicked() {
        *state = TxFlightStoryState::default();
    }
}
