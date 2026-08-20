//! `TimeSpine` story — the falsifier for "is egui why the surface feels flat?"
//!
//! One spine, one selection, three faces. Same Mekka-shaped fixtures as the
//! `CapitalFlow` story (160 buyers, 5 whales, the observed deployment split),
//! now composed instead of stacked:
//!
//! - **the spine** drives everything: scrub the tick lane to move the playhead,
//!   drag in the brush lane to filter, press play and watch;
//! - **the holder field** gives every asset a dot and every holder a pile. A
//!   dot is keyed by the ASSET, so when custody changes the same dot flies to
//!   its new pile and the field rebalances — the mint fills it, the
//!   aftermarket concentrates it. Hovering a pile names its holder in the
//!   shared selection;
//! - **capital flow** shares the same playhead, so both faces move together.
//!
//! What to look for: does motion + linkage make it stop feeling flat? If yes,
//! the framework was never the problem. If no, that is evidence for D3.

use crate::stories::capital_flow::{ada, arrivals, events, month, moves, RAISED};
use crate::TEXT_MUTED;
use egui_widgets::{
    capital_bands, format_date, AliasIndex, CapitalFlow, HolderField, MarkKind, PartyFinder,
    PartyFinderState, Selection, SpineState, TimeSpine, WalletIdentity,
};

pub struct TimeSpineState {
    spine: Option<SpineState>,
    selection: Selection,
    finder: PartyFinderState,
    aliases: Option<AliasIndex>,
}

impl Default for TimeSpineState {
    fn default() -> Self {
        Self {
            spine: None,
            selection: Selection::default(),
            finder: PartyFinderState::default(),
            aliases: None,
        }
    }
}

/// Every holder in the fixture, with the identifiers a real wallet carries:
/// a stake key, an address or two, and (for some) handles. This is what the
/// project ledger's `party_alias` table will feed in from real data.
fn aliases(arrivals: &[egui_widgets::Arrival<'_>]) -> AliasIndex {
    let mut seen: Vec<&str> = Vec::new();
    for a in arrivals {
        if !seen.contains(&a.holder) {
            seen.push(a.holder);
        }
    }
    let mut ix = AliasIndex::default();
    for (i, h) in seen.iter().enumerate() {
        // Fixture holders are named "whale-0" / "buyer-042"; give them the
        // shape of real identifiers so search feels like the real thing.
        let mut w = WalletIdentity::new(*h)
            .address(format!("addr1q{h}{:0>40}", i))
            .address(format!("addr1q{h}second{:0>34}", i));
        match *h {
            "whale-0" => w = w.handle("$mekka").handle("mekkalabs").label("S1 treasury"),
            "whale-1" => w = w.handle("$djo"),
            "whale-2" => w = w.handle("privateers"),
            "whale-3" => w = w.handle("$curiousfutures"),
            "whale-4" => w = w.handle("boef"),
            _ if i % 9 == 0 => w = w.handle(format!("buyer{i}")),
            _ => {}
        }
        ix.push(w);
    }
    ix
}

pub fn show(ui: &mut egui::Ui, state: &mut TimeSpineState) {
    let arrivals = arrivals();
    let moves = moves();
    let events = events();
    let bands = capital_bands(&events);
    if state.aliases.is_none() {
        state.aliases = Some(aliases(&arrivals));
    }

    // Domain from the data — first arrival to last deployment.
    // The domain has to cover the aftermarket too, or every trade sits off the
    // right-hand end of the spine and the field never reshuffles.
    let lo = moves
        .iter()
        .map(|m| m.timestamp)
        .chain(events.iter().map(|e| e.timestamp))
        .min()
        .unwrap_or(0);
    let hi = moves
        .iter()
        .map(|m| m.timestamp)
        .chain(events.iter().map(|e| e.timestamp))
        .max()
        .unwrap_or(1);
    let spine = state.spine.get_or_insert_with(|| SpineState::new((lo, hi)));

    ui.label(
        egui::RichText::new(
            "One spine, one selection, three faces. Find a wallet by handle, stake key, \
             address or label and it's pinned everywhere; scrub the ruler; drag the \
             lower lane to brush a range; press play. Hover a pile.",
        )
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(6.0);

    // ── find → pin → watch ─────────────────────────────────────────────────
    if let Some(ix) = &state.aliases {
        PartyFinder::new("ts-finder", ix, &mut state.finder, &mut state.selection).show(ui);
    }
    ui.add_space(6.0);

    // ── the spine ──────────────────────────────────────────────────────────
    let tick = |t: i64, spacing: i64| {
        if spacing >= 86_400 * 10 {
            month(t)
        } else {
            format_date(t)
        }
    };
    // Pinning a wallet MARKS ITS MOVES on the spine: acquisitions above the
    // midline, disposals below. Watching a wallet has to answer "when", and the
    // axis you brush is where that answer belongs.
    let watched = state.selection.active().map(|k| k.to_string());
    let marks: Vec<(i64, MarkKind)> = match &watched {
        Some(k) => moves
            .iter()
            .filter_map(|m| {
                if m.to == Some(k.as_str()) {
                    Some((m.timestamp, MarkKind::In))
                } else if m.from == Some(k.as_str()) {
                    Some((m.timestamp, MarkKind::Out))
                } else {
                    None
                }
            })
            .collect(),
        None => Vec::new(),
    };
    let sr = TimeSpine::new(spine)
        .format_tick(&tick)
        .marks(&marks)
        .height(58.0)
        .show(ui);
    let _ = sr;

    ui.add_space(10.0);

    // ── assets out: dots moving between holders (motion + selection) ──────
    ui.label(egui::RichText::new("assets out — who holds what, over time").strong());
    let ar = HolderField::new(&moves, spine, &mut state.selection)
        .height(320.0)
        .show(ui);

    ui.add_space(10.0);

    // ── capital out on the same playhead ──────────────────────────────────
    ui.label(egui::RichText::new("capital out — by destination").strong());
    let cr = CapitalFlow::new(&events, &bands, RAISED, spine.playhead, &ada, &month)
        .height(240.0)
        .show(ui);
    if let Some(t) = cr.scrubbed_to {
        // A face can drive the shared playhead too — one store, many writers.
        spine.set_playhead(t);
    }

    ui.add_space(8.0);
    let sel = state
        .selection
        .active()
        .map(|s| format!("selected: {s}"))
        .unwrap_or_else(|| "nothing selected — hover or click a pile".into());
    ui.label(
        egui::RichText::new(format!(
            "{sel}   ·   {} assets / {} holders shown   ·   playhead {}   ·   {}",
            ar.assets_shown,
            ar.holders_shown,
            format_date(spine.playhead),
            match spine.brush {
                Some((a, b)) => format!("brush {} - {}", format_date(a), format_date(b)),
                None => "no brush".to_string(),
            }
        ))
        .small()
        .color(TEXT_MUTED),
    );
}
