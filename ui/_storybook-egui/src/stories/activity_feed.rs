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
//! arriving against a payment) and a bare transfer with no tags at all.
//!
//! Every entry that HAS another side names it on the time line, and the feed
//! is `walkable`, so those are links. That pairing is the point of the story:
//! a card can be busy with four tags and still say who it was with, which is
//! what the old layout gave up — it showed the counterparty only when there
//! were no tags to show instead. Click a party and the response reports
//! `walk`, not `clicked`.

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
            .counterparty_label("to")
            .counterparty(Some(
                "addr1q8pytk5x0jv7m2q4h9c3n6d8s1f4g7j0l3p6r9t2w5y8b1e4h7k0m3q6t9",
            ))
            .tx_id("9d41b7e0c2a85f36b1e094d7a3c5f288")
            .asset(ActivityAsset::new("Walker183", -1)),
        ActivityEntry::new(1787704389, 2_430_840)
            .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("sale", ChipVariant::Info))
            .secondary("$0.59")
            .counterparty_label("from")
            .counterparty(Some(
                "addr1q9m4k7p0s3v6y9b2e5h8k1n4q7t0w3z6c9f2j5m8p1s4v7y0b3e6h9k2n5",
            ))
            .tx_id("13863a1933e18f62aa0c9c2e4d3f8b71")
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1729", 1))
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1724", 1)),
        ActivityEntry::new(1787704257, 2_430_840)
            .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
            .tag(ActivityTag::new("sale", ChipVariant::Info))
            .secondary("$0.59")
            .counterparty_label("from")
            .counterparty(Some(
                "addr1q9m4k7p0s3v6y9b2e5h8k1n4q7t0w3z6c9f2j5m8p1s4v7y0b3e6h9k2n5",
            ))
            .tx_id("bfd398b8ff02efa0119c7a5d6e2b4c88")
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1731", 1))
            .asset(ActivityAsset::new("HOSKY C(ash Grab)NFT 1702", 1)),
        ActivityEntry::new(1787704164, -9_612_000)
            .tag(ActivityTag::new("Minswap", ChipVariant::Info))
            .tag(ActivityTag::new("swap", ChipVariant::Tag))
            .secondary("$2.33")
            .counterparty_label("with")
            .counterparty(Some(
                "addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz2j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq0xmsha",
            ))
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
        // Previous day, and an untagged transfer — the party is the only
        // thing the card has to say about it.
        ActivityEntry::new(1787601442, 500_000_000)
            .counterparty_label("from")
            .counterparty(Some(
                "addr1qy8vw0xz4mkq9wtn3d6h2k5r7pl0s8ja4c6v2n9m3q7x5f8d2g4h6j",
            ))
            .tx_id("c04e91a7f3b28d56091e7a4c2b8f5d31"),
    ];

    // MARKED AND SCROLLED-TO — the deep-link case.
    //
    // A selection made somewhere OTHER than the feed: a link that named one
    // transaction, or a pick off the stave. Both halves are needed and they
    // are easy to mistake for one feature. Without the MARK the detail panel
    // describes a card the reader cannot pick out of the list; without the
    // SCROLL the card may be hundreds of rows down and never comes into view.
    //
    // Note the opposite lifetimes, which is why they are separate builders:
    // the mark persists for as long as the selection stands, while the scroll
    // is spent by the frame that serves it. The button below models the host's
    // job — request once, and let it be consumed. Holding `scroll_to` every
    // frame would pin the viewport and the reader could never look away.
    let marked_id = ui.id().with("activity_feed_marked");
    let scroll_id = ui.id().with("activity_feed_scroll");
    let mut marked: Option<usize> = ui.data(|d| d.get_temp(marked_id)).unwrap_or(None);
    // Read-then-remove: `remove_temp` returns `()`, so taking the value and
    // clearing it is two steps under one `data_mut` — which is what makes the
    // request "spent by the frame that serves it" rather than sticky.
    let scroll_to: Option<usize> = ui
        .data_mut(|d| {
            let requested = d.get_temp::<Option<usize>>(scroll_id);
            d.remove_temp::<Option<usize>>(scroll_id);
            requested
        })
        .flatten();

    ui.horizontal(|ui| {
        if ui.button("open card 4 from a link").clicked() {
            marked = Some(4);
            ui.data_mut(|d| d.insert_temp(scroll_id, Some(4usize)));
        }
        if marked.is_some() && ui.button("clear").clicked() {
            marked = None;
        }
    });
    ui.add_space(8.0);

    let resp = ActivityFeed::new(&entries, &ada)
        .walkable(true)
        .marked(marked)
        .scroll_to(scroll_to)
        .show(ui);
    // A click in the feed moves the mark too, so the two selection routes
    // agree — a card opened by hand looks the same as one opened by a link.
    if let Some(i) = resp.clicked {
        marked = Some(i);
    }
    ui.data_mut(|d| d.insert_temp(marked_id, marked));
    ui.add_space(8.0);
    match (resp.clicked, resp.walk) {
        // The party wins the click it is under, so these can never both be
        // set — showing them as one line keeps that visible.
        (_, Some(i)) => ui.label(
            egui::RichText::new(format!("walk to the party on card {i}"))
                .color(ACCENT)
                .small(),
        ),
        (Some(i), None) => ui.label(
            egui::RichText::new(format!("clicked card {i}"))
                .color(ACCENT)
                .small(),
        ),
        (None, None) => ui.label(
            egui::RichText::new(
                "cards are clickable — hover for the tx id, or click a party to walk to it",
            )
            .color(TEXT_MUTED)
            .small(),
        ),
    };
}
