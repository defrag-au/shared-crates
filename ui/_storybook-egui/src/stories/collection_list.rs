//! Story: `CollectionList` — the per-client collections list rendered on
//! the admin portal dashboard. Covers the realistic mix: fresh draft,
//! mid-mint ingesting, live with a real fill %, plus standard/network
//! variants (cip25 / cip68, preprod / mainnet) and the 2-column grid.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::collection_list::{
    CollectionList, CollectionListAction, CollectionListLayout, CollectionRow,
};
use egui_widgets::wallet_list::{WalletPoolBadge, WalletPoolBadgeHealth};

#[derive(Default)]
pub struct CollectionListState {
    /// Last test-mint action observed — proves the action channel is wired.
    pub last_test_mint: Option<String>,
    /// Last seed-stubs action observed.
    pub last_seed_stubs: Option<String>,
    /// Last activity action observed.
    pub last_activity: Option<String>,
    /// Last open-wallet action observed.
    pub last_open_wallet: Option<u32>,
    /// Last refuel action observed.
    pub last_refuel: Option<u32>,
}

/// Construct a sample row. The widget does no truncation itself — the
/// host pre-formats `policy_id_short` so the truncation strategy
/// (prefix/suffix widths) lives where the data lives.
#[allow(clippy::too_many_arguments)]
fn row(
    policy_id: &str,
    wallet_account_index: u32,
    title: &str,
    status: &str,
    standard: &str,
    network: &str,
    total_supply: u64,
    minted_count: u64,
) -> CollectionRow {
    CollectionRow {
        policy_id: policy_id.to_string(),
        policy_id_short: truncate_middle(policy_id, 8, 6),
        wallet_account_index,
        title: title.to_string(),
        status: status.to_string(),
        standard: standard.to_string(),
        network: network.to_string(),
        total_supply,
        minted_count,
        test_mint_open: false,
        seed_stubs_open: false,
        activity_open: false,
        wallet_address_short: None,
        wallet_address_full: None,
        pool: None,
    }
}

/// Construct a sample row pre-populated with a collection-wallet address
/// and a pool badge — exercises the Refuel-on-card variant.
#[allow(clippy::too_many_arguments)]
fn row_with_wallet(
    policy_id: &str,
    wallet_account_index: u32,
    title: &str,
    status: &str,
    standard: &str,
    network: &str,
    total_supply: u64,
    minted_count: u64,
    address: &str,
    pool: WalletPoolBadge,
) -> CollectionRow {
    let mut r = row(
        policy_id,
        wallet_account_index,
        title,
        status,
        standard,
        network,
        total_supply,
        minted_count,
    );
    r.wallet_address_short = Some(truncate_middle(address, 14, 8));
    r.wallet_address_full = Some(address.to_string());
    r.pool = Some(pool);
    r
}

fn truncate_middle(s: &str, prefix: usize, suffix: usize) -> String {
    if s.len() <= prefix + suffix + 1 {
        return s.to_string();
    }
    let p: String = s.chars().take(prefix).collect();
    let q: String = s
        .chars()
        .rev()
        .take(suffix)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{p}…{q}")
}

pub fn show(ui: &mut egui::Ui, state: &mut CollectionListState) {
    ui.label(
        egui::RichText::new(
            "Per-client collections list. Each card surfaces the title, status, \
             standard, network, supply progress, and a copy-able policy_id. \
             Action buttons fire `CollectionListAction` events; the parent owns \
             the forms below.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(16.0);

    // ── Variant 1: fresh draft, no inventory yet ───────────────────────
    ui.label(
        egui::RichText::new("Fresh draft — supply target set, no mints yet")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Right after `+ Create collection`. Status is `draft`, mint count is 0. \
             The supply bar is empty; the copy button on the policy_id is the \
             primary value of the card for `mintctl clone-policy` operators.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![row(
        "7f2b6f15a4c91c2d8e6a3b9f5d2e8c1a4b7d6e9f2c3a5b8d7e0f1c9a8922e0",
        4,
        "Foobar",
        "draft",
        "cip25",
        "cardano:preprod",
        1000,
        0,
    )];
    let resp = CollectionList::new(&rows)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 2: mid-mint, ingesting ─────────────────────────────────
    ui.label(
        egui::RichText::new("Mid-mint — ingesting with a partial fill")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The supply bar fills as `minted_count / total_supply`. Status chip \
             colour-codes the lifecycle phase (ingesting / ready / live / paused).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            "7f2b6f15a4c91c2d8e6a3b9f5d2e8c1a4b7d6e9f2c3a5b8d7e0f1c9a8922e0",
            4,
            "Foobar",
            "ingesting",
            "cip25",
            "cardano:preprod",
            1000,
            340,
        ),
        row(
            "a8d4e1c7b2f5a9d3c6e0b4f7a1c8e2d5b9a6c3f0e7d4b1a8c5e2f9b6a3d0e7",
            5,
            "Black Flag (preprod)",
            "ready",
            "cip25",
            "cardano:preprod",
            500,
            0,
        ),
    ];
    let resp = CollectionList::new(&rows)
        .with_columns(2)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 3: live + cip68 ────────────────────────────────────────
    ui.label(
        egui::RichText::new("Live mint — CIP-68 with progress")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Live status drives the bar fill to green. CIP-68 standard chip is a \
             distinct soft-teal to distinguish from CIP-25's soft-purple.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![row(
        "5c3a8e4d2f1b7a9c6e0d3b5f8a2c4e7d1b9f6a3c0e5d8b2f7a4c1e6d3b9f0a8",
        7,
        "Live Collection",
        "live",
        "cip68",
        "cardano:mainnet",
        2000,
        1247,
    )];
    let resp = CollectionList::new(&rows)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 4: 2-column grid, mixed statuses ───────────────────────
    ui.label(
        egui::RichText::new("Mixed — 2-column grid across the lifecycle")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "`with_columns(2)` packs cards into a grid. Status chips give a \
             scannable summary — draft / ingesting / ready / live / paused / \
             sold_out / ended each have their own palette.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            "7f2b6f15a4c91c2d8e6a3b9f5d2e8c1a4b7d6e9f2c3a5b8d7e0f1c9a8922e0",
            4,
            "Foobar",
            "draft",
            "cip25",
            "cardano:preprod",
            1000,
            0,
        ),
        row(
            "a8d4e1c7b2f5a9d3c6e0b4f7a1c8e2d5b9a6c3f0e7d4b1a8c5e2f9b6a3d0e7",
            5,
            "Black Flag (preprod)",
            "ingesting",
            "cip25",
            "cardano:preprod",
            500,
            120,
        ),
        row(
            "5c3a8e4d2f1b7a9c6e0d3b5f8a2c4e7d1b9f6a3c0e5d8b2f7a4c1e6d3b9f0a8",
            7,
            "Live Collection",
            "live",
            "cip68",
            "cardano:mainnet",
            2000,
            1247,
        ),
        row(
            "9b7a3c1e5d8f2a4c6e9d1b3f7a5c8e0d2b4f6a9c1e3d5b7f0a8c2e4d6b9f1c3",
            6,
            "Paused Drop",
            "paused",
            "cip25",
            "cardano:mainnet",
            10000,
            6432,
        ),
        row(
            "1e3d5b7f0a8c2e4d6b9f1c3a5e7d9b1f3a5c7e9d1b3f5a7c9e1d3b5f7a9c1e3",
            8,
            "Sold Out Genesis",
            "sold_out",
            "cip68",
            "cardano:mainnet",
            333,
            333,
        ),
        row(
            "0a2c4e6d8b0f2a4c6e8d0b2f4a6c8e0d2b4f6a8c0e2d4b6f8a0c2e4d6b8f0a2",
            9,
            "Ended Mint",
            "ended",
            "cip25",
            "cardano:mainnet",
            10000,
            8413,
        ),
    ];
    let resp = CollectionList::new(&rows)
        .with_columns(2)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 5: form-open toggles ───────────────────────────────────
    ui.label(
        egui::RichText::new("Toggle states — Test mint / Seed stubs open")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "When the parent has a form open for a collection, it sets \
             `test_mint_open` / `seed_stubs_open` on the row VM. The widget \
             renders the button label as `− Test mint` instead of `🧪 Test mint` \
             for a clear close-affordance.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let mut rows = vec![
        row(
            "7f2b6f15a4c91c2d8e6a3b9f5d2e8c1a4b7d6e9f2c3a5b8d7e0f1c9a8922e0",
            4,
            "Foobar (test-mint open)",
            "ready",
            "cip25",
            "cardano:preprod",
            1000,
            0,
        ),
        row(
            "a8d4e1c7b2f5a9d3c6e0b4f7a1c8e2d5b9a6c3f0e7d4b1a8c5e2f9b6a3d0e7",
            5,
            "Black Flag (seed-stubs open)",
            "draft",
            "cip25",
            "cardano:preprod",
            500,
            0,
        ),
    ];
    rows[0].test_mint_open = true;
    rows[1].seed_stubs_open = true;
    let resp = CollectionList::new(&rows)
        .with_columns(2)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 6: collection-centric — wallet + pool + Refuel ─────────
    ui.label(
        egui::RichText::new("Collection-centric — wallet sub-line + Refuel")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The collection card is the primary surface for everything wallet-shaped \
             that belongs to a collection. With `with_refuel(true)` plus a row that \
             ships `wallet_address_*` and a `pool` badge, the card grows a wallet \
             sub-line: clickable wallet # · address · 📋 · ● fuel/ADA · ⛽ Refuel. \
             Refuel copies the address to the clipboard and emits an action so the \
             host can flash a toast or open the testnet faucet.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row_with_wallet(
            "7f2b6f15a4c91c2d8e6a3b9f5d2e8c1a4b7d6e9f2c3a5b8d7e0f1c9a8922e0",
            4,
            "Foobar",
            "ready",
            "cip25",
            "cardano:preprod",
            1000,
            120,
            "addr_test1vpqxc4dpv2lcw4w98v7y7r9p5m0z3qc9x8f6gj2v0nq4t9sucgmds",
            WalletPoolBadge {
                fuel_count: 22,
                total_lovelace: 245_000_000,
                health: WalletPoolBadgeHealth::Healthy,
            },
        ),
        row_with_wallet(
            "a8d4e1c7b2f5a9d3c6e0b4f7a1c8e2d5b9a6c3f0e7d4b1a8c5e2f9b6a3d0e7",
            5,
            "Black Flag (preprod)",
            "live",
            "cip25",
            "cardano:preprod",
            500,
            340,
            "addr_test1vrm9k2e5xz3l0c8q7t2p6r4n3a1y0w9v8u7s5h2g0f9d8b6jq2fxzc",
            WalletPoolBadge {
                fuel_count: 4,
                total_lovelace: 38_000_000,
                health: WalletPoolBadgeHealth::Low,
            },
        ),
        row_with_wallet(
            "5c3a8e4d2f1b7a9c6e0d3b5f8a2c4e7d1b9f6a3c0e5d8b2f7a4c1e6d3b9f0a8",
            7,
            "Empty Pool",
            "ingesting",
            "cip68",
            "cardano:preprod",
            333,
            0,
            "addr_test1vp7l0c8q7t2p6r4n3a1y0w9v8u7s5h2g0f9d8b6jq2fxzcrm9k2e5xz",
            WalletPoolBadge {
                fuel_count: 0,
                total_lovelace: 0,
                health: WalletPoolBadgeHealth::Empty,
            },
        ),
    ];
    let resp = CollectionList::new(&rows)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .with_activity(true)
        .with_refuel(true)
        .show(ui);
    capture_actions(resp.actions, state);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 7: list layout ─────────────────────────────────────────
    ui.label(
        egui::RichText::new("List layout — compact for dense surfaces")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "`with_layout(List)` collapses each collection to a single horizontal \
             row — same chips, no supply bar, no separate footer. Good for an \
             admin index view of dozens of collections.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let _ = CollectionList::new(&rows)
        .with_layout(CollectionListLayout::List)
        .with_test_mint(true)
        .with_seed_stubs(true)
        .with_refuel(true)
        .show(ui);

    // ── Action receipt ─────────────────────────────────────────────────
    if state.last_test_mint.is_some()
        || state.last_seed_stubs.is_some()
        || state.last_open_wallet.is_some()
        || state.last_refuel.is_some()
    {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Action receipts")
                .color(ACCENT)
                .strong()
                .small(),
        );
        if let Some(p) = &state.last_test_mint {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                format!("✓ test-mint requested for {}", truncate_middle(p, 8, 6)),
            );
        }
        if let Some(p) = &state.last_seed_stubs {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                format!("✓ seed-stubs requested for {}", truncate_middle(p, 8, 6)),
            );
        }
        if let Some(idx) = state.last_open_wallet {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                format!("✓ open-wallet requested for #{idx}"),
            );
        }
        if let Some(idx) = state.last_refuel {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                format!("✓ refuel requested for #{idx} (address copied)"),
            );
        }
    }
}

fn capture_actions(actions: Vec<CollectionListAction>, state: &mut CollectionListState) {
    for a in actions {
        match a {
            CollectionListAction::TestMint { policy_id } => {
                state.last_test_mint = Some(policy_id);
            }
            CollectionListAction::SeedStubs { policy_id } => {
                state.last_seed_stubs = Some(policy_id);
            }
            CollectionListAction::Activity { policy_id } => {
                state.last_activity = Some(policy_id);
            }
            CollectionListAction::OpenWallet { account_index } => {
                state.last_open_wallet = Some(account_index);
            }
            CollectionListAction::Refuel { account_index } => {
                state.last_refuel = Some(account_index);
            }
        }
    }
}
