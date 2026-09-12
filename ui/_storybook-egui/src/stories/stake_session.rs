//! Storybook demo for the StakeSessionPanel widget — the wallet → sign-in →
//! signed-in strip every wallet-authed worker's admin opens with.
//!
//! The connect step needs a real CIP-30 wallet, so the story fakes the
//! connector's state instead: pick a phase and the panel draws it.

use egui_widgets::stake_session::{
    StakeSessionAction, StakeSessionPanel, StakeSessionPhase, StoredStakeSession,
};
use egui_widgets::wallet::{ConnectionState, Network, WalletConnector, WalletProvider};

use crate::{accent, bg, muted};

pub struct StakeSessionStoryState {
    pub connector: WalletConnector,
    pub phase: StakeSessionPhase,
    pub connected: bool,
    pub last_action: String,
}

impl Default for StakeSessionStoryState {
    fn default() -> Self {
        // `?stake_phase=signed-in|signing|failed|connected` presets the story so
        // a headless screenshot can land on a state that otherwise needs a
        // click (and a wallet extension) to reach.
        let preset = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|s| {
                s.trim_start_matches('?')
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("stake_phase=").map(str::to_string))
            });
        let (connected, phase) = match preset.as_deref() {
            Some("connected") => (true, StakeSessionPhase::SignedOut),
            Some("signing") => (true, StakeSessionPhase::SigningIn),
            Some("signed-in") => (true, StakeSessionPhase::SignedIn(session())),
            Some("failed") => (
                true,
                StakeSessionPhase::Failed(
                    "This wallet isn't on the abandonware admin allowlist.".into(),
                ),
            ),
            _ => (false, StakeSessionPhase::SignedOut),
        };
        let mut connector = WalletConnector::new();
        fake_connect(&mut connector, connected);
        Self {
            connector,
            phase,
            connected,
            last_action: String::new(),
        }
    }
}

fn session() -> StoredStakeSession {
    StoredStakeSession {
        token: "9f3c…".into(),
        stake_address: "stake_test1uzv7qd9rl65pck6vc0n32k6nkumzq7gj5tax6n0jdg4lp4sepylau".into(),
        tier: "super_admin".into(),
        expires_at_ms: js_sys::Date::now() as i64 + 7 * 3_600_000 + 52 * 60_000,
        wallet_name: "Eternl".into(),
    }
}

/// Put the connector into a fake connected state — the panel only reads it.
fn fake_connect(connector: &mut WalletConnector, connected: bool) {
    if connected {
        let provider = WalletProvider::from_api_name("eternl").unwrap_or(WalletProvider::all()[0]);
        connector.connection_state = ConnectionState::Connected {
            provider,
            address: "addr_test1qqpple6hh…".into(),
            network: Network::Preprod,
        };
        connector.stake_address =
            Some("stake_test1uzv7qd9rl65pck6vc0n32k6nkumzq7gj5tax6n0jdg4lp4sepylau".into());
    } else {
        connector.connection_state = ConnectionState::Disconnected;
        connector.stake_address = None;
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut StakeSessionStoryState) {
    ui.label(
        egui::RichText::new("StakeSessionPanel Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Connect a wallet, sign a worker's challenge, stay signed in. The panel \
             draws the phase the host holds and returns the click to act on.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Phase:")
                .color(crate::secondary(ui))
                .size(10.0),
        );
        let presets: &[(&str, bool, StakeSessionPhase)] = &[
            ("Disconnected", false, StakeSessionPhase::SignedOut),
            ("Connected, signed out", true, StakeSessionPhase::SignedOut),
            ("Signing in", true, StakeSessionPhase::SigningIn),
            ("Signed in", true, StakeSessionPhase::SignedIn(session())),
            (
                "Not allowlisted",
                true,
                StakeSessionPhase::Failed(
                    "This wallet isn't on the abandonware admin allowlist.".into(),
                ),
            ),
            (
                "Wrong network",
                true,
                StakeSessionPhase::Failed(
                    "Wrong network — this worker runs on preprod; switch your wallet to a \
                     preprod account and reconnect."
                        .into(),
                ),
            ),
        ];
        for (label, connected, phase) in presets {
            let selected = state.phase == *phase && state.connected == *connected;
            if ui.selectable_label(selected, *label).clicked() {
                state.connected = *connected;
                fake_connect(&mut state.connector, *connected);
                state.phase = phase.clone();
                state.last_action.clear();
            }
        }
    });
    ui.add_space(12.0);

    ui.allocate_ui(egui::vec2(360.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(
                crate::highlight(ui),
            ))
            .show(ui, |ui| {
                let action =
                    StakeSessionPanel::new(&state.connector, &state.phase, "abandonware admin")
                        .network(Some("cardano:preprod"))
                        .show(ui);
                match action {
                    StakeSessionAction::None => {}
                    StakeSessionAction::Connect(provider) => {
                        state.last_action = format!("Connect({provider:?})");
                        // A real host spawns `connect_wallet`; here, pretend it worked.
                        state.connected = true;
                        fake_connect(&mut state.connector, true);
                    }
                    StakeSessionAction::Disconnect => {
                        state.last_action = "Disconnect".into();
                        state.connected = false;
                        fake_connect(&mut state.connector, false);
                        state.phase = StakeSessionPhase::SignedOut;
                    }
                    StakeSessionAction::SignIn => {
                        state.last_action = "SignIn".into();
                        // Pretend the challenge round-trip succeeded.
                        state.phase = StakeSessionPhase::SignedIn(session());
                    }
                    StakeSessionAction::SignOut => {
                        state.last_action = "SignOut".into();
                        state.phase = StakeSessionPhase::SignedOut;
                    }
                }
            });
    });

    // The same strip inside a RIGHT-TO-LEFT header row, which is where a
    // header naturally wants it and where egui's `horizontal` would inherit
    // the direction and reverse every inner row. The widget lays itself out
    // top-down at its own width regardless — this frame is the proof.
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("Inside a right-to-left header row (must read the same)")
            .color(muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(bg(ui))
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui_widgets::theme::hairline(
            crate::highlight(ui),
        ))
        .show(ui, |ui| {
            ui.set_width(720.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Some app title")
                        .color(crate::tok(ui, egui_widgets::theme::Token::AccentCyan))
                        .size(16.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    let _ =
                        StakeSessionPanel::new(&state.connector, &state.phase, "abandonware admin")
                            .network(Some("cardano:preprod"))
                            .max_width(320.0)
                            .id_salt("stake_session_rtl")
                            .show(ui);
                });
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
        *state = StakeSessionStoryState::default();
    }
}
