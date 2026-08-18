//! `FlowRing` story — value moving between parties, live, on one spine.
//!
//! Fixture shaped like the real Mekka flow ledger: a small inner ring of the
//! project's own wallets (treasury, derived royalty address, two ops wallets)
//! and an outer ring of the people they dealt with.
//!
//! **What to look for:** press play. Value leaves the inner ring and crosses to
//! the rim as trains of dots, one dot per quantum, so a large payment reads as
//! a stream rather than a thicker line. Scrub instead of playing and the
//! particles sit genuinely mid-flight — position is a pure function of the
//! playhead, not an animation that has to be running. Hover any node for what
//! it held at that exact moment; click to pin it and everything unrelated
//! recedes.

use crate::TEXT_MUTED;
use egui_widgets::{FlowRing, RingFlow, RingNode, Selection, SpineState};

const DAY: i64 = 86_400;
const T0: i64 = 1_750_000_000;

pub struct FlowRingState {
    spine: Option<SpineState>,
    selection: Selection,
    /// Nodes the reader has switched off — density control that is not a
    /// threshold someone has to guess.
    off: Vec<String>,
}

impl Default for FlowRingState {
    fn default() -> Self {
        Self {
            spine: None,
            selection: Selection::default(),
            off: Vec::new(),
        }
    }
}

/// The project's own wallets. Ring 0.
const INNER: [&str; 4] = ["S1 treasury", "royalty (CIP-27)", "ops-payments", "team"];

/// Everyone else, in the four bands the ring encodes.
///
/// Deliberately covers ALL FOUR: a fixture that only exercised inner-vs-outer
/// left two of the ring tints unrendered, so nobody could see the palette it
/// was chosen for. The outermost band is the big one on purpose — unexamined
/// is the normal state of a real ledger, and watching it drain is the job.
const ASSOCIATES: [&str; 4] = ["contractor-01", "artist / Dwess", "infra", "marketing"];
const CUSTOMERS: [&str; 5] = ["payee-h", "payee-i", "payee-j", "payee-k", "grants"];
const UNEXAMINED: [&str; 5] = [
    "off-ramp script",
    "exchange-hot",
    "audit",
    "legal",
    "unresolved payer",
];

/// Every non-core party, in ring order — the flow fixture indexes this.
const OUTER: [&str; 14] = [
    "contractor-01",
    "artist / Dwess",
    "infra",
    "marketing",
    "payee-h",
    "payee-i",
    "payee-j",
    "payee-k",
    "grants",
    "off-ramp script",
    "exchange-hot",
    "audit",
    "legal",
    "unresolved payer",
];

fn nodes(off: &[String]) -> Vec<RingNode<'static>> {
    let mut out: Vec<RingNode<'static>> = Vec::new();
    let bands: [(&[&str], u8); 4] = [
        (&INNER, 0),
        (&ASSOCIATES, 1),
        (&CUSTOMERS, 2),
        (&UNEXAMINED, 3),
    ];
    for (keys, ring) in bands {
        for k in keys {
            out.push(RingNode::new(k, ring).active(!off.iter().any(|o| o == k)));
        }
    }
    out
}

fn flows() -> Vec<RingFlow<'static>> {
    let mut out = Vec::new();
    // Receipts nobody could attribute — the offline-walk reality.
    for k in 0..26i64 {
        out.push(RingFlow {
            timestamp: T0 + k * 5 * DAY,
            from: "unresolved payer",
            to: INNER[(k % 4) as usize],
            quantity: 12_000_000_000 + (k as u64 % 5) * 4_000_000_000,
        });
    }
    // The treasury paying out, biggest to the shared contractor.
    for k in 0..18i64 {
        out.push(RingFlow {
            timestamp: T0 + (6 + k * 7) * DAY,
            from: INNER[(k % 3) as usize],
            to: OUTER[(k % 9) as usize],
            quantity: 2_000_000_000 + (k as u64 % 6) * 9_000_000_000,
        });
    }
    // A recurring salary-shaped channel — steady, small, same payee.
    for k in 0..20i64 {
        out.push(RingFlow {
            timestamp: T0 + (10 + k * 6) * DAY,
            from: "ops-payments",
            to: "contractor-01",
            quantity: 1_500_000_000,
        });
    }
    // PEER-TO-PEER, out on the rim — the project is not involved at all.
    //
    // These are the chords the band routing exists for. Drawn the old way
    // they dived through the centre and landed on top of the treasury's
    // traffic, so a payment between two strangers was indistinguishable from
    // one the project made. Here they stay out in their own band, and the
    // middle is left to mean something: value crossing it involves a project
    // wallet.
    for (k, (from, to)) in [
        ("exchange-hot", "off-ramp script"),
        ("payee-h", "payee-j"),
        ("audit", "legal"),
        ("artist / Dwess", "infra"),
        ("payee-i", "exchange-hot"),
    ]
    .into_iter()
    .enumerate()
    {
        for j in 0..4i64 {
            out.push(RingFlow {
                timestamp: T0 + (14 + k as i64 * 9 + j * 21) * DAY,
                from,
                to,
                quantity: 3_000_000_000 + (j as u64 % 3) * 6_000_000_000,
            });
        }
    }
    out.sort_by_key(|f| f.timestamp);
    out
}

pub fn show(ui: &mut egui::Ui, state: &mut FlowRingState) {
    let flows = flows();
    let lo = flows.iter().map(|f| f.timestamp).min().unwrap_or(T0);
    let hi = flows.iter().map(|f| f.timestamp).max().unwrap_or(T0 + DAY) + 4 * DAY;
    let spine = state.spine.get_or_insert_with(|| SpineState::new((lo, hi)));

    ui.label(
        egui::RichText::new(
            "Rings are the CLASSIFICATION, inward: core team, associates (paid by the \
             project), customers, and — outermost, in grey — the wallets nobody has \
             examined yet. Colour means somebody made a call; grey means the work is \
             still to do. Value crosses the middle as particles, one dot per quantum, so \
             a large payment is a longer train, not a thicker line. Press play, or scrub: \
             particle position is a function of the playhead, so a still frame shows \
             value genuinely in flight.",
        )
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(4.0);

    let tick = |t: i64, _s: i64| egui_widgets::format_date(t);
    egui_widgets::TimeSpine::new(spine)
        .format_tick(&tick)
        .height(52.0)
        .show(ui);

    // Density control: switching a wallet off keeps its seat, so nothing that
    // is still on ever moves.
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("active:").small().color(TEXT_MUTED));
        for k in INNER.iter().chain(OUTER.iter()) {
            let on = !state.off.iter().any(|o| o == k);
            if ui.selectable_label(on, *k).clicked() {
                match on {
                    true => state.off.push((*k).to_string()),
                    false => state.off.retain(|o| o != k),
                }
            }
        }
    });

    let nodes = nodes(&state.off);
    let inventory = |key: &str, _at: i64| -> Vec<(String, String)> {
        // Stand-in for the reader's real inventory lookup: net ADA since the
        // wallet came under watch, plus a couple of notable holdings. Never the
        // full list — a tooltip with forty tokens in it is not a tooltip.
        vec![
            ("net ADA".into(), format!("{} ADA", key.len() * 1_237)),
            ("Mekka S1".into(), format!("{} NFTs", key.len() % 9)),
            ("USDM".into(), format!("{}", key.len() * 41)),
        ]
    };

    // Quantities are lovelace, so the quantum is too: 1,000 ADA per dot.
    let r = FlowRing::new(&nodes, &flows, "lovelace", spine, &mut state.selection)
        .quantum(1_000_000_000)
        .inventory(&inventory)
        .height(430.0)
        .show(ui);

    ui.add_space(6.0);
    let sel = state
        .selection
        .active()
        .map(|s| format!("watching: {s}"))
        .unwrap_or_else(|| "nothing pinned — click a node".into());
    ui.label(
        egui::RichText::new(format!(
            "{sel}   ·   {} in the air, {} particles   ·   {} nodes on, {} off",
            r.in_flight, r.particles, r.nodes_shown, r.nodes_inactive
        ))
        .small()
        .color(TEXT_MUTED),
    );
}
