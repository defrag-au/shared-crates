//! ArrivalField — every asset a dot, every holder a pile, and now they MOVE.
//!
//! The falsifier face for "is egui the reason the surface feels flat, or is it
//! the missing layer?" Same data and same rules as [`crate::MintArrivals`]
//! (dot `k` of a pile depends only on `k`; piles never rearrange; scale from
//! the peak over the whole series; dot size decoupled from spacing) — plus the
//! things it lacked:
//!
//! - **object constancy + motion**: each dot is keyed by `(holder, k)`. While
//!   the spine plays, a newly revealed dot ARCS in from the source (the mint,
//!   drawn as an emitter) with a short trail and an ease-out settle; on landing
//!   the pile PULSES — a ring at the pile plus a brief brighten of the whole
//!   pile — so growth registers even when the dot itself is a pixel wide;
//! - **the shared spine**: the playhead REVEALS, the brush FILTERS;
//! - **linked selection**: hovering a pile names its holder in a shared
//!   [`crate::Selection`]; whatever is selected anywhere is emphasised here and
//!   everything else recedes;
//! - **tooltips**: value first, on the pile as hit target;
//! - **packing**: greedy circle packing in FIRST-APPEARANCE order, radius ∝
//!   √(final count) over the whole series, fitted to the field. Whales get room,
//!   small holders stop sitting in giant empty cells, and nothing reflows as
//!   the playhead moves because the layout never depends on it.
//!
//! A paused or scrubbed frame settles instantly — a still must be readable,
//! because a still is what goes in a write-up. Flight and pulses happen only
//! while the spine is playing.

use egui::{
    Align2, Color32, CornerRadius, Id, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

use crate::mint_arrivals::{Arrival, pile_offset};
use crate::motion::{Easing, tween, tween_bool, tween_from};
use crate::selection::Selection;
use crate::time_spine::SpineState;

pub struct ArrivalFieldResponse {
    pub response: Response,
    /// Holder under the pointer this frame, if any.
    pub hovered: Option<String>,
    /// Assets and holders revealed at the playhead (after the brush filter).
    pub assets_shown: u32,
    pub holders_shown: usize,
}

pub struct ArrivalField<'a> {
    arrivals: &'a [Arrival<'a>],
    spine: &'a SpineState,
    selection: &'a mut Selection,
    flight_secs: f32,
    dot_color: Option<Color32>,
    height: f32,
    label: Option<&'a dyn Fn(&str) -> String>,
}

impl<'a> ArrivalField<'a> {
    /// `arrivals` must be sorted by timestamp.
    pub fn new(
        arrivals: &'a [Arrival<'a>],
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            arrivals,
            spine,
            selection,
            flight_secs: 0.7,
            dot_color: None,
            height: 320.0,
            label: None,
        }
    }

    /// Seconds a dot spends in flight when it appears during playback.
    pub fn flight_secs(mut self, s: f32) -> Self {
        self.flight_secs = s;
        self
    }

    pub fn dot_color(mut self, c: Color32) -> Self {
        self.dot_color = Some(c);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Labeller for the hovered/pinned holder (default: elided key).
    pub fn label(mut self, f: &'a dyn Fn(&str) -> String) -> Self {
        self.label = Some(f);
        self
    }

    pub fn show(self, ui: &mut Ui) -> ArrivalFieldResponse {
        let Self {
            arrivals,
            spine,
            selection,
            flight_secs,
            dot_color,
            height,
            label,
        } = self;
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let accent = dot_color.unwrap_or(Color32::from_rgb(0x39, 0x87, 0xe5));

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        let id = response.id;
        let hero_h = 30.0;
        let field = Rect::from_min_max(pos2(rect.left(), rect.top() + hero_h + 10.0), rect.max);
        let painter = ui.painter_at(rect);

        // ── holders in first-appearance order, with final counts ───────────
        let mut order: Vec<&str> = Vec::new();
        let mut totals: Vec<u32> = Vec::new();
        for a in arrivals {
            match order.iter().position(|h| *h == a.holder) {
                Some(i) => totals[i] += a.count,
                None => {
                    order.push(a.holder);
                    totals.push(a.count);
                }
            }
        }
        if order.is_empty() {
            return ArrivalFieldResponse {
                response,
                hovered: None,
                assets_shown: 0,
                holders_shown: 0,
            };
        }
        let n = order.len();
        let index_of = |h: &str| order.iter().position(|o| *o == h);

        // ── layout: circle-pack in first-appearance order, fit to field ─────
        let layout = pack(&totals, field, id, &ctx);
        let spacing = layout.spacing;
        let dot_r = (spacing * 0.42).clamp(1.15, 3.5);
        let centre_of = |i: usize| layout.centres[i];
        let radius_of = |i: usize| layout.radii[i];
        let source = pos2(field.center().x, field.top() - 4.0);

        // ── shown counts (brush + playhead) ─────────────────────────────────
        let (filt_lo, filt_hi) = spine.filter_range();
        let mut shown: Vec<u32> = vec![0; n];
        let mut first_seen: Vec<Option<i64>> = vec![None; n];
        for a in arrivals {
            if a.timestamp <= spine.playhead
                && a.timestamp >= filt_lo
                && a.timestamp <= filt_hi
                && let Some(i) = index_of(a.holder)
            {
                shown[i] += a.count;
                if first_seen[i].is_none() {
                    first_seen[i] = Some(a.timestamp);
                }
            }
        }
        let total_all: u32 = totals.iter().sum();

        // ── hover: nearest pile whose disc (+ a hit margin) contains the ptr ─
        let hovered_idx: Option<usize> = response.hover_pos().and_then(|p| {
            if !field.contains(p) {
                return None;
            }
            let mut best: Option<(usize, f32)> = None;
            for (i, count) in shown.iter().enumerate() {
                if *count == 0 {
                    continue;
                }
                let d = (p - centre_of(i)).length();
                // The hit target is bigger than the mark: disc + 8px, or at least
                // 12px for a one-dot pile. Nearest wins if discs overlap.
                let hit = (radius_of(i) + 8.0).max(12.0);
                if d <= hit && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            best.map(|(i, _)| i)
        });
        let hovered: Option<String> = hovered_idx.map(|i| order[i].to_string());
        match &hovered {
            Some(h) => selection.hover(h.clone()),
            None => {
                // Only clear a hover WE set: another face may own it this frame.
                if let Some(prev) = ui.data(|d| d.get_temp::<String>(id.with("last_hover"))) {
                    selection.clear_hover_if(&prev);
                }
            }
        }
        ui.data_mut(|d| match &hovered {
            Some(h) => {
                d.insert_temp(id.with("last_hover"), h.clone());
            }
            None => d.remove::<String>(id.with("last_hover")),
        });
        if response.clicked() {
            match &hovered {
                Some(h) => selection.toggle_pin(h.clone()),
                None => selection.clear_pin(),
            }
        }

        // ── emitter (the source) ───────────────────────────────────────────
        let playing = spine.playing;
        let glow = tween_bool(&ctx, id.with("emit"), playing, 0.3, Easing::InOutCubic);
        painter.circle_filled(source, 2.5, muted);
        if glow > 0.0 {
            painter.circle_stroke(
                source,
                4.0 + 6.0 * glow,
                Stroke::new(1.0_f32, accent.linear_multiply(0.5 * glow)),
            );
        }

        // ── dots ───────────────────────────────────────────────────────────
        // Emitted per ARRIVAL so each knows when it landed. Keyed by
        // (holder, k) so the same dot is the same dot across frames.
        let mut running: Vec<u32> = vec![0; n];
        let mut assets_shown = 0u32;
        let mut holders_seen = vec![false; n];
        let mut emph: Vec<f32> = Vec::with_capacity(n);
        for h in &order {
            let target = selection.emphasis(h);
            emph.push(tween(
                &ctx,
                id.with(("emph", *h)),
                target,
                0.18,
                Easing::OutCubic,
            ));
        }
        // Landing pulses: per pile, the wall-clock moment the newest dot lands.
        // Read this frame, updated as flights complete, drawn after the dots so
        // the ring sits over the pile.
        let mut pulse_at: Vec<Option<f64>> = (0..n)
            .map(|i| ctx.data(|d| d.get_temp::<f64>(id.with(("landed", i)))))
            .collect();

        for a in arrivals {
            let Some(i) = index_of(a.holder) else {
                continue;
            };
            let k0 = running[i];
            running[i] += a.count;
            let revealed = a.timestamp <= spine.playhead;
            let in_brush = a.timestamp >= filt_lo && a.timestamp <= filt_hi;
            if !revealed || !in_brush {
                continue;
            }
            assets_shown += a.count;
            holders_seen[i] = true;
            let centre = centre_of(i);
            let e = emph[i];
            let col = accent.linear_multiply(e);
            for k in k0..k0 + a.count {
                let (dx, dy) = pile_offset(k);
                let j = jitter(a.holder, k);
                let target = pos2(
                    centre.x + dx * spacing + j.x * spacing * 0.35,
                    centre.y + dy * spacing + j.y * spacing * 0.35,
                );
                let dot_id = id.with(("dot", a.holder, k));
                // Flight progress 0→1, keyed per dot. Playing: a fresh dot starts
                // at 0 and eases to 1. Scrubbed: snap to 1 (and record it) so a
                // later play doesn't replay every settled dot.
                let t = if playing {
                    tween_from(&ctx, dot_id, 0.0, 1.0, flight_secs, Easing::OutCubic)
                } else {
                    tween_from(&ctx, dot_id, 1.0, 1.0, 0.0, Easing::OutCubic)
                };
                if t < 1.0 {
                    // In flight: arc from the emitter, fanned per pile so
                    // simultaneous arrivals don't overprint. Trail = the same
                    // curve a few steps back, fading.
                    let ctrl = arc_control(source, target, j.x);
                    let p = bezier(source, ctrl, target, t);
                    for (back, a) in [(0.10f32, 0.18f32), (0.05, 0.36)] {
                        let tb = (t - back).max(0.0);
                        let q = bezier(source, ctrl, target, tb);
                        painter.circle_filled(q, dot_r * 1.1, accent.linear_multiply(a));
                    }
                    painter.circle_filled(p, dot_r * 1.7, accent);
                    // Landing is imminent: schedule the pulse for arrival.
                    if t > 0.985 {
                        pulse_at[i] = Some(now);
                        ctx.data_mut(|d| d.insert_temp(id.with(("landed", i)), now));
                    }
                } else {
                    painter.circle_filled(target, dot_r, col);
                }
            }
        }
        let holders_shown = holders_seen.iter().filter(|b| **b).count();

        // ── landing pulses: ring + brief brighten of the pile ──────────────
        const PULSE_SECS: f64 = 0.8;
        for (i, t0) in pulse_at.iter().enumerate() {
            let Some(t0) = *t0 else { continue };
            let age = ((now - t0) / PULSE_SECS) as f32;
            if !(0.0..1.0).contains(&age) {
                continue;
            }
            ctx.request_repaint();
            let c = centre_of(i);
            let r0 = radius_of(i).max(dot_r * 2.0);
            let ease = Easing::OutCubic.apply(age);
            // The pile brightens then fades — visible even on a one-dot pile.
            painter.circle_filled(c, r0 + 1.0, accent.linear_multiply(0.22 * (1.0 - age)));
            // Ring expands outward and thins as it fades.
            painter.circle_stroke(
                c,
                r0 + 2.0 + 14.0 * ease,
                Stroke::new(
                    1.5 * (1.0 - age) + 0.5,
                    accent.linear_multiply(0.9 * (1.0 - age)),
                ),
            );
        }

        // ── selected pile: ring + label ────────────────────────────────────
        if let Some(sel) = selection.active().map(|s| s.to_string())
            && let Some(i) = index_of(&sel)
            && shown[i] > 0
        {
            let c = centre_of(i);
            let ring_r = radius_of(i) + dot_r + 3.0;
            let ring_a = tween_bool(&ctx, id.with("ring"), true, 0.15, Easing::OutCubic);
            painter.circle_stroke(
                c,
                ring_r,
                Stroke::new(1.5_f32, ink.linear_multiply(0.85 * ring_a)),
            );
            let text = match label {
                Some(f) => f(&sel),
                None => elide(&sel),
            };
            let txt = format!("{text}  ·  {}", shown[i]);
            let galley =
                painter.layout_no_wrap(txt, egui::TextStyle::Small.resolve(ui.style()), ink);
            let mut at = pos2(
                c.x - galley.size().x * 0.5,
                c.y - ring_r - galley.size().y - 4.0,
            );
            at.x =
                at.x.clamp(rect.left() + 2.0, rect.right() - galley.size().x - 2.0);
            at.y = at.y.max(field.top());
            let bg = Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 2.0));
            painter.rect_filled(
                bg,
                CornerRadius::same(3),
                ui.visuals().extreme_bg_color.linear_multiply(0.85),
            );
            painter.galley(at, galley, ink);
        }

        // ── tooltip on the hovered pile — value first, then who ────────────
        if let Some(i) = hovered_idx {
            let key = order[i];
            let name = match label {
                Some(f) => f(key),
                None => elide(key),
            };
            let share = if total_all > 0 {
                100.0 * shown[i] as f32 / total_all as f32
            } else {
                0.0
            };
            let since = first_seen[i]
                .map(crate::time_spine::format_date)
                .unwrap_or_default();
            egui::Tooltip::always_open(
                ctx.clone(),
                ui.layer_id(),
                id.with("tip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.set_max_width(260.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}", shown[i]))
                            .strong()
                            .size(18.0),
                    );
                    ui.label(
                        egui::RichText::new(if shown[i] == 1 { "asset" } else { "assets" })
                            .color(ui.visuals().weak_text_color()),
                    );
                    if totals[i] != shown[i] {
                        ui.label(
                            egui::RichText::new(format!("of {} eventually", totals[i]))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
                ui.label(egui::RichText::new(name).small());
                ui.label(
                    egui::RichText::new(format!("{share:.1}% of supply · first {since}"))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });
        }

        // ── hero ───────────────────────────────────────────────────────────
        let hero = format!("{assets_shown} assets · {holders_shown} holders");
        painter.text(
            pos2(rect.left(), rect.top() + 4.0),
            Align2::LEFT_TOP,
            hero,
            egui::TextStyle::Body.resolve(ui.style()),
            ink,
        );
        let sub = if let Some((a, b)) = spine.brush {
            format!(
                "filtered to {} - {}",
                crate::time_spine::format_date(a),
                crate::time_spine::format_date(b)
            )
        } else {
            format!("of {total_all} · {n}")
        };
        painter.text(
            pos2(rect.right(), rect.top() + 4.0),
            Align2::RIGHT_TOP,
            sub,
            egui::TextStyle::Small.resolve(ui.style()),
            muted,
        );

        ArrivalFieldResponse {
            response,
            hovered,
            assets_shown,
            holders_shown,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout — greedy circle packing in insertion order
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Packed {
    centres: Vec<Pos2>,
    radii: Vec<f32>,
    /// Uniform dot spacing: `radius_i = spacing · √count_i`, so a pile's AREA
    /// is proportional to its count and a whale reads as a whale.
    spacing: f32,
}

/// Pack piles as circles with radius ∝ √count, placed in FIRST-APPEARANCE order
/// (never re-sorted), each at the first spot along an outward spiral where it
/// fits; then fit the whole arrangement to `field`. Deterministic, and it
/// depends only on the whole-series counts — never on the playhead — so the
/// picture never reflows while scrubbing. Cached per (counts, field) in ctx.
fn pack(counts: &[u32], field: Rect, id: Id, ctx: &egui::Context) -> Packed {
    let key = id.with((
        "pack",
        counts.len(),
        counts.iter().map(|c| *c as u64).sum::<u64>(),
        (field.width() * 10.0) as i32,
        (field.height() * 10.0) as i32,
    ));
    if let Some(p) = ctx.data(|d| d.get_temp::<Packed>(key)) {
        return p;
    }

    let n = counts.len();
    // Unit radii — scaled to fit at the end. A floor keeps one-asset piles
    // hittable and visible next to a 340-asset whale.
    let unit: Vec<f32> = counts
        .iter()
        .map(|c| ((*c as f32).sqrt()).max(1.4))
        .collect();
    let gap = 0.9;
    let mut placed: Vec<(f32, f32, f32)> = Vec::with_capacity(n); // (x, y, r)
    for &r in &unit {
        if placed.is_empty() {
            placed.push((0.0, 0.0, r));
            continue;
        }
        // Spiral outward from the origin; accept the first collision-free spot.
        let mut t = 0.0f32;
        let step = 0.35;
        loop {
            let rad = 0.9 * t;
            let (x, y) = (rad * t.cos(), rad * t.sin());
            let ok = placed
                .iter()
                .all(|&(px, py, pr)| ((x - px).powi(2) + (y - py).powi(2)).sqrt() >= r + pr + gap);
            if ok {
                placed.push((x, y, r));
                break;
            }
            t += step / (1.0 + rad * 0.15);
            if t > 4000.0 {
                placed.push((x, y, r));
                break;
            }
        }
    }
    // Fit: bounding box → field, uniform scale, centred.
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y, r) in &placed {
        minx = minx.min(x - r);
        miny = miny.min(y - r);
        maxx = maxx.max(x + r);
        maxy = maxy.max(y + r);
    }
    let bw = (maxx - minx).max(1e-3);
    let bh = (maxy - miny).max(1e-3);
    let pad = 6.0;
    let scale = ((field.width() - 2.0 * pad) / bw).min((field.height() - 2.0 * pad) / bh);
    let ox = field.center().x - (minx + maxx) * 0.5 * scale;
    let oy = field.center().y - (miny + maxy) * 0.5 * scale;
    let centres: Vec<Pos2> = placed
        .iter()
        .map(|&(x, y, _)| pos2(ox + x * scale, oy + y * scale))
        .collect();
    let radii: Vec<f32> = placed.iter().map(|&(_, _, r)| r * scale).collect();
    let packed = Packed {
        centres,
        radii,
        spacing: scale,
    };
    ctx.data_mut(|d| d.insert_temp(key, packed.clone()));
    packed
}

// ---------------------------------------------------------------------------
// Flight geometry
// ---------------------------------------------------------------------------

/// Control point for an arc from `from` to `to`: lifted above the chord and
/// swung sideways by `fan ∈ [-1,1]` so simultaneous arrivals spread out.
fn arc_control(from: Pos2, to: Pos2, fan: f32) -> Pos2 {
    let mid = pos2((from.x + to.x) * 0.5, (from.y + to.y) * 0.5);
    let d = to - from;
    let len = d.length().max(1.0);
    // Perpendicular to the chord, biased upward so arcs bulge away from the field.
    let perp = vec2(-d.y, d.x) / len;
    let lift = (len * 0.25).min(90.0);
    let side = fan * lift * 0.6;
    pos2(mid.x + perp.x * side, mid.y - lift + perp.y * side)
}

fn bezier(a: Pos2, c: Pos2, b: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    pos2(
        u * u * a.x + 2.0 * u * t * c.x + t * t * b.x,
        u * u * a.y + 2.0 * u * t * c.y + t * t * b.y,
    )
}

/// Deterministic per-(holder, k) jitter in `[-1, 1]²` — a tiny hash, no RNG
/// state, so the same dot always jitters the same way and a screenshot is
/// reproducible.
fn jitter(holder: &str, k: u32) -> Vec2 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in holder.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= k as u64;
    h = h.wrapping_mul(0x0000_0100_0000_01b3);
    let a = ((h >> 11) & 0xffff) as f32 / 65535.0 * 2.0 - 1.0;
    let b = ((h >> 33) & 0xffff) as f32 / 65535.0 * 2.0 - 1.0;
    vec2(a, b)
}

fn elide(key: &str) -> String {
    if key.len() <= 20 {
        return key.to_string();
    }
    format!("{}…{}", &key[..12], &key[key.len() - 6..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        let a = jitter("stake1abc", 3);
        let b = jitter("stake1abc", 3);
        assert_eq!(a, b);
        assert_ne!(jitter("stake1abc", 4), a);
        assert_ne!(jitter("stake1xyz", 3), a);
        for k in 0..500 {
            let j = jitter("h", k);
            assert!(j.x.abs() <= 1.0 && j.y.abs() <= 1.0);
        }
    }

    #[test]
    fn packing_is_collision_free_and_fits_the_field() {
        let ctx = egui::Context::default();
        let field = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 300.0));
        // Mekka-shaped: 5 whales then a long tail.
        let mut counts = vec![340u32, 210, 160, 120, 90];
        counts.extend((0..160).map(|i| 3 + (i % 5)));
        let p = pack(&counts, field, Id::new("t"), &ctx);
        assert_eq!(p.centres.len(), counts.len());
        // No two discs overlap.
        for i in 0..counts.len() {
            for j in (i + 1)..counts.len() {
                let d = (p.centres[i] - p.centres[j]).length();
                assert!(
                    d + 1e-3 >= p.radii[i] + p.radii[j],
                    "piles {i} and {j} overlap: d={d} r={}+{}",
                    p.radii[i],
                    p.radii[j]
                );
            }
        }
        // Everything inside the field.
        for (c, r) in p.centres.iter().zip(&p.radii) {
            assert!(field.contains(*c));
            assert!(c.x - r >= field.left() - 1.0 && c.x + r <= field.right() + 1.0);
            assert!(c.y - r >= field.top() - 1.0 && c.y + r <= field.bottom() + 1.0);
        }
        // Area ∝ count: the biggest whale is √(340/3)≈10.6× a small pile's radius.
        let ratio = p.radii[0] / p.radii[5];
        assert!((7.0..12.0).contains(&ratio), "ratio {ratio}");
        // Deterministic and cached: identical on a second call.
        let q = pack(&counts, field, Id::new("t"), &ctx);
        assert_eq!(p.centres, q.centres);
    }

    #[test]
    fn arc_bulges_upward_and_ends_at_the_target() {
        let a = pos2(100.0, 0.0);
        let b = pos2(300.0, 200.0);
        let c = arc_control(a, b, 0.0);
        assert!(
            c.y < (a.y + b.y) * 0.5,
            "control point lifted above the chord"
        );
        assert_eq!(bezier(a, c, b, 0.0), a);
        let end = bezier(a, c, b, 1.0);
        assert!((end - b).length() < 1e-4);
        // Fan swings the arc sideways, deterministically.
        assert_ne!(arc_control(a, b, 1.0).x, arc_control(a, b, -1.0).x);
    }

    /// Drive the widget headlessly through a Context: assets revealed follow the
    /// playhead, the brush filters, hover names the holder, click pins it.
    #[test]
    fn reveal_filter_and_selection_headless() {
        let arrivals = vec![
            Arrival::new(10, "alice", 3),
            Arrival::new(20, "bob", 1),
            Arrival::new(30, "alice", 2),
            Arrival::new(40, "carol", 5),
        ];
        let mut spine = SpineState::new((0, 50));
        let mut sel = Selection::default();
        let ctx = egui::Context::default();
        let run = |ctx: &egui::Context, spine: &SpineState, sel: &mut Selection| {
            let mut out = None;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 400.0))),
                ..Default::default()
            };
            ctx.begin_pass(raw);
            egui::Area::new(egui::Id::new("test-area")).show(ctx, |ui| {
                ui.set_min_size(vec2(600.0, 400.0));
                out = Some(ArrivalField::new(&arrivals, spine, sel).show(ui));
            });
            let _ = ctx.end_pass();
            out.unwrap()
        };

        // Opens at the end: everything shown.
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 11);
        assert_eq!(r.holders_shown, 3);

        // Scrub back: only alice's first arrival.
        spine.set_playhead(15);
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 3);
        assert_eq!(r.holders_shown, 1);

        // Brush filters independent of the playhead.
        spine.set_playhead(50);
        spine.set_brush(Some((18, 35)));
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 3, "bob 1 + alice 2");
        assert_eq!(r.holders_shown, 2);
        assert!(!sel.is_active("carol"));
    }
}
