//! `ActivityFeed` story — the run of Wayup sales that made the case for the
//! widget.
//!
//! These are real shapes from a HOSKY-trading wallet on 25 August 2026: five
//! near-identical sales minutes apart. In the table form they rendered as five
//! copies of `−9.6 ₳ · +2 items · addr1q8pyt…t9gu9y`, which is the failure the
//! widget exists to fix — same amount, same counterparty, nothing to tell them
//! apart and nothing saying what was actually traded. Here each card names its
//! venue and the two NFTs that moved.
//!
//! The last two entries are deliberately not sales: a mint (no venue, assets
//! arriving against a payment) and a bare transfer with no tags at all, which
//! is where the counterparty falls back into the tag row.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{ActivityAsset, ActivityEntry, ActivityFeed, ActivityTag, ChipVariant};

fn ada(v: i128) -> String {
    format!("{:+.2} ₳", v as f64 / 1e6)
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Activity Feed").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "A wallet's transactions as day-grouped cards: what it was, what moved, what it \
             cost. Assets are named, never counted — \"+2 items\" is the thing this replaces.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // 2026-08-25, five Wayup sales inside eight minutes.
    let entries = vec![
        // FOUR TAGS, ONE OF THEM A SENTENCE — the shape that broke the card on
        // a phone. The tag row does not wrap on its own, so it ran past the
        // card, left the amount column zero width, and the amount wrapped one
        // glyph per line into ~400pt of invisible height. Keep this entry: it
        // is the narrow-width regression, and the widest tag is the point.
        ActivityEntry::new(1787704512, -1_170_000)
            .tag(ActivityTag::new("$walkers.ada", ChipVariant::Success))
            .tag(ActivityTag::new("1 ₳ + 1 asset released", ChipVariant::Tag))
            .tag(ActivityTag::new("wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("sold", ChipVariant::Info))
            .secondary("$0.28")
            .tx_id("9d41b7e0c2a85f36b1e094d7a3c5f288")
            .asset(ActivityAsset::new("Walker183", -1)),
        ActivityEntry::new(1787704389, 2_430_840)
            .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("sale", ChipVariant::Info))
            .secondary("$0.59")
            .tx_id("13863a1933e18f62aa0c9c2e4d3f8b71")
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1729", 1))
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1724", 1)),
        ActivityEntry::new(1787704257, 2_430_840)
            .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("sale", ChipVariant::Info))
            .secondary("$0.59")
            .tx_id("bfd398b8ff02efa0119c7a5d6e2b4c88")
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1731", 1))
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1702", 1)),
        ActivityEntry::new(1787704164, -9_612_000)
            .tag(ActivityTag::new("Minswap", ChipVariant::Info))
            .tag(ActivityTag::new("swap", ChipVariant::Tag))
            .secondary("$2.33")
            .tx_id("5f0c2a71d9e84b3c6a1f7e05b8d4c932")
            .asset(ActivityAsset::new("SNEK", -420_000))
            .asset(ActivityAsset::new("MIN", 1_250)),
        // A mint: assets arrive, ADA leaves, no venue in the registry.
        ActivityEntry::new(1787690012, -45_000_000)
            .tag(ActivityTag::new("mint", ChipVariant::Success))
            .tx_id("a71e5c04b2d9f8637e0a4c1b5d92f664")
            .asset(ActivityAsset::new("Alien #0413", 1))
            .asset(ActivityAsset::new("Alien #0414", 1))
            .asset(ActivityAsset::new("Alien #0415", 1))
            .asset(ActivityAsset::new("Alien #0416", 1))
            .asset(ActivityAsset::new("Alien #0417", 1))
            .asset(ActivityAsset::new("Alien #0418", 1))
            .asset(ActivityAsset::new("Alien #0419", 1))
            .asset(ActivityAsset::new("Alien #0420", 1)),
        // An offer: ADA leaves, nothing arrives, and the only thing worth
        // knowing is what it was FOR. A Wayup cart puts many in one tx —
        // real ones reach 25 distinct collections — so the row caps and
        // says how many it hid.
        ActivityEntry::new(1787689433, -1_210_000)
            .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("offer_created", ChipVariant::Info))
            .secondary("2,500 ₳")
            .tx_id("ce01589f48219c3f548ecce3a0185cdbc3bccc72b267f133eb907fc03c88e4fb")
            .target(ActivityAsset::new("Alien #0413", 0))
            .target(ActivityAsset::new("HOSKY C(ash Grab)NFT 1729", 0))
            .target(ActivityAsset::new("collection 79c8e06f…", 0))
            .targets_meta("offer on", 25),
        // Previous day, and an untagged transfer — counterparty carries it.
        ActivityEntry::new(1787601442, 500_000_000)
            .counterparty(Some(
                "addr1qy8vw0xz4mkq9wtn3d6h2k5r7pl0s8ja4c6v2n9m3q7x5f8d2g4h6j",
            ))
            .tx_id("c04e91a7f3b28d56091e7a4c2b8f5d31"),
    ];

    let resp = ActivityFeed::new(&entries, &ada).show(ui);
    ui.add_space(8.0);
    match resp.clicked {
        Some(i) => ui.label(
            egui::RichText::new(format!("clicked card {i}"))
                .color(ACCENT)
                .small(),
        ),
        None => ui.label(
            egui::RichText::new("cards are clickable — hover for the tx id")
                .color(TEXT_MUTED)
                .small(),
        ),
    };
}
