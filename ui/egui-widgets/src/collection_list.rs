//! Collection roster — the per-client collections list rendered on the
//! admin portal dashboard. Sibling to [`crate::wallet_list`] and shares
//! its API shape: VM rows in, action stream out, no widget-owned state.
//!
//! ## Why it's a widget
//!
//! The portal's `render_client_detail` was rendering collections as a
//! single horizontal `for c in collections` line — fine for one
//! collection, falls apart at five. More importantly, the `policy_id`
//! was truncated without a copy affordance, which is exactly what an
//! operator running `mintctl clone-policy` needs to grab. Pushing this
//! into a widget gives us:
//!
//! - card layout that scales to a multi-collection client
//! - per-card `policy_id` truncation + copy-to-clipboard button
//! - per-card supply progress (mint count / total) with a thin bar
//! - per-card action row (Test mint, Seed stubs) wired through a
//!   `CollectionListAction` enum so the parent stays in charge of the
//!   forms below the card
//! - status/standard/network rendered as filled chips with consistent
//!   colours across screens (status from
//!   [`crate::wallet_list::WalletList`]'s `status_colour` lookup)
//!
//! ## What it does NOT do
//!
//! - **No async, no inbox.** Actions are returned in the response struct.
//! - **No state.** Form-open vs form-closed is read from
//!   `CollectionRow::test_mint_open` / `seed_stubs_open` — the parent
//!   owns the truth.
//! - **No "+ Create collection" button.** That stays on the section
//!   header in the parent layout (see `app.rs::render_client_detail`)
//!   so the widget composes cleanly with the parent's header chrome.
//!
//! ## Layouts
//!
//! Two presentations share the same row VM:
//!
//! - [`CollectionListLayout::Card`] (default) — full card with title,
//!   chips, supply bar, footer, action row. Used by the portal.
//! - [`CollectionListLayout::List`] — compact one-row-per-collection;
//!   useful for dense surfaces (e.g. an admin index view).
//!
//! ## Usage
//!
//! ```ignore
//! let rows: Vec<CollectionRow> = detail.collections.iter().map(to_vm).collect();
//! let resp = CollectionList::new(&rows)
//!     .with_columns(2)
//!     .with_test_mint(true)
//!     .with_seed_stubs(true)
//!     .show(ui);
//! for action in resp.actions {
//!     match action {
//!         CollectionListAction::TestMint { policy_id } => { /* dispatch */ }
//!         CollectionListAction::SeedStubs { policy_id } => { /* dispatch */ }
//!         CollectionListAction::OpenWallet { account_index } => { /* dispatch */ }
//!     }
//! }
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::wallet_list::{WalletPoolBadge, WalletPoolBadgeHealth};

// ─────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────

/// View-model for a single collection row. Pre-formatted by the
/// caller — the widget does no truncation, no enum mapping, and no
/// status-string interpretation beyond the chip colour lookup.
#[derive(Clone, Debug)]
pub struct CollectionRow {
    /// Full hex policy_id (56 chars). Written to the clipboard when
    /// the user clicks the copy icon.
    pub policy_id: String,
    /// Pre-truncated `policy_id` for inline display (e.g. middle-elided
    /// to `7f2b6f15…8922e0`). Caller picks the prefix/suffix widths.
    pub policy_id_short: String,
    /// BIP-44 account index of the wallet that owns the mint key for
    /// this collection. Renders as `wallet #N`, clickable to open that
    /// wallet's UTxO window via [`CollectionListAction::OpenWallet`].
    pub wallet_account_index: u32,
    /// Display title (operator-set, e.g. "Foobar").
    pub title: String,
    /// Lifecycle status: `draft` / `ingesting` / `ready` / `live` /
    /// `paused` / `sold_out` / `ended`. Unknown values render neutral
    /// grey. Lowercase by convention; the widget uppercases for display.
    pub status: String,
    /// CIP-25 / CIP-68 standard (lowercase: `cip25` / `cip68`). Drives
    /// the standard-chip colour; unknown values render neutral grey.
    pub standard: String,
    /// Network the collection is provisioned on, in canonical form
    /// (`cardano:preprod` / `cardano:mainnet`). The widget strips the
    /// `cardano:` prefix when rendering the chip.
    pub network: String,
    /// Total minted supply target. Rendered as `minted_count /
    /// total_supply` + a thin progress bar.
    pub total_supply: u64,
    /// Number of assets already minted. The progress bar fills based on
    /// `minted_count / total_supply`.
    pub minted_count: u64,
    /// `true` when the parent has the Test-mint form open below this
    /// row. The widget renders the button label as `− Test mint` so
    /// the user has a clear "close" affordance.
    pub test_mint_open: bool,
    /// `true` when the parent has the Seed-stubs form open below this
    /// row. Same toggle treatment as `test_mint_open`.
    pub seed_stubs_open: bool,
    /// `true` when the parent has the Activity panel open below this
    /// row. Same toggle treatment as the form-open flags.
    pub activity_open: bool,
    /// Pre-truncated wallet address (e.g. middle-elided
    /// `addr_test1vp…9fc465`) for inline display on the card's wallet
    /// sub-line. `None` when the parent hasn't resolved an address yet
    /// (rare — collection-wallet rows ship a derived address on the
    /// first whoami). The widget hides the wallet sub-line entirely
    /// when both `wallet_address_short` and `wallet_address_full` are
    /// `None`, so the card degrades cleanly on older snapshots.
    pub wallet_address_short: Option<String>,
    /// Full, un-truncated address to write to the clipboard when the
    /// user clicks either the row's wallet-address copy icon or the
    /// "Refuel" button. `None` hides both affordances.
    pub wallet_address_full: Option<String>,
    /// Optional fuel-pool summary for the collection's mint wallet —
    /// rendered as `● 20 fuel · 230 ADA` on the card's wallet sub-line.
    /// `None` when the parent hasn't refreshed UTxOs for the wallet
    /// yet (the host populates this from its cached UTxO snapshot).
    pub pool: Option<WalletPoolBadge>,
}

/// Actions emitted while the widget was on screen this frame. Parent
/// drains and dispatches.
#[derive(Clone, Debug)]
pub enum CollectionListAction {
    /// User clicked the Test-mint toggle. The parent decides whether to
    /// open/close the form for this `policy_id`.
    TestMint { policy_id: String },
    /// User clicked the Seed-stubs toggle. Same as above.
    SeedStubs { policy_id: String },
    /// User clicked the Activity toggle — open/close the recent-mint
    /// activity panel for this `policy_id`.
    Activity { policy_id: String },
    /// User clicked the `wallet #N` link in the footer. Parent opens
    /// that wallet's UTxO panel (or whatever it does for wallet focus).
    OpenWallet { account_index: u32 },
    /// User clicked the per-card "Refuel" button. The widget has already
    /// copied the wallet address to the clipboard (matches the in-widget
    /// copy-icon convention); the action exists so the host can flash a
    /// toast / emit telemetry / open a faucet tab if it wants to. Only
    /// emitted when [`CollectionList::with_refuel`] is `true` and the
    /// row supplies `wallet_address_full`.
    Refuel { account_index: u32 },
}

/// Rendering style. Both styles share the same row VM and action emission —
/// only the per-row geometry differs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollectionListLayout {
    /// Taller card with title, chips, supply bar, footer, action row.
    /// Default — the portal's main collections grid.
    #[default]
    Card,
    /// Compact one-row-per-collection. Useful for dense indexes; less
    /// information density loss vs the card's two extra rows.
    List,
}

/// Builder.
pub struct CollectionList<'a> {
    rows: &'a [CollectionRow],
    layout: CollectionListLayout,
    columns: usize,
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_refuel: bool,
}

/// Response — drained actions for this frame.
#[derive(Default, Debug)]
pub struct CollectionListResponse {
    pub actions: Vec<CollectionListAction>,
}

// ─────────────────────────────────────────────────────────────────────
// Colours — kept private. Status palette mirrors the portal's
// previous `status_colour()` helper so the visual semantics survive
// the widget extraction.
// ─────────────────────────────────────────────────────────────────────

const ROW_BG: Color32 = Color32::from_rgb(22, 22, 32);
const ROW_STROKE: Color32 = Color32::from_rgb(40, 40, 56);
const META_GREY: Color32 = Color32::from_gray(140);
const KEYHASH_GREY: Color32 = Color32::from_gray(120);

// Standard chips: CIP-25 = soft purple, CIP-68 = soft teal. Distinct
// enough to scan at a glance; tonal palette matches the wallet-role
// chips (cool blue / soft green / neutral grey).
const STD_CIP25_CHIP: Color32 = Color32::from_rgb(190, 170, 220);
const STD_CIP68_CHIP: Color32 = Color32::from_rgb(170, 220, 200);
const STD_UNKNOWN_CHIP: Color32 = Color32::from_gray(150);

// Network chip is intentionally muted — it's environmental context,
// not the primary identity of the collection.
const NETWORK_CHIP: Color32 = Color32::from_rgb(120, 130, 150);

// Progress bar — base track + filled portion. Fill colour shifts by
// status (live = green, otherwise neutral cyan) so an at-a-glance scan
// distinguishes "in progress" from "actively minting".
const BAR_TRACK: Color32 = Color32::from_rgb(30, 30, 44);
const BAR_FILL_NEUTRAL: Color32 = Color32::from_rgb(120, 160, 200);
const BAR_FILL_LIVE: Color32 = Color32::from_rgb(140, 200, 140);

impl<'a> CollectionList<'a> {
    pub fn new(rows: &'a [CollectionRow]) -> Self {
        Self {
            rows,
            layout: CollectionListLayout::default(),
            columns: 1,
            show_test_mint: false,
            show_seed_stubs: false,
            show_activity: false,
            show_refuel: false,
        }
    }

    /// Switch the per-row presentation. See [`CollectionListLayout`].
    pub fn with_layout(mut self, layout: CollectionListLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Lay rows out as a grid this many columns wide. Default `1`.
    /// Per-bucket clamp: a single-row collection still uses the full
    /// surface width even when 2+ cols are configured — keeps the
    /// layout from leaving a trailing empty cell that reads as a bug.
    pub fn with_columns(mut self, cols: usize) -> Self {
        self.columns = cols.max(1);
        self
    }

    /// Show the per-card "Test mint" button. Off by default since the
    /// host may not have a mint-form to render. Click → emits
    /// [`CollectionListAction::TestMint`].
    pub fn with_test_mint(mut self, enabled: bool) -> Self {
        self.show_test_mint = enabled;
        self
    }

    /// Show the per-card "Seed stubs" button. Off by default. Click →
    /// emits [`CollectionListAction::SeedStubs`]. The portal uses this
    /// for the local testing path; `mintctl clone-policy` replaces it
    /// for realistic inventory but the toggle remains useful for quick
    /// one-off seeding.
    pub fn with_seed_stubs(mut self, enabled: bool) -> Self {
        self.show_seed_stubs = enabled;
        self
    }

    /// Show the per-card "Activity" toggle. Off by default. Click → emits
    /// [`CollectionListAction::Activity`]; host opens/closes the recent
    /// mint-activity panel below the card and polls
    /// `PortalAction::FetchMintActivity` while open.
    pub fn with_activity(mut self, enabled: bool) -> Self {
        self.show_activity = enabled;
        self
    }

    /// Show the per-card "Refuel" button on the wallet sub-line. Off by
    /// default. Click → widget copies `wallet_address_full` to the
    /// clipboard *and* emits [`CollectionListAction::Refuel`] so the
    /// host can flash a toast / log it / open a faucet. Hidden on rows
    /// whose `wallet_address_full` is `None` (nothing to refuel).
    ///
    /// Why on the *collection* card rather than the wallet card: an
    /// operator reasons "this collection needs fuel", not "wallet #4
    /// is low" — the collection is the unit of work. Surfacing fuel
    /// state + the address on the collection card lets a low-fuel
    /// signal trigger the refuel directly, without a hop through the
    /// Wallets section.
    pub fn with_refuel(mut self, enabled: bool) -> Self {
        self.show_refuel = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> CollectionListResponse {
        let mut response = CollectionListResponse::default();

        if self.rows.is_empty() {
            return response;
        }

        // Per-grid clamp: a 1-row layout renders full-width even when
        // 2+ columns are configured.
        let effective_cols = self.columns.min(self.rows.len()).max(1);

        let gap = match self.layout {
            CollectionListLayout::Card => 8.0,
            CollectionListLayout::List => 4.0,
        };

        let chunks: Vec<&[CollectionRow]> = self.rows.chunks(effective_cols).collect();
        let last_chunk = chunks.len().saturating_sub(1);
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            if effective_cols == 1 {
                for r in chunk.iter() {
                    render_one(
                        ui,
                        r,
                        self.layout,
                        self.show_test_mint,
                        self.show_seed_stubs,
                        self.show_activity,
                        self.show_refuel,
                        &mut response,
                    );
                }
            } else {
                ui.columns(effective_cols, |cols| {
                    for (i, r) in chunk.iter().enumerate() {
                        render_one(
                            &mut cols[i],
                            r,
                            self.layout,
                            self.show_test_mint,
                            self.show_seed_stubs,
                            self.show_activity,
                            self.show_refuel,
                            &mut response,
                        );
                    }
                });
            }
            if chunk_idx < last_chunk {
                ui.add_space(gap);
            }
        }

        response
    }
}

// ─────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_one(
    ui: &mut Ui,
    row: &CollectionRow,
    layout: CollectionListLayout,
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_refuel: bool,
    response: &mut CollectionListResponse,
) {
    match layout {
        CollectionListLayout::Card => render_card(
            ui,
            row,
            show_test_mint,
            show_seed_stubs,
            show_activity,
            show_refuel,
            response,
        ),
        CollectionListLayout::List => render_list_row(
            ui,
            row,
            show_test_mint,
            show_seed_stubs,
            show_activity,
            show_refuel,
            response,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_card(
    ui: &mut Ui,
    row: &CollectionRow,
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_refuel: bool,
    response: &mut CollectionListResponse,
) {
    Frame::new()
        .fill(ROW_BG)
        .stroke(Stroke::new(1.0, ROW_STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // ── Title row: title + chips + (right) action buttons ──
            ui.horizontal(|ui| {
                ui.label(RichText::new(&row.title).heading());

                // Status chip — filled tag using the status colour. The
                // chip is the strongest secondary signal after the title,
                // so it sits closest to it.
                status_chip(ui, &row.status);
                standard_chip(ui, &row.standard);
                network_chip(ui, &row.network);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if show_test_mint {
                        let label = if row.test_mint_open {
                            "− Test mint"
                        } else {
                            "🧪 Test mint"
                        };
                        if ui
                            .small_button(RichText::new(label).small())
                            .on_hover_text(
                                "Open the test-mint form for this collection \
                                     (operator/super-admin only)",
                            )
                            .clicked()
                        {
                            response.actions.push(CollectionListAction::TestMint {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                    if show_seed_stubs {
                        let label = if row.seed_stubs_open {
                            "− Seed stubs"
                        } else {
                            "+ Seed stubs"
                        };
                        if ui
                            .small_button(RichText::new(label).small())
                            .on_hover_text(
                                "Seed placeholder mintable assets \
                                     (testing-only — real ingest replaces this)",
                            )
                            .clicked()
                        {
                            response.actions.push(CollectionListAction::SeedStubs {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                    if show_activity {
                        let label = if row.activity_open {
                            "− Activity"
                        } else {
                            "📜 Activity"
                        };
                        if ui
                            .small_button(RichText::new(label).small())
                            .on_hover_text(
                                "Recent mint activity for this collection — \
                                     fetched from the engine's mint_log",
                            )
                            .clicked()
                        {
                            response.actions.push(CollectionListAction::Activity {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                });
            });

            ui.add_space(8.0);

            // ── Supply: text + progress bar ──────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} / {}", row.minted_count, row.total_supply))
                        .monospace()
                        .small()
                        .color(META_GREY),
                );
                let pct = if row.total_supply == 0 {
                    0.0
                } else {
                    (row.minted_count as f32 / row.total_supply as f32).clamp(0.0, 1.0)
                };
                let label = if pct >= 1.0 {
                    "minted out".to_string()
                } else {
                    format!("{:.0}%", pct * 100.0)
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(label).small().color(KEYHASH_GREY));
                });
            });

            // Thin progress bar — caps at the surface width so it
            // mirrors the card chrome.
            ui.add_space(2.0);
            let bar_fill_colour = if row.status == "live" {
                BAR_FILL_LIVE
            } else {
                BAR_FILL_NEUTRAL
            };
            let pct = if row.total_supply == 0 {
                0.0
            } else {
                (row.minted_count as f32 / row.total_supply as f32).clamp(0.0, 1.0)
            };
            draw_thin_bar(ui, pct, bar_fill_colour);

            ui.add_space(10.0);

            // ── Wallet sub-line: address + copy + pool badge + Refuel ─
            //
            // The collection card is the primary surface for everything
            // wallet-shaped that belongs to a collection — address, fuel
            // level, refuel action. Collection wallets don't appear in
            // the parent's "Wallets" section anymore, so this sub-line
            // is the sole place where the operator interacts with the
            // mint wallet for this collection.
            //
            // The `wallet #N` link is kept (left-most) so the
            // collection-↔-wallet relationship is still visible, and
            // clicking it still opens that wallet's UTxO panel.
            ui.horizontal(|ui| {
                let wallet_text = format!("wallet #{}", row.wallet_account_index);
                if ui
                    .small_button(RichText::new(wallet_text).small().color(META_GREY))
                    .on_hover_text("Open this wallet's UTxOs")
                    .clicked()
                {
                    response.actions.push(CollectionListAction::OpenWallet {
                        account_index: row.wallet_account_index,
                    });
                }

                // Inline address (truncated) + copy icon. The copy is
                // done in-widget to match the policy_id pattern below
                // (no host round-trip for a value the row already
                // carries). Hidden when the row didn't supply one —
                // older snapshots or wallets without a derived address.
                if let Some(short) = &row.wallet_address_short {
                    ui.label(RichText::new("·").color(KEYHASH_GREY).monospace().small());
                    ui.label(RichText::new(short).monospace().small().color(KEYHASH_GREY));
                    if let Some(full) = &row.wallet_address_full {
                        if ui
                            .small_button(RichText::new("📋").small())
                            .on_hover_text("Copy wallet address to clipboard")
                            .clicked()
                        {
                            ui.ctx().copy_text(full.clone());
                        }
                    }
                }

                // Pool badge — same shape as the wallet-list card's
                // pool pill so the visual is consistent across surfaces.
                if let Some(pool) = &row.pool {
                    let fg = match pool.health {
                        WalletPoolBadgeHealth::Empty => Color32::from_rgb(220, 130, 130),
                        WalletPoolBadgeHealth::Low => Color32::from_rgb(220, 200, 130),
                        WalletPoolBadgeHealth::Healthy => Color32::from_rgb(150, 210, 160),
                    };
                    ui.label(RichText::new("●").small().color(fg));
                    let ada_whole = pool.total_lovelace / 1_000_000;
                    ui.label(
                        RichText::new(format!("{} fuel · {} ADA", pool.fuel_count, ada_whole))
                            .small()
                            .color(fg),
                    );
                }

                // Refuel — copies the address (silent, matches the
                // other copy icons) and emits an action so the host
                // can flash a toast / open a faucet / log it. Hidden
                // when the row has no `wallet_address_full` — nothing
                // to copy.
                if show_refuel {
                    if let Some(full) = &row.wallet_address_full {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button(RichText::new("⛽ Refuel").small())
                                .on_hover_text(
                                    "Copy this wallet's address to the clipboard so \
                                         you can paste it into the testnet faucet \
                                         (or send ADA from another wallet).",
                                )
                                .clicked()
                            {
                                ui.ctx().copy_text(full.clone());
                                response.actions.push(CollectionListAction::Refuel {
                                    account_index: row.wallet_account_index,
                                });
                            }
                        });
                    }
                }
            });

            ui.add_space(4.0);

            // ── Footer: policy_id + copy ────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&row.policy_id_short)
                        .monospace()
                        .small()
                        .color(KEYHASH_GREY),
                );
                if ui
                    .small_button(RichText::new("📋").small())
                    .on_hover_text("Copy policy_id to clipboard")
                    .clicked()
                {
                    ui.ctx().copy_text(row.policy_id.clone());
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn render_list_row(
    ui: &mut Ui,
    row: &CollectionRow,
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_refuel: bool,
    response: &mut CollectionListResponse,
) {
    Frame::new()
        .fill(ROW_BG)
        .stroke(Stroke::new(1.0, ROW_STROKE))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&row.title).strong());
                status_chip(ui, &row.status);
                ui.label(
                    RichText::new(format!("{}/{}", row.minted_count, row.total_supply))
                        .monospace()
                        .small()
                        .color(META_GREY),
                );
                standard_chip(ui, &row.standard);
                network_chip(ui, &row.network);
                ui.label(
                    RichText::new(format!(
                        "wallet #{} · {}",
                        row.wallet_account_index, row.policy_id_short
                    ))
                    .color(KEYHASH_GREY)
                    .monospace()
                    .small(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if show_test_mint {
                        let label = if row.test_mint_open {
                            "− Test mint"
                        } else {
                            "🧪 Test mint"
                        };
                        if ui.small_button(RichText::new(label).small()).clicked() {
                            response.actions.push(CollectionListAction::TestMint {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                    if show_seed_stubs {
                        let label = if row.seed_stubs_open {
                            "− Seed stubs"
                        } else {
                            "+ Seed stubs"
                        };
                        if ui.small_button(RichText::new(label).small()).clicked() {
                            response.actions.push(CollectionListAction::SeedStubs {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                    if show_activity {
                        let label = if row.activity_open {
                            "− Activity"
                        } else {
                            "📜 Activity"
                        };
                        if ui.small_button(RichText::new(label).small()).clicked() {
                            response.actions.push(CollectionListAction::Activity {
                                policy_id: row.policy_id.clone(),
                            });
                        }
                    }
                    if ui
                        .small_button(RichText::new("📋").small())
                        .on_hover_text("Copy policy_id to clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(row.policy_id.clone());
                    }
                    if show_refuel {
                        if let Some(full) = &row.wallet_address_full {
                            if ui
                                .small_button(RichText::new("⛽").small())
                                .on_hover_text(
                                    "Copy this wallet's address to the clipboard \
                                         (refuel via faucet or another wallet).",
                                )
                                .clicked()
                            {
                                ui.ctx().copy_text(full.clone());
                                response.actions.push(CollectionListAction::Refuel {
                                    account_index: row.wallet_account_index,
                                });
                            }
                        }
                    }
                });
            });
        });
}

/// Filled chip with the status text in uppercase. Colour mirrors the
/// portal's previous `status_colour()` helper.
fn status_chip(ui: &mut Ui, status: &str) {
    let (fg, bg) = status_chip_colours(status);
    chip(ui, &status.to_uppercase(), fg, bg);
}

fn standard_chip(ui: &mut Ui, standard: &str) {
    let colour = match standard.to_ascii_lowercase().as_str() {
        "cip25" => STD_CIP25_CHIP,
        "cip68" => STD_CIP68_CHIP,
        _ => STD_UNKNOWN_CHIP,
    };
    chip(
        ui,
        &standard.to_uppercase(),
        Color32::from_rgb(20, 20, 30),
        colour,
    );
}

fn network_chip(ui: &mut Ui, network: &str) {
    // `cardano:preprod` → `preprod`, `cardano:mainnet` → `mainnet`.
    let trimmed = network
        .strip_prefix("cardano:")
        .unwrap_or(network)
        .to_uppercase();
    chip(ui, &trimmed, Color32::WHITE, NETWORK_CHIP);
}

fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).small().strong());
        });
}

/// Status-to-chip-colour palette. Foreground is dark-on-light for the
/// vibrant statuses (live / ready / ingesting / sold_out), light-on-dark
/// for the neutral ones (draft / paused / ended). Same RGB values as the
/// portal's previous `status_colour()` helper.
fn status_chip_colours(status: &str) -> (Color32, Color32) {
    match status {
        "draft" => (Color32::from_rgb(20, 20, 30), Color32::from_gray(170)),
        "ingesting" => (
            Color32::from_rgb(20, 20, 30),
            Color32::from_rgb(180, 180, 220),
        ),
        "ready" => (
            Color32::from_rgb(20, 20, 30),
            Color32::from_rgb(180, 220, 220),
        ),
        "live" => (Color32::from_rgb(20, 20, 30), Color32::LIGHT_GREEN),
        "paused" => (Color32::from_rgb(20, 20, 30), Color32::LIGHT_YELLOW),
        "sold_out" => (
            Color32::from_rgb(20, 20, 30),
            Color32::from_rgb(220, 180, 240),
        ),
        "ended" => (Color32::WHITE, Color32::from_gray(140)),
        _ => (Color32::from_rgb(20, 20, 30), Color32::from_gray(160)),
    }
}

/// Thin horizontal progress bar — 4 px tall, full surface width.
fn draw_thin_bar(ui: &mut Ui, pct: f32, fill: Color32) {
    let height = 4.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let corner = CornerRadius::same(2);
    painter.rect_filled(rect, corner, BAR_TRACK);
    if pct > 0.0 {
        let mut filled = rect;
        filled.set_width(rect.width() * pct);
        painter.rect_filled(filled, corner, fill);
    }
}
