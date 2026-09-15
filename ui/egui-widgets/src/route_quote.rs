//! `RouteQuote` — a multi-venue route priced leg by leg, and how many transactions it takes to settle.
//!
//! NOT [`crate::route_summary`], which is *split* routing: one trade fanned
//! across several DEXes in parallel for the same pair, scored against the best
//! single pool. This is *sequential* routing — each leg's output is the next
//! leg's input, the assets change at every hop, and the interesting facts are
//! per-leg fee buckets in DIFFERENT assets and where the transaction
//! boundaries fall.
//!
//! Two things this exists to say plainly, because they are what a reader
//! actually needs before signing:
//!
//! - **There is no slippage.** Every leg names the exact contract UTxO it
//!   spends, so the route settles at these numbers or fails at phase 1 for
//!   free. There is deliberately no tolerance control anywhere near this
//!   widget — if you are reaching for [`crate::slippage_selector`] on a route
//!   surface, the route is the wrong shape.
//! - **What a partial failure leaves you holding.** When a venue refuses to
//!   share a transaction the route splits, and a reader is entitled to know
//!   exactly what lands in their own wallet if the second transaction never
//!   does. That is the hand-off callout, and it is the reason
//!   [`QuotePlan::Chained`] carries it rather than a bare count.
//!
//! The widget renders a quote; it never computes one. All arithmetic lives in
//! `cardano_tx::route`.

use egui::{RichText, Ui};

use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};

// ============================================================================
// Types
// ============================================================================

/// A quantity with enough context to render itself the same way everywhere.
///
/// Decimals travel WITH the amount because a route mixes assets that disagree
/// about them — ADA has six, a LumpPad token has none — and a widget that
/// assumed either would silently mis-state the other by a factor of a million.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amount {
    pub quantity: u64,
    pub decimals: u8,
    pub ticker: String,
}

impl Amount {
    pub fn new(quantity: u64, decimals: u8, ticker: impl Into<String>) -> Self {
        Self {
            quantity,
            decimals,
            ticker: ticker.into(),
        }
    }

    /// Lovelace, rendered as ADA.
    pub fn ada(lovelace: u64) -> Self {
        Self::new(lovelace, 6, "ADA")
    }

    /// The number alone, grouped, with decimals applied.
    pub fn figure(&self) -> String {
        if self.decimals == 0 {
            return group(self.quantity);
        }
        let scale = 10u64.pow(u32::from(self.decimals));
        let whole = self.quantity / scale;
        let frac = self.quantity % scale;
        format!(
            "{}.{:0width$}",
            group(whole),
            frac,
            width = usize::from(self.decimals)
        )
    }

    /// The number alone, grouped, at no more than `max_decimals` places.
    ///
    /// For a figure read for its magnitude rather than its exactness — a pool
    /// balance of ₳28,108.24 says everything ₳28,108.241247 does, with six
    /// fewer digits to scan past. Rounds rather than truncates, so the last
    /// place shown is the nearest one.
    ///
    /// Never use this for an amount a user is agreeing to pay or receive.
    /// Those are exact by construction and must print in full.
    pub fn figure_capped(&self, max_decimals: u8) -> String {
        if max_decimals >= self.decimals {
            return self.figure();
        }
        let dropped = 10u64.pow(u32::from(self.decimals - max_decimals));
        let kept = self.quantity / dropped;
        // Half-up on the last place kept. `dropped` is a power of ten above
        // one here, so it is even and the comparison is exact.
        let rounded = kept + u64::from((self.quantity % dropped) * 2 >= dropped);
        Self::new(rounded, max_decimals, self.ticker.clone()).figure()
    }

    /// Number and ticker.
    pub fn display(&self) -> String {
        format!("{} {}", self.figure(), self.ticker)
    }
}

/// One named fee bucket, in whatever asset the venue charges it in.
#[derive(Clone, Debug)]
pub struct QuoteFee {
    pub label: String,
    pub amount: Amount,
}

/// One hop.
#[derive(Clone, Debug)]
pub struct QuoteLeg {
    /// The venue, as a reader would name it — "Splash", "LumpPad".
    pub venue: String,
    pub amount_in: Amount,
    pub amount_out: Amount,
    /// Every bucket this leg charges, by name. Enumerated rather than summed
    /// so a reader can cross-check against the venue's own UI.
    pub fees: Vec<QuoteFee>,
    /// Input this leg could not place, which comes straight back.
    pub returned: Option<Amount>,
    /// Which transaction of the plan this leg settles in, zero-based.
    pub transaction: usize,
}

/// What the user is left holding if the NEXT transaction never lands.
#[derive(Clone, Debug)]
pub struct Handoff {
    /// Zero-based index of the transaction that produces it.
    pub after_transaction: usize,
    /// At the user's OWN address, spendable by their own key.
    pub holding: Vec<Amount>,
}

/// How many transactions the route takes, and what sits between them.
#[derive(Clone, Debug)]
pub enum QuotePlan {
    /// One transaction. Settles or fails as a unit — there is no state in
    /// which the user holds an intermediate asset.
    Atomic,
    /// Several, submitted in order. Carries the hand-offs rather than a count
    /// because the count alone does not tell a reader what they risk.
    Chained { handoffs: Vec<Handoff> },
}

impl QuotePlan {
    pub fn transactions(&self) -> usize {
        match self {
            QuotePlan::Atomic => 1,
            QuotePlan::Chained { handoffs } => handoffs.len() + 1,
        }
    }
}

/// A priced route.
#[derive(Clone, Debug)]
pub struct RouteQuoteData {
    pub legs: Vec<QuoteLeg>,
    /// What the user parts with.
    pub pay: Amount,
    /// What the user receives.
    pub receive: Amount,
    pub price_impact_bps: u32,
    /// Summed across every transaction in the plan, when it is known.
    ///
    /// `None` until the route is BUILT. A quote prices the trade against pool
    /// state; the fee depends on the body — which inputs coin selection took,
    /// how many script bytes are referenced, what ex-units the validators
    /// booked. Rendering an unknown fee as zero tells the user a route is
    /// free.
    pub network_fee_lovelace: Option<u64>,
    pub plan: QuotePlan,
}

impl RouteQuoteData {
    /// "ADA → LUMP → SWOLE".
    pub fn path(&self) -> String {
        let mut hops = vec![self.pay.ticker.clone()];
        hops.extend(self.legs.iter().map(|l| l.amount_out.ticker.clone()));
        // ASCII, not U+2192: the app's font set has no arrow glyph and would
        // render tofu. `tests/no_broken_glyphs.rs` enforces this.
        hops.join(" -> ")
    }
}

/// What the surface currently has to show.
///
/// An enum rather than an `Option` plus an error string: exactly one of these
/// is ever true, and "unavailable" is not a failure — the pools are fine, the
/// trade is not, and the two deserve different treatment.
#[derive(Clone, Debug)]
pub enum RouteQuoteState {
    /// A quote is in flight.
    Quoting,
    Ready(Box<RouteQuoteData>),
    /// This route cannot be priced at this size or against this state — the
    /// flat fee exceeds the input, the size is not sellable, a pool moved.
    Unavailable {
        reason: String,
    },
}

/// Display options.
///
/// Sizes are [`TextSize`] steps, not point sizes: the ramp belongs to the
/// theme, and a literal here is a size no theme switch and no density setting
/// can reach. They resolve at render time, which is the first moment a widget
/// has a `Ui` to ask.
#[derive(Clone, Debug)]
pub struct RouteQuoteConfig {
    /// Leg rows and totals labels.
    pub body: TextSize,
    /// Fee buckets and the settlement note.
    pub detail: TextSize,
    /// "You pay" / "You receive" and the path headline.
    pub total: TextSize,
    /// Show the per-leg fee buckets. Off gives a compact summary.
    pub show_fees: bool,
    /// Show the "settles exactly or fails free" note.
    pub show_settlement_note: bool,
}

impl Default for RouteQuoteConfig {
    fn default() -> Self {
        Self {
            body: TextSize::Md,
            detail: TextSize::Base,
            total: TextSize::Lg,
            show_fees: true,
            show_settlement_note: true,
        }
    }
}

/// The config's steps resolved against the live theme, once per frame.
struct Sizes {
    body: f32,
    detail: f32,
    total: f32,
}

impl Sizes {
    fn resolve(ui: &Ui, config: &RouteQuoteConfig) -> Self {
        Self {
            body: ui.text_size(config.body),
            detail: ui.text_size(config.detail),
            total: ui.text_size(config.total),
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the route quote panel.
pub fn show(ui: &mut Ui, state: &RouteQuoteState, config: &RouteQuoteConfig) {
    egui::Frame::new()
        .fill(ui.tokens().color.bg_secondary)
        .corner_radius(ui.tokens().corner(Radius::Md))
        .inner_margin(ui.tokens().margin(Space::Xl))
        .stroke(ui.tokens().geometry.border(ui.tokens().color.border))
        .show(ui, |ui| {
            // Read-only rows: opt out of the touch-target floor, which
            // otherwise sets the height of every `horizontal` row and turns a
            // fee breakdown into a ladder of air at a compact breakpoint. Set
            // on the REGION — by row-layout time the height is already decided.
            ui.spacing_mut().interact_size = egui::Vec2::ZERO;
            ui.spacing_mut().item_spacing.y = ui.tokens().space(Space::Sm);
            let sizes = Sizes::resolve(ui, config);
            match state {
                RouteQuoteState::Quoting => quoting(ui, &sizes),
                RouteQuoteState::Unavailable { reason } => unavailable(ui, reason, &sizes),
                RouteQuoteState::Ready(data) => ready(
                    ui,
                    data,
                    &sizes,
                    config.show_fees,
                    config.show_settlement_note,
                ),
            }
        });
}

fn quoting(ui: &mut Ui, sizes: &Sizes) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(
            RichText::new("Pricing against live pool state…")
                .color(ui.tokens().color.text_muted)
                .size(sizes.body),
        );
    });
}

fn unavailable(ui: &mut Ui, reason: &str, sizes: &Sizes) {
    ui.label(
        RichText::new("No quote at this size")
            .color(ui.tokens().color.text_primary)
            .strong()
            .size(sizes.total),
    );
    ui.gap(Space::Sm);
    ui.label(
        RichText::new(reason)
            .color(ui.tokens().color.text_muted)
            .size(sizes.body),
    );
}

fn ready(
    ui: &mut Ui,
    data: &RouteQuoteData,
    sizes: &Sizes,
    show_fees: bool,
    show_settlement_note: bool,
) {
    // The path, as a headline.
    ui.label(
        RichText::new(data.path())
            .color(ui.tokens().color.text_primary)
            .strong()
            .size(sizes.total),
    );

    let transactions = data.plan.transactions();
    if transactions > 1 {
        ui.gap(Space::Xs);
        ui.label(
            RichText::new(format!(
                "{transactions} transactions — a venue on this route will not share one"
            ))
            .color(ui.tokens().color.text_muted)
            .size(sizes.body),
        );
    }
    ui.gap(Space::Md);

    // Legs, grouped by the transaction they settle in.
    let mut current_tx = usize::MAX;
    for leg in &data.legs {
        if transactions > 1 && leg.transaction != current_tx {
            if current_tx != usize::MAX {
                // The hand-off sits BETWEEN transactions, which is where a
                // reader is looking when they ask "and if it stops here?".
                if let QuotePlan::Chained { handoffs } = &data.plan
                    && let Some(handoff) =
                        handoffs.iter().find(|h| h.after_transaction == current_tx)
                {
                    handoff_note(ui, handoff, sizes);
                }
                ui.gap(Space::Md);
            }
            ui.label(
                RichText::new(format!(
                    "Transaction {} of {transactions}",
                    leg.transaction + 1
                ))
                .color(ui.tokens().color.text_muted)
                .size(sizes.detail),
            );
            ui.gap(Space::Xs);
            current_tx = leg.transaction;
        }
        leg_row(ui, leg, sizes, show_fees);
    }

    separator(ui);

    total_row(
        ui,
        "You pay",
        &data.pay.display(),
        ui.tokens().color.text_primary,
        sizes.total,
    );
    total_row(
        ui,
        "You receive",
        &data.receive.display(),
        ui.tokens().color.accent_green,
        sizes.total,
    );

    ui.gap(Space::Sm);
    data_row(
        ui,
        "Price impact",
        &format!("{:.2}%", data.price_impact_bps as f64 / 100.0),
        sizes,
    );
    data_row(
        ui,
        if transactions > 1 {
            "Network fee (both)"
        } else {
            "Network fee"
        },
        &match data.network_fee_lovelace {
            Some(fee) => Amount::ada(fee).display(),
            None => "shown at review".to_string(),
        },
        sizes,
    );

    if show_settlement_note {
        ui.gap(Space::Md);
        ui.label(
            RichText::new("Settles exactly as quoted, or fails free — no slippage")
                .color(ui.tokens().color.text_muted)
                .italics()
                .size(sizes.detail),
        );
    }
}

fn leg_row(ui: &mut Ui, leg: &QuoteLeg, sizes: &Sizes, show_fees: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&leg.venue)
                .color(ui.tokens().color.text_secondary)
                .size(sizes.body),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(leg.amount_out.display())
                    .color(ui.tokens().color.text_primary)
                    .size(sizes.body),
            );
            ui.label(
                RichText::new("->")
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.body),
            );
            ui.label(
                RichText::new(leg.amount_in.display())
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.body),
            );
        });
    });

    if show_fees {
        for fee in &leg.fees {
            indented(ui, &fee.label, &fee.amount.display(), sizes, false);
        }
    }
    if let Some(returned) = &leg.returned {
        indented(ui, "returned unspent", &returned.display(), sizes, true);
    }
}

/// The safety statement: what lands in the reader's own wallet if the next
/// transaction never does.
fn handoff_note(ui: &mut Ui, handoff: &Handoff, sizes: &Sizes) {
    let holding = handoff
        .holding
        .iter()
        .map(Amount::display)
        .collect::<Vec<_>>()
        .join(" + ");
    ui.gap(Space::Sm);
    egui::Frame::new()
        .fill(ui.tokens().color.bg_highlight)
        .corner_radius(ui.tokens().corner(Radius::Sm))
        .inner_margin(ui.tokens().margin(Space::Md))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "If transaction {} does not land you keep {holding}",
                    handoff.after_transaction + 2
                ))
                .color(ui.tokens().color.text_secondary)
                .size(sizes.body),
            );
            ui.label(
                RichText::new("at your own address, spendable by your own key")
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.detail),
            );
        });
}

// ============================================================================
// Row helpers
// ============================================================================

/// A fee bucket. The AMOUNT is the point, so the label truncates under
/// pressure — a `horizontal` that does not fit draws its contents over each
/// other rather than complaining.
fn indented(ui: &mut Ui, label: &str, value: &str, sizes: &Sizes, positive: bool) {
    ui.horizontal(|ui| {
        ui.add_space(ui.tokens().space(Space::Xl));
        ui.add(
            egui::Label::new(
                RichText::new(label)
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.detail),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let colour = if positive {
                ui.tokens().color.accent_green
            } else {
                ui.tokens().color.text_muted
            };
            ui.label(RichText::new(value).color(colour).size(sizes.detail));
        });
    });
}

fn data_row(ui: &mut Ui, label: &str, value: &str, sizes: &Sizes) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(label)
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.body),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .color(ui.tokens().color.text_secondary)
                    .size(sizes.body),
            );
        });
    });
}

fn total_row(ui: &mut Ui, label: &str, value: &str, colour: egui::Color32, size: f32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .color(ui.tokens().color.text_primary)
                .strong()
                .size(size),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).color(colour).strong().size(size));
        });
    });
}

fn separator(ui: &mut Ui) {
    ui.gap(Space::Sm);
    let rect = ui.available_rect_before_wrap();
    let y = rect.min.y;
    ui.painter().line_segment(
        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
        ui.tokens().geometry.border(ui.tokens().color.border),
    );
    ui.gap(Space::Base);
}

/// Thousands separators.
fn group(amount: u64) -> String {
    if amount == 0 {
        return "0".into();
    }
    let s = amount.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capping is for figures read at a glance, and must round, not truncate.
    #[test]
    fn a_capped_figure_rounds_to_the_place_it_keeps() {
        let ada = Amount::ada(28_108_241_247);
        assert_eq!(ada.figure(), "28,108.241247");
        assert_eq!(ada.figure_capped(2), "28,108.24");
        // .245 rounds up, not away.
        assert_eq!(Amount::ada(1_245_000).figure_capped(2), "1.25");
        assert_eq!(Amount::ada(1_244_999).figure_capped(2), "1.24");

        // A cap above the asset's own scale changes nothing, and a
        // zero-decimal asset is never touched.
        assert_eq!(ada.figure_capped(9), "28,108.241247");
        assert_eq!(
            Amount::new(196_800_921, 0, "LUMP").figure_capped(2),
            "196,800,921"
        );
    }

    #[test]
    fn amounts_respect_their_own_decimals() {
        // A route mixes assets that disagree about decimals; the same
        // quantity must not render the same way for both.
        assert_eq!(Amount::ada(10_000_000).display(), "10.000000 ADA");
        assert_eq!(
            Amount::new(10_000_000, 0, "LUMP").display(),
            "10,000,000 LUMP"
        );
        assert_eq!(Amount::ada(1_163_700).figure(), "1.163700");
        assert_eq!(Amount::new(3_120_727, 0, "SWOLE").figure(), "3,120,727");
        assert_eq!(Amount::ada(0).display(), "0.000000 ADA");
    }

    #[test]
    fn the_path_reads_through_every_hop() {
        let data = RouteQuoteData {
            legs: vec![
                leg(
                    "Splash",
                    Amount::ada(10_000_000),
                    Amount::new(65_862, 0, "LUMP"),
                    0,
                ),
                leg(
                    "LumpPad",
                    Amount::new(65_862, 0, "LUMP"),
                    Amount::new(3_120_727, 0, "SWOLE"),
                    1,
                ),
            ],
            pay: Amount::ada(10_000_000),
            receive: Amount::new(3_120_727, 0, "SWOLE"),
            price_impact_bps: 103,
            network_fee_lovelace: Some(599_265),
            plan: QuotePlan::Atomic,
        };
        assert_eq!(data.path(), "ADA -> LUMP -> SWOLE");
    }

    /// A chained plan's transaction count comes from its hand-offs, so the
    /// two can never disagree.
    #[test]
    fn a_chained_plan_counts_its_transactions_from_its_handoffs() {
        assert_eq!(QuotePlan::Atomic.transactions(), 1);
        let chained = QuotePlan::Chained {
            handoffs: vec![Handoff {
                after_transaction: 0,
                holding: vec![Amount::new(65_862, 0, "LUMP")],
            }],
        };
        assert_eq!(chained.transactions(), 2);
    }

    fn leg(venue: &str, amount_in: Amount, amount_out: Amount, transaction: usize) -> QuoteLeg {
        QuoteLeg {
            venue: venue.to_string(),
            amount_in,
            amount_out,
            fees: Vec::new(),
            returned: None,
            transaction,
        }
    }
}
