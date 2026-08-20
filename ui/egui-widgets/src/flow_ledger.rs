//! `FlowLedger` — a wallet's movements in time order: what arrived, what left,
//! who was on the other side, and what the balance was after each step.
//!
//! The view a forensic trace actually runs on. Reading a wallet's history as a
//! ranked list of counterparties hides the thing that usually matters — *when*
//! a funding channel starts and stops. A channel that paid every month for
//! eight months and then paid nothing is invisible in a total and obvious in a
//! time-ordered ledger.
//!
//! ## What it enforces
//!
//! **Rows carry a NET amount.** There is no gross column, because a wallet
//! routinely appears as a minor input in a batched transaction whose outputs
//! belong to other people; showing those outputs as the wallet's own is the
//! single most productive error in this kind of work.
//!
//! **A running balance, reconciled.** [`FlowLedger::closing_balance`] lets the
//! host state the balance the rows imply, so it can be checked against the
//! wallet's actual balance. That comparison is independent of however the rows
//! were derived, which is what makes it a real check rather than a restatement.
//! When the two disagree the widget says so in the footer rather than rendering
//! a plausible total.
//!
//! **Channels are visible per row.** [`FlowRow::channel`] paints a colour bar
//! in the gutter, so a change of funding source reads as a change of colour
//! down the column without the host having to build a separate chart.
//!
//! ## Domain-free
//!
//! Amounts are `i128` in the smallest unit and formatted by a host-supplied
//! closure, so the same widget serves lovelace, lamports, or satoshis. The
//! widget knows nothing about any chain.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{FlowLedger, FlowRow, PartyBasis};
//!
//! let rows = vec![
//!     FlowRow::new(1780315284, 13_537_040_000, "conduit", PartyBasis::Observed)
//!         .tx_id("13863a1933e18f62…")
//!         .channel("withdrawal", CHANNEL_COLOR),
//!     FlowRow::new(1780432974, -20_520_170_000, "Pillar", PartyBasis::Observed)
//!         .tx_id("bfd398b8ff02efa0…"),
//! ];
//!
//! let resp = FlowLedger::new(&rows, &|v| format!("{:.2}", v as f64 / 1e6))
//!     .closing_balance(Some(18_970_000))
//!     .show(ui);
//! if let Some(i) = resp.clicked_row { open_tx(&rows[i]); }
//! ```

use egui::{Color32, Response, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::party_badge::{PartyBadge, PartyBasis};
use crate::timestamp::format_iso8601;

/// One movement. `amount` is the party's NET change in the smallest unit —
/// positive arrived, negative left.
pub struct FlowRow<'a> {
    pub timestamp: i64,
    pub amount: i128,
    /// Resolved counterparty label, or `None` to render the raw key.
    pub counterparty: Option<&'a str>,
    pub counterparty_key: Option<&'a str>,
    pub basis: PartyBasis,
    pub source: Option<&'a str>,
    pub stakeless: bool,
    pub tx_id: Option<&'a str>,
    /// Funding channel this row belongs to, with its colour.
    pub channel: Option<(&'a str, Color32)>,
    /// A short fact about the movement — see [`FlowRow::sale`].
    pub sale: Option<&'a str>,
    /// Set when this row's value is the wallet's own money returning — a round
    /// trip, not income. Rendered muted so it cannot be read as revenue.
    pub recycled: bool,
    /// NON-FUNGIBLE items that moved in the same transaction — positive
    /// arrived, negative left. Zero means none, and the column stays blank.
    ///
    /// On a chain where tokens ride along with value, the money and the goods
    /// are usually ONE event. Splitting them into two lists is what let a
    /// wallet be paid and handed supply at the same moment without anybody
    /// noticing: each list on its own looked unremarkable. Here it is one line
    /// that reads "paid, AND given the goods".
    pub items: i32,
}

impl<'a> FlowRow<'a> {
    pub fn new(timestamp: i64, amount: i128, counterparty: &'a str, basis: PartyBasis) -> Self {
        Self {
            timestamp,
            amount,
            counterparty: Some(counterparty),
            counterparty_key: None,
            basis,
            source: None,
            stakeless: false,
            tx_id: None,
            channel: None,
            sale: None,
            recycled: false,
            items: 0,
        }
    }

    /// A movement whose counterparty has no label yet.
    pub fn unlabelled(timestamp: i64, amount: i128, key: &'a str) -> Self {
        Self {
            timestamp,
            amount,
            counterparty: None,
            counterparty_key: Some(key),
            basis: PartyBasis::Observed,
            source: None,
            stakeless: false,
            tx_id: None,
            channel: None,
            sale: None,
            recycled: false,
            items: 0,
        }
    }

    pub fn counterparty_key(mut self, key: &'a str) -> Self {
        self.counterparty_key = Some(key);
        self
    }

    pub fn source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    pub fn stakeless(mut self, stakeless: bool) -> Self {
        self.stakeless = stakeless;
        self
    }

    pub fn tx_id(mut self, tx_id: &'a str) -> Self {
        self.tx_id = Some(tx_id);
        self
    }

    pub fn channel(mut self, name: &'a str, color: Color32) -> Self {
        self.channel = Some((name, color));
        self
    }

    /// A short fact about the movement itself — "sold 55 ₳ · jpg".
    ///
    /// Its own column rather than a [`FlowRow::channel`]: a channel is CLUSTER
    /// MEMBERSHIP, drawn as a coloured dot with the name only in a tooltip,
    /// which is right for "these wallets are one group" and useless for a price
    /// you need to read. Carrying a sale that way made the venue technically
    /// present and practically invisible.
    pub fn sale(mut self, label: &'a str) -> Self {
        self.sale = Some(label);
        self
    }

    pub fn recycled(mut self, recycled: bool) -> Self {
        self.recycled = recycled;
        self
    }

    /// Non-fungible items that moved with this value; signed like `amount`.
    pub fn items(mut self, items: i32) -> Self {
        self.items = items;
        self
    }

    pub fn is_inflow(&self) -> bool {
        self.amount > 0
    }
}

/// Totals the ledger implies, and whether they reconcile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerTotals {
    pub inflow: i128,
    pub outflow: i128,
    /// Inflow that is the wallet's own money returning.
    pub recycled: i128,
    pub net: i128,
    /// `Some(false)` when a supplied closing balance disagrees with the rows.
    pub reconciles: Option<bool>,
}

impl LedgerTotals {
    /// Inflow excluding round trips — the figure any "income" percentage should
    /// be computed from. A gross inflow that counts returned money inflates the
    /// base and flatters every ratio derived from it.
    pub fn genuine_inflow(&self) -> i128 {
        self.inflow - self.recycled
    }
}

/// Compute the totals for a set of rows.
///
/// `opening_balance` is the balance before the first row — non-zero whenever
/// the rows are a window rather than the wallet's whole history. It has to
/// participate in the check: comparing the rows' NET against the closing
/// balance only reconciles for a wallet traced from its first transaction, so
/// leaving it out makes every windowed ledger report a false mismatch, and a
/// reconciliation that cries wolf is worse than none.
///
/// `closing_balance` is the wallet's *actual* balance, if known. Tolerance is
/// in the same smallest unit and should cover accrued staking rewards or
/// anything else that moves a balance without a transaction.
pub fn totals(
    rows: &[FlowRow<'_>],
    opening_balance: i128,
    closing_balance: Option<i128>,
    tolerance: i128,
) -> LedgerTotals {
    let inflow: i128 = rows.iter().filter(|r| r.amount > 0).map(|r| r.amount).sum();
    let outflow: i128 = rows
        .iter()
        .filter(|r| r.amount < 0)
        .map(|r| -r.amount)
        .sum();
    let recycled: i128 = rows
        .iter()
        .filter(|r| r.amount > 0 && r.recycled)
        .map(|r| r.amount)
        .sum();
    let net = inflow - outflow;
    LedgerTotals {
        inflow,
        outflow,
        recycled,
        net,
        reconciles: closing_balance.map(|b| (opening_balance + net - b).abs() <= tolerance),
    }
}

pub struct FlowLedgerResponse {
    pub clicked_row: Option<usize>,
    pub totals: LedgerTotals,
}

pub struct FlowLedger<'a> {
    rows: &'a [FlowRow<'a>],
    format_amount: &'a dyn Fn(i128) -> String,
    opening_balance: i128,
    closing_balance: Option<i128>,
    tolerance: i128,
    show_running_balance: bool,
    row_height: f32,
    max_height: Option<f32>,
    /// Turns a row's `tx_id` into somewhere it can be inspected — a block
    /// explorer. Supplied by the host because the chain, network and preferred
    /// explorer are all its business, not the widget's.
    explorer: Option<&'a dyn Fn(&str) -> String>,
}

impl<'a> FlowLedger<'a> {
    /// `format_amount` renders a smallest-unit value; the widget adds the sign.
    pub fn new(rows: &'a [FlowRow<'a>], format_amount: &'a dyn Fn(i128) -> String) -> Self {
        Self {
            rows,
            format_amount,
            opening_balance: 0,
            closing_balance: None,
            tolerance: 1,
            show_running_balance: true,
            row_height: 22.0,
            max_height: None,
            explorer: None,
        }
    }

    /// Give each row with a `tx_id` a button that opens it on a block explorer.
    ///
    /// A ledger row is a DERIVED claim — this walk's reading of a transaction.
    /// One click to the transaction itself is what lets a reader check the
    /// claim instead of taking it, and it is the handoff point to any other
    /// tool. Without it the `tx_id` was collected, carried, and then dropped.
    pub fn explorer(mut self, url_for: &'a dyn Fn(&str) -> String) -> Self {
        self.explorer = Some(url_for);
        self
    }

    /// Balance before the first row — needed when the rows are a window rather
    /// than the wallet's whole history.
    pub fn opening_balance(mut self, balance: i128) -> Self {
        self.opening_balance = balance;
        self
    }

    /// The wallet's actual balance, for the reconciliation check.
    pub fn closing_balance(mut self, balance: Option<i128>) -> Self {
        self.closing_balance = balance;
        self
    }

    pub fn tolerance(mut self, tolerance: i128) -> Self {
        self.tolerance = tolerance;
        self
    }

    pub fn show_running_balance(mut self, show: bool) -> Self {
        self.show_running_balance = show;
        self
    }

    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }

    pub fn show(self, ui: &mut Ui) -> FlowLedgerResponse {
        let totals = totals(
            self.rows,
            self.opening_balance,
            self.closing_balance,
            self.tolerance,
        );
        let mut clicked_row = None;

        let muted = ui.visuals().weak_text_color();
        let pos = Color32::from_rgb(0x4a, 0xba, 0x7a);
        let neg = Color32::from_rgb(0xd0, 0x6b, 0x5c);

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(6.0)) // channel gutter
            .column(Column::exact(120.0)) // date + time
            .column(Column::exact(110.0)) // net
            .sense(Sense::click());

        // Items sits BETWEEN net and counterparty, so value and goods are read
        // together as one movement rather than at opposite ends of the row.
        // Only present when something actually carried items — a permanently
        // blank column is a column that teaches you to stop looking.
        let any_items = self.rows.iter().any(|r| r.items != 0);
        if any_items {
            builder = builder.column(Column::exact(70.0));
        }
        // Beside items, so value / goods / "what this was" read as one thought.
        // NOT clipped and NOT inside the counterparty column, which is — a
        // price truncated to nothing is worse than no column.
        let any_sale = self.rows.iter().any(|r| r.sale.is_some());
        if any_sale {
            builder = builder.column(Column::exact(130.0));
        }
        builder = builder.column(Column::initial(280.0).at_least(160.0).clip(true));

        if self.show_running_balance {
            builder = builder.column(Column::exact(110.0));
        }
        // Same rule as the items column: only present when there is something
        // to open, so it never becomes a column you learn to ignore.
        let any_tx = self.explorer.is_some() && self.rows.iter().any(|r| r.tx_id.is_some());
        if any_tx {
            builder = builder.column(Column::exact(28.0));
        }
        if let Some(h) = self.max_height {
            builder = builder.max_scroll_height(h);
        }

        builder
            .header(18.0, |mut header| {
                let head = |h: &mut egui_extras::TableRow, text: &str| {
                    h.col(|ui| {
                        ui.label(RichText::new(text).size(10.0).color(muted));
                    });
                };
                head(&mut header, "");
                head(&mut header, "date");
                head(&mut header, "net");
                if any_items {
                    head(&mut header, "items");
                }
                if any_sale {
                    head(&mut header, "sale");
                }
                head(&mut header, "counterparty");
                if self.show_running_balance {
                    head(&mut header, "balance");
                }
                if any_tx {
                    head(&mut header, "tx");
                }
            })
            .body(|body| {
                let mut running = self.opening_balance;
                // Pre-compute so a virtualised body can render any row without
                // replaying the ones above it.
                let balances: Vec<i128> = self
                    .rows
                    .iter()
                    .map(|r| {
                        running += r.amount;
                        running
                    })
                    .collect();

                body.rows(self.row_height, self.rows.len(), |mut row| {
                    let idx = row.index();
                    let r = &self.rows[idx];

                    row.col(|ui| {
                        if let Some((_, color)) = r.channel {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(4.0, self.row_height),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(rect, 1.0, color);
                        }
                    });

                    row.col(|ui| {
                        ui.label(
                            RichText::new(format_iso8601(r.timestamp, false))
                                .size(11.0)
                                .monospace()
                                .color(muted),
                        );
                    });

                    row.col(|ui| {
                        let color = if r.recycled {
                            muted
                        } else if r.is_inflow() {
                            pos
                        } else {
                            neg
                        };
                        let sign = if r.is_inflow() { "+" } else { "-" };
                        let text = format!("{sign}{}", (self.format_amount)(r.amount.abs()));
                        let resp =
                            ui.label(RichText::new(text).size(11.0).monospace().color(color));
                        if r.recycled {
                            resp.on_hover_text(
                                "Round trip — this is the wallet's own money returning, \
                                 not income. Excluded from genuine inflow.",
                            );
                        }
                    });

                    if any_items {
                        row.col(|ui| {
                            if r.items == 0 {
                                return;
                            }
                            // Same in/out colours as the value column, so a row
                            // where money and goods travel in OPPOSITE
                            // directions (a purchase) is instantly distinct from
                            // one where they travel together (being paid to take
                            // something).
                            let color = if r.items > 0 { pos } else { neg };
                            let sign = if r.items > 0 { "+" } else { "-" };
                            ui.label(
                                RichText::new(format!("{sign}{}", r.items.abs()))
                                    .size(11.0)
                                    .monospace()
                                    .color(color),
                            )
                            .on_hover_text(if r.items > 0 {
                                "items received in this transaction"
                            } else {
                                "items sent in this transaction"
                            });
                        });
                    }

                    if any_sale {
                        row.col(|ui| {
                            let Some(label) = r.sale else {
                                return;
                            };
                            // Gold: outside the in/out green and red, which
                            // encode DIRECTION. A sale is a fact about the
                            // movement, not a direction of it.
                            ui.label(
                                RichText::new(label)
                                    .size(11.0)
                                    .color(Color32::from_rgb(0xc9, 0xa2, 0x27)),
                            )
                            .on_hover_text(
                                "Recorded by a marketplace. The price is the SALE, not this \
                                 wallet's share of it — a seller nets it minus royalty and \
                                 venue fee. A blank cell means NO venue event, which includes \
                                 every peer-to-peer trade: it does not mean 'not sold'.",
                            );
                        });
                    }

                    row.col(|ui| {
                        let badge = match r.counterparty {
                            Some(label) => PartyBadge::new(label, r.basis),
                            None => PartyBadge::unlabelled(r.counterparty_key.unwrap_or("")),
                        };
                        let badge = badge.stakeless(r.stakeless).text_size(11.0);
                        let badge = match r.counterparty_key {
                            Some(k) if r.counterparty.is_some() => badge.key(k),
                            _ => badge,
                        };
                        let badge = match r.source {
                            Some(s) => badge.source(s),
                            None => badge,
                        };
                        let badge = match r.channel {
                            Some((name, color)) => badge.cluster(name, color),
                            None => badge,
                        };
                        badge.show(ui);
                    });

                    if self.show_running_balance {
                        row.col(|ui| {
                            ui.label(
                                RichText::new((self.format_amount)(balances[idx]))
                                    .size(11.0)
                                    .monospace()
                                    .color(muted),
                            );
                        });
                    }

                    if any_tx {
                        row.col(|ui| {
                            let (Some(url_for), Some(tx)) = (self.explorer, r.tx_id) else {
                                return;
                            };
                            crate::icons::install_phosphor_font(ui.ctx());
                            if ui
                                .small_button(
                                    crate::icons::PhosphorIcon::Eye
                                        .rich_text(11.0, muted)
                                        .small(),
                                )
                                .on_hover_text(format!("Open {tx} on a block explorer"))
                                .clicked()
                            {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url_for(tx)));
                            }
                        });
                    }

                    if row.response().clicked() {
                        clicked_row = Some(idx);
                    }
                });
            });

        self.footer(ui, &totals, muted);

        FlowLedgerResponse {
            clicked_row,
            totals,
        }
    }

    fn footer(&self, ui: &mut Ui, totals: &LedgerTotals, muted: Color32) -> Response {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let f = self.format_amount;

            ui.label(
                RichText::new(format!("in {}", f(totals.inflow)))
                    .size(11.0)
                    .color(muted),
            );
            ui.label(
                RichText::new(format!("out {}", f(totals.outflow)))
                    .size(11.0)
                    .color(muted),
            );

            if totals.recycled > 0 {
                ui.label(
                    RichText::new(format!("recycled {}", f(totals.recycled)))
                        .size(11.0)
                        .color(muted),
                )
                .on_hover_text(
                    "Inflow that is the wallet's own money returning. Any income \
                     percentage should be computed from genuine inflow, not gross.",
                );
                ui.label(
                    RichText::new(format!("genuine in {}", f(totals.genuine_inflow())))
                        .size(11.0)
                        .color(muted),
                );
            }

            // The reconciliation is the honest-reporting hook: a mismatch is
            // stated, never smoothed into a plausible-looking total.
            match totals.reconciles {
                Some(true) => {
                    ui.label(
                        RichText::new("reconciles")
                            .size(11.0)
                            .color(Color32::from_rgb(0x4a, 0xba, 0x7a)),
                    );
                }
                Some(false) => {
                    ui.label(
                        RichText::new("DOES NOT RECONCILE")
                            .size(11.0)
                            .strong()
                            .color(ui.visuals().warn_fg_color),
                    )
                    .on_hover_text(
                        "The rows do not sum to the wallet's actual balance. \
                         Either the window is incomplete or the attribution is \
                         wrong — do not report totals from this view until it \
                         reconciles.",
                    );
                }
                None => {}
            }
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64, amount: i128) -> FlowRow<'static> {
        FlowRow::new(ts, amount, "cp", PartyBasis::Observed)
    }

    #[test]
    fn totals_split_inflow_and_outflow() {
        let rows = vec![row(1, 1_000), row(2, -400), row(3, 250)];
        let t = totals(&rows, 0, None, 1);
        assert_eq!(t.inflow, 1_250);
        assert_eq!(t.outflow, 400);
        assert_eq!(t.net, 850);
        assert_eq!(t.reconciles, None);
    }

    /// Recycled inflow must not count toward genuine income — the base every
    /// percentage is computed from.
    #[test]
    fn recycled_inflow_is_excluded_from_genuine_income() {
        let rows = vec![
            row(1, 10_000),
            FlowRow::new(2, 4_024, "parking", PartyBasis::Observed).recycled(true),
        ];
        let t = totals(&rows, 0, None, 1);
        assert_eq!(t.inflow, 14_024, "gross inflow still reported");
        assert_eq!(t.recycled, 4_024);
        assert_eq!(t.genuine_inflow(), 10_000);
    }

    /// An outflow marked recycled is not inflow and must not be subtracted
    /// twice.
    #[test]
    fn recycled_only_counts_on_the_inflow_side() {
        let rows = vec![FlowRow::new(1, -5_000, "parking", PartyBasis::Observed).recycled(true)];
        let t = totals(&rows, 0, None, 1);
        assert_eq!(t.recycled, 0);
        assert_eq!(t.genuine_inflow(), 0);
    }

    #[test]
    fn reconciliation_passes_within_tolerance() {
        let rows = vec![row(1, 1_000), row(2, -400)];
        assert_eq!(totals(&rows, 0, Some(600), 0).reconciles, Some(true));
        assert_eq!(totals(&rows, 0, Some(601), 1).reconciles, Some(true));
    }

    /// A mismatch must be reported, not smoothed away.
    #[test]
    fn reconciliation_fails_loudly_outside_tolerance() {
        let rows = vec![row(1, 1_000), row(2, -400)];
        assert_eq!(totals(&rows, 0, Some(900), 1).reconciles, Some(false));
    }

    /// A window that does not start at the wallet's first transaction must
    /// reconcile against `opening + net`. Ignoring the opening balance made
    /// every windowed ledger report a false mismatch — caught by looking at
    /// the rendered story, not by the unit tests as originally written.
    #[test]
    fn windowed_ledger_reconciles_through_the_opening_balance() {
        let rows = vec![row(1, 1_000), row(2, -400)];
        assert_eq!(
            totals(&rows, 9_000, Some(9_600), 0).reconciles,
            Some(true),
            "opening 9,000 + net 600 == closing 9,600"
        );
        assert_eq!(
            totals(&rows, 9_000, Some(600), 0).reconciles,
            Some(false),
            "the old net-only comparison would have passed this"
        );
    }

    #[test]
    fn empty_ledger_is_well_defined() {
        let t = totals(&[], 0, Some(0), 0);
        assert_eq!(t.net, 0);
        assert_eq!(t.genuine_inflow(), 0);
        assert_eq!(t.reconciles, Some(true));

        let opened = totals(&[], 500, Some(500), 0);
        assert_eq!(opened.reconciles, Some(true));
    }
}
