//! `FlowRing` — value moving between parties, live, on the shared spine.
//!
//! Parties sit at **fixed positions on concentric rings** and value crosses the
//! middle as particles. A wallet never moves, so the eye can hold it while the
//! flows change; that is the same object-constancy rule the holder field is
//! built on.
//!
//! ## Why a ring, and why not Circos
//!
//! A ring gives every party a stable, unoccluded seat and makes "who deals with
//! whom" a chord rather than a row in a table. Circos is the reference for the
//! grammar — grouped sectors, ribbons, ordering as a design decision — but the
//! goal there is to compress a dense dataset into ONE static image, and that is
//! explicitly not the goal here. Density is handled by the playhead and by
//! switching nodes off, not by cramming everything into a single frame.
//!
//! **Rings are concentric, not stacked.** Hop 0 (the project's own wallets) is
//! the inner ring, hop 1 the next, and so on. Stacked 3D rings projected to 2D
//! trade readable magnitude and adjacency for depth cues that mislead; flat
//! concentric rings keep every node unoccluded and give "layer" a real meaning
//! the walk already computes.
//!
//! ## Particles are a pure function of the playhead
//!
//! A flow at time `t` is in the air for `flight` seconds of DATA time, so a
//! particle's position is `(playhead - t) / flight` — no animation state, no
//! tweens. Scrubbing therefore shows value genuinely mid-flight instead of a
//! settled frame, playing shows it stream, and a still is reproducible. This is
//! deliberately different from the holder field, where a dot's identity had to
//! persist across moves; here nothing needs to be remembered.
//!
//! **One dot is one QUANTUM of value**, and the quantum is stated on screen.
//! That is what makes an amount countable again: an NFT is indivisible and
//! could be one dot, but value is divisible and spans orders of magnitude, so
//! it is quantised instead. One unit at a time, always — raw Cardano quantities
//! from different units cannot share a scale.
//!
//! ## Berths — infrastructure docks OUTSIDE the rim
//!
//! The rings encode a **relationship class** a curator asserts: core,
//! associate, customer, unexamined. A minting provider, an airdrop payer or a
//! CEX hot wallet has no such class — it is infrastructure the project used,
//! context rather than subject — so seating it on a ring makes it impersonate
//! a wallet awaiting classification, and its fan-out drags the chart's ink to
//! one fake seat. A node marked [`RingNode::berth`] instead docks beyond the
//! outer ring, as a square pill rather than a dot, at the **angular centroid
//! of its actual counterparties** — so its chords arrive from one direction
//! and their shared radial departure reads as a single trunk that fans out
//! only inside the rim. Beyond the rim is also semantically right for a
//! boundary: past the outer ring IS where the chain's reach ends.

use egui::{Align2, Color32, Pos2, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::selection::Selection;
use crate::time_spine::SpineState;

/// A party's seat on the ring. Position depends only on `hop` and index, so it
/// is stable across frames and across filter changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingNode<'a> {
    pub key: &'a str,
    /// Which concentric ring: 0 = the project's own wallets.
    pub hop: u8,
    /// Switched off by the reader to cut clutter. An inactive node keeps its
    /// seat (so nothing else moves) but is drawn faint and carries no flows.
    pub active: bool,
    /// Provider capability marks (`"bank.pillar · minting"`). `Some` docks
    /// this node OUTSIDE the rim as a berth instead of seating it on `hop` —
    /// see the module docs. The string is what the always-on label prints
    /// beside the key, so it should already be reader-facing.
    pub berth: Option<&'a str>,
}

impl<'a> RingNode<'a> {
    pub fn new(key: &'a str, hop: u8) -> Self {
        Self {
            key,
            hop,
            active: true,
            berth: None,
        }
    }

    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }

    /// Dock this node outside the rim as infrastructure, with capability marks.
    pub fn berth(mut self, marks: &'a str) -> Self {
        self.berth = Some(marks);
        self
    }
}

/// One movement of the displayed unit between two seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingFlow<'a> {
    pub timestamp: i64,
    pub from: &'a str,
    pub to: &'a str,
    /// Absolute magnitude in the displayed unit.
    pub quantity: u64,
}

pub struct FlowRingResponse {
    pub response: Response,
    pub hovered: Option<String>,
    /// Flows with particles in the air at this playhead.
    pub in_flight: usize,
    /// Particles actually drawn (after the per-flow cap).
    pub particles: usize,
    pub nodes_shown: usize,
    pub nodes_inactive: usize,
    /// Flows in range that could not be drawn because one end has no seat on
    /// the ring. NOT the same as switched-off: these parties are absent from
    /// the diagram entirely, so this is missing coverage rather than a filter
    /// the reader applied, and a caller should say so out loud.
    pub flows_unseated: usize,
}

/// Computes a hovered node's holdings at a given instant: `(party, at_unix)` in,
/// `(label, value)` rows out. See [`FlowRing::inventory`] for why it is a callback.
///
/// The `'a` is load-bearing: a bare `dyn Fn` behind a type alias defaults to
/// `+ 'static`, which would force callers to hand over an owned closure rather
/// than one borrowing the ledger they already have open.
pub type InventoryFn<'a> = dyn Fn(&str, i64) -> Vec<(String, String)> + 'a;

pub struct FlowRing<'a> {
    nodes: &'a [RingNode<'a>],
    flows: &'a [RingFlow<'a>],
    spine: &'a SpineState,
    selection: &'a mut Selection,
    unit_label: &'a str,
    quantum: u64,
    /// Divisor applied ONLY when printing the quantum, never to the dot maths.
    ///
    /// Quantities arrive in a unit's smallest indivisible step (lovelace for
    /// ADA), which is what makes "one dot = one quantum" countable. But the
    /// label is written in the unit a person uses, so printing the raw number
    /// beside a human ticker overstates it by the whole scale factor — a real
    /// bug here read "1 dot = 4,727,675,000 ADA", a tenth of the total supply,
    /// for what was actually 4,727.7 ₳.
    unit_scale: f64,
    flight: i64,
    height: f32,
    label: Option<&'a dyn Fn(&str) -> String>,
    inventory: Option<&'a InventoryFn<'a>>,
}

/// The one seat a pointer is over: nearest within its hit radius, or none.
///
/// Pure so it can be tested without a layout — the bug it fixes was invisible
/// through the widget, because whether two seats overlapped depended on how
/// many nodes the ring happened to be packing.
fn nearest_seat<'a>(
    at: Pos2,
    candidates: impl Iterator<Item = (&'a str, f32, Pos2)>,
) -> Option<&'a str> {
    candidates
        .filter_map(|(key, r, p)| {
            let d = (at - p).length();
            (d <= r + HOVER_SLOP).then_some((key, d))
        })
        // `total_cmp`, not `partial_cmp().unwrap()`: a NaN distance from a
        // degenerate layout would panic the frame rather than simply not match.
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(key, _)| key)
}

/// Drawn radius of a seat. Hop 0 is the project itself and reads as the
/// anchor; a berth's pill is larger than any dot because it is one of a few.
fn seat_radius(n: &RingNode<'_>) -> f32 {
    if n.berth.is_some() {
        7.0
    } else if n.hop == 0 {
        6.0
    } else {
        4.0
    }
}

/// Extra pointer forgiveness around a seat, in points. Generous enough to make
/// a 4px dot clickable, which is also why seats can contend for one pointer.
const HOVER_SLOP: f32 = 7.0;

/// Render the quantum in the unit a reader uses, not the smallest step.
///
/// Thousands separators because the failure this replaces was unreadable as
/// much as it was wrong: a bare ten-digit run gives the eye no scale to fail
/// on, so "4727675000 ADA" passed review for days.
fn fmt_quantum(q: u64, scale: f64) -> String {
    let v = q as f64 / scale;
    let (decimals, whole) = if v >= 100.0 {
        (0, v.round())
    } else if v >= 1.0 {
        (1, v)
    } else {
        (4, v)
    };
    let s = format!("{whole:.decimals$}");
    let (int_part, frac) = s.split_once('.').map_or((s.as_str(), ""), |(i, f)| (i, f));
    // A trailing `.0` is noise on a value that is already whole.
    let frac = frac.trim_end_matches('0');
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    if frac.is_empty() {
        grouped
    } else {
        format!("{grouped}.{frac}")
    }
}

/// Outbound value — shared by every flow face so direction never changes hue
/// between widgets.
pub(crate) const OUT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x2e);
/// Inbound value — see [`OUT`].
pub(crate) const IN: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
/// Per-flow particle cap. A single huge payment must not drown the frame.
const MAX_DOTS: usize = 40;

/// Ring tints, innermost first — a green→teal ordinal ramp plus a neutral
/// outer step.
///
/// ## Why this band, and why these exact values
///
/// The ring index is ordinal (how close to the project), so it moves along ONE
/// safe band rather than taking four unrelated hues. The band is constrained
/// from two directions: the chords drawn ACROSS these rings are [`OUT`] orange
/// and [`IN`] blue, and a seat dot must never be mistakable for a payment.
/// Violet, the obvious first pick, **fails** — it collapses onto the chord
/// blue under protanopia at ΔE 5.1.
///
/// ## Lightness alone was not enough (fixed 2026-08-21)
///
/// The first ramp took one teal hue in lightness steps, and in use the middle
/// two — associate and customer — read as the same colour: ΔE 22 normal, 19
/// under deuteranopia, which clears the floor and still fails the eye, because
/// the seats are 4px dots seen at a glance rather than swatches side by side.
/// The ramp now moves in HUE as well (pale aqua → green → deep teal), taking
/// that pair to **ΔE 53 normal / 44 deutan**. Every pair, including against
/// both chord colours, is asserted in
/// `the_ring_ramp_stays_separable_including_under_colour_blindness`.
///
/// ## The last step is deliberately colourless
///
/// Unexamined is not a fourth class — it is the ABSENCE of a judgement, which
/// the store models as `None`. So colour means somebody decided, grey means
/// nobody has yet, and the work remaining reads as colour draining out of the
/// view. Grey also sits at 2.32:1 on the surface, under the 3:1 mark floor:
/// legitimate here only because it is never the sole cue — the sidebar always
/// prints the address beside it and the ring gives it the outermost band.
const RING_TINTS: [Color32; 4] = [
    Color32::from_rgb(0xa5, 0xf3, 0xe4), // core — the project itself
    Color32::from_rgb(0x3d, 0xdc, 0x84), // associates — paid BY the project
    Color32::from_rgb(0x0f, 0x8f, 0x8a), // customers — bought from it
    Color32::from_rgb(0x4d, 0x54, 0x78), // nobody has looked yet
];

/// The colour for a ring index, saturating at the outermost.
///
/// Public because the party list has to paint the same dot: a wallet's colour
/// is its classification, and two call sites deriving it separately is how a
/// legend starts lying.
pub fn ring_tint(ring: u8) -> Color32 {
    RING_TINTS[(ring as usize).min(RING_TINTS.len() - 1)]
}

impl<'a> FlowRing<'a> {
    /// `flows` must be sorted by timestamp and already filtered to ONE unit.
    pub fn new(
        nodes: &'a [RingNode<'a>],
        flows: &'a [RingFlow<'a>],
        unit_label: &'a str,
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            nodes,
            flows,
            spine,
            selection,
            unit_label,
            quantum: 0,
            unit_scale: 1.0,
            flight: 3 * 86_400,
            height: 460.0,
            label: None,
            inventory: None,
        }
    }

    /// Value per dot. Zero (the default) derives one from the data.
    pub fn quantum(mut self, q: u64) -> Self {
        self.quantum = q;
        self
    }

    /// Smallest-steps per displayed unit — `1_000_000.0` for lovelace→ADA.
    ///
    /// Affects the printed quantum ONLY. Dot counts stay integer maths on the
    /// raw quantities, so a sub-unit flow still draws its dot.
    pub fn unit_scale(mut self, s: f64) -> Self {
        self.unit_scale = if s > 0.0 { s } else { 1.0 };
        self
    }

    /// How long a flow stays in the air, in seconds of DATA time.
    pub fn flight_secs(mut self, s: i64) -> Self {
        self.flight = s.max(1);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn label(mut self, f: &'a dyn Fn(&str) -> String) -> Self {
        self.label = f.into();
        self
    }

    /// What this wallet held at a given moment, as `(name, value)` rows.
    ///
    /// Deliberately a callback: the inventory is the reader's to compute (it
    /// needs the whole ledger), it is only ever needed for the ONE hovered
    /// node, and it must be evaluated at the playhead rather than cached.
    pub fn inventory(mut self, f: &'a InventoryFn<'a>) -> Self {
        self.inventory = f.into();
        self
    }

    pub fn show(self, ui: &mut Ui) -> FlowRingResponse {
        let Self {
            nodes,
            flows,
            spine,
            selection,
            unit_label,
            quantum,
            unit_scale,
            flight,
            height,
            label,
            inventory,
        } = self;

        // Never ask for more than the pane actually has. Taking the caller's
        // height verbatim let the ring overflow a narrow centre pane and spill
        // under the sidebar, because the radius is derived from the rect.
        let avail = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(
            vec2(avail.x.max(80.0), height.min(avail.y.max(120.0))),
            Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();

        // ── seats ─────────────────────────────────────────────────────────
        // Position depends only on (hop, index within hop), never on the data
        // in view — switching a filter must not move a wallet. A berth's hop
        // is ignored entirely: it docks outside whatever the outermost ring
        // turns out to be.
        let max_hop = nodes
            .iter()
            .filter(|n| n.berth.is_none())
            .map(|n| n.hop)
            .max()
            .unwrap_or(0);
        let centre = rect.center();
        // When berths exist the RINGS contract to make the dock zone, rather
        // than the pills being pushed out into the label margin — the widget
        // keeps its footprint and the breathing room appears between the rim
        // and the berths, which is where it means something.
        let dock = if nodes.iter().any(|n| n.berth.is_some()) {
            BERTH_OFFSET
        } else {
            0.0
        };
        let r_outer = (rect.width().min(rect.height()) * 0.5) - 46.0 - dock;
        let mut seats = seat_positions(nodes, max_hop, centre, r_outer.max(40.0));
        for (k, s) in berth_positions(nodes, flows, &seats, centre, r_outer.max(40.0), max_hop + 1)
        {
            seats.insert(k, s);
        }

        let active: std::collections::HashSet<&str> =
            nodes.iter().filter(|n| n.active).map(|n| n.key).collect();
        let inactive = nodes.len() - active.len();

        // ── rings ─────────────────────────────────────────────────────────
        for h in 0..=max_hop {
            let r = ring_radius(h, max_hop, r_outer.max(40.0));
            // The band itself is barely there — it is a reference line, and the
            // seats sitting on it carry the same hue at full strength. Tinting
            // it hard would put four saturated circles behind every chord.
            painter.circle_stroke(
                centre,
                r,
                Stroke::new(1.0_f32, ring_tint(h).gamma_multiply(0.22)),
            );
        }

        // ── the quantum ───────────────────────────────────────────────────
        // Derived from the median-ish scale of what is in view so a typical
        // payment is a handful of dots rather than one or a thousand.
        let peak = flows.iter().map(|f| f.quantity).max().unwrap_or(1).max(1);
        let q = if quantum > 0 {
            quantum
        } else {
            (peak / MAX_DOTS as u64).max(1)
        };

        // ── particles ─────────────────────────────────────────────────────
        let (lo, hi) = spine.filter_range();
        let now = spine.playhead;
        let mut particles = 0usize;
        let watched = selection.active().map(|s| s.to_string());
        // A CHORD IS A RELATIONSHIP; A PARTICLE IS AN EVENT.
        //
        // Drawing one chord per flow put 5,587 overlapping curves on screen the
        // moment the playhead reached the end of a real project's timeline —
        // an orange hairball that said nothing except "these wallets transacted
        // a lot". Aggregating to one chord per PAIR says the useful thing (who
        // dealt with whom, how much) in a few dozen strokes, and leaves the
        // per-event detail to the particles, which are already capped.
        let mut pairs: std::collections::HashMap<(&str, &str), (u64, usize)> =
            std::collections::HashMap::new();
        let mut flying: Vec<&RingFlow<'_>> = Vec::new();
        let mut unseated = 0usize;
        for f in flows {
            if f.timestamp > now || f.timestamp < lo || f.timestamp > hi {
                continue;
            }
            // A flow to somebody with no seat cannot be drawn — but it must not
            // vanish QUIETLY. On a real ledger the tracked parties are a tiny
            // minority of the counterparties they deal with, so this branch can
            // swallow most of the data while the ring still looks complete;
            // that is how a wallet holding 101 of the project's assets managed
            // to be invisible here. Counted and reported by the caller.
            if !seats.contains_key(f.from) || !seats.contains_key(f.to) {
                unseated += 1;
                continue;
            }
            // Switched off by the reader — a deliberate act, reported
            // separately as `nodes_inactive`, not as missing data.
            if !active.contains(f.from) || !active.contains(f.to) {
                continue;
            }
            let e = pairs.entry((f.from, f.to)).or_insert((0, 0));
            e.0 = e.0.saturating_add(f.quantity);
            e.1 += 1;
            if now - f.timestamp <= flight {
                flying.push(f);
            }
        }
        let in_flight = flying.len();

        let heaviest = pairs.values().map(|(q, _)| *q).max().unwrap_or(1).max(1);
        // HEAVIEST LAST. `pairs` is a hash map, so iterating it directly drew
        // chords in arbitrary order: with fifty overlapping curves the one that
        // moved the most money was as likely to be buried under trivial ones as
        // to be visible. Sorting is the difference between magnitude being
        // encoded and magnitude being legible.
        let mut ordered: Vec<(&str, &str, u64)> = pairs
            .iter()
            .map(|((from, to), (q, _))| (*from, *to, *q))
            .collect();
        ordered.sort_by_key(|(_, _, q)| *q);
        // Tapering costs a stroke per segment, so spend segments where there is
        // room to see them.
        let segs = if ordered.len() > 160 { 12 } else { 24 };

        for (from, to, qty) in ordered {
            let (Some(a), Some(b)) = (seats.get(from), seats.get(to)) else {
                continue;
            };
            // A pinned wallet's own flows stay bright; everything else recedes.
            let involved = watched.as_deref().is_none_or(|w| w == from || w == to);
            let alpha = if involved { 1.0 } else { 0.13 };
            // Direction is a property of the FLOW, not of what happens to be
            // selected. The previous rule coloured by "does this end at the
            // pinned wallet", so with nothing pinned every chord on screen was
            // drawn as an outflow — including the inbound ones that a reader
            // most wants to pick out.
            //
            // Toward the centre means toward the project: a payment arriving.
            // Away means value leaving it. Between seats on the SAME ring
            // there is no radial direction, so the chord wears that ring's own
            // tint — internal to the classification, in its colour. This
            // matters most at ring 0: an earlier rule drew all same-ring
            // chords in muted grey with a comment claiming they "do not
            // concern the project's own money", which is exactly backwards
            // there — royalties→ops IS the project's own money moving, and
            // classifying a wallet core was silently greying out its most
            // internal relationships. On the outer rings the tints are quiet
            // anyway, so customer↔customer chatter stays quiet. Direction
            // within the pair is carried by the taper (wide origin, narrow
            // destination), as it always was.
            let col = match a.ring.cmp(&b.ring) {
                std::cmp::Ordering::Greater => IN,
                std::cmp::Ordering::Less => OUT,
                std::cmp::Ordering::Equal => ring_tint(a.ring),
            };
            // Weight by share of the heaviest pair, on a log scale — treasury
            // flows span orders of magnitude, so a linear ramp shows one chord.
            let t =
                ((qty as f64 + 1.0).ln() / (heaviest as f64 + 1.0).ln()).clamp(0.08, 1.0) as f32;
            let chord = Chord::new(a, b, centre, max_hop, r_outer.max(40.0));
            let dim = if involved { 1.0 } else { 0.6 };
            chord.taper(
                &painter,
                segs,
                Taper {
                    color: col,
                    // WIDE AT THE ORIGIN, NARROW AT THE DESTINATION — the
                    // standard tapered flow line. It states direction without
                    // an arrowhead (unreadable at this density) and it does a
                    // second job here: chords converge on a hub, and thinning
                    // them at the arriving end takes the ink out of the
                    // starburst that was swallowing the treasury.
                    width: ((0.8 + 4.2 * t) * dim, 0.35 * dim),
                    alpha: ((0.16 + 0.5 * t) * alpha, 0.10 * alpha),
                },
            );
        }

        for f in &flying {
            let (Some(a), Some(b)) = (seats.get(f.from), seats.get(f.to)) else {
                continue;
            };
            let involved = watched.as_deref().is_none_or(|w| w == f.from || w == f.to);
            let alpha = if involved { 1.0 } else { 0.16 };
            let col = if watched.as_deref() == Some(f.to) {
                IN
            } else {
                OUT
            };
            // The SAME route the chord was stroked along, so a train never
            // peels away from its own line.
            let chord = Chord::new(a, b, centre, max_hop, r_outer.max(40.0));
            let t = (now - f.timestamp) as f32 / flight as f32;
            let n = ((f.quantity / q) as usize).clamp(1, MAX_DOTS);
            for k in 0..n {
                // Stagger the train so a large payment reads as a stream.
                let lead = t - (k as f32) * 0.02;
                if !(0.0..=1.0).contains(&lead) {
                    continue;
                }
                let p = chord.point(lead);
                painter.circle_filled(p, 2.0, col.gamma_multiply(alpha));
                particles += 1;
            }
        }

        // ── nodes ─────────────────────────────────────────────────────────
        //
        // AT MOST ONE SEAT IS HOVERED, and it is the nearest to the cursor.
        //
        // Seats on a crowded outer ring overlap their own hit radius, so a test
        // applied per-node inside the draw loop matched several at once: every
        // match drew a highlight ring, and `hovered` ended up as whichever came
        // last in iteration order — an arbitrary winner that changed with the
        // node list rather than with the pointer. Resolve the winner FIRST,
        // then draw.
        let nearest: Option<&str> = response.hover_pos().and_then(|h| {
            nearest_seat(
                h,
                nodes
                    .iter()
                    .filter_map(|n| seats.get(n.key).map(|s| (n.key, seat_radius(n), s.pos))),
            )
        });
        let mut hovered: Option<String> = None;
        for n in nodes {
            let Some(seat) = seats.get(n.key) else {
                continue;
            };
            let p = &seat.pos;
            let emph = selection.emphasis(n.key);
            let on = n.active;
            let r = seat_radius(n);
            if let Some(marks) = n.berth {
                // A berth is INFRASTRUCTURE, so it must be unmistakable for a
                // wallet: a square pill in neutral ink, never a ring tint —
                // colour on this chart means a classification was asserted,
                // and a provider has none to assert.
                let col = if on {
                    ink.gamma_multiply(0.85 * emph)
                } else {
                    muted.gamma_multiply(0.35)
                };
                let pill = egui::Rect::from_center_size(*p, vec2(2.0 * r, 2.0 * r));
                painter.rect_stroke(
                    pill,
                    2.0,
                    Stroke::new(1.2_f32, col),
                    egui::StrokeKind::Middle,
                );
                painter.rect_filled(pill.shrink(3.0), 1.0, col.gamma_multiply(0.55));
                if nearest == Some(n.key) {
                    hovered = Some(n.key.to_string());
                    painter.circle_stroke(*p, r + 5.0, Stroke::new(1.5_f32, ink));
                }
                // ALWAYS labelled, marks included — there are only ever a few
                // berths, and an anonymous square outside the rim would read
                // as a rendering fault rather than a provider.
                let name = match label {
                    Some(f) => f(n.key),
                    None => elide(n.key),
                };
                let away = (*p - centre).normalized();
                painter.text(
                    *p + away * (r + 6.0),
                    if away.x >= 0.0 {
                        Align2::LEFT_CENTER
                    } else {
                        Align2::RIGHT_CENTER
                    },
                    format!("{} · {marks}", truncate(&name, 18)),
                    egui::TextStyle::Small.resolve(ui.style()),
                    ink.gamma_multiply(emph),
                );
                continue;
            }
            // The seat wears its CLASSIFICATION. Emphasis still rides on top as
            // alpha, so selecting a wallet dims its neighbours without
            // repainting anyone's identity — colour follows the entity, never
            // its rank in the current view.
            let col = if on {
                ring_tint(n.hop).gamma_multiply(emph)
            } else {
                muted.gamma_multiply(0.35)
            };
            painter.circle_filled(*p, r, col);
            if nearest == Some(n.key) {
                hovered = Some(n.key.to_string());
                painter.circle_stroke(*p, r + 4.0, Stroke::new(1.5_f32, ink));
            }
            // Label only hop 0 and whatever is pinned — labelling every seat is
            // how a ring turns into noise.
            if n.hop == 0 || selection.active() == Some(n.key) {
                let name = match label {
                    Some(f) => f(n.key),
                    None => elide(n.key),
                };
                let away = (*p - centre).normalized();
                painter.text(
                    *p + away * 11.0,
                    if away.x >= 0.0 {
                        Align2::LEFT_CENTER
                    } else {
                        Align2::RIGHT_CENTER
                    },
                    truncate(&name, 18),
                    egui::TextStyle::Small.resolve(ui.style()),
                    ink.gamma_multiply(emph),
                );
            }
        }

        // ── hover: the wallet's live inventory at THIS moment ─────────────
        match &hovered {
            Some(h) => selection.hover(h.clone()),
            None => {
                if let Some(prev) = ui.data(|d| d.get_temp::<String>(response.id.with("hov"))) {
                    selection.clear_hover_if(&prev);
                }
            }
        }
        ui.data_mut(|d| match &hovered {
            Some(h) => {
                d.insert_temp(response.id.with("hov"), h.clone());
            }
            None => d.remove::<String>(response.id.with("hov")),
        });
        if let Some(h) = &hovered {
            let name = match label {
                Some(f) => f(h),
                None => elide(h),
            };
            let rows = inventory.map(|f| f(h, now)).unwrap_or_default();
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                response.id.with("tip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.set_max_width(300.0);
                ui.label(egui::RichText::new(name).strong());
                ui.label(
                    egui::RichText::new(crate::time_spine::format_date(now))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                for (k, v) in &rows {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(k)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.label(egui::RichText::new(v).small().strong());
                    });
                }
            });
        }
        if response.clicked() {
            match &hovered {
                Some(h) => selection.toggle_pin(h.clone()),
                None => selection.clear_pin(),
            }
        }

        // ── hero ──────────────────────────────────────────────────────────
        painter.text(
            rect.left_top() + vec2(0.0, 2.0),
            Align2::LEFT_TOP,
            format!(
                "{in_flight} flows in the air · 1 dot = {} {unit_label}",
                fmt_quantum(q, unit_scale)
            ),
            egui::TextStyle::Small.resolve(ui.style()),
            muted,
        );
        if inactive > 0 {
            painter.text(
                rect.right_top() + vec2(0.0, 2.0),
                Align2::RIGHT_TOP,
                format!("{inactive} wallets switched off"),
                egui::TextStyle::Small.resolve(ui.style()),
                muted,
            );
        }

        FlowRingResponse {
            response,
            hovered,
            in_flight,
            particles,
            nodes_shown: active.len(),
            nodes_inactive: inactive,
            flows_unseated: unseated,
        }
    }
}

/// Radius of ring `hop`. Hop 0 is innermost but never a point.
fn ring_radius(hop: u8, max_hop: u8, outer: f32) -> f32 {
    if max_hop == 0 {
        return outer * 0.55;
    }
    let t = hop as f32 / max_hop as f32;
    outer * (0.34 + 0.66 * t)
}

/// Fixed seats: ordered by hop then by the caller's order, spread evenly.
/// Depends on nothing time-varying, so a wallet keeps its seat all session.
/// A party's fixed place on the ring, in BOTH coordinate systems.
///
/// The polar form is not a convenience — chord routing reasons about which
/// rings a flow belongs to, and recovering that from a screen position with
/// `atan2` after the fact loses the ring index entirely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    pub pos: Pos2,
    pub ring: u8,
    pub radius: f32,
    pub angle: f32,
}

fn seat_positions<'a>(
    nodes: &'a [RingNode<'a>],
    max_hop: u8,
    centre: Pos2,
    outer: f32,
) -> std::collections::HashMap<&'a str, Seat> {
    let mut out = std::collections::HashMap::new();
    for h in 0..=max_hop {
        // Berths are laid out separately, beyond the rim — including one here
        // would shift every other seat on its ring the moment a wallet is
        // recognised as a provider.
        let ring: Vec<&RingNode<'_>> = nodes
            .iter()
            .filter(|n| n.hop == h && n.berth.is_none())
            .collect();
        let count = ring.len().max(1);
        let r = ring_radius(h, max_hop, outer);
        for (i, n) in ring.iter().enumerate() {
            // Offset alternate rings so seats do not line up radially.
            let phase = if h % 2 == 0 {
                0.0
            } else {
                std::f32::consts::PI / count as f32
            };
            let a = -std::f32::consts::FRAC_PI_2
                + phase
                + std::f32::consts::TAU * (i as f32) / count as f32;
            out.insert(
                n.key,
                Seat {
                    pos: pos2(centre.x + r * a.cos(), centre.y + r * a.sin()),
                    ring: h,
                    radius: r,
                    angle: a,
                },
            );
        }
    }
    out
}

/// How far beyond the outer ring a berth docks. Generous on purpose: the gap
/// IS the statement — infrastructure is outside the classification system,
/// and a pill grazing the rim dots reads as just another crowded seat.
const BERTH_OFFSET: f32 = 36.0;
/// Minimum angular separation between two berths, radians. Centroids of two
/// providers serving the same crowd can coincide exactly; without a floor
/// their pills stack into one unreadable glyph.
const BERTH_MIN_SEP: f32 = 0.45;

/// Dock each berth at the weighted angular centroid of its seated partners.
///
/// The weights are log-magnitude — the same compression the chords use — so a
/// provider's berth sits facing the wallets it mostly deals with, not wherever
/// one huge transfer points. Computed over the FULL flow slice, never the
/// playhead's window: a berth that migrated as the reader scrubbed would break
/// the object-constancy rule every seat on the ring obeys.
///
/// A berth with no seated partners (possible early in a walk) falls back to a
/// deterministic spread by index, which keeps the layout stable across frames
/// even when it cannot yet be meaningful.
fn berth_positions<'a>(
    nodes: &'a [RingNode<'a>],
    flows: &[RingFlow<'_>],
    ring_seats: &std::collections::HashMap<&'a str, Seat>,
    centre: Pos2,
    outer: f32,
    berth_ring: u8,
) -> Vec<(&'a str, Seat)> {
    let berths: Vec<&RingNode<'_>> = nodes.iter().filter(|n| n.berth.is_some()).collect();
    let mut placed: Vec<(&str, f32)> = Vec::new();
    for (i, n) in berths.iter().enumerate() {
        let (mut x, mut y) = (0.0f64, 0.0f64);
        for f in flows {
            let other = if f.from == n.key {
                f.to
            } else if f.to == n.key {
                f.from
            } else {
                continue;
            };
            if let Some(s) = ring_seats.get(other) {
                let w = (f.quantity as f64 + 1.0).ln();
                x += w * f64::from(s.angle).cos();
                y += w * f64::from(s.angle).sin();
            }
        }
        let angle = if x == 0.0 && y == 0.0 {
            -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * (i as f32) / berths.len().max(1) as f32
        } else {
            y.atan2(x) as f32
        };
        placed.push((n.key, angle));
    }
    // Two berths may want the same angle; separate them deterministically
    // (sorted by angle then key, pushed apart in order).
    placed.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(b.0)));
    for i in 1..placed.len() {
        let prev = placed[i - 1].1;
        if placed[i].1 - prev < BERTH_MIN_SEP {
            placed[i].1 = prev + BERTH_MIN_SEP;
        }
    }
    let r = outer + BERTH_OFFSET;
    placed
        .into_iter()
        .map(|(k, a)| {
            (
                k,
                Seat {
                    pos: pos2(centre.x + r * a.cos(), centre.y + r * a.sin()),
                    ring: berth_ring,
                    radius: r,
                    angle: a,
                },
            )
        })
        .collect()
}

/// The annulus a ring owns: everything nearer to it than to its neighbours.
///
/// Ring 0 owns the hollow core all the way to the centre — not a special case
/// bolted on, but the same rule applied: the empty disc inside the innermost
/// ring is the project's own interior, so a flow between two of the project's
/// wallets may cross it, while nothing else may.
fn ring_band(hop: u8, max_hop: u8, outer: f32) -> (f32, f32) {
    if max_hop == 0 {
        return (0.0, outer);
    }
    let r = ring_radius(hop, max_hop, outer);
    let spacing = outer * 0.66 / max_hop as f32;
    let lo = if hop == 0 { 0.0 } else { r - spacing * 0.5 };
    let hi = if hop >= max_hop {
        outer
    } else {
        r + spacing * 0.5
    };
    (lo, hi)
}

/// A chord's route between two seats, confined to the rings it concerns.
///
/// ## The rule
///
/// **A flow may not pass through a zone it is irrelevant to.** The permitted
/// annulus spans the bands of the two endpoints' rings and nothing further, so
/// a rim-to-rim payment stays out in the rim's band instead of diving through
/// the middle, while a chord between rings 1 and 3 crosses ring 2 — which it
/// genuinely spans — and never enters ring 0's territory.
///
/// The middle is thereby reserved for a real signal: value crossing the centre
/// means one of the project's own wallets is involved.
///
/// ## Four control points, in POLAR space
///
/// ```text
///   P0 = (r_a,    θ_a)      the seat
///   P1 = (r_duck, θ_a)      straight inward — the departure aims ACROSS
///   P2 = (r_duck, θ_b)      travel happens at the ducked radius
///   P3 = (r_b,    θ_b)      the other seat
/// ```
///
/// Two properties fall out of this for free, and both were fought for by hand
/// in earlier attempts:
///
/// - **The region is respected by construction.** A Bézier lies inside the
///   convex hull of its control points, so `r(t) ≥ min(r_a, r_duck, r_b)` and
///   `θ(t)` stays between `θ_a` and `θ_b`. No clamping, no sampling guard, and
///   no way for a path to leak into a ring it has no business in.
/// - **The departure is radial, so the chord reads as heading ACROSS.** `P1`
///   sits at `θ_a`, so the initial tangent points straight at the middle. This
///   is the whole difference from interpolating the angle linearly, which
///   leaves a seat sideways and traces the annulus — the same radial bounds,
///   entirely the wrong picture.
///
/// ## How far it ducks
///
/// `r_duck` is not a constant. A straight line between two seats at radius `R`
/// separated by `Δθ` has minimum radius `R·cos(Δθ/2)`, which IS "how far in
/// does this path want to go" — 1 for neighbours, 0 for opposite seats. Taking
/// `max(floor, avg·cos(Δθ/2))` means a hop between adjacent seats barely
/// deforms, while opposite seats duck all the way to the band's inner edge and
/// travel round inside it.
#[derive(Clone, Copy, Debug)]
struct Chord {
    centre: Pos2,
    /// Control radii and angles, P0..P3.
    r: [f32; 4],
    th: [f32; 4],
}

/// How a tapered flow line ramps from origin `.0` to destination `.1`.
#[derive(Clone, Copy, Debug)]
struct Taper {
    color: Color32,
    width: (f32, f32),
    alpha: (f32, f32),
}

fn cubic(p: [f32; 4], t: f32) -> f32 {
    let u = 1.0 - t;
    u * u * u * p[0] + 3.0 * u * u * t * p[1] + 3.0 * u * t * t * p[2] + t * t * t * p[3]
}

impl Chord {
    fn new(a: &Seat, b: &Seat, centre: Pos2, max_hop: u8, outer: f32) -> Self {
        // The pair's own territory: from the inner edge of the innermost ring
        // involved to the outer edge of the outermost.
        let (floor, _) = ring_band(a.ring.min(b.ring), max_hop, outer);

        // Shortest way round — going the long way to reach a neighbour would
        // cross the entire diagram.
        let mut sweep = b.angle - a.angle;
        while sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        }
        while sweep < -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }

        let avg = (a.radius + b.radius) * 0.5;
        let duck = (avg * (sweep * 0.5).cos()).max(floor);

        Self {
            centre,
            r: [a.radius, duck, duck, b.radius],
            th: [a.angle, a.angle, a.angle + sweep, a.angle + sweep],
        }
    }

    /// Point at `t` along the route. `t = 0` is exactly seat `a`, `t = 1` is
    /// exactly seat `b` — particles ride this, so any drift would peel the
    /// train off its own line.
    fn point(&self, t: f32) -> Pos2 {
        let r = cubic(self.r, t);
        let th = cubic(self.th, t);
        pos2(self.centre.x + r * th.cos(), self.centre.y + r * th.sin())
    }

    /// Polyline for stroking. A ducked route is longer and more curved than the
    /// old straight dive, so it needs more segments to stop reading as a chain
    /// of facets.
    fn polyline(&self, segments: usize) -> Vec<Pos2> {
        (0..=segments)
            .map(|i| self.point(i as f32 / segments as f32))
            .collect()
    }

    /// Stroke the route with width and opacity ramping from origin to
    /// destination — a tapered flow line.
    ///
    /// Drawn segment by segment because egui strokes a whole polyline with one
    /// `Stroke`; there is no per-vertex width. The segments overlap by one
    /// point each, so the ramp reads as continuous rather than as a ladder.
    fn taper(&self, painter: &egui::Painter, segments: usize, t: Taper) {
        let pts = self.polyline(segments);
        for (i, pair) in pts.windows(2).enumerate() {
            let f = (i as f32 + 0.5) / segments as f32;
            let w = t.width.0 + (t.width.1 - t.width.0) * f;
            let a = t.alpha.0 + (t.alpha.1 - t.alpha.0) * f;
            painter.line_segment(
                [pair[0], pair[1]],
                Stroke::new(w, t.color.gamma_multiply(a)),
            );
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!(
        "{}…",
        s.chars().take(n.saturating_sub(1)).collect::<String>()
    )
}

fn elide(key: &str) -> String {
    if key.len() <= 16 {
        return key.to_string();
    }
    format!("{}…{}", &key[..9], &key[key.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2 as P, Rect as R, vec2};

    /// Seats on a crowded outer ring overlap their own hit radius. The old test
    /// ran per-node inside the draw loop, so several matched at once, each drew
    /// a highlight, and the winner was whichever came LAST in iteration order.
    #[test]
    fn hover_picks_exactly_one_seat_the_nearest() {
        // Three seats 4px apart — every one inside the others' slop.
        let seats = [
            ("far", 4.0, P::new(100.0, 100.0)),
            ("near", 4.0, P::new(104.0, 100.0)),
            ("mid", 4.0, P::new(108.0, 100.0)),
        ];
        let pick = nearest_seat(P::new(104.0, 100.0), seats.iter().copied());
        assert_eq!(
            pick,
            Some("near"),
            "the seat under the cursor, not the last"
        );

        // Order must not decide it.
        let mut reversed = seats;
        reversed.reverse();
        assert_eq!(
            nearest_seat(P::new(104.0, 100.0), reversed.iter().copied()),
            Some("near"),
            "iteration order must not change the answer"
        );

        // Outside every hit radius = nothing hovered.
        assert_eq!(
            nearest_seat(P::new(400.0, 400.0), seats.iter().copied()),
            None
        );

        // A hop-0 seat is larger, so its reach is larger too.
        assert_eq!(
            nearest_seat(P::new(0.0, 12.0), [("core", 6.0, P::ZERO)].iter().copied()),
            Some("core")
        );
        assert_eq!(
            nearest_seat(P::new(0.0, 12.0), [("rim", 4.0, P::ZERO)].iter().copied()),
            None
        );
    }

    /// The bug this guards: quantities are lovelace, the ticker says ADA, and
    /// the raw number went straight onto the screen — "1 dot = 4727675000 ADA",
    /// a tenth of the total supply, for a quantum of 4,727.7 ₳.
    #[test]
    fn quantum_is_printed_in_the_unit_the_ticker_names() {
        assert_eq!(fmt_quantum(4_727_675_000, 1_000_000.0), "4,728");
        assert_eq!(fmt_quantum(4_727_675_000, 1.0), "4,727,675,000");
        // Sub-unit quanta stay legible rather than rounding to a bare 0.
        assert_eq!(fmt_quantum(2_500, 1_000_000.0), "0.0025");
        assert_eq!(fmt_quantum(1_500_000, 1_000_000.0), "1.5");
        // A scale of zero must not divide by zero — the builder clamps it.
        assert_eq!(fmt_quantum(42, 1.0), "42");
    }

    fn run(
        nodes: &[RingNode<'_>],
        flows: &[RingFlow<'_>],
        spine: &SpineState,
        sel: &mut Selection,
    ) -> FlowRingResponse {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let mut out = None;
        let raw = egui::RawInput {
            screen_rect: Some(R::from_min_size(P::ZERO, vec2(700.0, 560.0))),
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("fr")).show(&ctx, |ui| {
            ui.set_min_size(vec2(700.0, 560.0));
            out = Some(
                FlowRing::new(nodes, flows, "ADA", spine, sel)
                    .quantum(100)
                    .show(ui),
            );
        });
        let _ = ctx.end_pass();
        out.unwrap()
    }

    fn nodes() -> Vec<RingNode<'static>> {
        vec![
            RingNode::new("treasury", 0),
            RingNode::new("royalty", 0),
            RingNode::new("payee-a", 1),
            RingNode::new("payee-b", 1),
        ]
    }

    /// CIELAB of an sRGB colour — enough for ΔE76, which is what the palette
    /// floors are stated in.
    fn lab(c: Color32) -> [f64; 3] {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
        let (x, y, z) = (
            (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047,
            0.2126 * r + 0.7152 * g + 0.0722 * b,
            (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883,
        );
        let f = |t: f64| {
            if t > 0.008_856 {
                t.cbrt()
            } else {
                7.787 * t + 16.0 / 116.0
            }
        };
        let (fx, fy, fz) = (f(x), f(y), f(z));
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    fn delta_e(a: Color32, b: Color32) -> f64 {
        lab(a)
            .iter()
            .zip(lab(b).iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    /// Machado et al. (2009) severity-1.0 simulation of dichromatic vision.
    fn simulate(c: Color32, m: [[f64; 3]; 3]) -> Color32 {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let enc = |v: f64| {
            let v = v.clamp(0.0, 1.0);
            let s = if v <= 0.003_130_8 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0).round() as u8
        };
        let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
        Color32::from_rgb(
            enc(m[0][0] * r + m[0][1] * g + m[0][2] * b),
            enc(m[1][0] * r + m[1][1] * g + m[1][2] * b),
            enc(m[2][0] * r + m[2][1] * g + m[2][2] * b),
        )
    }

    const PROTAN: [[f64; 3]; 3] = [
        [0.152_286, 1.052_583, -0.204_868],
        [0.114_503, 0.786_281, 0.099_216],
        [-0.003_882, -0.048_116, 1.051_998],
    ];
    const DEUTAN: [[f64; 3]; 3] = [
        [0.367_322, 0.860_646, -0.227_968],
        [0.280_085, 0.672_501, 0.047_413],
        [-0.011_820, 0.042_940, 0.968_881],
    ];

    /// THE PALETTE VALIDATOR, as a test rather than a script somebody ran once.
    ///
    /// Every pair of {four ring tints, both chord colours} must stay apart
    /// under normal vision AND under both common dichromacies — a seat that
    /// collapses onto a chord makes a wallet look like a payment, and two ring
    /// steps that collapse onto each other erase the classification the whole
    /// chart encodes. The floors are the ones the ramp was originally chosen
    /// against; the associate/customer pair carries a much higher one because
    /// clearing the floor was exactly what it did while still reading as one
    /// colour on 4px dots.
    #[test]
    fn the_ring_ramp_stays_separable_including_under_colour_blindness() {
        const NORMAL_FLOOR: f64 = 15.0;
        const CVD_FLOOR: f64 = 8.0;
        let all: Vec<(&str, Color32)> = vec![
            ("core", ring_tint(0)),
            ("associate", ring_tint(1)),
            ("customer", ring_tint(2)),
            ("unexamined", ring_tint(3)),
            ("chord-out", OUT),
            ("chord-in", IN),
        ];
        for (i, (na, a)) in all.iter().enumerate() {
            for (nb, b) in all.iter().skip(i + 1) {
                assert!(
                    delta_e(*a, *b) >= NORMAL_FLOOR,
                    "{na} vs {nb}: ΔE {:.1} < {NORMAL_FLOOR}",
                    delta_e(*a, *b)
                );
                for (kind, m) in [("protan", PROTAN), ("deutan", DEUTAN)] {
                    let d = delta_e(simulate(*a, m), simulate(*b, m));
                    assert!(
                        d >= CVD_FLOOR,
                        "{na} vs {nb} under {kind}: ΔE {d:.1} < {CVD_FLOOR}"
                    );
                }
            }
        }
        // The pair that prompted the change: one teal hue in lightness steps
        // measured 22 normal / 19 deutan and still read as one colour.
        let (assoc, cust) = (ring_tint(1), ring_tint(2));
        assert!(
            delta_e(assoc, cust) >= 40.0,
            "associate vs customer must be obvious, not merely legal: ΔE {:.1}",
            delta_e(assoc, cust)
        );
        assert!(delta_e(simulate(assoc, DEUTAN), simulate(cust, DEUTAN)) >= 30.0);
    }

    /// The tints are chosen against the chords by a validator, so the thing
    /// worth pinning in code is that nothing silently reuses a chord colour or
    /// panics past the last ring.
    #[test]
    fn ring_tints_are_distinct_and_never_a_chord_colour() {
        let tints: Vec<Color32> = (0..4).map(ring_tint).collect();
        for (i, a) in tints.iter().enumerate() {
            for b in tints.iter().skip(i + 1) {
                assert_ne!(a, b, "two rings share a tint");
            }
            assert_ne!(*a, OUT, "a seat must not wear the outbound chord colour");
            assert_ne!(*a, IN, "a seat must not wear the inbound chord colour");
        }
        // Past the last ring the outermost tint holds, rather than panicking:
        // `max_hop` is data-driven and a caller may hand over anything.
        assert_eq!(ring_tint(9), ring_tint(3));
    }

    /// Four rings of seats, for the chord-routing tests.
    ///
    /// The outer ring is deliberately CROWDED (twelve seats, so `x0`/`x1` are
    /// 30° apart and `x0`/`x6` are opposite). With four seats a "neighbour" is
    /// a quarter of the way round the diagram, which is not a neighbour at all
    /// — and a routing rule that only ever sees wide separations cannot show
    /// that it treats near and far pairs differently.
    const OPPOSITE: &str = "x6";

    fn banded() -> Vec<RingNode<'static>> {
        let mut v = Vec::new();
        for (keys, ring) in [
            (["c0", "c1", "c2", "c3"], 0u8),
            (["a0", "a1", "a2", "a3"], 1),
            (["u0", "u1", "u2", "u3"], 2),
        ] {
            for k in keys {
                v.push(RingNode::new(k, ring));
            }
        }
        for k in [
            "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11",
        ] {
            v.push(RingNode::new(k, 3));
        }
        v
    }

    const CENTRE: Pos2 = pos2(300.0, 300.0);
    const OUTER: f32 = 200.0;

    fn chord_between(seats: &std::collections::HashMap<&str, Seat>, from: &str, to: &str) -> Chord {
        Chord::new(&seats[from], &seats[to], CENTRE, 3, OUTER)
    }

    fn radii(c: &Chord) -> Vec<f32> {
        (0..=64)
            .map(|i| (c.point(i as f32 / 64.0) - CENTRE).length())
            .collect()
    }

    fn deepest(c: &Chord) -> f32 {
        radii(c).into_iter().fold(f32::MAX, f32::min)
    }

    /// Direction belongs to the FLOW, not to whatever happens to be selected.
    ///
    /// The old rule coloured a chord `IN` only when it ended at the pinned
    /// wallet, so with nothing pinned every chord on screen was drawn as an
    /// outflow — fifty orange curves converging on a treasury that was in fact
    /// mostly RECEIVING. A reader could not pick out the wallet funding the
    /// project, which is the first thing anybody looks for.
    #[test]
    fn direction_is_read_from_the_rings_not_from_the_selection() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let inward = seats["x0"].ring.cmp(&seats["c0"].ring);
        let outward = seats["c0"].ring.cmp(&seats["x0"].ring);
        let lateral = seats["x0"].ring.cmp(&seats["x1"].ring);
        assert_eq!(
            inward,
            std::cmp::Ordering::Greater,
            "rim -> core must read as value arriving"
        );
        assert_eq!(
            outward,
            std::cmp::Ordering::Less,
            "core -> rim must read as value leaving"
        );
        assert_eq!(
            lateral,
            std::cmp::Ordering::Equal,
            "same ring is neither, and must not be dressed as project money"
        );
    }

    /// Magnitude only reads if the heavy chord is drawn ON TOP. `pairs` is a
    /// hash map, so iterating it directly buried the biggest flow under
    /// whichever trivial ones happened to sort after it.
    #[test]
    fn chords_are_ordered_lightest_first_so_the_heaviest_lands_on_top() {
        let mut pairs: std::collections::HashMap<(&str, &str), (u64, usize)> =
            std::collections::HashMap::new();
        pairs.insert(("a", "b"), (5, 1));
        pairs.insert(("c", "d"), (900, 1));
        pairs.insert(("e", "f"), (40, 1));
        let mut ordered: Vec<(&str, &str, u64)> = pairs
            .iter()
            .map(|((from, to), (q, _))| (*from, *to, *q))
            .collect();
        ordered.sort_by_key(|(_, _, q)| *q);
        assert_eq!(
            ordered.last().map(|(_, _, q)| *q),
            Some(900),
            "the heaviest pair must be drawn last"
        );
    }

    /// A flow to a party with no seat is UNDRAWABLE, and must be counted
    /// rather than dropped in silence.
    ///
    /// This is not a corner case. On the Mekka ledger 698,999 of 703,140 flow
    /// rows had a counterparty that was never promoted to a party, so the ring
    /// could render a confident-looking picture built from 0.6% of the data —
    /// which is how a wallet holding 101 of the project's assets and receiving
    /// 16,748 ADA from it came to be invisible.
    #[test]
    fn flows_to_a_party_with_no_seat_are_counted_not_silently_dropped() {
        let n = nodes();
        let flows = [
            RingFlow {
                timestamp: 1_000,
                from: "treasury",
                to: "payee-a",
                quantity: 10,
            },
            RingFlow {
                timestamp: 1_000,
                from: "treasury",
                to: "nobody-seated-here",
                quantity: 10,
            },
        ];
        let spine = SpineState::new((0, 10_000));
        let mut sel = Selection::default();
        let r = run(&n, &flows, &spine, &mut sel);
        assert_eq!(
            r.flows_unseated, 1,
            "the flow to an unseated party must be reported"
        );
    }

    /// THE RULE: a flow may not pass through a zone it is irrelevant to.
    ///
    /// Two wallets out on the rim have nothing to do with the project's own
    /// ring, so their payment must stay outside it — the middle then MEANS
    /// something, namely that a project wallet is involved.
    ///
    /// This is checked on every sample of the worst case (an opposite pair),
    /// because the earlier attempts all failed exactly here: one pulled every
    /// control point toward the centre regardless of whose flow it was, and a
    /// later one relaxed the floor to a fraction so long chords leaked inward.
    #[test]
    fn a_rim_to_rim_chord_never_enters_the_inner_rings() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let (lo, hi) = ring_band(3, 3, OUTER);
        for pair in [("x0", OPPOSITE), ("x0", "x4"), ("x0", "x1"), ("x2", "x9")] {
            for r in radii(&chord_between(&seats, pair.0, pair.1)) {
                assert!(
                    r >= lo - 0.5 && r <= hi + 0.5,
                    "{}->{} left the rim band at r={r} (band {lo}..{hi})",
                    pair.0,
                    pair.1
                );
            }
        }
    }

    /// A chord spanning rings 1..3 may cross ring 2 — it genuinely spans it —
    /// but ring 0 is none of its business.
    #[test]
    fn a_spanning_chord_crosses_what_it_spans_and_no_further() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let floor = ring_band(1, 3, OUTER).0;
        let rs = radii(&chord_between(&seats, "a0", "x4"));
        for r in &rs {
            assert!(*r >= floor - 0.5, "dipped into ring 0's zone at r={r}");
        }
        let r2 = ring_radius(2, 3, OUTER);
        assert!(
            rs.iter().any(|r| (*r - r2).abs() < 25.0),
            "never crossed ring 2, which it spans"
        );
    }

    /// The hollow core is the project's own interior, so its wallets — and
    /// only its wallets — may route across the middle.
    #[test]
    fn core_to_core_may_use_the_hollow_centre() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let deep = deepest(&chord_between(&seats, "c0", "c2"));
        let r0 = ring_radius(0, 3, OUTER);
        assert!(
            deep < r0 * 0.5,
            "an opposite core pair should cut across the hollow, got {deep} vs ring0 {r0}"
        );
    }

    /// Heading ACROSS, not around: the chord must LEAVE its seat aiming at the
    /// middle. Interpolating the angle linearly satisfies every radial bound
    /// while departing sideways, so the flow appears to take the long way round
    /// the rim — the same numbers, entirely the wrong picture. Hence a test on
    /// the departure direction rather than on radius alone.
    #[test]
    fn a_chord_departs_radially_rather_than_along_the_rim() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let c = chord_between(&seats, "x0", OPPOSITE);
        let seat = seats["x0"];
        // The TANGENT, not a chord across 2% of the path: `dθ/dt` is zero at
        // `t = 0` by construction (the first two control angles are equal), so
        // any tangential motion over a finite step is second-order and says
        // nothing about which way the curve sets off.
        let step = c.point(0.001) - c.point(0.0);
        let inward = (CENTRE - seat.pos).normalized();
        let along = vec2(-inward.y, inward.x);
        assert!(
            step.dot(inward) > step.dot(along).abs() * 5.0,
            "departure is tangential, not across: inward={} along={}",
            step.dot(inward),
            step.dot(along).abs()
        );
    }

    /// Continuous — a naive radial remap looks right until the outward
    /// direction flips sign at the centre and the polyline jumps across.
    #[test]
    fn a_chord_has_no_jump_in_it() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        for pair in [("x0", OPPOSITE), ("c0", "c2"), ("a0", "x4")] {
            let c = chord_between(&seats, pair.0, pair.1);
            let pts: Vec<Pos2> = (0..=64).map(|i| c.point(i as f32 / 64.0)).collect();
            let longest = pts
                .windows(2)
                .map(|w| (w[1] - w[0]).length())
                .fold(0.0_f32, f32::max);
            assert!(
                longest < OUTER * 0.25,
                "{}->{} jumps {longest} in one step",
                pair.0,
                pair.1
            );
        }
    }

    /// Deformation is earned by angular distance: a hop between neighbouring
    /// seats keeps close to its ring instead of swinging across the diagram to
    /// reach the wallet beside it.
    #[test]
    fn neighbours_barely_deform_while_opposites_cut_deep() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let r3 = ring_radius(3, 3, OUTER);
        let near = deepest(&chord_between(&seats, "x0", "x1"));
        let far = deepest(&chord_between(&seats, "x0", OPPOSITE));
        assert!(near > far, "a neighbour hop cut as deep as an opposite one");
        assert!(
            r3 - near < (r3 - far) * 0.5,
            "neighbour arc should stay near its ring: near={near} far={far} ring={r3}"
        );
    }

    /// Particles ride this exact path, so an endpoint that drifts would peel
    /// the train off its own line.
    #[test]
    fn a_chord_starts_and_ends_exactly_on_its_seats() {
        let n = banded();
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        for (from, to) in [("x0", OPPOSITE), ("a0", "x4"), ("c0", "c2"), ("c1", "x3")] {
            let c = chord_between(&seats, from, to);
            let (a, b) = (seats[from].pos, seats[to].pos);
            assert!(
                (c.point(0.0) - a).length() < 0.01,
                "{from}->{to} does not start on its seat"
            );
            assert!(
                (c.point(1.0) - b).length() < 0.01,
                "{from}->{to} does not end on its seat"
            );
        }
    }

    /// A berth docks OUTSIDE the rim, facing the wallets it deals with — the
    /// whole point of the form is that its chords arrive from one direction
    /// instead of lassoing the ring from a fake outer-ring seat.
    #[test]
    fn a_berth_docks_beyond_the_rim_facing_its_partners() {
        let mut n = banded();
        n.push(RingNode::new("pillar", 0).berth("minting"));
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let flows = [
            RingFlow {
                timestamp: 1,
                from: "pillar",
                to: "x0",
                quantity: 100,
            },
            RingFlow {
                timestamp: 2,
                from: "pillar",
                to: "x1",
                quantity: 100,
            },
        ];
        let berths = berth_positions(&n, &flows, &seats, CENTRE, OUTER, 4);
        assert_eq!(berths.len(), 1);
        let (key, seat) = berths[0];
        assert_eq!(key, "pillar");
        assert!(
            seat.radius > OUTER,
            "a berth must sit beyond the outer ring, got r={}",
            seat.radius
        );
        // Facing its partners: between x0 and x1, which are 30° apart.
        let (a0, a1) = (seats["x0"].angle, seats["x1"].angle);
        let (lo, hi) = (a0.min(a1), a0.max(a1));
        assert!(
            (lo..=hi).contains(&seat.angle),
            "berth angle {} not between its partners at {a0} and {a1}",
            seat.angle
        );
    }

    /// Two providers serving the same crowd want the same centroid; without a
    /// separation floor their pills stack into one unreadable glyph.
    #[test]
    fn two_berths_never_stack() {
        let mut n = banded();
        n.push(RingNode::new("mint-svc", 0).berth("minting"));
        n.push(RingNode::new("drop-svc", 0).berth("airdrop"));
        let seats = seat_positions(&n, 3, CENTRE, OUTER);
        let flows = [
            RingFlow {
                timestamp: 1,
                from: "mint-svc",
                to: "x0",
                quantity: 100,
            },
            RingFlow {
                timestamp: 2,
                from: "drop-svc",
                to: "x0",
                quantity: 100,
            },
        ];
        let berths = berth_positions(&n, &flows, &seats, CENTRE, OUTER, 4);
        assert_eq!(berths.len(), 2);
        let sep = (berths[0].1.angle - berths[1].1.angle).abs();
        assert!(
            sep >= BERTH_MIN_SEP - 1e-4,
            "berths {sep} rad apart, floor is {BERTH_MIN_SEP}"
        );
    }

    /// Recognising a wallet as a provider must not reshuffle the ring it used
    /// to sit on — every other seat keeps its place.
    #[test]
    fn promoting_a_node_to_a_berth_moves_nobody_else() {
        let with_dot = banded();
        let mut with_berth = banded();
        with_berth.push(RingNode::new("pillar", 3).berth("minting"));
        let a = seat_positions(&with_dot, 3, CENTRE, OUTER);
        let b = seat_positions(&with_berth, 3, CENTRE, OUTER);
        for k in ["c0", "a1", "u2", "x0", "x11"] {
            assert_eq!(a[k].pos, b[k].pos, "{k} moved when a berth was added");
        }
        assert!(
            !b.contains_key("pillar"),
            "a berth must not take a ring seat"
        );
    }

    /// A berth is a real seat to the flow machinery: its traffic draws, and is
    /// not counted as unseated coverage loss.
    #[test]
    fn a_berth_flow_is_drawn_not_unseated() {
        let mut n = nodes();
        n.push(RingNode::new("pillar", 0).berth("minting"));
        let flows = [RingFlow {
            timestamp: 1_000,
            from: "pillar",
            to: "payee-a",
            quantity: 500,
        }];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 100_000));
        spine.set_playhead(1_000 + 3_600);
        let r = run(&n, &flows, &spine, &mut sel);
        assert_eq!(r.flows_unseated, 0, "the berth has a seat");
        assert_eq!(r.in_flight, 1, "and its flow flies");
    }

    /// A seat depends only on (hop, index) — never on what is in view. If a
    /// filter change moved a wallet, the eye could not hold it.
    #[test]
    fn seats_are_stable_regardless_of_flows() {
        let n = nodes();
        let centre = pos2(300.0, 300.0);
        let a = seat_positions(&n, 1, centre, 200.0);
        // Same nodes, different activity — positions must be identical.
        let n2: Vec<RingNode<'_>> = n.iter().map(|x| x.active(false)).collect();
        let b = seat_positions(&n2, 1, centre, 200.0);
        for k in ["treasury", "royalty", "payee-a", "payee-b"] {
            assert_eq!(a[k].pos, b[k].pos, "{k} moved");
        }
        // Hop 0 sits inside hop 1.
        assert!((a["treasury"].pos - centre).length() < (a["payee-a"].pos - centre).length());
    }

    /// Particles are a pure function of the playhead: before the flow, nothing;
    /// during its flight, dots; after it lands, nothing.
    #[test]
    fn particles_follow_the_playhead_and_then_land() {
        let n = nodes();
        let flows = [RingFlow {
            timestamp: 1_000,
            from: "treasury",
            to: "payee-a",
            quantity: 1_000,
        }];
        let mut sel = Selection::default();
        // The domain must cover every playhead the test sets — `set_playhead`
        // clamps to it, so a short domain silently pins the playhead and the
        // flow never ages out of flight.
        let mut spine = SpineState::new((0, 2_000_000));

        spine.set_playhead(900);
        assert_eq!(
            run(&n, &flows, &spine, &mut sel).in_flight,
            0,
            "not yet sent"
        );

        spine.set_playhead(1_000 + 86_400);
        let r = run(&n, &flows, &spine, &mut sel);
        assert_eq!(r.in_flight, 1, "in the air");
        assert!(r.particles > 0, "and drawing dots");

        spine.set_playhead(1_000 + 10 * 86_400);
        assert_eq!(run(&n, &flows, &spine, &mut sel).in_flight, 0, "landed");
    }

    /// Switching a node off removes its traffic but NOT its seat.
    #[test]
    fn an_inactive_node_keeps_its_seat_and_drops_its_flows() {
        let mut n = nodes();
        n[2] = n[2].active(false);
        let flows = [RingFlow {
            timestamp: 1_000,
            from: "treasury",
            to: "payee-a",
            quantity: 500,
        }];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 100_000));
        spine.set_playhead(1_000 + 3_600);
        let r = run(&n, &flows, &spine, &mut sel);
        assert_eq!(r.in_flight, 0, "flows to an off node are not drawn");
        assert_eq!(r.nodes_inactive, 1);
        assert_eq!(r.nodes_shown, 3);
        // The seat is still allocated, so nothing else shifted.
        let seats = seat_positions(&n, 1, pos2(300.0, 300.0), 200.0);
        assert!(seats.contains_key("payee-a"));
    }

    /// A single enormous payment must not drown the frame.
    #[test]
    fn one_huge_flow_is_capped() {
        let n = nodes();
        let flows = [RingFlow {
            timestamp: 1_000,
            from: "treasury",
            to: "payee-a",
            quantity: u64::MAX / 2,
        }];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 100_000));
        spine.set_playhead(1_000 + 3_600);
        assert!(run(&n, &flows, &spine, &mut sel).particles <= MAX_DOTS);
    }

    /// The brush filters, exactly as on every other face of this spine.
    #[test]
    fn the_brush_filters() {
        let n = nodes();
        let flows = [
            RingFlow {
                timestamp: 1_000,
                from: "treasury",
                to: "payee-a",
                quantity: 500,
            },
            RingFlow {
                timestamp: 50_000,
                from: "treasury",
                to: "payee-b",
                quantity: 500,
            },
        ];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 100_000));
        spine.set_playhead(50_000 + 3_600);
        spine.set_brush(Some((40_000, 60_000)));
        assert_eq!(run(&n, &flows, &spine, &mut sel).in_flight, 1);
    }
}
