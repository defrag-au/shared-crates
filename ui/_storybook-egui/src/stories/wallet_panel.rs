//! `WalletPanel` story — the wallet summary, and the manager behind it.
//!
//! The whole flow is live: `Manage` opens the manager, the roster inside it
//! adds, resolves, links and drops rows for real, and the summary above it
//! re-totals every frame. Nothing here is a screenshot of a state — the host
//! half (the actions) is a mock, and everything the widget itself does is real.

use egui_widgets::wallet::{WalletConnector, WalletInfo};
use egui_widgets::wallet_editor::{Submission, WalletEditorEntry, WalletEntryStatus, WalletOrigin};
use egui_widgets::{WalletPanel, WalletPanelAction, WalletPanelState};

use crate::{controls, muted};

pub struct WalletPanelStoryState {
    panel: WalletPanelState,
    entries: Vec<WalletEditorEntry>,
    /// Seeded with detected wallets, because a headless browser has none — the
    /// picker is the part of the manager worth looking at.
    connector: WalletConnector,
    /// What the app was told to do, in the app's own words.
    last_action: String,
    /// Rows still "resolving", as `(index, frames remaining)`.
    pending: Vec<(usize, u32)>,
    /// Rows mid-link: `(index, frames remaining)`. The host's real flow waits on
    /// a wallet dialog, which takes as long as the reader takes.
    linking: Vec<(usize, u32)>,
    /// Frames left on the mock connect, standing in for the wallet dialog.
    connecting: Option<u32>,
    sidebar_w: f32,
}

impl Default for WalletPanelStoryState {
    fn default() -> Self {
        let mut connector = WalletConnector::new();
        connector.available_wallets = vec![
            detected("eternl", "Eternl"),
            detected("vespr", "VESPR"),
            detected("brave", "Brave"),
        ];
        Self {
            panel: WalletPanelState::default(),
            entries: mock_entries(),
            connector,
            last_action: String::new(),
            pending: Vec::new(),
            linking: Vec::new(),
            connecting: None,
            sidebar_w: 320.0,
        }
    }
}

fn detected(api_name: &str, name: &str) -> WalletInfo {
    WalletInfo {
        api_name: api_name.into(),
        name: name.into(),
        // The extensions hand over a base64 icon; a story has none, and the
        // picker falls back to the name alone.
        icon: None,
    }
}

/// A roster that shows every mark the summary draws: both chains, all three
/// origins, a resolving row and a failed one.
fn mock_entries() -> Vec<WalletEditorEntry> {
    vec![
        WalletEditorEntry::resolving("stake1u8boef")
            .handle("boef")
            .status(WalletEntryStatus::Ready),
        WalletEditorEntry::resolving("stake1u8djo")
            .handle("djo")
            .status(WalletEntryStatus::Ready),
        // The browser wallet the app found and can prove.
        WalletEditorEntry::resolving("stake1u8perplord")
            .handle("perplord")
            .status(WalletEntryStatus::Ready)
            .origin(WalletOrigin::Browser)
            .linkable(true),
        // A signature the server holds.
        WalletEditorEntry::resolving("stake1u8hodler")
            .handle("hodler")
            .status(WalletEntryStatus::Ready)
            .origin(WalletOrigin::Linked),
        // An owner string from another chain — the same roster, a different one.
        WalletEditorEntry::resolving("0x7980aa64093853cb78c927e05b88fed96e945f81")
            .status(WalletEntryStatus::Ready)
            .linkable(true),
        WalletEditorEntry::resolving("$curiousfutures"),
        WalletEditorEntry::resolving("stake1qy2ffk39dj2mmz8tt5lq0wgc3xn7v4hp9k9fp")
            .status(WalletEntryStatus::Failed("no such handle".into())),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletPanelStoryState) {
    crate::heading(ui, "WalletPanel");
    crate::caption(
        ui,
        "The roster as a summary: one mark per wallet, the chains it reaches, and \
         how many are proven. Everything the reader can DO with it — connect, add, \
         link, remove — is behind `Manage`, in the popup.",
    );
    crate::caption(
        ui,
        "The marks are the two systems the rows already use: tinted by where the \
         wallet came from (blue typed, cyan browser, green linked), hollow while \
         it resolves, red if it failed. A strip that cannot disagree with the rows \
         it summarises is the whole point of deriving rather than storing it.",
    );
    ui.add_space(8.0);

    // The mock host: timers instead of the network, and the same action shape a
    // real host answers.
    tick_resolutions(ui, state);

    crate::caption(
        ui,
        "Drag the width to see what the summary does where it lives: a 320pt \
         sidebar, and a phone.",
    );
    controls(ui, |ui| {
        egui_widgets::slider_group::SliderGroup::new()
            .fader(
                egui_widgets::slider_group::Fader::new(
                    "sidebar",
                    &mut state.sidebar_w,
                    160.0..=520.0,
                )
                .suffix("px"),
            )
            .show(ui);
    });
    ui.add_space(8.0);

    let width = state.sidebar_w;
    ui.allocate_ui(egui::vec2(width, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(crate::bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
            .show(ui, |ui| {
                ui.set_width(width - 24.0);
                let config = egui_widgets::WalletEditorConfig::default();
                let resp = WalletPanel::new(&state.entries, &state.connector, &config)
                    .show(ui, &mut state.panel);
                if let Some(action) = resp.action {
                    apply(state, action);
                }
            });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    crate::heading(ui, "What the app was told to do");
    crate::caption(
        ui,
        "The widget never writes, spawns or persists — every change above left as \
         an action for the host. That is the same contract `WalletEditor` and \
         `WalletButton` keep, which is what lets this story drive the whole flow \
         with a mock and a real app answer it with fetches and a session.",
    );
    ui.add_space(6.0);
    let note = match state.last_action.is_empty() {
        true => "— nothing yet".to_string(),
        false => state.last_action.clone(),
    };
    ui.label(egui::RichText::new(note).color(muted(ui)).small());
}

fn tick_resolutions(ui: &mut egui::Ui, state: &mut WalletPanelStoryState) {
    // The connect, on a timer: a wallet dialog takes as long as the reader takes,
    // and the `Connecting…` strip is a state worth being able to see.
    if let Some(frames) = &mut state.connecting {
        ui.ctx().request_repaint();
        *frames = frames.saturating_sub(1);
        if *frames == 0 {
            state.connecting = None;
            let identity = egui_widgets::wallet_mock::alice();
            egui_widgets::wallet_mock::connect(&mut state.connector, &identity);
            state
                .connector
                .apply_balance(egui_widgets::wallet_mock::balance(&identity));
            // The browser wallet is a row in the roster too — the real host adds
            // it when the async connect lands, and the summary counts it.
            if !state
                .entries
                .iter()
                .any(|e| e.origin == WalletOrigin::Browser)
            {
                state.entries.push(
                    WalletEditorEntry::resolving(identity.stake.clone())
                        .handle(
                            identity
                                .handle
                                .clone()
                                .unwrap_or_default()
                                .trim_start_matches('$'),
                        )
                        .status(WalletEntryStatus::Ready)
                        .origin(WalletOrigin::Browser)
                        .linkable(true),
                );
            }
        }
    }

    let mut finished: Vec<usize> = Vec::new();
    for (idx, frames) in &mut state.pending {
        *frames = frames.saturating_sub(1);
        if *frames == 0 {
            finished.push(*idx);
        }
    }
    if !state.pending.is_empty() {
        ui.ctx().request_repaint();
    }
    state.pending.retain(|(_, f)| *f > 0);
    for idx in finished {
        if let Some(e) = state.entries.get_mut(idx) {
            e.status = match idx % 3 == 2 {
                true => WalletEntryStatus::Failed("no such handle".into()),
                false => WalletEntryStatus::Ready,
            };
            if e.handle.is_none() && !e.key.starts_with("stake1") && !e.key.starts_with("0x") {
                let key = e.key.clone();
                e.handle = Some(key);
            }
        }
    }

    // Tick the mock links — a wallet dialog's worth of waiting, then the proof.
    let mut linked: Vec<usize> = Vec::new();
    for (idx, frames) in &mut state.linking {
        *frames = frames.saturating_sub(1);
        if *frames == 0 {
            linked.push(*idx);
        }
    }
    if !state.linking.is_empty() {
        ui.ctx().request_repaint();
    }
    state.linking.retain(|(_, f)| *f > 0);
    for idx in linked {
        if let Some(e) = state.entries.get_mut(idx) {
            e.origin = WalletOrigin::Linked;
            e.linkable = false;
            e.status = WalletEntryStatus::Ready;
        }
    }
}

/// The mock host's write side. A real one spawns a fetch or a wallet dialog per
/// arm; here each arm lands in the note under the panel.
fn apply(state: &mut WalletPanelStoryState, action: WalletPanelAction) {
    match action {
        WalletPanelAction::Connect(provider) => {
            state.last_action = format!(
                "Connect {provider:?} — the host runs the CIP-30 dialog and reports \
                 the result back"
            );
            state.connector.set_connecting();
            state.connecting = Some(60);
        }
        WalletPanelAction::Disconnect => {
            state.last_action = "Disconnect — the host drops the session and the row \
                                 it seeded, but never a LINKED row"
                .to_string();
            state.connector.disconnect();
        }
        WalletPanelAction::Editor(action) => apply_editor(state, action),
    }
}

fn apply_editor(state: &mut WalletPanelStoryState, action: egui_widgets::WalletEditorAction) {
    use egui_widgets::WalletEditorAction as A;
    match action {
        A::Add(sub) => {
            state.last_action = match &sub {
                Submission::Handle(h) => format!("Add handle ${h}"),
                Submission::StakeAddress(a) => format!("Add stake address {a}"),
                Submission::PaymentAddress(a) => {
                    format!("Add PAYMENT address {a} — refused, and the row says why")
                }
                Submission::Account(a) => format!("Add EVM account {a}"),
            };
            let entry = match &sub {
                Submission::Handle(h) => WalletEditorEntry::resolving(h.clone()),
                Submission::StakeAddress(a) | Submission::PaymentAddress(a) => {
                    WalletEditorEntry::resolving(a.clone()).status(WalletEntryStatus::Loading)
                }
                Submission::Account(a) => {
                    WalletEditorEntry::resolving(a.clone()).status(WalletEntryStatus::Loading)
                }
            };
            state.entries.push(entry);
            state.pending.push((state.entries.len() - 1, 90));
        }
        A::Link(idx) => {
            state.last_action = format!(
                "Link [{idx}] {} — challenge → sign → verify, then the server's list \
                 comes back",
                state.entries.get(idx).map_or("?".into(), |e| e.display())
            );
            if let Some(e) = state.entries.get_mut(idx) {
                e.status = WalletEntryStatus::Loading;
            }
            state.linking.push((idx, 60));
        }
        A::Unlink(idx) => {
            state.last_action = format!(
                "Unlink [{idx}] {} — the server releases it; dropping the row here \
                 would leave it standing there",
                state.entries.get(idx).map_or("?".into(), |e| e.display())
            );
            if let Some(e) = state.entries.get_mut(idx) {
                e.origin = WalletOrigin::Typed;
                e.linkable = true;
            }
        }
        A::Remove(idx) => {
            state.last_action = format!(
                "Remove [{idx}] {}",
                state.entries.get(idx).map_or("?".into(), |e| e.display())
            );
            if idx < state.entries.len() {
                state.entries.remove(idx);
                // Indices shift, and a stale one would resolve the wrong row.
                state.pending.clear();
            }
        }
    }
}
