//! `PricingLadder` story — structural rungs, with sales drawn against them.

use egui_widgets::pricing_ladder::{self, Fill, LadderRung, LaneStyle, PricingLadderConfig};

use crate::{accent, muted};

/// `now` for the lanes — 2026-09-18, the day the fills below were pulled.
/// Fixed rather than the wall clock so the story keeps saying the same thing;
/// a drifting right edge would slowly turn a live ladder into a dead one and
/// nobody would notice the fixture had rotted.
const NOW: i64 = 1_789_683_535;

/// The real Black Flag `Rank` ladder, measured 2026-09-18 — **every figure on
/// this page came off mainnet**, none are invented.
///
/// Supply, ratio and ladder price are the server's resolved rungs. `realized`
/// and `support` are the demand model's own recency-weighted p50 and group
/// size. The fills are the raw market-ledger fills for each rung with their
/// buyer prices, joined through the live trait bitmaps — 213 fills across
/// 2,000 assets with zero unmatched, reproducing the server's support exactly
/// on 11 of 13 rungs.
///
/// The two that don't are the point of the third section. And the tempo is the
/// point of all of it: a fixture with invented timing would flatter the lane
/// exactly the way the block-train fixture once flattered its widget at 20×
/// real block size — the thing being tested here IS the timing.
///
/// Note the 2.5 ADA fills on Deckhand, Quartermaster and Captain. Those are
/// real: nominal min-ADA transfers wearing a sale's clothes. They are why the
/// scatter's y-domain is clamped rather than fitted.
#[rustfmt::skip]
fn black_flag() -> Vec<LadderRung> {
    let rows: [(&str, u64, f64, f64, Option<f64>, usize, &[(i64, f64)]); 13] = [
        ("Swab", 389, 1.00, 49.0, Some(23.0), 55, &[(1737336666,22.2), (1744324877,24.2), (1744482717,27.0), (1744483421,27.0), (1744483421,27.0), (1744483421,27.0), (1744484144,27.0), (1744922946,30.2), (1745170410,28.1), (1745170410,29.2), (1745170779,27.0), (1745170919,27.0), (1745186762,27.0), (1745186762,27.0), (1745215945,27.0), (1745215945,27.0), (1745215945,27.0), (1745215945,35.2), (1745215945,35.2), (1764378077,21.2), (1764675850,21.2), (1764706925,23.0), (1764706925,28.3), (1764745770,29.0), (1765640147,31.0), (1765640147,32.2), (1765640147,32.2), (1766265681,31.0), (1767998517,35.2), (1768014589,31.0), (1768020252,31.0), (1771095402,31.0), (1771095402,31.0), (1771095711,31.0), (1771105236,34.0), (1771105236,34.0), (1772689357,48.4), (1772690067,48.8), (1772690067,48.8), (1773666797,39.0), (1778630451,19.0), (1780593331,16.0), (1780593527,16.0), (1785009849,17.0), (1788873467,22.0), (1788873467,22.0), (1788873467,22.0), (1788873467,22.0), (1788873467,22.0), (1788873467,23.0), (1788873467,40.0), (1788873467,44.0), (1788873467,45.0), (1788873467,45.0), (1788873467,45.0)]),
        ("Deckhand", 320, 1.22, 59.6, Some(39.0), 46, &[(1734723500,15.0), (1734809000,15.0), (1735291616,15.0), (1735377611,22.2), (1740622666,20.2), (1743478390,2.5), (1744324877,23.1), (1744386878,27.0), (1744412186,25.2), (1745186693,29.0), (1745216002,27.0), (1745216002,31.0), (1757734548,24.2), (1758267606,24.2), (1761130690,25.2), (1764199377,22.2), (1764378077,21.2), (1764675850,21.2), (1764675850,21.2), (1764731725,22.2), (1764731853,28.1), (1764745770,32.1), (1768011257,39.0), (1768011257,45.2), (1768020252,41.0), (1768206890,32.2), (1768206890,32.2), (1768222875,39.0), (1768222875,42.2), (1770999536,34.2), (1771105236,34.0), (1771905228,40.2), (1775086247,48.0), (1776214381,32.2), (1778629961,19.0), (1778631527,20.0), (1778631527,32.0), (1779057172,19.0), (1779057172,19.0), (1780593405,16.0), (1788873467,39.0), (1788873467,39.0), (1788873467,39.0), (1788873467,39.0), (1788873467,44.0), (1788873467,55.0)]),
        ("Lookout", 280, 1.39, 68.1, Some(18.0), 24, &[(1735436671,22.2), (1737336666,22.2), (1738279733,20.1), (1742776829,23.2), (1743023814,22.2), (1745181767,25.0), (1745191128,20.0), (1745216002,27.0), (1757720262,21.2), (1758267606,21.1), (1758267606,22.1), (1761130673,20.2), (1761130749,28.2), (1761130749,28.2), (1761130909,32.2), (1764731853,29.2), (1767595322,29.2), (1768247452,29.2), (1769211760,31.0), (1772690067,45.6), (1773668540,44.0), (1773668594,44.0), (1778631527,30.0), (1782858260,18.0)]),
        ("Cook", 220, 1.77, 86.6, Some(78.2), 15, &[(1734809725,15.0), (1735377242,32.2), (1739641566,25.2), (1744399554,25.2), (1744402940,48.2), (1745076479,30.2), (1745216002,31.0), (1745216797,30.2), (1764731803,31.0), (1764745770,38.3), (1765944424,30.0), (1768016660,50.2), (1768083880,62.0), (1772689357,62.6), (1772712331,78.2)]),
        ("Sailmaker", 200, 1.95, 95.3, Some(58.0), 27, &[(1735290794,30.0), (1736806965,21.2), (1737678110,20.0), (1737945247,22.2), (1738279733,22.2), (1740622666,20.2), (1744669249,44.2), (1745216797,34.0), (1745216797,37.0), (1747085759,25.2), (1747727090,49.0), (1750706185,35.2), (1756107045,24.0), (1756107097,24.0), (1764377759,27.2), (1764731639,53.9), (1768283338,97.5), (1769678143,53.9), (1770103481,68.0), (1770103481,97.5), (1772690018,100.0), (1782895601,32.0), (1788873467,55.0), (1788873467,56.0), (1788873467,58.0), (1788873467,60.0), (1788873467,90.0)]),
        ("Carpenter", 180, 2.16, 105.9, Some(78.0), 15, &[(1735290418,15.0), (1735377231,32.2), (1736743369,20.0), (1742511336,23.2), (1745181767,35.4), (1745187837,44.0), (1745216797,37.0), (1748554302,23.0), (1748554302,27.0), (1768011252,59.0), (1769678143,68.6), (1770102802,65.0), (1788873467,77.0), (1788873467,78.0), (1788873467,79.0)]),
        ("Gunner", 160, 2.43, 119.1, Some(50.0), 12, &[(1734933369,30.0), (1745187493,68.6), (1745187627,68.6), (1745188670,70.6), (1748513509,18.0), (1757645837,49.1), (1757645837,50.1), (1761285590,25.0), (1767333480,59.0), (1769678143,98.0), (1770103378,117.6), (1788053650,50.0)]),
        ("Boatswain", 100, 3.89, 190.6, Some(190.0), 7, &[(1745186762,73.5), (1757833293,65.0), (1761130842,85.0), (1761130842,100.0), (1770103481,147.0), (1775022406,147.0), (1784652177,190.0)]),
        ("Quartermaster", 60, 6.48, 317.7, Some(85.2), 4, &[(1739825053,2.5), (1741291227,2.5), (1745187446,83.3), (1757645837,85.2)]),
        ("Navigator", 40, 9.72, 476.5, None, 0, &[(1754682598,50.0), (1764163517,69.0)]),
        ("First Mate", 30, 12.97, 635.4, None, 0, &[(1748901925,249.0), (1754706534,70.0)]),
        ("Captain", 15, 25.93, 1270.7, Some(1960.0), 4, &[(1742029849,2.5), (1753449574,250.0), (1763048209,600.0), (1789560748,1960.0)]),
        ("Legendary", 6, 64.83, 3176.8, None, 0, &[]),
    ];
    rows.into_iter()
        .map(|(value, supply, ratio, target, realized, support, fills)| LadderRung {
            value: value.to_string(),
            supply,
            ratio,
            target: Some(target),
            realized,
            support,
            fills: fills.iter().map(|&(at, price)| Fill { at, price }).collect(),
        })
        .collect()
}

fn config(id: &str, lane: LaneStyle) -> PricingLadderConfig {
    PricingLadderConfig {
        category: "Rank".into(),
        sentinel: Some("Swab".into()),
        id_salt: id.into(),
        now: NOW,
        lane,
        ..Default::default()
    }
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Pricing Ladder")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A collection priced structurally: every rung a fixed multiple of the \
             anchor's price, derived from supply. Sales are drawn against the \
             ladder as evidence, never folded into it. Real Black Flag data, \
             measured 2026-09-18.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    pricing_ladder::show(
        ui,
        &black_flag(),
        &PricingLadderConfig {
            highlight: Some("Carpenter".into()),
            ..config("story_scatter", LaneStyle::PriceScatter)
        },
    );

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("One instrument, both questions")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The lane's rule IS the rung's ladder price, and height is a multiple \
             of it — so a 2× move looks the same on a 49 ADA rung and a 3,176 ADA \
             one. Almost every mark sits below the rule: on this collection the \
             ladder price is a ceiling nobody reaches. Read Carpenter and \
             Sailmaker left to right and the marks climb toward it; read \
             Quartermaster and there is nothing in the right half at all.",
        )
        .color(muted(ui))
        .small(),
    );

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Time only, for comparison")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The same rungs with LaneStyle::Ticks — when, but not at what. \
             Compact, and the only option for a ladder whose rungs have no \
             price to measure against.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);
    pricing_ladder::show(ui, &black_flag(), &config("story_ticks", LaneStyle::Ticks));

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Where the model and the lane disagree")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Navigator and First Mate each traded twice. The demand model gates \
             its median at a minimum group size, so it reports support 0 for \
             both — indistinguishable from never having traded. The count column \
             shows what was OBSERVED, so these say 2; the median column stays \
             empty because the model will not stand behind a number. Only \
             Legendary, with an empty lane, says \"no sales\".",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);

    let disputed: Vec<LadderRung> = black_flag()
        .into_iter()
        .filter(|r| matches!(r.value.as_str(), "Navigator" | "First Mate" | "Legendary"))
        .collect();
    pricing_ladder::show(
        ui,
        &disputed,
        &config("story_disputed", LaneStyle::PriceScatter),
    );

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Before anything has traded")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The case a sales chart cannot draw at all. The ladder exists the \
             moment the collection does, so the rungs price even with an empty \
             book — and with no anchor resolved yet, the price column says so \
             rather than showing a zero. No fills anywhere, so no lane.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(8.0);

    let unanchored: Vec<LadderRung> = black_flag()
        .into_iter()
        .take(4)
        .map(|r| LadderRung {
            target: None,
            realized: None,
            support: 0,
            fills: Vec::new(),
            ..r
        })
        .collect();
    pricing_ladder::show(
        ui,
        &unanchored,
        &PricingLadderConfig {
            now: 0,
            ..config("story_unanchored", LaneStyle::PriceScatter)
        },
    );
}
