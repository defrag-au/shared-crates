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

use crate::button_group::{ButtonGroup, ButtonGroupButton};
use crate::icons::install_phosphor_font;
use crate::id_pill::{IdPill, IdPillLayout};
use crate::wallet_list::{WalletPoolBadge, WalletPoolBadgeHealth};
use crate::PhosphorIcon;

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
    /// `true` while a refuel tx is in flight for this wallet. The
    /// widget renders the Refuel button disabled with a spinner-style
    /// label so the operator can't double-fire. Host clears this when
    /// the action ack lands (success or failure).
    pub refuel_in_flight: bool,
    /// Soft-archive timestamp (unix seconds), or `None` if active.
    /// Archived rows render dimmed with an "archived" chip; the
    /// Archive button flips to Restore. When
    /// [`CollectionList::with_hide_archived`] is set they're hidden
    /// behind a "show archived" toggle.
    pub archived_at: Option<i64>,
    /// The collection's bech32 deposit (payment-receiving) address — buyers
    /// send ADA here; the host's payment monitor turns each inbound tx into a
    /// `crypto` mint order. Full value goes to the clipboard on copy.
    /// `None` hides the row (legacy collections allocated before the deposit
    /// derivation was wired).
    pub deposit_address: Option<String>,
    /// Pre-truncated `deposit_address` for inline display (caller picks the
    /// prefix/suffix widths). `None` hides the row.
    pub deposit_address_short: Option<String>,
    /// BIP-44 account index of the collection's deposit wallet (the
    /// `CollectionDeposit` band — `mint + 100_000`). Carried so the deposit
    /// address pill can offer an Inspect button that opens that wallet's
    /// UTxO panel via [`CollectionListAction::OpenWallet`], same as the mint
    /// wallet. `None` hides the deposit Inspect affordance (legacy rows /
    /// hosts that haven't wired it).
    pub deposit_account_index: Option<u32>,
    /// `true` when the parent has the Ingest-payment form open below this
    /// row. Same toggle treatment as `test_mint_open` — `+ Ingest` flips to
    /// `− Ingest`.
    pub ingest_payment_open: bool,
    /// `true` while a `ScanPayments` action is in flight for this collection.
    /// Widget renders the Scan button disabled with a spinner-style label so
    /// the operator can't double-fire while the engine is walking UTxOs.
    pub scan_payments_in_flight: bool,
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
    /// User clicked an Inspect button on a wallet pill (the mint wallet
    /// or — when `deposit_account_index` is set — the deposit wallet).
    /// Parent opens that wallet's UTxO panel.
    OpenWallet { account_index: u32 },
    /// User clicked the per-card "Refuel" button. The host fires a
    /// server-side fan-out tx (keyed by `policy_id` — the collection
    /// owns the wallet) that sweeps the wallet's pure-ADA UTxOs into
    /// N × 10 ADA fuel slots, so the wallet is mint-ready before the
    /// first mint lands. Only emitted when [`CollectionList::with_refuel`]
    /// is `true`, the row has a `pool` (so the widget can verify the
    /// pool isn't already healthy), `refuel_in_flight` is `false`, and
    /// `pool.health != Healthy`. Widget self-enforces the no-op rule
    /// — host doesn't have to check.
    Refuel { policy_id: String },
    /// User clicked the per-card "Fund" button on the mint-wallet pill. The
    /// host opens its top-up flow (fund the collection's mint/fuel wallet
    /// from the operator's browser wallet). Only emitted when
    /// [`CollectionList::with_fund`] is `true` and the row has a mint
    /// wallet address.
    FundWallet { policy_id: String },
    /// User clicked Archive on an active card. Host soft-archives the
    /// collection (halts its engine DO + hides it). Only emitted when
    /// [`CollectionList::with_archive`] is `true`.
    Archive { policy_id: String },
    /// User clicked Restore on an archived card. Symmetric to
    /// [`Archive`]; only emitted from archived rows.
    Unarchive { policy_id: String },
    /// User clicked the `+ Ingest` toggle. Host opens/closes a single-field
    /// (tx_hash) form below the row and submits a manual `IngestPayment`
    /// action on confirm. Only emitted when [`CollectionList::with_payments`]
    /// is `true`.
    IngestPayment { policy_id: String },
    /// User clicked the `🔍 Scan` button. Host fires a `ScanPayments` action
    /// that walks every unspent payment on the collection's deposit address.
    /// No form — single click, result lands in a toast. Only emitted when
    /// [`CollectionList::with_payments`] is `true` and `scan_payments_in_flight`
    /// is `false` (widget self-enforces the no-double-fire rule).
    ScanPayments { policy_id: String },
    /// User clicked the `⚙ Configure` button. Host opens (or focuses) the
    /// floating Configure window for this collection — the operator's
    /// surface for editing phases, gates, and allowlist. Only emitted when
    /// [`CollectionList::with_configure`] is `true`.
    Configure { policy_id: String },
    /// User clicked the `Settlement` button. Host opens (or focuses) the
    /// floating Settlement window — treasury config (founder split, float
    /// targets, fee waiver) + the manual settle trigger. Only emitted when
    /// [`CollectionList::with_settlement`] is `true`.
    Settlement { policy_id: String },
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
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    show_fund: bool,
    hide_archived: bool,
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
const ROW_BG_ARCHIVED: Color32 = Color32::from_rgb(18, 18, 24);
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
            show_archive: false,
            show_payments: false,
            show_configure: false,
            show_settlement: false,
            show_fund: false,
            hide_archived: false,
        }
    }

    /// Show a per-card Archive button (Restore on archived cards). Off by
    /// default. Click → [`CollectionListAction::Archive`] /
    /// [`CollectionListAction::Unarchive`].
    pub fn with_archive(mut self, enabled: bool) -> Self {
        self.show_archive = enabled;
        self
    }

    /// Hide archived collections by default behind a "Show N archived" toggle
    /// rendered below the grid. Off by default (archived cards render inline,
    /// dimmed). The reveal flag is cosmetic — it lives in egui memory, not the
    /// host. Mirrors [`crate::wallet_list::WalletList::with_hide_archived`].
    pub fn with_hide_archived(mut self, hide: bool) -> Self {
        self.hide_archived = hide;
        self
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

    /// Show the per-card payment-monitor buttons (`🔍 Scan` + `+ Ingest`).
    /// Off by default. The host wires the matching actions: `ScanPayments`
    /// fires immediately (single click, result toast); `IngestPayment`
    /// toggles a single-field form for a tx hash. See
    /// `docs/design/MINT_PAYMENT_MONITOR.md`.
    pub fn with_payments(mut self, enabled: bool) -> Self {
        self.show_payments = enabled;
        self
    }

    /// Show the per-card `⚙ Configure` button. Off by default. Click →
    /// emits [`CollectionListAction::Configure`]; host opens / focuses the
    /// floating Configure window for editing phases, gates, and allowlist.
    pub fn with_configure(mut self, enabled: bool) -> Self {
        self.show_configure = enabled;
        self
    }

    /// Show the per-card `Settlement` button. Off by default. Click →
    /// emits [`CollectionListAction::Settlement`]; host opens / focuses the
    /// floating Settlement window for editing treasury config + triggering a
    /// settlement run.
    pub fn with_settlement(mut self, enabled: bool) -> Self {
        self.show_settlement = enabled;
        self
    }

    /// Show a "Fund" button on the mint-wallet pill. Off by default. Click →
    /// emits [`CollectionListAction::FundWallet`]; the host opens its
    /// browser-wallet top-up flow for the collection's mint/fuel wallet.
    pub fn with_fund(mut self, enabled: bool) -> Self {
        self.show_fund = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> CollectionListResponse {
        let mut response = CollectionListResponse::default();

        // Archived collections are hidden by default when `hide_archived` is
        // set — they're noise on the active dashboard. The reveal flag is
        // cosmetic, so it lives in egui memory keyed by this Ui's id rather
        // than round-tripping through the host. When hiding is off, archived
        // cards render inline (dimmed).
        let archived_total = self.rows.iter().filter(|r| r.archived_at.is_some()).count();
        let toggle_id = ui.id().with("collection_list_show_archived");
        let show_archived = if self.hide_archived {
            ui.ctx()
                .data_mut(|d| d.get_temp::<bool>(toggle_id))
                .unwrap_or(false)
        } else {
            true
        };
        let visible: Vec<&CollectionRow> = self
            .rows
            .iter()
            .filter(|r| show_archived || r.archived_at.is_none())
            .collect();

        if !visible.is_empty() {
            // Per-grid clamp: a 1-row layout renders full-width even when
            // 2+ columns are configured.
            let effective_cols = self.columns.min(visible.len()).max(1);

            let gap = match self.layout {
                CollectionListLayout::Card => 8.0,
                CollectionListLayout::List => 4.0,
            };

            let chunks: Vec<&[&CollectionRow]> = visible.chunks(effective_cols).collect();
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
                            self.show_archive,
                            self.show_payments,
                            self.show_configure,
                            self.show_settlement,
                            self.show_fund,
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
                                self.show_archive,
                                self.show_payments,
                                self.show_configure,
                                self.show_settlement,
                                self.show_fund,
                                &mut response,
                            );
                        }
                    });
                }
                if chunk_idx < last_chunk {
                    ui.add_space(gap);
                }
            }
        }

        // Reveal toggle — only when hiding is enabled and there's something to
        // reveal. Flips the cosmetic flag in egui memory.
        if self.hide_archived && archived_total > 0 {
            ui.add_space(6.0);
            let label = if show_archived {
                format!("Hide {archived_total} archived")
            } else {
                format!("Show {archived_total} archived")
            };
            if ui
                .add(egui::Button::new(RichText::new(label).small().color(META_GREY)).small())
                .clicked()
            {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(toggle_id, !show_archived));
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
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    show_fund: bool,
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
            show_archive,
            show_payments,
            show_configure,
            show_settlement,
            show_fund,
            response,
        ),
        CollectionListLayout::List => render_list_row(
            ui,
            row,
            show_test_mint,
            show_seed_stubs,
            show_activity,
            show_refuel,
            show_archive,
            show_payments,
            show_configure,
            show_settlement,
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
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    show_fund: bool,
    response: &mut CollectionListResponse,
) {
    // `PhosphorIcon::*.rich_text()` doesn't auto-install the font (unlike
    // `.show()`); the inline copy / chip / configure buttons below all
    // rely on the glyph being available. Idempotent.
    install_phosphor_font(ui.ctx());
    let archived = row.archived_at.is_some();
    let fill = if archived { ROW_BG_ARCHIVED } else { ROW_BG };
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, ROW_STROKE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // ── Title row: title + chips only ──────────────────────
            // Action buttons moved out to their own dedicated bar below so
            // the title stays readable at any card width (the previous
            // shape squeezed the title under {chips × ≥4 buttons} at
            // ~half-width). The bar wraps via `horizontal_wrapped`.
            ui.horizontal(|ui| {
                let title = RichText::new(&row.title).heading();
                ui.label(if archived {
                    title.color(META_GREY)
                } else {
                    title
                });

                // Status chip — filled tag using the status colour. The
                // chip is the strongest secondary signal after the title,
                // so it sits closest to it.
                status_chip(ui, &row.status);
                standard_chip(ui, &row.standard);
                network_chip(ui, &row.network);
                if archived {
                    ui.colored_label(Color32::LIGHT_YELLOW, RichText::new("archived").small());
                }
            });

            // ── Action bar ─────────────────────────────────────────
            if has_any_action(
                show_test_mint,
                show_seed_stubs,
                show_activity,
                show_refuel,
                show_archive,
                show_payments,
                show_configure,
                show_settlement,
                row,
            ) {
                ui.add_space(6.0);
                render_operator_actions(
                    ui,
                    row,
                    archived,
                    show_test_mint,
                    show_seed_stubs,
                    show_activity,
                    show_archive,
                    show_payments,
                    show_configure,
                    show_settlement,
                    true, // wrap — Card layout has its own dedicated bar row
                    response,
                );
            }

            ui.add_space(8.0);

            // ── Identity: policy_id as a stacked pill, above the supply
            //    bar. The policy_id is the collection's primary on-chain
            //    identity (and what `mintctl clone-policy` needs), so it
            //    leads the body rather than trailing in a footer. The
            //    stacked `IdPill` gives it a labelled, copy-able frame
            //    consistent with the wallet/deposit pills below.
            IdPill::new("policy", &row.policy_id)
                .layout(IdPillLayout::Stacked)
                .with_short(row.policy_id_short.clone())
                .show(ui);

            ui.add_space(10.0);

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

            // ── Wallets: stacked address pills, each inspectable ──────
            //
            // The collection card is the sole surface for the collection's
            // wallets (they're filtered out of the parent's "Wallets"
            // section), so both the **mint** wallet (fuel + minting) and the
            // **deposit** wallet (where buyers send ADA) live here. Each is
            // a stacked `IdPill` (labelled, copy-able address frame) with an
            // Inspect button that opens its UTxO panel. The mint pill also
            // carries the fuel-pool badge + Refuel on the controls row.
            // Under settle-as-you-mint the mint wallet IS the payment wallet —
            // it signs the mint AND receives the buyer's ADA (no separate
            // deposit address). Label it so the operator knows both roles live
            // in one wallet. Legacy two-wallet collections (a separate deposit
            // address is present) keep the plain "mint wallet" label, with the
            // deposit pill rendered below.
            let mint_label = if row.deposit_address.is_some() {
                format!("mint wallet #{}", row.wallet_account_index)
            } else {
                format!("mint + payments #{}", row.wallet_account_index)
            };
            render_wallet_pill(
                ui,
                &mint_label,
                row.wallet_address_full.as_deref(),
                row.wallet_address_short.as_deref(),
                Some(row.wallet_account_index),
                response,
                |ui, response| {
                    // Fund — top up the mint/fuel wallet from the operator's
                    // browser wallet. The primary action for an empty wallet,
                    // so it leads the controls row (before the fuel state +
                    // Refuel, which only shape funds already present).
                    if show_fund
                        && row.wallet_address_full.is_some()
                        && ui
                            .small_button(RichText::new("Fund").small())
                            .on_hover_text("Top up this collection's mint wallet from your wallet")
                            .clicked()
                    {
                        response.actions.push(CollectionListAction::FundWallet {
                            policy_id: row.policy_id.clone(),
                        });
                    }
                    // Pool badge — same shape as the wallet-list card's
                    // pool pill so the visual is consistent across surfaces.
                    if let Some(pool) = &row.pool {
                        let fg = match pool.health {
                            WalletPoolBadgeHealth::Empty => Color32::from_rgb(220, 130, 130),
                            WalletPoolBadgeHealth::Low => Color32::from_rgb(220, 200, 130),
                            WalletPoolBadgeHealth::Healthy => Color32::from_rgb(150, 210, 160),
                        };
                        let ada_whole = pool.total_lovelace / 1_000_000;
                        ui.label(
                            RichText::new(format!("{} fuel · {} ADA", pool.fuel_count, ada_whole))
                                .small()
                                .color(fg),
                        );
                    }
                    // Refuel — fan-out tx that splits the wallet's pure-ADA
                    // balance into N × 10 ADA fuel slots. Self-disables (no
                    // pool / healthy / in-flight) so the host doesn't re-check.
                    if show_refuel {
                        if let Some(pool) = &row.pool {
                            refuel_button(ui, row, pool, response);
                        }
                    }
                },
            );

            // ── Deposit wallet — only when allocated (legacy rows hide it
            //    cleanly). Inspectable too: the deposit wallet is a real
            //    `CollectionDeposit` account, so its Inspect opens the same
            //    UTxO panel (handy for eyeballing inbound payments).
            if row.deposit_address.is_some() {
                ui.add_space(4.0);
                render_wallet_pill(
                    ui,
                    "deposit",
                    row.deposit_address.as_deref(),
                    row.deposit_address_short.as_deref(),
                    row.deposit_account_index,
                    response,
                    |_ui, _response| {},
                );
            }
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
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    response: &mut CollectionListResponse,
) {
    // `PhosphorIcon::*.rich_text()` doesn't auto-install the font (unlike
    // `.show()`); the inline copy / chip / configure buttons below all
    // rely on the glyph being available. Idempotent.
    install_phosphor_font(ui.ctx());
    let archived = row.archived_at.is_some();
    let fill = if archived { ROW_BG_ARCHIVED } else { ROW_BG };
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, ROW_STROKE))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let title = RichText::new(&row.title).strong();
                ui.label(if archived {
                    title.color(META_GREY)
                } else {
                    title
                });
                status_chip(ui, &row.status);
                if archived {
                    ui.colored_label(Color32::LIGHT_YELLOW, RichText::new("archived").small());
                }
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
                    // Right-edge: Refuel → Copy → operator action group.
                    // Within `right_to_left`, items added first sit furthest
                    // right; ButtonGroup's internal layout still flows
                    // left-to-right inside its own cluster.
                    if show_refuel {
                        if let Some(pool) = &row.pool {
                            refuel_button(ui, row, pool, response);
                        }
                    }
                    if ui
                        .small_button(PhosphorIcon::Copy.rich_text(11.0, META_GREY).small())
                        .on_hover_text("Copy policy_id to clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(row.policy_id.clone());
                    }
                    render_operator_actions(
                        ui,
                        row,
                        archived,
                        show_test_mint,
                        show_seed_stubs,
                        show_activity,
                        show_archive,
                        show_payments,
                        show_configure,
                        show_settlement,
                        false, // wrap — List layout is single-row by design
                        response,
                    );
                });
            });
        });
}

/// Render one collection wallet as a stacked `IdPill` (labelled, copy-able
/// address frame) followed by a controls row: an **Inspect** button that
/// opens the wallet's UTxO panel (via [`CollectionListAction::OpenWallet`])
/// plus any caller-supplied controls (`extra` — the mint wallet passes its
/// pool badge + Refuel; the deposit wallet passes nothing).
///
/// - `address_full` / `address_short`: the bech32 address + its pre-elided
///   form. `None` (legacy rows without a derived address) falls back to just
///   the label, no pill.
/// - `account_index`: drives the Inspect button. `None` hides it (the host
///   hasn't wired a UTxO panel for this wallet).
fn render_wallet_pill(
    ui: &mut Ui,
    label: &str,
    address_full: Option<&str>,
    address_short: Option<&str>,
    account_index: Option<u32>,
    response: &mut CollectionListResponse,
    extra: impl FnOnce(&mut Ui, &mut CollectionListResponse),
) {
    if let Some(full) = address_full {
        let mut pill = IdPill::new(label, full).layout(IdPillLayout::Stacked);
        if let Some(short) = address_short {
            pill = pill.with_short(short.to_string());
        }
        pill.show(ui);
    } else {
        ui.label(RichText::new(label).small().color(META_GREY));
    }

    ui.add_space(2.0);
    ui.horizontal(|ui| {
        if let Some(idx) = account_index {
            if ui
                .small_button(RichText::new("Inspect").small().color(META_GREY))
                .on_hover_text("Open this wallet's UTxOs")
                .clicked()
            {
                response
                    .actions
                    .push(CollectionListAction::OpenWallet { account_index: idx });
            }
        }
        extra(ui, response);
    });
}

/// Shared Refuel button used by both card and list layouts. Owns the
/// no-op-when-healthy + in-flight policies so neither call site (and
/// no host) has to re-check the rules.
///
/// States, in priority order:
///   - `refuel_in_flight` → disabled, label `⛽ Refuelling…`
///   - `pool.health == Healthy` → disabled, label `⛽ Refuel`,
///     explanatory hover ("pool is healthy")
///   - otherwise → enabled, label `⛽ Refuel`, click emits the action
fn refuel_button(
    ui: &mut Ui,
    row: &CollectionRow,
    pool: &WalletPoolBadge,
    response: &mut CollectionListResponse,
) {
    if row.refuel_in_flight {
        ui.add_enabled(
            false,
            egui::Button::new(RichText::new("Refuelling…").small()),
        )
        .on_disabled_hover_text(
            "A refuel tx is in flight — wait for it to confirm before submitting another.",
        );
        return;
    }
    let healthy = pool.health == WalletPoolBadgeHealth::Healthy;
    if healthy {
        ui.add_enabled(false, egui::Button::new(RichText::new("Refuel").small()))
            .on_disabled_hover_text(
                "Fuel pool is healthy — no refuel needed. \
                 The pool tops up automatically during mints.",
            );
        return;
    }
    if ui
        .small_button(RichText::new("Refuel").small())
        .on_hover_text(
            "Split this wallet's pure-ADA balance into 10 ADA fuel UTxOs \
             so it's ready for parallel mints. Submits a self-spend tx.",
        )
        .clicked()
    {
        response.actions.push(CollectionListAction::Refuel {
            policy_id: row.policy_id.clone(),
        });
    }
}

/// Cheap predicate so we can suppress the spacing + action bar entirely
/// on a card with no buttons configured (e.g. a list view that doesn't
/// opt into any of the operator flags).
#[allow(clippy::too_many_arguments)]
fn has_any_action(
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_refuel: bool,
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    row: &CollectionRow,
) -> bool {
    show_test_mint
        || show_seed_stubs
        || show_activity
        || show_archive
        || show_configure
        || show_payments
        || show_settlement
        // Refuel is a card-only affordance + sits in the wallet sub-line,
        // not the action bar — but the predicate is shared with the bar's
        // visibility check; counting it keeps the bar logic uniform.
        || (show_refuel && row.pool.is_some())
}

/// Operator-action cluster — Test mint / Activity / Configure / Ingest
/// / Scan / Seed stubs / Archive, all routed through a single
/// [`ButtonGroup`] so layout / spacing / Phosphor-icon labelling are
/// handled by the shared primitive rather than re-implemented inline.
///
/// `wrap = true` is the Card layout (a dedicated row below the title
/// that wraps onto a second line on narrow surfaces). `wrap = false` is
/// the List layout (one compact line, no wrap).
///
/// Archive lives at the natural end of the cluster rather than being
/// pinned to the right edge (the previous shape) — when the cluster
/// wraps, Archive lands on the last line which is visually equivalent.
#[allow(clippy::too_many_arguments)]
fn render_operator_actions(
    ui: &mut Ui,
    row: &CollectionRow,
    archived: bool,
    show_test_mint: bool,
    show_seed_stubs: bool,
    show_activity: bool,
    show_archive: bool,
    show_payments: bool,
    show_configure: bool,
    show_settlement: bool,
    wrap: bool,
    response: &mut CollectionListResponse,
) {
    // Stable click ids — match the dispatch arm below. Locally scoped
    // because the host never sees these; the widget emits its own
    // `CollectionListAction` enum.
    const ID_TEST_MINT: u64 = 1;
    const ID_ACTIVITY: u64 = 2;
    const ID_CONFIGURE: u64 = 3;
    const ID_INGEST: u64 = 4;
    const ID_SCAN: u64 = 5;
    const ID_SEED_STUBS: u64 = 6;
    const ID_ARCHIVE: u64 = 7;
    const ID_SETTLEMENT: u64 = 8;

    let mut group = ButtonGroup::new().wrap(wrap);

    if show_test_mint {
        let label = if row.test_mint_open {
            "- Test mint"
        } else {
            "Test mint"
        };
        group = group.add(ButtonGroupButton::new(ID_TEST_MINT, label).hover_text(
            "Open the test-mint form for this collection \
                 (operator/super-admin only)",
        ));
    }
    if show_activity {
        let label = if row.activity_open {
            "- Activity"
        } else {
            "Activity"
        };
        group = group.add(ButtonGroupButton::new(ID_ACTIVITY, label).hover_text(
            "Recent mint activity for this collection — \
                 fetched from the engine's mint_log",
        ));
    }
    if show_configure {
        group = group.add(
            ButtonGroupButton::new(ID_CONFIGURE, "Configure")
                .icon(crate::PhosphorIcon::Gear)
                .hover_text(
                    "Open the mint configuration — phases (price + time window + \
                     per-wallet limit), gates (public / allowlist / token-held), \
                     and allowlist entries.",
                ),
        );
    }
    if show_settlement {
        group = group.add(
            ButtonGroupButton::new(ID_SETTLEMENT, "Settlement").hover_text(
                "Open the settlement config — founder distribution split, float \
             targets, fee waiver — and trigger a settlement run.",
            ),
        );
    }
    if show_payments {
        let ingest_label = if row.ingest_payment_open {
            "- Ingest"
        } else {
            "+ Ingest"
        };
        group = group.add(ButtonGroupButton::new(ID_INGEST, ingest_label).hover_text(
            "Manually feed an on-chain payment tx through the resolver — \
                 creates a mint order (or queues a refund). Idempotent on tx hash.",
        ));
        let scan_label = if row.scan_payments_in_flight {
            "Scanning…"
        } else {
            "Scan"
        };
        let scan_hover = if row.scan_payments_in_flight {
            "A payments scan is in flight — wait for it to complete before re-running."
        } else {
            "Walk this collection's deposit address and process every unspent \
             payment through the resolver. Idempotent — already-handled txs \
             short-circuit."
        };
        group = group.add(
            ButtonGroupButton::new(ID_SCAN, scan_label)
                .icon(crate::PhosphorIcon::MagnifyingGlass)
                .enabled(!row.scan_payments_in_flight)
                .hover_text(scan_hover),
        );
    }
    if show_seed_stubs {
        let label = if row.seed_stubs_open {
            "- Seed stubs"
        } else {
            "+ Seed stubs"
        };
        group = group.add(ButtonGroupButton::new(ID_SEED_STUBS, label).hover_text(
            "Seed placeholder mintable assets \
                 (testing-only — real ingest replaces this)",
        ));
    }
    if show_archive {
        let (label, hover) = if archived {
            (
                "Restore",
                "Bring this collection back — the engine resumes processing on the next signal.",
            )
        } else {
            (
                "Archive",
                "Retire this collection: halts its mint engine + hides it from the \
                 dashboard. Reversible.",
            )
        };
        group = group.add(ButtonGroupButton::new(ID_ARCHIVE, label).hover_text(hover));
    }

    let clicked = group.show(ui).clicked;
    let action = match clicked {
        Some(ID_TEST_MINT) => Some(CollectionListAction::TestMint {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_ACTIVITY) => Some(CollectionListAction::Activity {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_CONFIGURE) => Some(CollectionListAction::Configure {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_SETTLEMENT) => Some(CollectionListAction::Settlement {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_INGEST) => Some(CollectionListAction::IngestPayment {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_SCAN) => Some(CollectionListAction::ScanPayments {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_SEED_STUBS) => Some(CollectionListAction::SeedStubs {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_ARCHIVE) if archived => Some(CollectionListAction::Unarchive {
            policy_id: row.policy_id.clone(),
        }),
        Some(ID_ARCHIVE) => Some(CollectionListAction::Archive {
            policy_id: row.policy_id.clone(),
        }),
        _ => None,
    };
    if let Some(a) = action {
        response.actions.push(a);
    }
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
