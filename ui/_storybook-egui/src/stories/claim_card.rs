//! `ClaimCard` story — four real claims from the investigation this widget came
//! out of, one in each state, with a live expand toggle.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{ClaimCard, ClaimSupport, FalsifierStatus, PartyBasis};

#[derive(Default)]
pub struct ClaimCardState {
    expanded: [bool; 4],
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(TEXT_MUTED).small());
}

pub fn show(ui: &mut egui::Ui, state: &mut ClaimCardState) {
    ui.label(egui::RichText::new("Claim Card").color(ACCENT).strong());
    note(
        ui,
        "Scannable first: the three-pip track is stated → falsifiable → tested. Click any card \
         for the falsifier, the outcome and what it rests on.",
    );
    ui.add_space(12.0);

    // ── SURVIVED ───────────────────────────────────────────────────────
    let survived_support = vec![
        ClaimSupport::new(
            "conduit → reward wallet: 3 transfers, 30,531.41 ADA",
            PartyBasis::Observed,
        )
        .reference("13863a1933e18f62"),
        ClaimSupport::new(
            "99.94% of conduit funding traces to one 32.6M ADA wallet",
            PartyBasis::Observed,
        ),
        ClaimSupport::new(
            "sample: 1 inflow source, 79 outflow destinations, 0 token txs",
            PartyBasis::Derived,
        )
        .source("200-tx sample, Apr–Aug 2026"),
    ];
    if ClaimCard::new(
        "The 2026-06-05 payout was funded by a custodial withdrawal, not mining revenue.",
    )
    .falsifier(
        "If the conduit were the mining pool, it would have paid during the seven months the \
         fleet was producing 6.3 PH/s.",
    )
    .status(FalsifierStatus::Survived)
    .id("claim-04")
    .support(&survived_support)
    .outcome(
        "Run 2026-08-16: the conduit has existed since 2024-09-02 and paid nothing until \
         2026-05-01.",
    )
    .expanded(state.expanded[0])
    .show(ui)
    .toggled
    {
        state.expanded[0] = !state.expanded[0];
    }

    ui.add_space(8.0);

    // ── PENDING ────────────────────────────────────────────────────────
    let pending_support = vec![
        ClaimSupport::new(
            "off-ramp received ~9,340 ADA across 10 payments in May 2026",
            PartyBasis::Observed,
        ),
        ClaimSupport::new("no receipts 2026-05-31 → 2026-07-01", PartyBasis::Observed),
        ClaimSupport::new("27 machines × $187 deposit", PartyBasis::Asserted),
    ];
    if ClaimCard::new("The 'remaining treasury' was never deployed before the claim was made.")
        .falsifier("Find any late-May transfer large enough to constitute a treasury deployment.")
        .status(FalsifierStatus::Pending)
        .id("claim-07")
        .support(&pending_support)
        .expanded(state.expanded[1])
        .show(ui)
        .toggled
    {
        state.expanded[1] = !state.expanded[1];
    }

    ui.add_space(8.0);

    // ── UNTESTED — the default ─────────────────────────────────────────
    let jotted = vec![ClaimSupport::new(
        "balance 7.39 ADA, 0 assets, 1 address, 5,415 txs",
        PartyBasis::Observed,
    )];
    if ClaimCard::new("The conduit is a custodial withdrawal leg, not a trading wallet.")
        .id("claim-09")
        .support(&jotted)
        .expanded(state.expanded[2])
        .show(ui)
        .toggled
    {
        state.expanded[2] = !state.expanded[2];
    }

    ui.add_space(8.0);

    // ── REFUTED — the reason the state exists ──────────────────────────
    let refuted_support = vec![
        ClaimSupport::new(
            "conduit delivered 30,531.41 ADA in 3 transfers",
            PartyBasis::Observed,
        ),
        ClaimSupport::new("27 machines × $187 = $5,049", PartyBasis::Asserted),
        ClaimSupport::new("implied ADA price $0.1654", PartyBasis::Derived),
    ];
    if ClaimCard::new("The 30,531.41 ADA is returned electricity deposits: 27 × $187 = $5,049.")
        .falsifier("Look up actual ADA spot on the three transfer dates.")
        .status(FalsifierStatus::Refuted)
        .id("claim-02")
        .support(&refuted_support)
        .outcome("Run 2026-08-16: spot was $0.2462 / $0.25205 / $0.23549. $276/machine, not $187.")
        .expanded(state.expanded[3])
        .show(ui)
        .toggled
    {
        state.expanded[3] = !state.expanded[3];
    }

    ui.add_space(12.0);
    note(
        ui,
        "Dashed edge = provisional. Struck through = refuted, and kept — that one was believed \
         for three days.",
    );
    note(
        ui,
        "Right-hand pips are the support, shaped by basis: filled observed, half derived, hollow \
         asserted. Orange = nobody attributed it.",
    );
}
