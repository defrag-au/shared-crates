//! Wallet roster — the per-client list rendered on the admin portal
//! dashboard. Groups wallets by role (Primary → Collections → Custom), keeps
//! the layout scannable when one wallet is present and structured when many
//! are.
//!
//! ## Why it's a widget
//!
//! The portal's `render_client_detail` was rendering a flat `for w in
//! wallets` loop. That falls apart the moment a client has 5+ collection
//! wallets: no grouping, no visual hierarchy, archive button mashed against
//! the key-hash. Pushing this into a widget gives us:
//!
//! - section headers (Primary / Collections / Custom) with a row count
//! - a single-wallet "compact" layout that skips the section header
//! - per-row role chip with consistent colours across screens
//! - a uniform `WalletListAction` action stream for the host to handle —
//!   today only `Archive`, but trivial to extend to Rename / SetDefault / etc.
//!
//! ## What it does NOT do
//!
//! - **No async, no inbox.** Actions are returned in the response struct;
//!   the parent fires whatever IO it needs.
//! - **No state.** Selection / hover are intentionally absent — wallet rows
//!   are clickable in a future revision; this version mirrors the read-only
//!   list the portal already had.
//! - **No add-wallet form.** That stays as a parent-owned inline form
//!   immediately below the list (see `app.rs::render_client_detail`).
//!
//! ## Layouts
//!
//! Two presentations driven from the same `WalletListRow` VM:
//!
//! - [`WalletListLayout::List`] (default) — compact one-row-per-wallet,
//!   used today by the portal's main dashboard.
//! - [`WalletListLayout::Card`] — taller tiles with a filled role pill,
//!   a heading-sized label, and a footer line for the address. Better
//!   when wallet identity is the focal element (e.g. an "Identities &
//!   wallets" sub-page); leaves room for future per-card slots (balance,
//!   collection count) without re-doing the layout.
//!
//! ## Usage
//!
//! ```ignore
//! let rows: Vec<WalletListRow> = detail.wallets.iter().map(to_vm).collect();
//! let resp = WalletList::new(&rows)
//!     .with_layout(WalletListLayout::Card)
//!     .with_can_archive_primary(false)
//!     .show(ui);
//! for action in resp.actions {
//!     match action {
//!         WalletListAction::Archive { account_index } => { /* dispatch */ }
//!     }
//! }
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

// ─────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────

/// Logical role of a wallet within a client. Mirrors
/// `shared_types::client_management::WalletRole`; we keep a local copy so
/// the widget crate doesn't take a hard dependency on it (shared-types
/// lives in a different workspace).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletListRole {
    /// `account_index = 0`, exactly one per client. Highlighted at the top.
    Primary,
    /// Per-collection wallet, auto-created on collection provisioning.
    Collection,
    /// Operator-named wallet (rare; advanced users).
    Custom,
}

/// Optional pool-health pill rendered on the wallet card. Set on rows
/// whose UTxOs have been fetched at least once; absent on rows where the
/// host hasn't refreshed (the card doesn't gain a "no data" badge — it
/// simply doesn't render the pill). The widget renders fuel count +
/// total ADA + a coloured dot for the health bucket.
#[derive(Clone, Debug)]
pub struct WalletPoolBadge {
    pub fuel_count: u32,
    pub total_lovelace: u64,
    pub health: WalletPoolBadgeHealth,
}

/// Health bucket — a local enum so the widget crate stays
/// shared-types-free. Hosts map their canonical
/// `WalletPoolHealth` to this at the VM boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletPoolBadgeHealth {
    Empty,
    Low,
    Healthy,
}

/// View-model for a single wallet row. Pre-formatted by the caller — the
/// widget does no truncation itself.
#[derive(Clone, Debug)]
pub struct WalletListRow {
    /// BIP-44 account index. Rendered as `#N` in monospace.
    pub account_index: u32,
    /// Display name. "Primary" / collection title / operator-set label.
    pub label: String,
    /// Pre-truncated wallet identity for inline display — typically the
    /// bech32 enterprise address (e.g. `addr_test1vp…9fc465`). Caller
    /// picks the truncation strategy (prefix/suffix widths).
    pub address_short: String,
    /// Full, un-truncated value to write to the clipboard when the user
    /// clicks the copy icon. `None` hides the copy button — useful when
    /// no canonical "copy this" value exists (e.g. row only shows a
    /// derived placeholder).
    pub address_full: Option<String>,
    /// Role bucket — drives section placement + chip colour.
    pub role: WalletListRole,
    /// `true` if the wallet was auto-provisioned. Renders an "auto" chip
    /// unless the role is `Collection` (where every wallet is auto, so the
    /// chip would be noise).
    pub auto_created: bool,
    /// Unix seconds of archive, or `None` if active. Archived rows render
    /// dimmed with an "archived" chip.
    pub archived_at: Option<i64>,
    /// Optional fuel-pool summary. Rendered as a single-line inline pill
    /// below the address: `🟢 20 fuel · 230 ADA`. `None` → no pill (the
    /// host hasn't refreshed UTxOs for this wallet yet).
    pub pool: Option<WalletPoolBadge>,
}

/// Actions emitted while the widget was on screen this frame. Parent
/// drains and dispatches.
#[derive(Clone, Debug)]
pub enum WalletListAction {
    /// User clicked the archive button on the row with this
    /// `account_index`. The widget already enforces the
    /// "Primary cannot be archived" rule when
    /// [`WalletList::with_can_archive_primary`] is left at the default
    /// `false`, so the parent doesn't need to defend against it again.
    Archive { account_index: u32 },
    /// User clicked the Restore button on an archived row. Symmetric
    /// to [`Archive`]; only emitted from archived rows (the widget
    /// swaps Archive ↔ Restore based on `archived_at`).
    Unarchive { account_index: u32 },
    /// User clicked the "View UTxOs" button on a row. Only emitted
    /// when [`WalletList::with_view_button`] is `true`; the parent
    /// owns the panel state (toggle / refresh / cache) and the
    /// indexer round-trip.
    ViewUtxos { account_index: u32 },
}

/// Rendering style. Both styles share the same row VM, bucketing, and
/// action emission — only the per-row geometry differs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WalletListLayout {
    /// Compact horizontal row (default). Scannable when many wallets are
    /// on screen — the current portal dashboard look.
    #[default]
    List,
    /// Taller tile with a filled role pill, large title, and the address
    /// on a dedicated footer line. Good when wallet identity is the
    /// focal element rather than one entry in a long list.
    Card,
}

/// Builder.
pub struct WalletList<'a> {
    rows: &'a [WalletListRow],
    layout: WalletListLayout,
    columns: usize,
    can_archive_primary: bool,
    show_section_headers_for_single: bool,
    view_button: bool,
}

/// Response — drained actions for this frame.
#[derive(Default, Debug)]
pub struct WalletListResponse {
    pub actions: Vec<WalletListAction>,
}

// ─────────────────────────────────────────────────────────────────────
// Colours — kept private. The chip palette matches `mnemonic_display`
// (cool blue for "platform-managed", soft green for "collection",
// neutral grey for "custom"); rebalance here, not at the call site.
// ─────────────────────────────────────────────────────────────────────

const ROLE_PRIMARY_CHIP: Color32 = Color32::from_rgb(150, 180, 230);
const ROLE_COLLECTION_CHIP: Color32 = Color32::from_rgb(180, 220, 180);
const ROLE_CUSTOM_CHIP: Color32 = Color32::from_gray(150);
const META_GREY: Color32 = Color32::from_gray(140);
const KEYHASH_GREY: Color32 = Color32::from_gray(120);
const SECTION_HEADER: Color32 = Color32::from_gray(170);
const ROW_BG: Color32 = Color32::from_rgb(22, 22, 32);
const ROW_BG_PRIMARY: Color32 = Color32::from_rgb(26, 30, 44);
const ROW_BG_ARCHIVED: Color32 = Color32::from_rgb(18, 18, 24);
const ROW_STROKE: Color32 = Color32::from_rgb(40, 40, 56);
const ROW_STROKE_PRIMARY: Color32 = Color32::from_rgb(60, 80, 110);

impl<'a> WalletList<'a> {
    pub fn new(rows: &'a [WalletListRow]) -> Self {
        Self {
            rows,
            layout: WalletListLayout::default(),
            columns: 1,
            can_archive_primary: false,
            show_section_headers_for_single: false,
            view_button: false,
        }
    }

    /// Show a per-row "UTxOs" button alongside Archive. Off by
    /// default — surfaces an extra affordance which only makes
    /// sense when the parent has somewhere to render the panel
    /// (e.g. the portal dashboard). Click → emits
    /// [`WalletListAction::ViewUtxos`].
    pub fn with_view_button(mut self, enabled: bool) -> Self {
        self.view_button = enabled;
        self
    }

    /// Switch the per-row presentation. See [`WalletListLayout`].
    pub fn with_layout(mut self, layout: WalletListLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Lay rows out as a grid this many columns wide. Default `1`
    /// (single-column stack). Per-bucket clamped to `min(cols,
    /// rows_in_bucket)` so a one-wallet bucket (e.g. Primary) still uses
    /// the full surface width when the configured count would otherwise
    /// leave dead space. Set to 2–3 for card layouts on wide surfaces;
    /// list mode tolerates 2 cols, more than that crowds the address.
    pub fn with_columns(mut self, cols: usize) -> Self {
        self.columns = cols.max(1);
        self
    }

    /// Override the safety default and allow the Primary row to be
    /// archived. The portal never enables this — archiving the primary
    /// orphans the client's on-chain identity — but tooling that's about
    /// to purge a client (`offboard` flow) may want to.
    pub fn with_can_archive_primary(mut self, allow: bool) -> Self {
        self.can_archive_primary = allow;
        self
    }

    /// Force section headers even when only one bucket is populated. Off
    /// by default — a fresh client with a single Primary wallet shouldn't
    /// see a "Primary (1)" header floating above one row.
    pub fn with_section_headers_for_single(mut self, force: bool) -> Self {
        self.show_section_headers_for_single = force;
        self
    }

    pub fn show(self, ui: &mut Ui) -> WalletListResponse {
        let mut response = WalletListResponse::default();

        // Bucket the rows in a stable order: Primary → Collections (by
        // account_index ASC) → Custom (by account_index ASC).
        let mut primary: Vec<&WalletListRow> = Vec::new();
        let mut collections: Vec<&WalletListRow> = Vec::new();
        let mut custom: Vec<&WalletListRow> = Vec::new();
        for r in self.rows {
            match r.role {
                WalletListRole::Primary => primary.push(r),
                WalletListRole::Collection => collections.push(r),
                WalletListRole::Custom => custom.push(r),
            }
        }
        collections.sort_by_key(|r| r.account_index);
        custom.sort_by_key(|r| r.account_index);

        let populated_buckets = usize::from(!primary.is_empty())
            + usize::from(!collections.is_empty())
            + usize::from(!custom.is_empty());
        let show_headers = self.show_section_headers_for_single || populated_buckets > 1;

        render_bucket(
            ui,
            "Primary",
            &primary,
            show_headers,
            self.layout,
            self.columns,
            self.can_archive_primary,
            self.view_button,
            &mut response,
        );

        if !primary.is_empty() && (!collections.is_empty() || !custom.is_empty()) {
            ui.add_space(8.0);
        }

        render_bucket(
            ui,
            "Collections",
            &collections,
            show_headers,
            self.layout,
            self.columns,
            /* allow_archive = */ true,
            self.view_button,
            &mut response,
        );

        if !collections.is_empty() && !custom.is_empty() {
            ui.add_space(8.0);
        }

        render_bucket(
            ui,
            "Custom",
            &custom,
            show_headers,
            self.layout,
            self.columns,
            /* allow_archive = */ true,
            self.view_button,
            &mut response,
        );

        response
    }
}

// ─────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)] // honest set of knobs; bundling them
                                     // into a struct would just move the args.
fn render_bucket(
    ui: &mut Ui,
    title: &str,
    rows: &[&WalletListRow],
    show_header: bool,
    layout: WalletListLayout,
    columns: usize,
    allow_archive: bool,
    view_button: bool,
    response: &mut WalletListResponse,
) {
    if rows.is_empty() {
        return;
    }

    if show_header {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).color(SECTION_HEADER).small().strong());
            ui.label(
                RichText::new(format!("({})", rows.len()))
                    .color(META_GREY)
                    .small(),
            );
        });
        ui.add_space(4.0);
    }

    // Card mode wants a touch more breathing room between tiles than the
    // dense list mode does.
    let gap = match layout {
        WalletListLayout::List => 4.0,
        WalletListLayout::Card => 8.0,
    };

    // Per-bucket clamp: a 1-row bucket (Primary) renders full-width even
    // when the caller asked for 2+ columns; a 5-row bucket honours the
    // configured grid. Keeps each bucket from leaving empty trailing
    // columns that look like a layout bug.
    let effective_cols = columns.min(rows.len()).max(1);

    let chunks: Vec<&[&WalletListRow]> = rows.chunks(effective_cols).collect();
    let last_chunk = chunks.len().saturating_sub(1);
    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        if effective_cols == 1 {
            // Single-column — render directly into the bucket's UI, no
            // nested `ui.columns` wrapper (which would add layout cost
            // for zero visual gain).
            for r in chunk.iter() {
                render_one(ui, r, layout, allow_archive, view_button, response);
            }
        } else {
            // Multi-column — split into `effective_cols` equal sub-UIs.
            // The last chunk may be short; the closure simply doesn't
            // touch the trailing columns and they stay empty (preserves
            // grid alignment with cells above).
            ui.columns(effective_cols, |cols| {
                for (i, r) in chunk.iter().enumerate() {
                    render_one(
                        &mut cols[i],
                        r,
                        layout,
                        allow_archive,
                        view_button,
                        response,
                    );
                }
            });
        }
        if chunk_idx < last_chunk {
            ui.add_space(gap);
        }
    }
}

fn render_one(
    ui: &mut Ui,
    row: &WalletListRow,
    layout: WalletListLayout,
    allow_archive: bool,
    view_button: bool,
    response: &mut WalletListResponse,
) {
    match layout {
        WalletListLayout::List => render_row(ui, row, allow_archive, view_button, response),
        WalletListLayout::Card => render_card(ui, row, allow_archive, view_button, response),
    }
}

fn render_row(
    ui: &mut Ui,
    row: &WalletListRow,
    allow_archive: bool,
    view_button: bool,
    response: &mut WalletListResponse,
) {
    let archived = row.archived_at.is_some();
    let (fill, stroke) = match (row.role, archived) {
        (_, true) => (ROW_BG_ARCHIVED, ROW_STROKE),
        (WalletListRole::Primary, false) => (ROW_BG_PRIMARY, ROW_STROKE_PRIMARY),
        _ => (ROW_BG, ROW_STROKE),
    };

    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // ── Account index ────────────────────────────────────
                ui.label(
                    RichText::new(format!("#{}", row.account_index))
                        .monospace()
                        .color(if archived {
                            META_GREY
                        } else {
                            Color32::from_gray(200)
                        }),
                );

                // ── Label ───────────────────────────────────────────
                let label_text = RichText::new(&row.label).strong();
                let label_text = if archived {
                    label_text.color(META_GREY)
                } else {
                    label_text
                };
                ui.label(label_text);

                // ── Role chip ───────────────────────────────────────
                let (chip_text, chip_colour) = match row.role {
                    WalletListRole::Primary => ("primary", ROLE_PRIMARY_CHIP),
                    WalletListRole::Collection => ("collection", ROLE_COLLECTION_CHIP),
                    WalletListRole::Custom => ("custom", ROLE_CUSTOM_CHIP),
                };
                ui.colored_label(chip_colour, RichText::new(chip_text).small());

                // 'collection' wallets are always auto-created — chipping
                // both would be noise. Show 'auto' only outside that bucket.
                if row.auto_created && row.role != WalletListRole::Collection {
                    ui.colored_label(META_GREY, RichText::new("auto").small());
                }

                if archived {
                    ui.colored_label(Color32::LIGHT_YELLOW, RichText::new("archived").small());
                }

                // ── Address (right-aligned with copy + archive) ─────
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Archive ↔ Restore — placed first because of
                    // the right-to-left layout (ends up rightmost).
                    // Archive hidden for Primary (unless explicitly
                    // enabled). Restore appears only on archived
                    // rows; clicking flips the wallet back to active.
                    if archived {
                        if ui
                            .small_button(RichText::new("Restore").small())
                            .on_hover_text(
                                "Bring this wallet back from archived. \
                                     Clears `archived_at`; no key material touched.",
                            )
                            .clicked()
                        {
                            response.actions.push(WalletListAction::Unarchive {
                                account_index: row.account_index,
                            });
                        }
                    } else {
                        let can_archive = row.role != WalletListRole::Primary || allow_archive;
                        if can_archive
                            && ui
                                .small_button(RichText::new("Archive").small())
                                .on_hover_text(
                                    "Soft-delete this wallet (reversible — historical \
                                         collections still resolve their key_hash)",
                                )
                                .clicked()
                        {
                            response.actions.push(WalletListAction::Archive {
                                account_index: row.account_index,
                            });
                        }
                    }

                    // View-UTxOs button. Opt-in via
                    // `with_view_button(true)` since not every
                    // host has a panel to render them into.
                    if view_button {
                        let clicked = ui
                            .small_button(RichText::new("UTxOs").small())
                            .on_hover_text("View on-chain UTxOs for this wallet")
                            .clicked();
                        if clicked {
                            response.actions.push(WalletListAction::ViewUtxos {
                                account_index: row.account_index,
                            });
                        }
                    }

                    // Copy-to-clipboard icon button — copies the full
                    // address (not the truncation) so the value is
                    // usable. egui has a built-in clipboard helper;
                    // we don't need to round-trip through a parent
                    // action. Hidden when no `address_full` is set.
                    if let Some(full) = &row.address_full {
                        let copy = ui
                            .small_button(RichText::new("📋").small())
                            .on_hover_text("Copy address to clipboard");
                        if copy.clicked() {
                            ui.ctx().copy_text(full.clone());
                        }
                    }

                    ui.label(
                        RichText::new(&row.address_short)
                            .color(KEYHASH_GREY)
                            .monospace()
                            .small(),
                    );
                });
            });
        });
}

fn render_card(
    ui: &mut Ui,
    row: &WalletListRow,
    allow_archive: bool,
    view_button: bool,
    response: &mut WalletListResponse,
) {
    let archived = row.archived_at.is_some();
    let (fill, stroke) = match (row.role, archived) {
        (_, true) => (ROW_BG_ARCHIVED, ROW_STROKE),
        (WalletListRole::Primary, false) => (ROW_BG_PRIMARY, ROW_STROKE_PRIMARY),
        _ => (ROW_BG, ROW_STROKE),
    };
    let (role_text, role_colour) = match row.role {
        WalletListRole::Primary => ("PRIMARY", ROLE_PRIMARY_CHIP),
        WalletListRole::Collection => ("COLLECTION", ROLE_COLLECTION_CHIP),
        WalletListRole::Custom => ("CUSTOM", ROLE_CUSTOM_CHIP),
    };

    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            // Take the full available width — when laid out in a grid the
            // parent column already constrains us; when single-column we
            // grow to the surface width.
            ui.set_width(ui.available_width());

            // ── Title row: label + #N + role pill + meta chips,
            //    right-aligned archive button ──────────────────────────
            //
            // Role is co-located with name + number (no banner above) so
            // the strongest visual cue is the *identity*, with role
            // qualifying it. Archive lives at the far right.
            ui.horizontal(|ui| {
                let title = RichText::new(&row.label).heading();
                let title = if archived {
                    title.color(META_GREY)
                } else {
                    title
                };
                ui.label(title);
                ui.label(
                    RichText::new(format!("#{}", row.account_index))
                        .monospace()
                        .color(META_GREY),
                );

                // Filled role pill — same palette as the inline list
                // chip but with a stronger background so it reads as a
                // proper tag rather than coloured text.
                Frame::new()
                    .fill(role_colour)
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(7, 1))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(role_text)
                                .color(Color32::from_rgb(20, 20, 30))
                                .small()
                                .strong(),
                        );
                    });

                if row.auto_created && row.role != WalletListRole::Collection {
                    ui.colored_label(META_GREY, RichText::new("auto").small());
                }
                if archived {
                    ui.colored_label(Color32::LIGHT_YELLOW, RichText::new("archived").small());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if archived {
                        if ui
                            .small_button(RichText::new("Restore").small())
                            .on_hover_text(
                                "Bring this wallet back from archived. \
                                     Clears `archived_at`; no key material touched.",
                            )
                            .clicked()
                        {
                            response.actions.push(WalletListAction::Unarchive {
                                account_index: row.account_index,
                            });
                        }
                    } else {
                        let can_archive = row.role != WalletListRole::Primary || allow_archive;
                        if can_archive
                            && ui
                                .small_button(RichText::new("Archive").small())
                                .on_hover_text(
                                    "Soft-delete this wallet (reversible — historical \
                                         collections still resolve their key_hash)",
                                )
                                .clicked()
                        {
                            response.actions.push(WalletListAction::Archive {
                                account_index: row.account_index,
                            });
                        }
                    }
                    if view_button
                        && ui
                            .small_button(RichText::new("UTxOs").small())
                            .on_hover_text("View on-chain UTxOs for this wallet")
                            .clicked()
                    {
                        response.actions.push(WalletListAction::ViewUtxos {
                            account_index: row.account_index,
                        });
                    }
                });
            });

            ui.add_space(8.0);

            // ── Footer: address + copy ─────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&row.address_short)
                        .monospace()
                        .color(KEYHASH_GREY),
                );
                if let Some(full) = &row.address_full {
                    let copy = ui
                        .small_button(RichText::new("📋").small())
                        .on_hover_text("Copy address to clipboard");
                    if copy.clicked() {
                        ui.ctx().copy_text(full.clone());
                    }
                }
            });

            // ── Pool badge (optional) — fuel-UTxO count + total ADA.
            // Only rendered when the host has cached UTxOs for this
            // wallet (i.e. user has hit Refresh at least once). The dot
            // is `●` (U+25CF) rather than 🟢/🟡/🔴 because the latter
            // (U+1F7E0..2) are outside egui's `default_fonts` and
            // render as the missing-glyph box.
            if let Some(pool) = &row.pool {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
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
                });
            }
        });
}
