use crate::app::TEXT_MUTED;
use cardano_assets::resolver::{NullResolver, PolicyResolver};
use egui_widgets::egui_inbox::{UiInbox, UiInboxSender};
use egui_widgets::{classify_utxos, ShelfConfig, ShelfData, ShelfState, WalletAction};
use std::collections::HashMap;

/// Messages from async wallet operations.
enum WalletMsg {
    Connected(
        Result<
            (
                egui_widgets::wallet::WalletConnectResult,
                egui_widgets::wallet::WalletApi,
            ),
            String,
        >,
    ),
    UtxosFetched(Result<Vec<cardano_assets::utxo::UtxoApi>, String>),
}

pub struct UtxoMapStoryState {
    shelf_state: ShelfState,
    data: Option<ShelfData>,
    // Async inbox for wallet messages
    inbox: UiInbox<WalletMsg>,
    sender: UiInboxSender<WalletMsg>,
    fetching: bool,
    fetch_error: Option<String>,
    raw_utxo_count: usize,
}

impl Default for UtxoMapStoryState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        Self {
            shelf_state: ShelfState::default(),
            data: None,
            inbox,
            sender,
            fetching: false,
            fetch_error: None,
            raw_utxo_count: 0,
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut UtxoMapStoryState,
    wallet_btn: &mut egui_widgets::WalletButton,
    connector: &mut egui_widgets::wallet::WalletConnector,
) {
    // Drain inbox for async wallet messages
    for msg in state.inbox.read(ui) {
        match msg {
            WalletMsg::Connected(Ok((result, api))) => {
                connector.apply_connect_result(result);
                connector.api = Some(api);
            }
            WalletMsg::Connected(Err(e)) => {
                connector.set_error(e.clone());
                state.fetch_error = Some(e);
            }
            WalletMsg::UtxosFetched(Ok(utxos)) => {
                state.fetching = false;
                state.raw_utxo_count = utxos.len();
                // Mainnet coinsPerUTxOByte
                state.data = Some(classify_utxos(&utxos, 4310));
                state.shelf_state = ShelfState::default();
                state.fetch_error = None;
            }
            WalletMsg::UtxosFetched(Err(e)) => {
                state.fetching = false;
                state.fetch_error = Some(e);
            }
        }
    }

    // --- Wallet connect ---
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(15, 15, 30))
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_max_width(220.0);
            let action = wallet_btn.show(ui, connector);
            match action {
                WalletAction::Connect(provider) => {
                    connector.set_connecting();
                    let sender = state.sender.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result = egui_widgets::wallet::connect_wallet(provider).await;
                        let _ = sender.send(WalletMsg::Connected(result));
                    });
                }
                WalletAction::Disconnect => {
                    connector.disconnect();
                    state.data = None;
                    state.fetch_error = None;
                }
                WalletAction::None => {}
            }
        });

    ui.add_space(8.0);

    if connector.is_connected() {
        ui.horizontal(|ui| {
            let fetch_enabled = connector.has_api() && !state.fetching;
            if ui
                .add_enabled(
                    fetch_enabled,
                    egui::Button::new(if state.fetching {
                        "Fetching..."
                    } else {
                        "Fetch UTxOs"
                    }),
                )
                .clicked()
            {
                state.fetching = true;
                state.fetch_error = None;
                let api = connector.api.clone().unwrap();
                let sender = state.sender.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = egui_widgets::wallet::fetch_wallet_utxos(&api).await;
                    let _ = sender.send(WalletMsg::UtxosFetched(result));
                });
            }

            if let Some(ref addr) = connector.stake_address {
                ui.label(
                    egui::RichText::new(addr)
                        .color(TEXT_MUTED)
                        .monospace()
                        .size(10.0),
                );
            }
        });
    } else if connector.is_connecting() {
        ui.label(
            egui::RichText::new("Connecting...")
                .color(TEXT_MUTED)
                .small(),
        );
    } else {
        ui.label(
            egui::RichText::new("Connect a wallet to fetch real UTxO data")
                .color(TEXT_MUTED)
                .small(),
        );
    }

    if let Some(ref err) = state.fetch_error {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("Error: {err}"))
                .color(egui_widgets::theme::ACCENT_RED)
                .small(),
        );
    }

    ui.add_space(12.0);

    // --- Shelf rendering ---
    if let Some(ref data) = state.data {
        // Stats summary
        let utxo_count = data.utxos.len();
        let policy_count = {
            let mut pols: Vec<&str> = data
                .utxos
                .iter()
                .flat_map(|u| u.policies.iter().map(|(pid, _)| pid.as_str()))
                .collect();
            pols.sort_unstable();
            pols.dedup();
            pols.len()
        };
        let total_ada = data.total_lovelace as f64 / 1_000_000.0;
        let spendable_ada = data.spendable_lovelace as f64 / 1_000_000.0;

        let collateral_icon = if data.has_collateral { "yes" } else { "NO" };
        let collateral_color = if data.has_collateral {
            egui_widgets::theme::ACCENT_GREEN
        } else {
            egui_widgets::theme::ACCENT_RED
        };

        ui.horizontal(|ui| {
            ui.label(format!(
                "{utxo_count} UTxOs, {policy_count} policies, {total_ada:.2} ADA total, {spendable_ada:.2} ADA spendable"
            ));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("Collateral: {collateral_icon}"))
                    .color(collateral_color)
                    .strong()
                    .size(11.0),
            );
        });
        ui.add_space(8.0);

        let config = ShelfConfig {
            width: ui.available_width().min(700.0),
            ..Default::default()
        };
        let _response = config.show(ui, data, &mut state.shelf_state);

        // Info bar — one-line hover summary (no tooltip, no flicker)
        ui.add_space(2.0);
        if let Some(ref hovered_ref) = state.shelf_state.hovered_utxo {
            if let Some(utxo) = data.utxos.iter().find(|u| u.utxo_ref == *hovered_ref) {
                let ada = utxo.lovelace as f64 / 1_000_000.0;
                let policy_info = if utxo.policies.is_empty() {
                    "Pure ADA".to_string()
                } else {
                    let total_assets: u64 = utxo.policies.iter().map(|(_, c)| *c).sum();
                    format!("{} policies, {total_assets} assets", utxo.policies.len())
                };
                let datum_info = if utxo.has_tag(cardano_assets::utxo::UtxoTag::HasDatum) {
                    " [datum]"
                } else if utxo.has_tag(cardano_assets::utxo::UtxoTag::ScriptAddress) {
                    " [script addr]"
                } else {
                    ""
                };
                ui.label(
                    egui::RichText::new(format!(
                        "{} \u{2014} {ada:.2} ADA \u{2014} {} \u{2014} {policy_info}{datum_info}",
                        egui_widgets::truncate_hex(&utxo.utxo_ref, 8, 6),
                        utxo.tier.label(),
                    ))
                    .monospace()
                    .size(10.0)
                    .color(utxo.tier.color()),
                );
            }
        } else {
            // Reserve space so layout doesn't jump
            ui.label(egui::RichText::new(" ").monospace().size(10.0));
        }

        // Detail panel — shown when a block is selected
        if let Some(ref selected_ref) = state.shelf_state.selected_utxo.clone() {
            if let Some(utxo) = data.utxos.iter().find(|u| u.utxo_ref == *selected_ref) {
                // TODO: replace with a real resolver once the resolver service is wired up
                let resolver: &dyn PolicyResolver = &NullResolver;

                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(egui_widgets::theme::BG_SECONDARY)
                    .inner_margin(egui::Margin::same(12))
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        // UTxO ref (full) + close button
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&utxo.utxo_ref)
                                    .monospace()
                                    .size(10.0)
                                    .color(egui_widgets::theme::TEXT_SECONDARY),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("\u{2715}").clicked() {
                                        state.shelf_state.selected_utxo = None;
                                    }
                                },
                            );
                        });

                        // ADA amount
                        let ada = utxo.lovelace as f64 / 1_000_000.0;
                        ui.label(
                            egui::RichText::new(format!("{ada:.6} ADA"))
                                .size(16.0)
                                .strong()
                                .color(utxo.tier.color()),
                        );

                        // Tier badge + description
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(utxo.tier.label())
                                    .size(11.0)
                                    .strong()
                                    .color(utxo.tier.color()),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{2014} {}",
                                    utxo.tier.description()
                                ))
                                .size(10.0)
                                .color(TEXT_MUTED),
                            );
                        });

                        // UTxO tag indicators
                        if !utxo.tags.is_empty() {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                for tag in &utxo.tags {
                                    let label = match tag {
                                        cardano_assets::utxo::UtxoTag::HasDatum => "Datum",
                                        cardano_assets::utxo::UtxoTag::HasScriptRef => "Script Ref",
                                        cardano_assets::utxo::UtxoTag::ScriptAddress => {
                                            "Script Address"
                                        }
                                    };
                                    ui.label(
                                        egui::RichText::new(label)
                                            .size(10.0)
                                            .color(egui_widgets::theme::ACCENT_ORANGE),
                                    );
                                }
                            });
                        }

                        // Per-policy asset breakdown
                        if !utxo.assets.is_empty() {
                            ui.add_space(6.0);

                            // Group assets by policy
                            let mut by_policy: HashMap<
                                &str,
                                Vec<&cardano_assets::utxo::AssetQuantity>,
                            > = HashMap::new();
                            for aq in &utxo.assets {
                                by_policy
                                    .entry(aq.asset_id.policy_id.as_str())
                                    .or_default()
                                    .push(aq);
                            }

                            // Sort policies to match the shelf order
                            let mut policy_ids: Vec<&str> = by_policy.keys().copied().collect();
                            policy_ids.sort();

                            let total_assets: usize = utxo.assets.len();
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} policies, {total_assets} assets",
                                    policy_ids.len()
                                ))
                                .size(10.0)
                                .color(TEXT_MUTED),
                            );

                            for pid in &policy_ids {
                                let assets = &by_policy[pid];
                                let color = egui_widgets::utxo_map::policy_color(pid);

                                ui.add_space(4.0);

                                // Policy header: swatch + resolved name or full policy ID + token type
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(
                                        egui::vec2(8.0, 8.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(r, 2.0, color);

                                    if let Some(resolved) = resolver.resolve(pid) {
                                        // Resolved: show name + token type badge
                                        ui.label(
                                            egui::RichText::new(&resolved.name)
                                                .size(10.0)
                                                .strong()
                                                .color(egui_widgets::theme::TEXT_SECONDARY),
                                        );
                                        ui.label(
                                            egui::RichText::new(resolved.token_type.label())
                                                .size(9.0)
                                                .color(TEXT_MUTED),
                                        );
                                        if resolved.is_verified() {
                                            ui.label(
                                                egui::RichText::new("\u{2713}")
                                                    .size(9.0)
                                                    .color(egui_widgets::theme::ACCENT_GREEN),
                                            );
                                        }
                                        if resolved.has_warnings() {
                                            for tag in &resolved.tags {
                                                if tag.is_warning() {
                                                    ui.label(
                                                        egui::RichText::new(tag.label())
                                                            .size(9.0)
                                                            .strong()
                                                            .color(egui_widgets::theme::ACCENT_RED),
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        // Unresolved: show full policy ID
                                        ui.label(
                                            egui::RichText::new(*pid)
                                                .monospace()
                                                .size(9.0)
                                                .color(egui_widgets::theme::TEXT_SECONDARY),
                                        );
                                    }

                                    ui.label(
                                        egui::RichText::new(format!("({} assets)", assets.len()))
                                            .size(9.0)
                                            .color(TEXT_MUTED),
                                    );
                                });

                                // Individual assets
                                for aq in assets {
                                    let name = aq.asset_id.asset_name();
                                    let qty = aq.quantity;
                                    ui.horizontal(|ui| {
                                        ui.add_space(16.0); // indent
                                        let qty_label = if qty == 1 {
                                            String::new()
                                        } else {
                                            format!(" x{qty}")
                                        };
                                        ui.label(
                                            egui::RichText::new(format!("{name}{qty_label}"))
                                                .monospace()
                                                .size(9.0)
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                }
                            }
                        } else {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Pure ADA \u{2014} no native assets")
                                    .size(10.0)
                                    .color(TEXT_MUTED),
                            );
                        }
                    });
            }
        }
    } else {
        ui.add_space(40.0);
        ui.label(
            egui::RichText::new("Connect a wallet and fetch UTxOs to visualize").color(TEXT_MUTED),
        );
    }
}
