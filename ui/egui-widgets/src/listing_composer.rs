//! ListingComposer — price a batch of assets for sale by what the BUYER pays.
//!
//! A marketplace quotes one number to a buyer. The seller therefore types
//! that number, and this widget shows, per row and in total, how it splits:
//! the fee the validator will demand and what the seller's datum will
//! promise. The arithmetic is the contract's own ([`FeeFormula`]), so the
//! preview and the transaction never disagree by a lovelace; the floor is
//! the fee output's minimum, below which the buyer pays the floor rather
//! than the percentage — which is why a cheap listing's fee looks steep.
//!
//! Follows the 4-type pattern: Config, State, Action, show(). The host adds
//! rows (from a wallet picker), reads the priced rows back, and builds.

/// The contract's fee expression and its split, re-exported so a host that
/// prices listings needs no direct dependency on the registry crate.
pub use address_registry::{FeeFormula, PriceSplit};
use egui::{RichText, Ui};

use crate::amount_input::parse_ada_input;
use crate::icons::PhosphorIcon;
use crate::theme::{Space, SpaceExt, ThemeExt};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One asset being priced.
#[derive(Clone, Debug)]
pub struct ComposerRow {
    /// Host-assigned; travels through to the build as the item id.
    pub id: String,
    /// What the seller sees: the asset's display name.
    pub label: String,
    /// Muted second line — the collection, or the policy id shortened.
    pub detail: String,
    pub quantity: u64,
    /// The buyer-pays price as typed, in ADA.
    pub price_text: String,
}

impl ComposerRow {
    pub fn new(id: impl Into<String>, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: detail.into(),
            quantity: 1,
            price_text: String::new(),
        }
    }

    pub fn with_quantity(mut self, quantity: u64) -> Self {
        self.quantity = quantity;
        self
    }

    pub fn with_price_ada(mut self, ada: f64) -> Self {
        self.price_text = format_ada_input(ada);
        self
    }

    /// The typed price in lovelace, if it parses.
    pub fn buyer_price_lovelace(&self) -> Option<u64> {
        parse_ada_input(&self.price_text)
    }
}

pub struct ListingComposerConfig {
    pub title: &'static str,
    /// The contract's fee expression.
    pub formula: FeeFormula,
    /// The fee output's minimum. Below the percentage's reach the buyer pays
    /// this instead.
    pub fee_floor_lovelace: u64,
    /// The least a row may be priced at. The host sets this above the floor
    /// so every listing leaves the seller a real payout.
    pub min_buyer_price_lovelace: u64,
    pub font_size: f32,
}

impl Default for ListingComposerConfig {
    fn default() -> Self {
        Self {
            title: "Price your listings",
            formula: FeeFormula::GrossPercent { pct: 5 },
            fee_floor_lovelace: 1_155_080,
            min_buyer_price_lovelace: 2_000_000,
            font_size: 12.0,
        }
    }
}

/// Why a row cannot be built yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowProblem {
    /// Nothing typed, or not a number.
    Unpriced,
    /// Below the host's minimum.
    BelowMinimum,
    /// So low that no positive payout fits under the fee floor.
    NoRoomForPayout,
}

impl RowProblem {
    pub fn label(self) -> &'static str {
        match self {
            RowProblem::Unpriced => "enter a price",
            RowProblem::BelowMinimum => "below minimum",
            RowProblem::NoRoomForPayout => "fee leaves nothing",
        }
    }
}

/// A row's price taken apart, or why it cannot be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowQuote {
    Ready(PriceSplit),
    Blocked(RowProblem),
}

/// The footer's figures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposerTotals {
    pub rows: usize,
    pub ready: usize,
    pub buyer_pays: u64,
    pub fee: u64,
    pub payout: u64,
}

#[derive(Default)]
pub struct ListingComposerState {
    pub rows: Vec<ComposerRow>,
    /// The "set every row to" box.
    pub bulk_price_text: String,
}

impl ListingComposerState {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Add a row; a second add of the same id replaces the first, keeping
    /// any price already typed.
    pub fn add(&mut self, row: ComposerRow) {
        match self.rows.iter_mut().find(|r| r.id == row.id) {
            Some(existing) => {
                let price_text = std::mem::take(&mut existing.price_text);
                *existing = row;
                if existing.price_text.is_empty() {
                    existing.price_text = price_text;
                }
            }
            None => self.rows.push(row),
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.rows.retain(|r| r.id != id);
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    pub fn contains(&self, id: &str) -> bool {
        self.rows.iter().any(|r| r.id == id)
    }

    /// Price one row against the config.
    pub fn quote(&self, config: &ListingComposerConfig, row: &ComposerRow) -> RowQuote {
        let Some(buyer_pays) = row.buyer_price_lovelace() else {
            return RowQuote::Blocked(RowProblem::Unpriced);
        };
        if buyer_pays < config.min_buyer_price_lovelace {
            return RowQuote::Blocked(RowProblem::BelowMinimum);
        }
        match config
            .formula
            .split_buyer_price(buyer_pays, config.fee_floor_lovelace)
        {
            Some(split) => RowQuote::Ready(split),
            None => RowQuote::Blocked(RowProblem::NoRoomForPayout),
        }
    }

    /// Every row with its quote, in order.
    pub fn quotes(&self, config: &ListingComposerConfig) -> Vec<(&ComposerRow, RowQuote)> {
        self.rows
            .iter()
            .map(|r| (r, self.quote(config, r)))
            .collect()
    }

    /// True when there is at least one row and every row is priced.
    pub fn all_ready(&self, config: &ListingComposerConfig) -> bool {
        !self.rows.is_empty()
            && self
                .rows
                .iter()
                .all(|r| matches!(self.quote(config, r), RowQuote::Ready(_)))
    }

    pub fn totals(&self, config: &ListingComposerConfig) -> ComposerTotals {
        let mut t = ComposerTotals {
            rows: self.rows.len(),
            ..ComposerTotals::default()
        };
        for row in &self.rows {
            if let RowQuote::Ready(split) = self.quote(config, row) {
                t.ready += 1;
                t.buyer_pays += split.buyer_pays;
                t.fee += split.fee;
                t.payout += split.payout;
            }
        }
        t
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListingComposerAction {
    /// A row's price text changed.
    PriceChanged {
        id: String,
    },
    Removed {
        id: String,
    },
    Cleared,
    /// The bulk box was applied to every row.
    AppliedToAll {
        buyer_price_lovelace: u64,
    },
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

pub fn show(
    ui: &mut Ui,
    state: &mut ListingComposerState,
    config: &ListingComposerConfig,
) -> Option<ListingComposerAction> {
    crate::icons::ensure_fonts(ui);
    let mut action = None;
    let size = config.font_size;
    let small = size - 2.0;

    // The widget lays itself out top-down whatever the host's direction —
    // egui's horizontal inherits it, and a right-to-left host would mirror
    // every row.
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.label(
            RichText::new(config.title)
                .color(ui.tokens().color.accent_cyan)
                .size(size + 2.0)
                .strong(),
        );
        ui.label(
            RichText::new(format!(
                "Prices are what the buyer pays. The {} fee comes out of that; below {} the \
                 buyer pays the floor instead.",
                fee_label(config.formula),
                ada(floor_reach(config))
            ))
            .color(ui.tokens().color.text_muted)
            .size(small),
        );
        ui.gap(Space::Base);

        if state.rows.is_empty() {
            ui.label(
                RichText::new("Nothing to price yet — pick assets to list.")
                    .color(ui.tokens().color.text_muted)
                    .size(size),
            );
            return;
        }

        // One grid for the header, every row and the totals, so the columns
        // share a spine whatever the labels measure.
        let quotes: Vec<RowQuote> = state.rows.iter().map(|r| state.quote(config, r)).collect();
        let totals = state.totals(config);
        let mut remove: Option<String> = None;
        let text = |t: &str, color: egui::Color32, s: f32| RichText::new(t).color(color).size(s);
        egui::Grid::new(("listing-composer", config.title))
            .num_columns(5)
            .striped(true)
            .min_col_width(88.0)
            .spacing(egui::vec2(14.0, 6.0))
            .show(ui, |ui| {
                ui.label(text("Asset", ui.tokens().color.text_muted, small));
                ui.label(text("Buyer pays ₳", ui.tokens().color.text_muted, small));
                ui.label(text("Fee", ui.tokens().color.text_muted, small));
                ui.label(text("You receive", ui.tokens().color.text_muted, small));
                ui.label("");
                ui.end_row();

                for (row, quote) in state.rows.iter_mut().zip(quotes) {
                    ui.vertical(|ui| {
                        // Wide enough that a name and a shortened policy id
                        // stay on one line each; the grid's per-column
                        // minimum is one figure for every column.
                        ui.set_min_width(170.0);
                        let name = if row.quantity > 1 {
                            format!("{} ×{}", row.label, row.quantity)
                        } else {
                            row.label.clone()
                        };
                        ui.add(
                            egui::Label::new(text(&name, ui.tokens().color.text_primary, size))
                                .truncate(),
                        );
                        if !row.detail.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    text(&row.detail, ui.tokens().color.text_muted, small)
                                        .monospace(),
                                )
                                .truncate(),
                            );
                        }
                    });
                    let edit = egui::TextEdit::singleline(&mut row.price_text)
                        .desired_width(84.0)
                        .hint_text("0.00")
                        .font(egui::TextStyle::Monospace);
                    if ui.add(edit).changed() {
                        action = Some(ListingComposerAction::PriceChanged { id: row.id.clone() });
                    }
                    match quote {
                        RowQuote::Ready(split) => {
                            ui.label(text(
                                &ada(split.fee),
                                ui.tokens().color.text_secondary,
                                size,
                            ));
                            ui.label(text(
                                &ada(split.payout),
                                ui.tokens().color.accent_green,
                                size,
                            ));
                        }
                        RowQuote::Blocked(problem) => {
                            ui.label(text(problem.label(), ui.tokens().color.warning, small));
                            ui.label("");
                        }
                    }
                    // Phosphor, not a bare `✕`: U+2715 is absent from the default
                    // font stack and rendered as tofu. `no_broken_glyphs` catches
                    // this, and muted matches how `service_banner` paints the same
                    // remove affordance.
                    if ui
                        .add(
                            egui::Button::new(
                                PhosphorIcon::X.rich_text(small, ui.tokens().color.text_muted),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        remove = Some(row.id.clone());
                    }
                    ui.end_row();
                }

                let ready = if totals.ready == totals.rows {
                    format!("{} listing{}", totals.rows, plural(totals.rows))
                } else {
                    format!("{} of {} priced", totals.ready, totals.rows)
                };
                ui.label(text(&ready, ui.tokens().color.text_secondary, size).strong());
                ui.label(
                    text(
                        &ada(totals.buyer_pays),
                        ui.tokens().color.text_primary,
                        size,
                    )
                    .strong(),
                );
                ui.label(text(&ada(totals.fee), ui.tokens().color.text_secondary, size).strong());
                ui.label(text(&ada(totals.payout), ui.tokens().color.accent_green, size).strong());
                ui.label("");
                ui.end_row();
            });
        if let Some(id) = remove {
            state.remove(&id);
            action = Some(ListingComposerAction::Removed { id });
        }

        // Bulk price + totals.
        ui.gap(Space::Sm);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Set every row to")
                    .color(ui.tokens().color.text_muted)
                    .size(small),
            );
            ui.add(
                egui::TextEdit::singleline(&mut state.bulk_price_text)
                    .desired_width(72.0)
                    .hint_text("₳")
                    .font(egui::TextStyle::Monospace),
            );
            let bulk = parse_ada_input(&state.bulk_price_text);
            if ui
                .add_enabled(
                    bulk.is_some(),
                    egui::Button::new(RichText::new("Apply").size(small)),
                )
                .clicked()
                && let Some(lovelace) = bulk
            {
                let text = state.bulk_price_text.trim().to_string();
                for row in &mut state.rows {
                    row.price_text = text.clone();
                }
                action = Some(ListingComposerAction::AppliedToAll {
                    buyer_price_lovelace: lovelace,
                });
            }
            ui.gap(Space::Xl);
            if ui
                .add(egui::Button::new(RichText::new("Clear all").size(small)))
                .clicked()
            {
                state.clear();
                action = Some(ListingComposerAction::Cleared);
            }
        });
    });

    action
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The buyer price above which the percentage, not the floor, sets the fee.
fn floor_reach(config: &ListingComposerConfig) -> u64 {
    match config.formula {
        // fee = payout * pct / (100 - pct) reaches the floor when payout =
        // floor * (100 - pct) / pct; the buyer pays payout + floor there.
        FeeFormula::GrossPercent { pct } if pct > 0 => {
            config.fee_floor_lovelace * (100 - pct) / pct + config.fee_floor_lovelace
        }
        // jpg: ~2% of gross; the floor is reached at 50× the floor.
        _ => config.fee_floor_lovelace * 50,
    }
}

fn fee_label(formula: FeeFormula) -> String {
    match formula {
        FeeFormula::GrossPercent { pct } => format!("{pct}%"),
        FeeFormula::JpgV2 => "2%".to_string(),
    }
}

pub fn ada(lovelace: u64) -> String {
    format!("{:.2} ₳", lovelace as f64 / 1_000_000.0)
}

fn format_ada_input(ada: f64) -> String {
    if ada.fract() == 0.0 {
        format!("{ada:.0}")
    } else {
        format!("{ada:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ListingComposerConfig {
        ListingComposerConfig::default()
    }

    #[test]
    fn a_priced_row_splits_into_fee_and_payout_that_sum_exactly() {
        let mut state = ListingComposerState::default();
        state.add(ComposerRow::new("a", "Perp 0011", "").with_price_ada(52.631578));
        let RowQuote::Ready(split) = state.quote(&config(), &state.rows[0]) else {
            panic!("expected a ready quote");
        };
        assert_eq!(split.buyer_pays, split.payout + split.fee);
        assert!(split.fee >= config().formula.due_on_payouts(split.payout));
        assert!(split.fee >= config().fee_floor_lovelace);
    }

    #[test]
    fn a_cheap_row_pays_the_floor_not_the_percentage() {
        let mut state = ListingComposerState::default();
        state.add(ComposerRow::new("a", "x", "").with_price_ada(10.0));
        let RowQuote::Ready(split) = state.quote(&config(), &state.rows[0]) else {
            panic!("expected a ready quote");
        };
        assert_eq!(split.fee, 1_155_080);
        assert_eq!(split.payout, 10_000_000 - 1_155_080);
    }

    #[test]
    fn rows_below_the_minimum_are_blocked_and_totals_skip_them() {
        let mut state = ListingComposerState::default();
        state.add(ComposerRow::new("a", "x", "").with_price_ada(1.0));
        state.add(ComposerRow::new("b", "y", "").with_price_ada(20.0));
        assert_eq!(
            state.quote(&config(), &state.rows[0]),
            RowQuote::Blocked(RowProblem::BelowMinimum)
        );
        let totals = state.totals(&config());
        assert_eq!((totals.rows, totals.ready), (2, 1));
        assert_eq!(totals.buyer_pays, 20_000_000);
        assert!(!state.all_ready(&config()));
    }

    #[test]
    fn re_adding_a_row_keeps_its_price() {
        let mut state = ListingComposerState::default();
        state.add(ComposerRow::new("a", "x", "").with_price_ada(20.0));
        state.add(ComposerRow::new("a", "x renamed", ""));
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.rows[0].label, "x renamed");
        assert_eq!(state.rows[0].buyer_price_lovelace(), Some(20_000_000));
    }
}
