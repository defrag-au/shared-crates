//! TX Cart widget — displays a list of pending chain actions with batch execution.
//!
//! Follows the standard 4-type pattern: Config, State, Action, show().
//! The widget is provider-agnostic — it renders items and manages the execution
//! flow, while the caller handles the actual TX building and signing.

use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, TextSize, Theme, ThemeExt};
use egui::{RichText, Ui};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Display configuration for the cart.
pub struct TxCartConfig {
    pub title: &'static str,
}

impl Default for TxCartConfig {
    fn default() -> Self {
        Self { title: "TX Cart" }
    }
}

/// A single item in the cart.
#[derive(Clone, Debug)]
pub struct TxCartItem {
    pub id: String,
    /// Collection/asset name (e.g., "Helmies")
    pub label: String,
    /// Policy ID (truncated for display)
    pub policy_id: String,
    /// Provider name (e.g., "jpg.store")
    pub provider: String,
    /// Action type for grouping display (e.g., "Created coll. offers", "Cancel coll. offers")
    pub action_label: String,
    /// Number of offers in this item
    pub quantity: u32,
    /// What the whole row costs.
    pub price: TxCartPrice,
    /// Optional hero image URL for the collection
    pub image_url: Option<String>,
    pub status: TxCartItemStatus,
}

/// What a row costs, as far as anyone knows yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TxCartPrice {
    /// In ADA, for the whole row (not per unit).
    Total(f64),
    /// Priced by a build that has not happened — a row whose only cost is the
    /// fees of transactions not yet built. Drawn as "TBA": as a zero it read
    /// "0.00 ADA", a figure, and a false one.
    Tba,
}

impl TxCartPrice {
    pub fn figure(&self) -> String {
        match self {
            TxCartPrice::Total(ada) => ada_figure(*ada),
            TxCartPrice::Tba => TBA.to_string(),
        }
    }
}

const TBA: &str = "TBA";

/// Status of a cart item.
#[derive(Clone, Debug, PartialEq)]
pub enum TxCartItemStatus {
    Pending,
    Building,
    Signing,
    Signed,
    Submitting,
    Submitted { tx_hash: String },
    Error { message: String },
}

impl TxCartItemStatus {
    pub fn label(&self) -> &str {
        match self {
            TxCartItemStatus::Pending => "Pending",
            TxCartItemStatus::Building => "Building...",
            TxCartItemStatus::Signing => "Signing...",
            TxCartItemStatus::Signed => "Signed",
            TxCartItemStatus::Submitting => "Submitting...",
            TxCartItemStatus::Submitted { .. } => "Submitted",
            TxCartItemStatus::Error { .. } => "Error",
        }
    }

    /// Takes the theme rather than reading one: a status is a plain value with no
    /// `Ui` of its own, and baking the palette here would make the cart the one
    /// widget a theme could not reach.
    pub fn color(&self, t: &Theme) -> egui::Color32 {
        match self {
            TxCartItemStatus::Pending => t.color.text_muted,
            TxCartItemStatus::Building => t.color.accent_cyan,
            TxCartItemStatus::Signing => t.color.accent_cyan,
            TxCartItemStatus::Signed => t.color.accent_green,
            TxCartItemStatus::Submitting => t.color.accent_cyan,
            TxCartItemStatus::Submitted { .. } => t.color.accent_green,
            TxCartItemStatus::Error { .. } => t.color.accent_red,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TxCartItemStatus::Submitted { .. } | TxCartItemStatus::Error { .. }
        )
    }
}

/// A planned transaction grouping cart items.
#[derive(Clone, Debug)]
pub struct TxCartPlannedTx {
    pub unsigned_tx_cbor: String,
    pub fee: u64,
    pub item_ids: Vec<String>,
    pub summary: String,
    /// What the builder wants read before signing. Shown under the row when
    /// the reader opens it, not beside it — see [`TxCartState::open_tx`].
    pub review: Vec<TxCartReviewRow>,
}

/// One line of a transaction's detail.
///
/// The widget's own type rather than the cart wire's, so the catalogue stays
/// domain-free; the client maps across.
#[derive(Clone, Debug)]
pub struct TxCartReviewRow {
    pub label: String,
    pub value: String,
    /// An on-chain identifier renders middle-elided with a copy button. In
    /// full it is unreadable, uncopyable, and sets the width of the column.
    pub is_reference: bool,
}

/// Cart execution phase.
#[derive(Clone, Debug, PartialEq)]
pub enum TxCartPhase {
    /// User is adding/removing items.
    Editing,
    /// Server is building TXs.
    Building,
    /// TXs built, showing preview.
    Preview,
    /// Signing and submitting TXs sequentially.
    Executing { total: usize, completed: usize },
    /// All done.
    Done,
    /// Error during build/execute.
    Error { message: String },
}

/// Whether a phase lets the operator change what is IN the cart.
///
/// A named decision rather than a bare `matches!` at the call site, because
/// getting it wrong is not a cosmetic bug: the remove control used to require
/// [`TxCartPhase::Editing`], so a build that failed left the cart in
/// [`TxCartPhase::Error`] with no per-row remove and a footer offering only
/// "Retry" (which fails identically) and "Clear" (which discards everything).
/// One unbuyable listing therefore cost you the whole cart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowEditing {
    Allowed,
    /// The cart is mid-flight or already committed; changing it now would
    /// invalidate work in progress or rewrite history.
    Locked,
}

impl TxCartPhase {
    /// Exhaustive by design — a new phase must state its answer here rather
    /// than inherit one from a wildcard.
    pub fn row_editing(&self) -> RowEditing {
        match self {
            // Error is editable ON PURPOSE: removing the offending item is the
            // only way to make the cart buildable again.
            TxCartPhase::Editing | TxCartPhase::Error { .. } => RowEditing::Allowed,
            TxCartPhase::Building
            | TxCartPhase::Preview
            | TxCartPhase::Executing { .. }
            | TxCartPhase::Done => RowEditing::Locked,
        }
    }
}

/// Cart state — managed by the caller, rendered by the widget.
pub struct TxCartState {
    pub items: Vec<TxCartItem>,
    pub planned_txs: Vec<TxCartPlannedTx>,
    pub phase: TxCartPhase,
    /// Which planned transaction has its detail open, if any.
    ///
    /// One at a time: the detail is long — a two-leg route runs to a dozen
    /// rows — and a cart that opened all of them at once is the wall of text
    /// this replaced. An `Option` rather than a set gives that for free.
    pub open_tx: Option<usize>,
}

impl Default for TxCartState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            planned_txs: Vec::new(),
            phase: TxCartPhase::Editing,
            open_tx: None,
        }
    }
}

impl TxCartState {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.status, TxCartItemStatus::Pending))
            .count()
    }

    pub fn add_item(&mut self, item: TxCartItem) {
        self.items.push(item);
        self.phase = TxCartPhase::Editing;
    }

    pub fn remove_item(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
        if self.items.is_empty() {
            self.phase = TxCartPhase::Editing;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.planned_txs.clear();
        self.phase = TxCartPhase::Editing;
    }

    /// Update item statuses for a given TX's items.
    pub fn set_items_status(&mut self, item_ids: &[String], status: TxCartItemStatus) {
        for item in &mut self.items {
            if item_ids.contains(&item.id) {
                item.status = status.clone();
            }
        }
    }
}

/// Actions emitted by the cart widget for the caller to handle.
#[derive(Debug)]
pub enum TxCartAction {
    /// Remove an item from the cart.
    RemoveItem(String),
    /// Build all pending items into TXs (call /api/build-cart).
    Execute,
    /// Sign and submit a specific planned TX.
    SignTx(usize),
    /// Go back to editing (from Preview).
    BackToEditing,
    /// Clear the cart.
    Clear,
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

/// Render the TX cart widget. Renders title + items + footer
/// inline. Most callers want this.
///
/// For panel layouts that need to pin the footer (Total +
/// Prepare button) at the bottom while the items list scrolls
/// independently — typical of long carts in a fixed-height side
/// panel — call [`show_items`] inside a `ScrollArea` and
/// [`show_footer`] inside a `TopBottomPanel::bottom`. Both halves
/// can return actions, so the caller has to merge them.
pub fn show(ui: &mut Ui, state: &mut TxCartState, config: &TxCartConfig) -> Option<TxCartAction> {
    let mut action = show_items(ui, state, config);
    if let Some(a) = show_footer(ui, state) {
        action = Some(a);
    }
    action
}

/// Render the cart's title + items list. Pair with
/// [`show_footer`] when you need a pinned footer (e.g. inside a
/// `SidePanel` where the items overflow but Total + Prepare must
/// stay visible). For a single inline render use [`show`].
pub fn show_items(
    ui: &mut Ui,
    state: &mut TxCartState,
    config: &TxCartConfig,
) -> Option<TxCartAction> {
    let mut action = None;

    // Title
    ui.label(
        RichText::new(config.title)
            .color(ui.tokens().color.text_primary)
            .size(ui.text_size(TextSize::Xl2))
            .strong(),
    );
    ui.gap(Space::Sm);

    if state.items.is_empty() {
        ui.gap(Space::Xl2);
        ui.label(
            RichText::new("Your cart is empty")
                .color(ui.tokens().color.text_muted)
                .size(ui.text_size(TextSize::Md)),
        );
        ui.gap(Space::Sm);
        ui.label(
            RichText::new("Add offers from the Browse tab")
                .color(ui.tokens().color.text_muted)
                .size(ui.text_size(TextSize::Sm)),
        );
        return action;
    }

    // Group items by action_label for section display
    let mut groups: Vec<(String, Vec<&TxCartItem>)> = Vec::new();
    for item in &state.items {
        if let Some(group) = groups
            .iter_mut()
            .find(|(label, _)| *label == item.action_label)
        {
            group.1.push(item);
        } else {
            groups.push((item.action_label.clone(), vec![item]));
        }
    }

    // Render each group
    let mut remove_id = None;

    for (group_label, items) in &groups {
        // No per-section ADA total beside the heading. Set in the debit red
        // next to a verb, "Buy · 20 ADA" read as a charge on the section
        // itself, and it repeated the footer's Total for the common one-section
        // cart — while for a section of cancellations it painted ADA coming
        // BACK as money going out.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(group_label)
                    .color(ui.tokens().color.text_primary)
                    .size(ui.text_size(TextSize::Lg))
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if matches!(state.phase, TxCartPhase::Editing)
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new("Clear")
                                    .color(ui.tokens().color.text_muted)
                                    .size(ui.text_size(TextSize::Sm)),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    action = Some(TxCartAction::Clear);
                }
            });
        });

        ui.gap(Space::Xs);
        ui.separator();
        ui.gap(Space::Sm);

        // Item cards
        for item in items {
            ui.horizontal(|ui| {
                // Collection image placeholder (only show if we have a URL)
                if let Some(ref url) = item.image_url {
                    let image = egui::Image::new(url.as_str())
                        .fit_to_exact_size(egui::vec2(44.0, 44.0))
                        .corner_radius(ui.tokens().corner(Radius::Base));
                    ui.add(image);
                    ui.gap(Space::Base);
                }

                // Info column
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&item.label)
                            .color(ui.tokens().color.text_primary)
                            .size(ui.text_size(TextSize::Md))
                            .strong(),
                    );
                    // Truncated policy ID
                    let pid = &item.policy_id;
                    if !pid.is_empty() {
                        let truncated = if pid.len() > 16 {
                            format!("{}...{}", &pid[..8], &pid[pid.len() - 4..])
                        } else {
                            pid.clone()
                        };
                        ui.label(
                            RichText::new(truncated)
                                .color(ui.tokens().color.text_muted)
                                .size(ui.text_size(TextSize::Xs))
                                .monospace(),
                        );
                    }

                    // Status (if not pending)
                    match &item.status {
                        TxCartItemStatus::Pending => {}
                        TxCartItemStatus::Submitted { tx_hash } => {
                            let short = if tx_hash.len() > 16 {
                                format!("{}...{}", &tx_hash[..8], &tx_hash[tx_hash.len() - 4..])
                            } else {
                                tx_hash.clone()
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    PhosphorIcon::CheckCircle
                                        .rich_text(10.0, ui.tokens().color.accent_green),
                                );
                                ui.label(
                                    RichText::new(short)
                                        .color(ui.tokens().color.accent_green)
                                        .size(ui.text_size(TextSize::Xs))
                                        .monospace(),
                                );
                            });
                        }
                        other => {
                            ui.label(
                                RichText::new(other.label())
                                    .color(other.color(&ui.tokens()))
                                    .size(ui.text_size(TextSize::Xs)),
                            );
                        }
                    }
                });

                // Right side: quantity x price + remove
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Remove button. Gated on the phase's own answer, and
                    // on the ROW not already being committed — a row that
                    // is building or submitted cannot be taken back.
                    if matches!(
                        item.status,
                        TxCartItemStatus::Pending | TxCartItemStatus::Error { .. }
                    ) && state.phase.row_editing() == RowEditing::Allowed
                    {
                        if ui
                            .add(
                                egui::Button::new(
                                    PhosphorIcon::Trash
                                        .rich_text(14.0, ui.tokens().color.text_muted),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            remove_id = Some(item.id.clone());
                        }
                        ui.gap(Space::Sm);
                    }

                    // Price
                    ui.label(
                        RichText::new(item.price.figure())
                            .color(ui.tokens().color.text_primary)
                            .size(ui.text_size(TextSize::Base)),
                    );
                    if item.quantity > 1 {
                        ui.label(
                            RichText::new(format!("{}x", item.quantity))
                                .color(ui.tokens().color.text_muted)
                                .size(ui.text_size(TextSize::Sm)),
                        );
                    }
                });
            });
            // No border per row. It was a hairline painted 2px outside the
            // content, so every row sat in a box barely larger than its own
            // text — cramped, and a frame the rows never needed: the section
            // heading and its rule already group them, and the gap below
            // separates one from the next.

            // Error detail (truncated to first line)
            if let TxCartItemStatus::Error { message } = &item.status {
                let short = message.lines().next().unwrap_or(message);
                let short = if short.len() > 80 {
                    format!("{}...", &short[..77])
                } else {
                    short.to_string()
                };
                ui.label(
                    RichText::new(short)
                        .color(ui.tokens().color.accent_red)
                        .size(ui.text_size(TextSize::Xs)),
                );
            }

            // `Md`, not `Sm`: without a border, two-line rows four points apart
            // run together into one block.
            ui.gap(Space::Md);
        }

        ui.gap(Space::Md);
    }

    if let Some(id) = remove_id {
        action = Some(TxCartAction::RemoveItem(id));
    }

    action
}

/// Render the cart's bottom action area — Total + Prepare button
/// in `Editing`, Sign & Submit in `Preview`, error/success
/// states, etc. Standalone counterpart to [`show_items`]; both
/// halves are combined by [`show`] for the inline-render path.
/// One transaction's detail, as a label/value grid.
///
/// An identifier goes through [`IdPill`](crate::id_pill::IdPill) rather than
/// being printed: in full, a UTxO reference is 66 characters that nobody can
/// read, nobody can copy, and that sets the width of the whole sidebar.
fn review_rows(ui: &mut Ui, rows: &[TxCartReviewRow]) {
    // Compact by STRUCTURE, not by type size.
    //
    // A first pass bought density by dropping these rows to `Xs`/`Sm` while
    // everything around them stayed at body size, which made the one part of
    // the cart a signer must read into fine print. The height and width were
    // never in the glyphs: they were in a label column sized as a flat 40% of
    // the sidebar — a dead gutter a hundred-odd points wide — and in labels
    // top-aligned above their own values. So: body size throughout, hierarchy
    // carried by colour, a label column exactly as wide as its widest label,
    // and one shared centre line per row.
    let body = ui.text_size(TextSize::Base);
    let muted = ui.tokens().color.text_muted;
    let ink = ui.tokens().color.text_primary;

    // Measured OUTSIDE the rows, and the values WRAP. `available_width` read
    // inside a `horizontal` is what is left after whatever has already been
    // placed in it; and a `horizontal` whose content overflows widens its
    // PARENT rather than clipping — one long value once pushed the whole cart
    // wider than the drawer holding it.
    let font = egui::FontId::proportional(body);
    let widest = rows
        .iter()
        .map(|row| {
            ui.painter()
                .layout_no_wrap(row.label.clone(), font.clone(), muted)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let total = ui.available_width();
    // Half the width at most: a label long enough to want more wraps, rather
    // than squeezing every value in the list.
    let label_width = (widest + ui.tokens().space(Space::Md)).min(total * 0.5);

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = ui.tokens().space(Space::Xs);
        for row in rows {
            review_row(ui, row, label_width, body, muted, ink);
        }
    });
}

/// One label/value line of [`review_rows`].
fn review_row(
    ui: &mut Ui,
    row: &TxCartReviewRow,
    label_width: f32,
    body: f32,
    muted: egui::Color32,
    ink: egui::Color32,
) {
    ui.horizontal(|ui| {
        // The label is a DIRECT child of the row, padded out to the column
        // width — not a nested cell. A nested `allocate_ui_with_layout` centres
        // its label on the cell's own height rather than the row's, which sat
        // every label a few points below its value and made each row taller
        // than its text. As siblings, both share the row's one centre line.
        // Labels are short by construction (the column is sized to the widest
        // of them), so nothing here needs to wrap; the value still can.
        let label = ui.label(RichText::new(&row.label).color(muted).size(body));
        ui.add_space((label_width - label.rect.width()).max(0.0));
        // Values left-aligned against the label column, NOT ragged-right.
        //
        // Two reasons, and the second is the load-bearing one. A label/value
        // list reads down its value edge, so a common left edge is what makes
        // it scannable. And `IdPill` lays itself out `value, copy, link` with
        // a plain `ui.horizontal`, which takes its direction from the parent
        // — right-aligning the cell put the copy button in front of the value
        // it copies.
        if row.is_reference {
            // At body size like its neighbours. `IdPill`'s inline default is
            // `.small()`, which set the one row a signer may need to copy a
            // size smaller than the amounts either side of it.
            crate::id_pill::IdPill::new("", &row.value)
                .layout(crate::id_pill::IdPillLayout::Inline)
                .with_widths(8, 6)
                .value_size(TextSize::Base)
                .show(ui);
        } else {
            ui.add(egui::Label::new(RichText::new(&row.value).color(ink).size(body)).wrap());
        }
    });
}

/// An ADA figure that never reads as zero unless it IS zero.
///
/// This column used to be a flat `{:.0}`, which is right for a cart of NFT
/// listings priced in the hundreds and silently wrong for anything smaller:
/// a half-ADA charge rendered "0 ADA", indistinguishable from free. Whole
/// numbers where the magnitude carries the meaning, more places where
/// dropping them would round a real amount away.
fn ada_figure(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 1000.0 {
        format!("{value:.0} ADA")
    } else if magnitude >= 0.005 || value == 0.0 {
        format!("{value:.2} ADA")
    } else {
        format!("{value:.6} ADA")
    }
}

/// A sum of row prices: the known part, plus a note when some of it is not
/// priced yet. Counting an unpriced row as zero would understate the total
/// exactly as the row itself used to.
fn total_figure(prices: impl Iterator<Item = TxCartPrice>) -> String {
    let (mut known, mut any_known, mut unpriced) = (0.0, false, false);
    for price in prices {
        match price {
            TxCartPrice::Total(ada) => {
                known += ada;
                any_known = true;
            }
            TxCartPrice::Tba => unpriced = true,
        }
    }
    match (any_known, unpriced) {
        (_, false) => ada_figure(known),
        (false, true) => TBA.to_string(),
        (true, true) => format!("{} + {TBA}", ada_figure(known)),
    }
}

pub fn show_footer(ui: &mut Ui, state: &mut TxCartState) -> Option<TxCartAction> {
    let mut action = None;

    // Empty cart has no footer; nothing to show.
    if state.items.is_empty() {
        return action;
    }

    ui.gap(Space::Sm);

    // Bottom action area
    match &state.phase {
        TxCartPhase::Editing => {
            if state.pending_count() > 0 {
                let total = total_figure(
                    state
                        .items
                        .iter()
                        .filter(|i| matches!(i.status, TxCartItemStatus::Pending))
                        .map(|i| i.price),
                );

                ui.separator();
                ui.gap(Space::Sm);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Total: {total}"))
                            .color(ui.tokens().color.text_secondary)
                            .size(ui.text_size(TextSize::Base)),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Prepare")
                                        .color(ui.tokens().color.bg_primary)
                                        .size(ui.text_size(TextSize::Lg))
                                        .strong(),
                                )
                                .fill(ui.tokens().color.accent_green)
                                .corner_radius(ui.tokens().corner(Radius::Md))
                                .min_size(egui::vec2(100.0, 32.0)),
                            )
                            .clicked()
                        {
                            action = Some(TxCartAction::Execute);
                        }
                    });
                });
            }
        }

        TxCartPhase::Building => {
            // Was a bare `ui.spinner()`, which takes `interact_size.y` — and
            // under touch sizing that is floored at the 44pt tap target, so
            // the mark came out roughly three times the height of the words
            // beside it.
            crate::labelled_progress::LabelledProgress::new("Building transactions")
                .size(TextSize::Md)
                .colour(ui.tokens().color.accent_cyan)
                .show(ui);
        }

        TxCartPhase::Preview => {
            ui.separator();
            ui.gap(Space::Sm);

            ui.label(
                RichText::new(format!(
                    "{} transaction(s) to sign",
                    state.planned_txs.len()
                ))
                .color(ui.tokens().color.text_secondary)
                .size(ui.text_size(TextSize::Base)),
            );
            ui.gap(Space::Sm);

            // Each row IS the disclosure for its own detail.
            //
            // The detail used to render as a card per transaction below the
            // list, which said everything twice: the summary and the fee were
            // in the row AND heading the card. Folding it under the row it
            // describes removes the repetition and the scrolling both.
            let mut toggle: Option<usize> = None;
            for (i, planned) in state.planned_txs.iter().enumerate() {
                let open = state.open_tx == Some(i);
                let has_detail = !planned.review.is_empty();

                let row = ui
                    .scope(|ui| {
                        ui.horizontal(|ui| {
                            if has_detail {
                                let caret = if open {
                                    PhosphorIcon::CaretDown
                                } else {
                                    PhosphorIcon::CaretRight
                                };
                                ui.label(caret.rich_text(
                                    ui.text_size(TextSize::Sm),
                                    ui.tokens().color.text_muted,
                                ));
                            }
                            // Body size, like the detail it opens. A header
                            // set smaller than its own rows reads upside down.
                            ui.label(
                                RichText::new(format!("TX {}", i + 1))
                                    .color(ui.tokens().color.text_muted)
                                    .size(ui.text_size(TextSize::Base)),
                            );
                            ui.label(
                                RichText::new(&planned.summary)
                                    .color(ui.tokens().color.text_primary)
                                    .size(ui.text_size(TextSize::Base)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.2} ADA fee",
                                            planned.fee as f64 / 1_000_000.0
                                        ))
                                        .color(ui.tokens().color.text_muted)
                                        .size(ui.text_size(TextSize::Sm)),
                                    );
                                },
                            );
                        });
                    })
                    .response
                    .interact(if has_detail {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    });
                if has_detail {
                    if row.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if row.clicked() {
                        // Clicking the open one closes it.
                        toggle = Some(i);
                    }
                }

                crate::disclosure::Disclosure::new(("tx_cart_review", i), open).show(ui, |ui| {
                    review_rows(ui, &planned.review);
                });
            }
            if let Some(i) = toggle {
                state.open_tx = (state.open_tx != Some(i)).then_some(i);
            }

            ui.gap(Space::Base);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("< Edit")
                                .color(ui.tokens().color.text_muted)
                                .size(ui.text_size(TextSize::Base)),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    action = Some(TxCartAction::BackToEditing);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Sign & Submit")
                                    .color(ui.tokens().color.bg_primary)
                                    .size(ui.text_size(TextSize::Lg))
                                    .strong(),
                            )
                            .fill(ui.tokens().color.accent_green)
                            .corner_radius(ui.tokens().corner(Radius::Md))
                            .min_size(egui::vec2(120.0, 32.0)),
                        )
                        .clicked()
                    {
                        action = Some(TxCartAction::SignTx(0));
                    }
                });
            });
        }

        TxCartPhase::Executing { total, completed } => {
            ui.horizontal(|ui| {
                ui.spinner();
                let label = if *completed == 0
                    && state
                        .items
                        .iter()
                        .all(|i| matches!(i.status, TxCartItemStatus::Signing))
                {
                    "Waiting for wallet...".to_string()
                } else {
                    format!("Submitting {completed}/{total}...")
                };
                ui.label(
                    RichText::new(label)
                        .color(ui.tokens().color.accent_cyan)
                        .size(ui.text_size(TextSize::Md)),
                );
            });
        }

        TxCartPhase::Done => {
            ui.separator();
            ui.gap(Space::Sm);
            ui.horizontal(|ui| {
                ui.label(PhosphorIcon::CheckCircle.rich_text(16.0, ui.tokens().color.accent_green));
                ui.label(
                    RichText::new("All transactions submitted")
                        .color(ui.tokens().color.accent_green)
                        .size(ui.text_size(TextSize::Lg))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Clear")
                                    .color(ui.tokens().color.text_primary)
                                    .size(ui.text_size(TextSize::Md)),
                            )
                            .fill(ui.tokens().color.bg_secondary)
                            .corner_radius(ui.tokens().corner(Radius::Md))
                            .min_size(egui::vec2(70.0, 28.0)),
                        )
                        .clicked()
                    {
                        action = Some(TxCartAction::Clear);
                    }
                });
            });
        }

        TxCartPhase::Error { message } => {
            let short = message.lines().next().unwrap_or(message);
            let short = if short.len() > 100 {
                format!("{}...", &short[..97])
            } else {
                short.to_string()
            };
            ui.label(
                RichText::new(format!("Error: {short}"))
                    .color(ui.tokens().color.accent_red)
                    .size(ui.text_size(TextSize::Base)),
            );
            ui.gap(Space::Sm);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Retry")
                                .color(ui.tokens().color.text_primary)
                                .size(ui.text_size(TextSize::Md)),
                        )
                        .fill(ui.tokens().color.bg_secondary)
                        .corner_radius(ui.tokens().corner(Radius::Md))
                        .min_size(egui::vec2(80.0, 30.0)),
                    )
                    .clicked()
                {
                    action = Some(TxCartAction::Execute);
                }
                ui.gap(Space::Md);
                // Between "try the identical thing again" and "throw the whole
                // cart away" there has to be a middle option, or a single bad
                // item costs the operator everything else they queued.
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Edit cart")
                                .color(ui.tokens().color.text_primary)
                                .size(ui.text_size(TextSize::Md)),
                        )
                        .fill(ui.tokens().color.bg_secondary)
                        .corner_radius(ui.tokens().corner(Radius::Md))
                        .min_size(egui::vec2(80.0, 30.0)),
                    )
                    .clicked()
                {
                    action = Some(TxCartAction::BackToEditing);
                }
                ui.gap(Space::Md);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Clear")
                                .color(ui.tokens().color.text_muted)
                                .size(ui.text_size(TextSize::Md)),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    action = Some(TxCartAction::Clear);
                }
            });
        }
    }

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real charge must never render as "0 ADA".
    ///
    /// The routes cart showed exactly that for a 20 ADA buy, and it was
    /// indistinguishable from a cart that was genuinely free — so the figure
    /// never disagrees with the row's own label about whether money moves.
    #[test]
    fn a_nonzero_charge_never_renders_as_zero() {
        assert_eq!(ada_figure(20.0), "20.00 ADA");
        assert_eq!(ada_figure(0.5), "0.50 ADA");
        assert_eq!(ada_figure(0.000001), "0.000001 ADA");
        assert_eq!(ada_figure(0.004), "0.004000 ADA");
        // Zero is the one case allowed to say zero.
        assert_eq!(ada_figure(0.0), "0.00 ADA");
    }

    /// A row priced by its build reads TBA, never "0.00 ADA", and a total that
    /// includes one says so.
    #[test]
    fn an_unpriced_row_reads_tba() {
        assert_eq!(TxCartPrice::Tba.figure(), "TBA");
        assert_eq!(total_figure([TxCartPrice::Tba].into_iter()), "TBA");
        assert_eq!(
            total_figure([TxCartPrice::Total(10.0), TxCartPrice::Tba].into_iter()),
            "10.00 ADA + TBA"
        );
        assert_eq!(
            total_figure([TxCartPrice::Total(10.0)].into_iter()),
            "10.00 ADA"
        );
    }

    /// Large carts keep the whole-number column they were designed around.
    #[test]
    fn large_totals_stay_whole() {
        assert_eq!(ada_figure(1_247.0), "1247 ADA");
        assert_eq!(ada_figure(999.5), "999.50 ADA");
    }

    /// A sell hands ADA back; the sign is the only honest way to say so.
    #[test]
    fn a_receipt_reads_negative() {
        assert_eq!(ada_figure(-20.0), "-20.00 ADA");
    }
}
