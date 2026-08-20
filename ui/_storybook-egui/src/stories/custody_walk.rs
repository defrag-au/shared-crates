//! `CustodyWalk` story — the real UTxO trace behind the Mekka 2026-06-05
//! payout, plus the same walk cut short so the PARTIAL state is visible.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{CustodyStrength, CustodyWalk, PartyBasis, WalkNode};

fn ada(v: i128) -> String {
    format!("{:>10.2}", v as f64 / 1e6)
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Custody Walk").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Where one specific sum came from. Each row is a share of its parent; leaves are \
             where the money actually entered the wallet.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── The real thing ─────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Complete walk — every leaf resolved")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);

    let complete = vec![
        WalkNode::root(20_520_170_000, "payout — 20,520 ADA to 173 holders"),
        WalkNode::received(1, 13_537_040_000, "conduit", PartyBasis::Observed)
            .at(1780315284)
            .party_key("stake1uydxeqvw9j66x4jka0gp7zqsdrt7jaue4u27t5j92w5yttc8s96yj"),
        WalkNode::received(
            1,
            1_956_320_000,
            "exchange-scale wallet",
            PartyBasis::Derived,
        )
        .at(1779912000),
        WalkNode::change(1, 3_624_660_000, "$mekkaops", PartyBasis::Observed).at(1780258000),
        WalkNode::received(2, 3_624_660_000, "parking wallet", PartyBasis::Observed).at(1779073908),
        WalkNode::change(1, 1_958_560_000, "off-ramp address", PartyBasis::Observed)
            .at(1779912000)
            .stakeless(true),
        WalkNode::received(2, 1_958_170_000, "conduit", PartyBasis::Observed).at(1779073908),
        WalkNode::received(2, 390_000, "Byron address", PartyBasis::Observed)
            .at(1779200000)
            .stakeless(true),
    ];

    CustodyWalk::new(&complete, &ada)
        .strength(CustodyStrength::Proven)
        .height(190.0)
        .column_width(300.0)
        .show(ui);

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Bar height is the share of the traced sum, so the conduit's 66% leg looks like \
             two thirds of the payout rather than being a number you have to add up. The two \
             grey 'change from paying' bars are pass-throughs — that money was already the \
             wallet's, so the flow continues past the payee instead of stopping and naming \
             it as the source.",
        )
        .color(TEXT_MUTED)
        .small(),
    );

    ui.add_space(20.0);

    // ── The state that must never be silent ────────────────────────────
    ui.label(
        egui::RichText::new("Partial walk — bounds are leaves, not omissions")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The same trace with a depth ceiling of 1 and a budget that ran out. The header \
             reads PARTIAL and the untraced value is stated — a walk that quietly stops looks \
             exactly like one that finished.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let partial = vec![
        WalkNode::root(20_520_170_000, "payout — 20,520 ADA to 173 holders"),
        WalkNode::received(1, 13_537_040_000, "conduit", PartyBasis::Observed).at(1780315284),
        WalkNode::received(
            1,
            1_956_320_000,
            "exchange-scale wallet",
            PartyBasis::Derived,
        )
        .at(1779912000),
        WalkNode::beyond_depth(1, 3_624_660_000, 1),
        WalkNode::budget_exhausted(1, 1_402_150_000),
    ];

    CustodyWalk::new(&partial, &ada)
        .strength(CustodyStrength::Proven)
        .height(150.0)
        .column_width(300.0)
        .show(ui);

    ui.add_space(20.0);

    // ── Account chains ─────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Account chain — INFERRED, not proven")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Solana and other account chains have no input→output link, so the path is \
             reconstructed from instruction ordering. Same layout, different badge — because \
             rendering them identically invites someone to call a reconstruction a trace.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let inferred = vec![
        WalkNode::root(4_200_000_000, "transfer — 4.2 SOL"),
        WalkNode::received(1, 3_000_000_000, "program vault", PartyBasis::Asserted),
        WalkNode::received(1, 1_200_000_000, "unlabelled owner", PartyBasis::Observed),
    ];

    CustodyWalk::new(&inferred, &|v| format!("{:>8.4}", v as f64 / 1e9))
        .strength(CustodyStrength::Inferred)
        .height(90.0)
        .column_width(300.0)
        .show(ui);
}
