//! `PartyBadge` story — the three bases, the unsourced-assertion warning, and
//! what an un-annotated party looks like.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{PartyBadge, PartyBasis};

const CLUSTER_PROJECT: egui::Color32 = egui::Color32::from_rgb(0x5b, 0x8f, 0xd6);
const CLUSTER_OFFRAMP: egui::Color32 = egui::Color32::from_rgb(0xd6, 0x9b, 0x5b);

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Party Badge").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A counterparty plus how firmly its identity is known. The basis is a positional \
             argument on `new` — a call site cannot render a party without stating it.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── The three bases ────────────────────────────────────────────────
    ui.label(egui::RichText::new("Basis").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Shape-coded, not colour-coded, so it survives both themes and colour-vision \
             differences. Filled = observed, half = derived, hollow = asserted. Hover any \
             badge for the basis and source.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        PartyBadge::new("reward wallet", PartyBasis::Observed)
            .key("stake1u9yfhm5la35av8te20s8ezprz568ap5d8zzfz2mnqmgrltgq6z7yj")
            .detail("142,132 ADA lifetime · 89 txs")
            .show(ui);
        PartyBadge::new("the artist", PartyBasis::Asserted)
            .source("project operator, Discord 2026-08-12")
            .show(ui);
        PartyBadge::new("custodial hot wallet", PartyBasis::Derived)
            .source("follows from claim #4")
            .detail("32,672,197 ADA · 509 policies · 1 address")
            .show(ui);
    });

    ui.add_space(16.0);

    // ── The failure this widget exists to prevent ──────────────────────
    ui.label(
        egui::RichText::new("Unsourced assertion")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "An asserted identity with no source renders in the warning colour, with a hollow \
             warning-coloured marker. This is the state that let a figure someone supplied in \
             conversation harden into an established fact in a published write-up.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        PartyBadge::new("founder's personal wallet", PartyBasis::Asserted).show(ui);
        PartyBadge::new("founder's personal wallet", PartyBasis::Asserted)
            .source("operator statement, 2026-08-12")
            .show(ui);
    });

    ui.add_space(16.0);

    // ── Clusters + shape markers ───────────────────────────────────────
    ui.label(
        egui::RichText::new("Clusters and address shape")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The cluster colour is a thin left bar rather than a fill, so it doesn't compete \
             with Chip or read as a status. `no-stake` marks an address with no staking \
             credential — a shape, never a claim about where the money went.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        PartyBadge::new("S1 treasury", PartyBasis::Observed)
            .cluster("project wallets", CLUSTER_PROJECT)
            .show(ui);
        PartyBadge::new("ops relay", PartyBasis::Observed)
            .cluster("project wallets", CLUSTER_PROJECT)
            .show(ui);
        PartyBadge::new("deployment address", PartyBasis::Derived)
            .cluster("off-ramp", CLUSTER_OFFRAMP)
            .stakeless(true)
            .source("follows from claim #2")
            .show(ui);
    });

    ui.add_space(16.0);

    // ── Un-annotated ───────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Not yet labelled")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Renders the middle-elided key in monospace. An un-annotated wallet must never \
             look like a named one — the absence of a label is information.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        PartyBadge::unlabelled("stake1uxd5jxns935zjp6lswke9upwdzz634ukfwwcapk02x8sgfcas4spp")
            .detail("32,672,197 ADA · 509 policies")
            .show(ui);
        PartyBadge::unlabelled("addr1v99v28vx3u2c9lm24x9edm458zvxkfkjdmp9uv3mgkh2aqg8zqsxh")
            .stakeless(true)
            .show(ui);
    });
}
