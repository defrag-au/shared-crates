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
}

impl<'a> RingNode<'a> {
    pub fn new(key: &'a str, hop: u8) -> Self {
        Self {
            key,
            hop,
            active: true,
        }
    }

    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
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
    flight: i64,
    height: f32,
    label: Option<&'a dyn Fn(&str) -> String>,
    inventory: Option<&'a InventoryFn<'a>>,
}

const OUT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x2e);
const IN: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
/// Per-flow particle cap. A single huge payment must not drown the frame.
const MAX_DOTS: usize = 40;

/// Ring tints, innermost first — a TEAL ordinal ramp plus a neutral outer step.
///
/// ## Why teal, and why these exact values
///
/// The ring index is ordinal (how close to the project), so it takes one hue in
/// monotone lightness steps rather than four unrelated hues. The hue is
/// constrained from two directions: the chords drawn ACROSS these rings are
/// [`OUT`] orange and [`IN`] blue, and a seat dot must never be mistakable for
/// a payment.
///
/// Both were chosen by running the palette validator, not by eye, over every
/// pair of {3 ring steps, neutral, orange, blue} on this dark surface —
/// violet, the obvious first pick, **fails**: it collapses onto the chord blue
/// under protanopia at ΔE 5.1. Teal clears with worst-pair ΔE 12.4 under CVD
/// (deutan) and 16.2 under normal vision, against floors of 8 and 15.
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
    Color32::from_rgb(0x19, 0xc2, 0xad), // associates — paid BY the project
    Color32::from_rgb(0x10, 0x89, 0x7c), // customers — bought from it
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
        // in view — switching a filter must not move a wallet.
        let max_hop = nodes.iter().map(|n| n.hop).max().unwrap_or(0);
        let centre = rect.center();
        let r_outer = (rect.width().min(rect.height()) * 0.5) - 46.0;
        let seats = seat_positions(nodes, max_hop, centre, r_outer.max(40.0));

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
        for f in flows {
            if f.timestamp > now || f.timestamp < lo || f.timestamp > hi {
                continue;
            }
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
        for ((from, to), (qty, _count)) in &pairs {
            let (Some(a), Some(b)) = (seats.get(*from), seats.get(*to)) else {
                continue;
            };
            // A pinned wallet's own flows stay bright; everything else recedes.
            let involved = watched.as_deref().is_none_or(|w| w == *from || w == *to);
            let alpha = if involved { 1.0 } else { 0.13 };
            let col = if watched.as_deref() == Some(*to) {
                IN
            } else {
                OUT
            };
            // Weight by share of the heaviest pair, on a log scale — treasury
            // flows span orders of magnitude, so a linear ramp shows one chord.
            let t =
                ((*qty as f64 + 1.0).ln() / (heaviest as f64 + 1.0).ln()).clamp(0.08, 1.0) as f32;
            let chord = Chord::new(a, b, centre, max_hop, r_outer.max(40.0));
            painter.add(egui::Shape::line(
                chord.polyline(28),
                Stroke::new(
                    (0.6 + 1.4 * t) * if involved { 1.0 } else { 0.6 },
                    col.gamma_multiply((0.10 + 0.35 * t) * alpha),
                ),
            ));
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
        let mut hovered: Option<String> = None;
        for n in nodes {
            let Some(seat) = seats.get(n.key) else {
                continue;
            };
            let p = &seat.pos;
            let emph = selection.emphasis(n.key);
            let on = n.active;
            let r = if n.hop == 0 { 6.0 } else { 4.0 };
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
            if response
                .hover_pos()
                .is_some_and(|h| (h - *p).length() <= r + 7.0)
            {
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
            format!("{in_flight} flows in the air · 1 dot = {q} {unit_label}"),
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
        let ring: Vec<&RingNode<'_>> = nodes.iter().filter(|n| n.hop == h).collect();
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
