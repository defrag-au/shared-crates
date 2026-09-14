//! Storybook demo for the RouteQuote widget.

use egui_widgets::route_quote::{
    self, Amount, Handoff, QuoteFee, QuoteLeg, QuotePlan, RouteQuoteConfig, RouteQuoteData,
    RouteQuoteState,
};

use crate::{accent, bg, muted, secondary};

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("RouteQuote Widget")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "SEQUENTIAL multi-venue routing — each leg's output feeds the next. \
             Not route_summary, which splits one trade across DEXes in parallel.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(12.0);

    let config = RouteQuoteConfig::default();

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "Chained — LumpPad will not share a transaction",
            "The pilot as it actually settles. Note the hand-off: exactly what \
             lands in the user's own wallet if the second transaction never does.",
            |ui| route_quote::show(ui, &RouteQuoteState::Ready(Box::new(pilot())), &config),
        );
        panel(
            ui,
            "Atomic — one transaction",
            "The same route if both venues composed: no hand-off, because there \
             is no state in which the user holds LUMP.",
            |ui| route_quote::show(ui, &RouteQuoteState::Ready(Box::new(atomic())), &config),
        );
    });

    ui.add_space(12.0);

    ui.horizontal_top(|ui| {
        panel(
            ui,
            "Unavailable — flat fee exceeds the input",
            "Not an error: the pools are fine, the trade is not. A LumpPad trade \
             pays 10,000 LUMP flat however small it is.",
            |ui| {
                route_quote::show(
                    ui,
                    &RouteQuoteState::Unavailable {
                        reason: "5,000 LUMP does not cover the venue's flat fee of 10,000 LUMP — \
                                 this trade cannot be priced."
                            .into(),
                    },
                    &config,
                )
            },
        );
        panel(
            ui,
            "Unavailable — not sellable at this size",
            "The curve would return less than the fees charged against it, so the \
             seller's net is non-positive.",
            |ui| {
                route_quote::show(
                    ui,
                    &RouteQuoteState::Unavailable {
                        reason:
                            "Not sellable at this size: the fees exceed what the curve returns."
                                .into(),
                    },
                    &config,
                )
            },
        );
        panel(ui, "Quoting", "Pricing against fresh pool state.", |ui| {
            route_quote::show(ui, &RouteQuoteState::Quoting, &config)
        });
    });
}

/// The pilot, at the live mainnet numbers of 2026-09-14: 10 ADA through the
/// Splash LUMP/ADA pool, then into the LumpPad SWOLE pool. Chained, because
/// the LumpPad validator refuses to share a transaction.
fn pilot() -> RouteQuoteData {
    RouteQuoteData {
        legs: vec![
            QuoteLeg {
                venue: "Splash".into(),
                amount_in: Amount::ada(10_000_000),
                amount_out: Amount::new(65_862, 0, "LUMP"),
                fees: vec![
                    fee("Splash LP", Amount::ada(90_000)),
                    fee("Splash treasury", Amount::ada(5_000)),
                    fee("Splash royalty", Amount::ada(5_000)),
                ],
                returned: None,
                transaction: 0,
            },
            QuoteLeg {
                venue: "LumpPad".into(),
                amount_in: Amount::new(65_862, 0, "LUMP"),
                amount_out: Amount::new(3_120_727, 0, "SWOLE"),
                fees: vec![
                    fee("LumpPad swap (stays in pool)", Amount::new(166, 0, "LUMP")),
                    fee(
                        "LumpPad platform (half burns)",
                        Amount::new(10_275, 0, "LUMP"),
                    ),
                    fee("LumpPad creator", Amount::new(550, 0, "LUMP")),
                ],
                // The gross solver cannot place the last LUMP; it comes back.
                returned: Some(Amount::new(1, 0, "LUMP")),
                transaction: 1,
            },
        ],
        pay: Amount::ada(10_000_000),
        receive: Amount::new(3_120_727, 0, "SWOLE"),
        price_impact_bps: 103,
        network_fee_lovelace: 599_265,
        plan: QuotePlan::Chained {
            handoffs: vec![Handoff {
                after_transaction: 0,
                holding: vec![Amount::new(65_862, 0, "LUMP"), Amount::ada(1_163_700)],
            }],
        },
    }
}

/// The same numbers with both legs in one transaction — what the design
/// assumed before the LumpPad validator said otherwise. Kept so the two read
/// side by side.
fn atomic() -> RouteQuoteData {
    let mut data = pilot();
    for leg in &mut data.legs {
        leg.transaction = 0;
    }
    data.network_fee_lovelace = 349_000;
    data.plan = QuotePlan::Atomic;
    data
}

fn fee(label: &str, amount: Amount) -> QuoteFee {
    QuoteFee {
        label: label.into(),
        amount,
    }
}

fn panel(ui: &mut egui::Ui, title: &str, caption: &str, body: impl FnOnce(&mut egui::Ui)) {
    // `allocate_ui` inherits the PARENT's layout, and these panels sit inside
    // a `horizontal_top` — so without an explicit top-down layout the title,
    // caption and widget lay out left to right and run off the surface.
    ui.allocate_ui_with_layout(
        egui::vec2(330.0, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new()
                .fill(bg(ui))
                .corner_radius(6.0)
                .inner_margin(12.0)
                .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .color(secondary(ui))
                            .size(11.0)
                            .strong(),
                    );
                    ui.label(egui::RichText::new(caption).color(muted(ui)).size(10.0));
                    ui.add_space(8.0);
                    body(ui);
                });
        },
    );
}
