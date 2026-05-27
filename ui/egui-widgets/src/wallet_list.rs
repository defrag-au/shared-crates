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
//! ## Usage
//!
//! ```ignore
//! let rows: Vec<WalletListRow> = detail.wallets.iter().map(to_vm).collect();
//! let resp = WalletList::new(&rows)
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
}

/// Builder.
pub struct WalletList<'a> {
    rows: &'a [WalletListRow],
    can_archive_primary: bool,
    show_section_headers_for_single: bool,
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
            can_archive_primary: false,
            show_section_headers_for_single: false,
        }
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

        let populated_buckets =
            usize::from(!primary.is_empty()) + usize::from(!collections.is_empty()) + usize::from(!custom.is_empty());
        let show_headers = self.show_section_headers_for_single || populated_buckets > 1;

        render_bucket(
            ui,
            "Primary",
            &primary,
            show_headers,
            self.can_archive_primary,
            &mut response,
        );

        if !primary.is_empty()
            && (!collections.is_empty() || !custom.is_empty())
        {
            ui.add_space(8.0);
        }

        render_bucket(
            ui,
            "Collections",
            &collections,
            show_headers,
            /* allow_archive = */ true,
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
            /* allow_archive = */ true,
            &mut response,
        );

        response
    }
}

// ─────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────

fn render_bucket(
    ui: &mut Ui,
    title: &str,
    rows: &[&WalletListRow],
    show_header: bool,
    allow_archive: bool,
    response: &mut WalletListResponse,
) {
    if rows.is_empty() {
        return;
    }

    if show_header {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .color(SECTION_HEADER)
                    .small()
                    .strong(),
            );
            ui.label(
                RichText::new(format!("({})", rows.len()))
                    .color(META_GREY)
                    .small(),
            );
        });
        ui.add_space(4.0);
    }

    for r in rows {
        render_row(ui, r, allow_archive, response);
        ui.add_space(4.0);
    }
}

fn render_row(
    ui: &mut Ui,
    row: &WalletListRow,
    allow_archive: bool,
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
                        .color(if archived { META_GREY } else { Color32::from_gray(200) }),
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
                    ui.colored_label(
                        Color32::LIGHT_YELLOW,
                        RichText::new("archived").small(),
                    );
                }

                // ── Address (right-aligned with copy + archive) ─────
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Archive button — placed first because of the
                        // right-to-left layout (it ends up rightmost).
                        // Hidden for Primary (unless explicitly enabled)
                        // and already-archived rows.
                        let can_archive = !archived
                            && (row.role != WalletListRole::Primary || allow_archive);
                        if can_archive {
                            let clicked = ui
                                .small_button(RichText::new("Archive").small())
                                .on_hover_text(
                                    "Soft-delete this wallet (reversible — historical \
                                     collections still resolve their key_hash)",
                                )
                                .clicked();
                            if clicked {
                                response.actions.push(WalletListAction::Archive {
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
                    },
                );
            });
        });
}
