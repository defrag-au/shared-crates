//! `TxCard` story — the form argument, at three densities, over the six shapes
//! a transaction actually comes in.
//!
//! ## The case this story is making
//!
//! The transaction at the top is real: a MachineHeadz collection offer filled
//! on Wayup, 3 September 2026. The feed row that shipped for it said
//!
//! ```text
//! [$elchapojr] [10 ₳ released] [wayup] [bought (CO)]           −7.63 ₳
//! 1h ago · 2026-09-03 04:07:29 · to addr1q8k6…7h6nqg   10 ₳ collection_offer_accepted
//! [MachineHeadz527 +1] [MachineHeadz357 +1]
//! ```
//!
//! — four chips doing the work of one clause, the *wallet net* as the largest
//! number on the row, and the settlement price as the smallest text on it next
//! to a raw database slug. The social card for the same transaction had already
//! solved this: `10 ₳ total` as the headline, `2 × MachineHeadz` as the
//! subject, `$boef bought from $elchapojr` as a sentence, and the net last and
//! grey. Toggle the density and compare.
//!
//! ## Why six entries and not one
//!
//! Each is a shape the widget has to survive, and four of them broke a live
//! social card before they were understood:
//!
//! 1. **Settlement** — the MachineHeadz fill. The `total` qualifier is there
//!    because `10 ₳` beside `2 × MachineHeadz` reads as ten *each* about as
//!    readily as ten for the pair.
//! 2. **Venue batch** — six listings, two offers and a cancellation in one
//!    Wayup submission. No item represents it and no price describes it, so it
//!    gets the venue's mark and a breakdown. Picking one item's artwork here
//!    captioned nine others; putting an asking price in success green said a
//!    wallet received money it never received.
//! 3. **Mint** — eleven NFTs into existence. The COUNT is the headline, never
//!    the ADA: `+1.49 ₳` on a mint is min-UTxO carrier ADA, the lovelace that
//!    must ride with a token for its output to be valid. Real, and not income.
//! 4. **Plain movement** — no subject and no price, so the net is the headline
//!    because it is the only figure there is. The one shape where leading with
//!    the net is correct.
//! 5. **Policy pair** — the same machinery from a policy's point of view. There
//!    is no "us", so there is no verb: `$elchapojr → $boef`, and no net line at
//!    all, because a `PolicyRow` carries no lovelace.
//! 6. **The two absences** — a source below the walk floor (resolves by
//!    deepening; pulses while a pass runs, offers to reach back when none is)
//!    beside a genuinely ambiguous side (never resolves, and must never offer).
//!    They are adjacent here on purpose: rendered the same they are a lie, and
//!    the toggle shows which sentence each one gets.
//!
//! Entries 5 and 6 are what a policy feed is made of, and they are the reason
//! the viewpoint is an enum rather than a flag — a `Pair` has no verb slot to
//! fill, so nothing here can claim a policy bought something.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::chip::ChipVariant;
use egui_widgets::image_loader::{iiif_asset_url, AssetImageSize};
use egui_widgets::party_badge::PartyBasis;
use egui_widgets::{
    Tone, TxArt, TxCard, TxCardData, TxDensity, TxHeadline, TxParty, TxPrint, TxVerb, TxViewpoint,
};

/// Real artwork, so the pile is judged against real images. A stack of grey
/// placeholder squares looks acceptable at any settings and proves nothing —
/// see the `image_stack` story, which exists because of exactly that.
const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";
const ART: &[(&str, &str)] = &[
    ("5069726174653834", "Pirate84"),
    ("506972617465323733", "Pirate273"),
    ("50697261746531303430", "Pirate1040"),
];

fn art_urls() -> Vec<String> {
    ART.iter()
        .map(|(hex, _)| iiif_asset_url(POLICY_ID, hex, AssetImageSize::Thumbnail))
        .collect()
}

/// Pinned so every frame renders identically — a story that drifts with the
/// wall clock cannot be screenshotted twice and compared.
const NOW: i64 = 1_788_412_049;

/// The MachineHeadz collection-offer fill: 2026-09-03 04:07:29 UTC, an hour
/// before `NOW`.
const FILL: i64 = 1_788_408_449;

#[derive(Default)]
pub struct TxCardState {
    pub density: Option<TxDensity>,
    pub walking: bool,
    pub last_action: Option<String>,
}

pub fn show(ui: &mut egui::Ui, state: &mut TxCardState) {
    let density = state.density.get_or_insert(TxDensity::Feature);

    ui.label(egui::RichText::new("Tx Card").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "One transaction as a VERDICT — what it was, who it was between, and the one figure \
             that says it. The feed row this replaces led with the wallet's net; the social card \
             for the same transaction leads with the price. This is the card's ranking, in the app.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Density").color(TEXT_MUTED).small());
        for d in TxDensity::ALL {
            if ui.selectable_label(*density == d, d.label()).clicked() {
                *density = d;
            }
        }
        ui.add_space(16.0);
        ui.checkbox(&mut state.walking, "walk in flight");
        ui.label(
            egui::RichText::new("— changes what a below-floor source says")
                .color(TEXT_MUTED)
                .small(),
        );
    });
    ui.add_space(4.0);
    if let Some(action) = &state.last_action {
        ui.label(egui::RichText::new(action).color(ACCENT).small());
    }
    ui.add_space(12.0);

    let d = *density;
    let mut action = None;
    let urls = art_urls();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── 1. SETTLEMENT ────────────────────────────────────────────────
        // The card the whole design argues from. `total` qualifies the price
        // because the lot is two; the subject is the shared STEM, never one
        // member's name — "MachineHeadz527" over a line counting two says the
        // sale was #527 and something else came along.
        section(ui, "Settlement", "Something changed hands for money. The PRICE is the headline; the net goes last, grey, for whoever is reconciling.");
        let prints = [
            TxPrint::new("MachineHeadz527").image(&urls[0]),
            TxPrint::new("MachineHeadz357").image(&urls[1]),
        ];
        let settlement = TxCardData::new(
            "collection offer accepted · wayup",
            TxHeadline::new("10 ₳", Tone::Positive).qualifier("total"),
            TxViewpoint::Wallet {
                who: TxParty::observed("$boef"),
                verb: TxVerb::Bought,
                // The SELLER on a collection-offer fill is the venue's word —
                // `seller_stake` off the market verdict, not an output the walk
                // resolved. It renders as derived, and that distinction is the
                // whole reason `PartyBasis` is positional.
                other: TxParty::derived("$elchapojr")
                    .key("addr1q8k6zq3v9m2n5p8s1t4w7y0b3e6h9k2n5q8t1w4y7h6nqg"),
            },
            FILL,
        )
        .subject("2 × MachineHeadz")
        .art(TxArt::Prints(&prints))
        // THE TAGS SURVIVE — but as facets, not as the verdict. The row this
        // replaced said everything in chips and nothing in words; the fix was
        // to write the sentence, not to delete the chips. These are what a
        // reader clicks to slice the feed, which is why they sit last.
        .tag("wayup", ChipVariant::Warning)
        .tag("collection offer", ChipVariant::Info)
        .tag("bought", ChipVariant::Success)
        .caution("2 items in this transaction")
        .footnote("wallet net −7.6295 ₳");
        action = action.take().or(card(ui, &settlement, d, state.walking));

        // ── 2. VENUE BATCH ───────────────────────────────────────────────
        // No item represents it and no price describes it. The venue's mark and
        // a breakdown is the true thing to say about a transaction that resists
        // being summarised.
        section(ui, "Venue batch", "Six listings, two offers and a cancellation in one submission. Neither an item nor a price can stand for it — so neither is shown.");
        let batch = TxCardData::new(
            "wayup",
            TxHeadline::new("6 listed · 2 offers made · 1 delisted", Tone::Neutral),
            TxViewpoint::Sole {
                who: TxParty::observed("$boef"),
            },
            FILL - 3_600,
        )
        .art(TxArt::Mark {
            image_url: None,
            label: "wayup",
        })
        .tag("wayup", ChipVariant::Warning)
        .tag("batch", ChipVariant::Info)
        // Value PARKED at a script, not spent. Amber rather than red: it is not
        // a loss, and it comes back on a delist.
        .caution("813.4 ₳ locked into contracts")
        .footnote("wallet net −1.8412 ₳");
        action = action.take().or(card(ui, &batch, d, state.walking));

        // ── 3. MINT ──────────────────────────────────────────────────────
        // The COUNT leads. The net is min-UTxO carrier ADA and belongs in the
        // footnote — recorded, never presented as income.
        section(ui, "Mint", "Eleven things came into existence. The count is the headline; +1.49 ₳ is carrier ADA, not income, so it goes to the footnote.");
        let minted = [
            TxPrint::new("Spanner #0041").image(&urls[0]),
            TxPrint::new("Spanner #0042").image(&urls[1]),
            TxPrint::new("Spanner #0043").image(&urls[2]),
        ];
        let mint = TxCardData::new(
            "minted",
            TxHeadline::new("11 × Spanner", Tone::Positive),
            TxViewpoint::Sole {
                who: TxParty::observed("$boef"),
            },
            FILL - 86_400,
        )
        .art(TxArt::Prints(&minted))
        .tag("mint", ChipVariant::Success)
        .caution("11 items in this transaction")
        .footnote("wallet net +1.49126 ₳");
        action = action.take().or(card(ui, &mint, d, state.walking));

        // ── 4. PLAIN MOVEMENT ────────────────────────────────────────────
        // The one shape where the net IS the story, because it is the only
        // figure there is.
        section(ui, "Plain movement", "No subject and no price. The net is the headline here — and only here — because nothing else is available to lead with.");
        let movement = TxCardData::new(
            "send",
            TxHeadline::new("−240 ₳", Tone::Negative),
            TxViewpoint::Wallet {
                who: TxParty::observed("$boef"),
                verb: TxVerb::SentTo,
                other: TxParty::observed("$privateers"),
            },
            FILL - 172_800,
        )
        .footnote("fee 0.1743 ₳");
        action = action.take().or(card(ui, &movement, d, state.walking));

        // ── 5. POLICY PAIR ───────────────────────────────────────────────
        // The SAME transaction as entry 1, seen from the policy instead of the
        // wallet. No "us", so no verb and no net — a `PolicyRow` carries no
        // lovelace at all.
        section(ui, "Policy pair — no point of view", "The settlement above, from the POLICY's side. Nobody here is 'us', so there is no verb of ownership and no wallet net — only the pair, and what the unit went for.");
        let pair_prints = [TxPrint::new("MachineHeadz527").image(&urls[0])];
        let pair = TxCardData::new(
            "collection offer accepted · wayup",
            TxHeadline::new("10 ₳", Tone::Positive).qualifier("lot of 2"),
            TxViewpoint::Pair {
                from: TxParty::derived("$elchapojr"),
                to: TxParty::observed("$boef"),
            },
            FILL,
        )
        .subject("MachineHeadz527")
        .art(TxArt::Prints(&pair_prints))
        // The bundle caveat, in the words the policy feed uses: the price is
        // the LOT's, and a reader who totals it per asset doubles the volume.
        .caution("price is the lot's — 2 assets, not this one's");
        action = action.take().or(card(ui, &pair, d, state.walking));

        // ── 6. THE TWO ABSENCES ──────────────────────────────────────────
        // Adjacent on purpose. Rendered the same they are a lie: one is fixed
        // by walking deeper, the other is not fixable at all.
        section(ui, "Two absences, two sentences", "Below-floor resolves by walking deeper — it pulses while a pass runs and offers to reach back when none is. Ambiguous NEVER resolves, so it states itself and offers nothing. Toggle 'walk in flight' above.");
        let below_prints = [TxPrint::new("MachineHeadz882").image(&urls[1])];
        let below = TxCardData::new(
            "transfer",
            TxHeadline::new("1 unit", Tone::Neutral),
            TxViewpoint::Pair {
                from: TxParty::BelowFloor,
                to: TxParty::observed("$djo"),
            },
            FILL - 259_200,
        )
        .subject("MachineHeadz882")
        .art(TxArt::Prints(&below_prints));
        action = action.take().or(card(ui, &below, d, state.walking));

        let amb_prints = [TxPrint::new("MachineHeadz119").image(&urls[2])];
        let ambiguous = TxCardData::new(
            "offer accepted · jpg",
            TxHeadline::new("47 ₳", Tone::Positive).qualifier("lot of 9"),
            TxViewpoint::Pair {
                from: TxParty::Ambiguous { count: 4 },
                to: TxParty::observed("$mr.wilford"),
            },
            FILL - 345_600,
        )
        .subject("MachineHeadz119")
        .art(TxArt::Prints(&amb_prints))
        .caution("price is the lot's — 9 assets, not this one's");
        action = action.take().or(card(ui, &ambiguous, d, state.walking));
    });

    if let Some(a) = action {
        state.last_action = Some(a);
    }
}

/// One card plus the response readout, so the story shows that `walk` and
/// `clicked` are different answers rather than asserting it in prose.
fn card(ui: &mut egui::Ui, data: &TxCardData<'_>, d: TxDensity, walking: bool) -> Option<String> {
    let resp = TxCard::new(data, d).now(NOW).walking(walking).show(ui);
    ui.add_space(10.0);
    if let Some(party) = resp.walk {
        return Some(format!("walk → {party}"));
    }
    // A tag click is a THIRD answer, distinct from opening the row and from
    // following its money — which is the whole reason the chips came back.
    if let Some(tag) = resp.filtered {
        return Some(format!("filter feed → {tag}"));
    }
    if resp.deepen {
        return Some("deepen → reach further back".to_string());
    }
    if resp.clicked {
        return Some(format!("open tx → {}", data.kicker));
    }
    None
}

fn section(ui: &mut egui::Ui, title: &str, why: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).color(ACCENT).small().strong());
    ui.label(egui::RichText::new(why).color(TEXT_MUTED).small());
    ui.add_space(6.0);
}

/// Keeps the import honest: the story names `PartyBasis` because the settlement
/// entry's seller is DERIVED, and a reader skimming the imports should see that
/// the distinction is being exercised rather than defaulted past.
const _: Option<PartyBasis> = Some(PartyBasis::Derived);
