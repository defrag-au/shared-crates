//! HolderField — the holder graph, reshuffling as assets change hands.
//!
//! Every asset is a dot, every holder a pile, and **a dot is keyed by its
//! ASSET**. That one choice is what makes the whole thing move: a transfer is
//! not a new dot, it is the *same* dot with a new target, so the keyed tweens
//! fly it from one pile to the other and the concentration visibly rebalances.
//! Mint, transfer and burn are the same event with different ends:
//!
//! | move | `from` | `to` | reads as |
//! |---|---|---|---|
//! | mint | `None` | holder | flies in from the emitter |
//! | transfer | holder | holder | flies pile → pile |
//! | burn | holder | `None` | fades out where it sat |
//!
//! This supersedes the mint-only arrivals view: during the mint window it looks
//! the same (everything arriving from the emitter), and afterwards it keeps
//! going — which is where the interesting part is. Marketplace sales show up
//! here because they genuinely move the stake address; this is not a
//! marketplace ledger and makes no claim about *why* something moved, only
//! that it did.
//!
//! ## What stays fixed and what moves
//!
//! - **Pile POSITION is fixed**, packed in first-appearance order. A wallet
//!   never slides across the screen; only its contents change.
//! - **Piles are packed by PEAK holdings over the whole series**, so a pile at
//!   its high-water mark exactly fills its slot and can never overlap a
//!   neighbour — the same "scale from the peak, not the visible max" rule the
//!   rest of the catalog uses, applied to space.
//! - **Dot slot within a pile is by arrival order at that holder**, so existing
//!   dots don't reshuffle when one leaves.
//!
//! ## Playhead vs brush
//!
//! The playhead REVEALS: holdings are the state as of the playhead. The brush
//! FILTERS: with a brush set, only assets that changed hands inside that window
//! are drawn — "show me what moved in October", with the rest of the field
//! dimmed away rather than silently dropped.

use egui::{
    Align2, Color32, CornerRadius, Id, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

use crate::motion::{Easing, tween, tween_bool, tween_from};
use crate::selection::Selection;
use crate::time_spine::SpineState;

/// One change of custody. `from`/`to` are party keys; `None` means the asset
/// did not exist yet (a mint) or ceased to (a burn).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetMove<'a> {
    pub timestamp: i64,
    /// Stable identity of the asset — the dot's key.
    pub asset: &'a str,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
}

impl<'a> AssetMove<'a> {
    pub fn mint(timestamp: i64, asset: &'a str, to: &'a str) -> Self {
        Self {
            timestamp,
            asset,
            from: None,
            to: Some(to),
        }
    }

    pub fn transfer(timestamp: i64, asset: &'a str, from: &'a str, to: &'a str) -> Self {
        Self {
            timestamp,
            asset,
            from: Some(from),
            to: Some(to),
        }
    }

    pub fn burn(timestamp: i64, asset: &'a str, from: &'a str) -> Self {
        Self {
            timestamp,
            asset,
            from: Some(from),
            to: None,
        }
    }
}

pub struct HolderFieldResponse {
    pub response: Response,
    pub hovered: Option<String>,
    /// Assets held at the playhead (after any brush filter).
    pub assets_shown: u32,
    /// Holders with at least one asset at the playhead.
    pub holders_shown: usize,
    /// Moves that happened inside the brush window (0 when no brush).
    pub moves_in_window: usize,
}

pub struct HolderField<'a> {
    moves: &'a [AssetMove<'a>],
    spine: &'a SpineState,
    selection: &'a mut Selection,
    flight_secs: f32,
    dot_color: Option<Color32>,
    height: f32,
    label: Option<&'a dyn Fn(&str) -> String>,
}

impl<'a> HolderField<'a> {
    /// `moves` must be sorted by timestamp.
    pub fn new(
        moves: &'a [AssetMove<'a>],
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            moves,
            spine,
            selection,
            // Longer than it looks: the batch spread eats the first
            // [`BATCH_SPREAD`] of this, so each individual dot flies for
            // ~60% of it. At 0.7s a large sale still read as sudden.
            flight_secs: 1.2,
            dot_color: None,
            height: 320.0,
            label: None,
        }
    }

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

    pub fn label(mut self, f: &'a dyn Fn(&str) -> String) -> Self {
        self.label = Some(f);
        self
    }

    pub fn show(self, ui: &mut Ui) -> HolderFieldResponse {
        let Self {
            moves,
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

        let model = Model::build(moves, &ctx, id);
        if model.parties.is_empty() {
            return HolderFieldResponse {
                response,
                hovered: None,
                assets_shown: 0,
                holders_shown: 0,
                moves_in_window: 0,
            };
        }
        let n = model.parties.len();
        let layout = pack(&model.peak, field, id, &ctx);
        let spacing = layout.spacing;
        let dot_r = (spacing * 0.42).clamp(1.15, 3.5);
        let source = pos2(field.center().x, field.top() - 4.0);

        // ── state at the playhead ─────────────────────────────────────────
        let (brush_lo, brush_hi) = spine.filter_range();
        let brushed = spine.brush.is_some();
        let mut moves_in_window = 0usize;
        // Per asset: who holds it now, and the move that put it there.
        let mut held: Vec<Option<(usize, usize)>> = vec![None; model.assets.len()]; // (party, move idx)
        let mut in_window: Vec<bool> = vec![false; model.assets.len()];

        // A dot's SLOT in its pile is allocated when it ARRIVES and released
        // when it leaves — never recomputed from the current holdings.
        //
        // The obvious implementation (walk the held set, number them 1..n per
        // party) renumbers every asset after any departure, and since slot maps
        // to a phyllotaxis position, one asset leaving a 500-dot pile makes all
        // 500 jump. On real data that is most piles, most of the time, and it
        // reads as the whole chart being unstable.
        //
        // Freed slots go back to a min-heap so the lowest is reused: dots that
        // stayed put keep their exact position, and the pile stays as compact
        // as its peak concurrent holdings rather than growing with churn.
        let mut slot_of: Vec<u32> = vec![0; model.assets.len()];
        // The SEAT a dot vacated, so it can fly from where it actually sat.
        // Without this the origin is the old pile's centre, which makes every
        // departure teleport to the middle of its pile before setting off —
        // and on a large pile that jump is bigger than the flight.
        let mut left_seat: Vec<Option<(usize, u32)>> = vec![None; model.assets.len()];
        let mut next_slot: Vec<u32> = vec![0; n];
        let mut free: Vec<std::collections::BinaryHeap<std::cmp::Reverse<u32>>> = (0..n)
            .map(|_| std::collections::BinaryHeap::new())
            .collect();

        for (mi, m) in model.timeline.iter().enumerate() {
            if m.timestamp > spine.playhead {
                break;
            }
            // Leaving its previous pile frees that seat.
            if let Some((old, _)) = held[m.asset] {
                free[old].push(std::cmp::Reverse(slot_of[m.asset]));
                left_seat[m.asset] = Some((old, slot_of[m.asset]));
            }
            held[m.asset] = m.to.map(|p| (p, mi));
            if let Some(p) = m.to {
                slot_of[m.asset] = match free[p].pop() {
                    Some(std::cmp::Reverse(s)) => s,
                    None => {
                        let s = next_slot[p];
                        next_slot[p] += 1;
                        s
                    }
                };
            }
            if m.timestamp >= brush_lo && m.timestamp <= brush_hi {
                in_window[m.asset] = true;
                moves_in_window += 1;
            }
        }

        let mut shown: Vec<u32> = vec![0; n];
        for (ai, h) in held.iter().enumerate() {
            if let Some((p, _)) = h
                && (!brushed || in_window[ai])
            {
                shown[*p] += 1;
            }
        }
        let assets_shown: u32 = shown.iter().sum();
        let holders_shown = shown.iter().filter(|c| **c > 0).count();

        // ── hover ─────────────────────────────────────────────────────────
        let hovered_idx: Option<usize> = response.hover_pos().and_then(|p| {
            if !field.contains(p) {
                return None;
            }
            let mut best: Option<(usize, f32)> = None;
            for (i, count) in shown.iter().enumerate() {
                if *count == 0 {
                    continue;
                }
                let d = (p - layout.centres[i]).length();
                let hit = (layout.radii[i] + 8.0).max(12.0);
                if d <= hit && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            best.map(|(i, _)| i)
        });
        let hovered: Option<String> = hovered_idx.map(|i| model.parties[i].clone());
        match &hovered {
            Some(h) => selection.hover(h.clone()),
            None => {
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

        // ── emitter ───────────────────────────────────────────────────────
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

        // ── emphasis per pile ─────────────────────────────────────────────
        let mut emph: Vec<f32> = Vec::with_capacity(n);
        for p in &model.parties {
            let target = selection.emphasis(p);
            emph.push(tween(
                &ctx,
                id.with(("emph", p.as_str())),
                target,
                0.18,
                Easing::OutCubic,
            ));
        }

        // ── pile discs ────────────────────────────────────────────────────
        // A recessive disc sized to what the pile holds NOW (the packing is
        // sized to its peak). Without it 160 piles of the same blue dot read as
        // one cloud; with it, a pile that sold down visibly deflates inside the
        // slot it once filled. This is the rebalancing, at a glance.
        let disc_r = |i: usize| {
            let peak = model.peak[i].max(1) as f32;
            layout.radii[i] * (shown[i] as f32 / peak).sqrt() + dot_r
        };
        for i in 0..n {
            // The RESERVED footprint, drawn whether or not anyone is home.
            //
            // Slots are sized by PEAK over the whole series, so the largest
            // slot belongs to whoever ends up largest — and on real data that
            // can be a wallet holding NOTHING for two thirds of the timeline.
            // Drawing nothing for an empty pile left a hole in the middle of
            // the chart that later filled with 500 dots, and the eye read that
            // as the mass having moved rather than as a newcomer arriving.
            //
            // An outline says "this space is spoken for". Growth then reads as
            // filling up, which is what actually happened.
            if layout.radii[i] > 6.0 {
                painter.circle_stroke(
                    layout.centres[i],
                    layout.radii[i],
                    Stroke::new(1.0_f32, ink.linear_multiply(0.05 * emph[i].max(0.35))),
                );
            }
            if shown[i] == 0 {
                continue;
            }
            let r = disc_r(i);
            if r < 5.0 {
                continue;
            }
            painter.circle_filled(
                layout.centres[i],
                r,
                ink.linear_multiply(0.055 * emph[i].max(0.35)),
            );
        }

        // ── dots ──────────────────────────────────────────────────────────
        // Keyed by ASSET: a transfer changes this dot's target, so the tween
        // carries it across the field instead of it blinking from one pile to
        // another. That IS the reshuffle.
        let mut pulse: Vec<Option<f64>> = (0..n)
            .map(|i| ctx.data(|d| d.get_temp::<f64>(id.with(("landed", i)))))
            .collect();
        for (ai, h) in held.iter().enumerate() {
            let Some((p, mi)) = *h else {
                // Burned (or not yet minted): let its tween go so a re-mint
                // would arrive fresh.
                continue;
            };
            if brushed && !in_window[ai] {
                continue;
            }
            let asset = model.assets[ai].as_str();
            let k = slot_of[ai];
            let j = jitter(asset, k);
            // ONE formula for both ends of the flight. Origin and target are
            // the same kind of thing — a seat in a pile — and computing them
            // differently is what let the origin quietly become a centre.
            let seat = |party: usize, slot: u32| -> Pos2 {
                let c = layout.centres[party];
                let (dx, dy) = crate::mint_arrivals::pile_offset(slot);
                let jj = jitter(asset, slot);
                pos2(
                    c.x + dx * spacing + jj.x * spacing * 0.35,
                    c.y + dy * spacing + jj.y * spacing * 0.35,
                )
            };
            let target = seat(p, k);
            let dot_id = id.with(("dot", asset));
            // Where it came from: the emitter for a mint, otherwise THE SEAT
            // IT VACATED — not its old pile's centre. A dot peels off from
            // where it was actually sitting, which is what makes a big sale
            // read as a pile emptying rather than a burst from a midpoint.
            let origin = match (model.timeline[mi].from, left_seat[ai]) {
                (Some(_), Some((prev, prev_slot))) => seat(prev, prev_slot),
                (Some(prev), None) => layout.centres[prev],
                (None, _) => source,
            };
            let t = if playing {
                tween_from(&ctx, dot_id, 0.0, 1.0, flight_secs, Easing::OutCubic)
            } else {
                tween_from(&ctx, dot_id, 1.0, 1.0, 0.0, Easing::OutCubic)
            };
            // A NEW move retargets the same key: reset progress so it flies the
            // new leg rather than snapping.
            let last_move: Option<usize> = ctx.data(|d| d.get_temp(dot_id.with("leg")));
            if last_move != Some(mi) {
                ctx.data_mut(|d| d.insert_temp(dot_id.with("leg"), mi));
                if playing && last_move.is_some() {
                    crate::motion::forget(&ctx, dot_id);
                }
            }
            let col = accent.linear_multiply(emph[p]);
            // EACH DOT LEAVES ON ITS OWN BEAT. A hundred-asset sale where
            // every dot shares one start is a single clump crossing the field
            // and landing at once — the eye reads a blink, not a movement.
            // The offset is derived from the asset key, so a dot's beat is
            // identical every frame and across runs; a per-frame random would
            // make the whole batch shimmer.
            let t = stagger_progress(t, model.assets[ai].as_str());
            if t < 1.0 {
                let ctrl = arc_control(origin, target, j.x);
                let pnow = bezier(origin, ctrl, target, t);
                // VAPOUR TRAIL. Samples are evenly spaced in PROGRESS, not in
                // distance, so the trail stretches out where the dot is moving
                // fast (the launch, under OutCubic) and bunches up as it
                // settles — a comet, not a fixed-length streak. Drawn back to
                // front so the head sits on top.
                for k in (1..=TRAIL_SAMPLES).rev() {
                    let tb = t - TRAIL_STEP * k as f32;
                    if tb <= 0.0 {
                        continue;
                    }
                    let fade = 1.0 - k as f32 / (TRAIL_SAMPLES + 1) as f32;
                    let q = bezier(origin, ctrl, target, tb);
                    painter.circle_filled(
                        q,
                        dot_r * (0.3 + 0.9 * fade),
                        accent.linear_multiply(0.42 * fade * fade),
                    );
                }
                painter.circle_filled(pnow, dot_r * 1.7, accent);
                if t > 0.985 {
                    pulse[p] = Some(now);
                    ctx.data_mut(|d| d.insert_temp(id.with(("landed", p)), now));
                }
            } else {
                painter.circle_filled(target, dot_r, col);
            }
        }

        // ── landing pulses ────────────────────────────────────────────────
        const PULSE_SECS: f64 = 0.8;
        for (i, t0) in pulse.iter().enumerate() {
            let Some(t0) = *t0 else { continue };
            let age = ((now - t0) / PULSE_SECS) as f32;
            if !(0.0..1.0).contains(&age) {
                continue;
            }
            ctx.request_repaint();
            let c = layout.centres[i];
            let r0 = layout.radii[i].max(dot_r * 2.0);
            let ease = Easing::OutCubic.apply(age);
            painter.circle_filled(c, r0 + 1.0, accent.linear_multiply(0.22 * (1.0 - age)));
            painter.circle_stroke(
                c,
                r0 + 2.0 + 14.0 * ease,
                Stroke::new(
                    1.5 * (1.0 - age) + 0.5,
                    accent.linear_multiply(0.9 * (1.0 - age)),
                ),
            );
        }

        // ── direct labels on the biggest piles ────────────────────────────
        // Selective, never one per pile: the four largest holders at the
        // playhead, named on the chart so identity is not hover-only. They are
        // recomputed each frame, so a wallet that trades its way to the top
        // acquires a label as it gets there.
        // The selected pile gets its own, larger label under the ring below —
        // labelling it twice just prints the name on top of itself.
        let sel_idx = selection
            .active()
            .and_then(|s| model.parties.iter().position(|p| p == s))
            .filter(|i| shown[*i] > 0);
        let mut rank: Vec<usize> = (0..n).filter(|i| shown[*i] > 0).collect();
        rank.sort_by(|a, b| shown[*b].cmp(&shown[*a]));
        rank.truncate(4);
        for &i in &rank {
            if disc_r(i) < 9.0 || Some(i) == hovered_idx || Some(i) == sel_idx {
                continue;
            }
            let name = match label {
                Some(f) => f(&model.parties[i]),
                None => elide(&model.parties[i]),
            };
            let galley = fitted_label(
                &painter,
                format!("{name} {}", shown[i]),
                egui::TextStyle::Small.resolve(ui.style()),
                muted,
                rect.width() - 4.0,
            );
            let mut at = pos2(
                layout.centres[i].x - galley.size().x * 0.5,
                layout.centres[i].y + disc_r(i) + 2.0,
            );
            at.x = clamp_into(
                at.x,
                rect.left() + 2.0,
                rect.right() - galley.size().x - 2.0,
            );
            // Never below the field — `painter_at` clips silently.
            at.y = at.y.min(field.bottom() - galley.size().y);
            painter.galley(at, galley, muted);
        }

        // ── selected pile ─────────────────────────────────────────────────
        if let Some(i) = sel_idx {
            let c = layout.centres[i];
            let ring_r = layout.radii[i] + dot_r + 3.0;
            let a = tween_bool(&ctx, id.with("ring"), true, 0.15, Easing::OutCubic);
            painter.circle_stroke(
                c,
                ring_r,
                Stroke::new(1.5_f32, ink.linear_multiply(0.85 * a)),
            );
            let name = match label {
                Some(f) => f(&model.parties[i]),
                None => elide(&model.parties[i]),
            };
            let txt = format!("{name}  ·  {}", shown[i]);
            let galley = fitted_label(
                &painter,
                txt,
                egui::TextStyle::Small.resolve(ui.style()),
                ink,
                rect.width() - 4.0,
            );
            let mut at = pos2(
                c.x - galley.size().x * 0.5,
                c.y - ring_r - galley.size().y - 4.0,
            );
            at.x = clamp_into(
                at.x,
                rect.left() + 2.0,
                rect.right() - galley.size().x - 2.0,
            );
            at.y = at.y.max(field.top());
            let bg = Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 2.0));
            painter.rect_filled(
                bg,
                CornerRadius::same(3),
                ui.visuals().extreme_bg_color.linear_multiply(0.85),
            );
            painter.galley(at, galley, ink);
        }

        // ── tooltip ───────────────────────────────────────────────────────
        if let Some(i) = hovered_idx {
            let key = &model.parties[i];
            let name = match label {
                Some(f) => f(key),
                None => elide(key),
            };
            let share = if !model.assets.is_empty() {
                100.0 * shown[i] as f32 / model.assets.len() as f32
            } else {
                0.0
            };
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
                    if model.peak[i] != shown[i] {
                        ui.label(
                            egui::RichText::new(format!("peak {}", model.peak[i]))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
                ui.label(egui::RichText::new(name).small());
                ui.label(
                    egui::RichText::new(format!(
                        "{share:.1}% of supply · +{} in / -{} out",
                        model.gained[i], model.lost[i]
                    ))
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            });
        }

        // ── hero ──────────────────────────────────────────────────────────
        painter.text(
            pos2(rect.left(), rect.top() + 4.0),
            Align2::LEFT_TOP,
            format!("{assets_shown} assets · {holders_shown} holders"),
            egui::TextStyle::Body.resolve(ui.style()),
            ink,
        );
        let sub = if brushed {
            format!(
                "{moves_in_window} moves {} - {}",
                crate::time_spine::format_date(brush_lo),
                crate::time_spine::format_date(brush_hi)
            )
        } else {
            format!(
                "{} assets · {} moves to here",
                model.assets.len(),
                model
                    .timeline
                    .iter()
                    .filter(|m| m.timestamp <= spine.playhead)
                    .count()
            )
        };
        painter.text(
            pos2(rect.right(), rect.top() + 4.0),
            Align2::RIGHT_TOP,
            sub,
            egui::TextStyle::Small.resolve(ui.style()),
            muted,
        );

        HolderFieldResponse {
            response,
            hovered,
            assets_shown,
            holders_shown,
            moves_in_window,
        }
    }
}

// ---------------------------------------------------------------------------
// Model — interned parties/assets + per-party peak, built once per data set
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Step {
    timestamp: i64,
    asset: usize,
    from: Option<usize>,
    to: Option<usize>,
}

#[derive(Clone)]
struct Model {
    parties: Vec<String>,
    assets: Vec<String>,
    timeline: Vec<Step>,
    /// High-water holdings per party over the WHOLE series — what the packing
    /// is scaled to, so a pile never outgrows its slot.
    peak: Vec<u32>,
    gained: Vec<u32>,
    lost: Vec<u32>,
}

impl Model {
    fn build(moves: &[AssetMove<'_>], ctx: &egui::Context, id: Id) -> Self {
        let key = id.with(("model", moves.len(), moves.first().map(|m| m.timestamp)));
        if let Some(m) = ctx.data(|d| d.get_temp::<Model>(key)) {
            return m;
        }
        let mut parties: Vec<String> = Vec::new();
        let mut assets: Vec<String> = Vec::new();
        let mut timeline = Vec::with_capacity(moves.len());
        // Index-backed, not a linear scan: 50,000 moves over 10,000 assets is
        // 500M string compares as a scan (~2s, on the frame that opens the
        // window) and a few ms as a lookup.
        let mut party_ix: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut asset_ix: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        fn intern<'s>(
            ix: &mut std::collections::HashMap<&'s str, usize>,
            v: &mut Vec<String>,
            s: &'s str,
        ) -> usize {
            *ix.entry(s).or_insert_with(|| {
                v.push(s.to_owned());
                v.len() - 1
            })
        }
        for m in moves {
            // Parties are interned in FIRST-APPEARANCE order, receivers first,
            // which is the packing order and therefore the on-screen order.
            let to = m.to.map(|t| intern(&mut party_ix, &mut parties, t));
            let from = m.from.map(|f| intern(&mut party_ix, &mut parties, f));
            let asset = intern(&mut asset_ix, &mut assets, m.asset);
            timeline.push(Step {
                timestamp: m.timestamp,
                asset,
                from,
                to,
            });
        }
        let n = parties.len();
        let (mut cur, mut peak) = (vec![0i64; n], vec![0u32; n]);
        let (mut gained, mut lost) = (vec![0u32; n], vec![0u32; n]);
        for s in &timeline {
            if let Some(f) = s.from {
                cur[f] -= 1;
                lost[f] += 1;
            }
            if let Some(t) = s.to {
                cur[t] += 1;
                gained[t] += 1;
                peak[t] = peak[t].max(cur[t].max(0) as u32);
            }
        }
        let model = Self {
            parties,
            assets,
            timeline,
            peak,
            gained,
            lost,
        };
        ctx.data_mut(|d| d.insert_temp(key, model.clone()));
        model
    }
}

// ---------------------------------------------------------------------------
// Layout — greedy circle packing, radius ∝ √peak
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Packed {
    centres: Vec<Pos2>,
    radii: Vec<f32>,
    spacing: f32,
}

/// Unit-space placement `(x, y, r)` per pile — cached, resolution-independent.
#[derive(Clone)]
struct Placement(Vec<(f32, f32, f32)>);

fn pack(peak: &[u32], field: Rect, id: Id, ctx: &egui::Context) -> Packed {
    // **A PILE'S POSITION DEPENDS ON THE DATA AND NOTHING ELSE.**
    //
    // The placement is computed once in unit space and cached against the pile
    // sizes ALONE; every frame just fits it to whatever rect exists (an O(n)
    // rescale). The aspect used for the spiral is captured on that first pack
    // and reused forever after.
    //
    // Keying the cache on the rect's aspect as well — even coarsely bucketed —
    // meant the layout could be rebuilt by anything that changed the panel's
    // shape: pinning a wallet makes the watched row appear, which shortens the
    // viewport, which shifts the aspect, which crossed a bucket and moved every
    // pile on screen. Selecting something is the single most common action in
    // this tool, and it must not scramble the picture you are reading.
    //
    // The cost of never re-packing is a little letterboxing after a drastic
    // resize. That is a trade worth making twice over: a resize should not
    // scramble the chart either.
    let key = id.with((
        "pack",
        peak.len(),
        peak.iter().map(|c| *c as u64).sum::<u64>(),
    ));
    let placed: Vec<(f32, f32, f32)> = match ctx.data(|d| d.get_temp::<Placement>(key)) {
        Some(p) => p.0,
        None => {
            let aspect = (field.width() / field.height().max(1.0)).clamp(1.0, 8.0);
            let p = spiral_pack(peak, aspect);
            ctx.data_mut(|d| d.insert_temp(key, Placement(p.clone())));
            p
        }
    };
    fit(placed, field)
}

/// Unit-space greedy packing, biggest first, along an ellipse of `aspect`.
fn spiral_pack(peak: &[u32], aspect: f32) -> Vec<(f32, f32, f32)> {
    let unit: Vec<f32> = peak.iter().map(|c| ((*c as f32).sqrt()).max(1.4)).collect();
    let gap = 0.9;
    // Spiral out along an ELLIPSE matching the rect, not a circle. A circular
    // blob in a 1500x320 field is fitted to the short axis and throws away
    // three quarters of the width — every pile then packs down to a few pixels
    // and the field reads as one undifferentiated cloud.
    //
    // Biggest first. Greedy packing that starts small strands the whales out on
    // the rim; starting big puts them in the middle and lets the tail fill the
    // gaps — the same reason every circle-packer sorts descending.
    let mut order: Vec<usize> = (0..unit.len()).collect();
    order.sort_by(|a, b| unit[*b].total_cmp(&unit[*a]));

    // Collision checks go through a uniform grid. Checking every already-placed
    // pile is O(n^2) per candidate and O(n^3) overall — at 7,900 holders that
    // is a layout that never finishes, on the first frame, on the UI thread.
    let cell = unit.iter().cloned().fold(1.4f32, f32::max) * 2.0 + gap;
    let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
        std::collections::HashMap::new();
    let cell_of = |x: f32, y: f32| ((x / cell).floor() as i32, (y / cell).floor() as i32);
    let mut placed: Vec<(f32, f32, f32)> = vec![(0.0, 0.0, 0.0); unit.len()];

    // Resume the spiral near where the last pile landed instead of re-walking
    // it from the centre every time — without this the walk is quadratic in
    // the number of holders and the first frame costs seconds.
    //
    // Back off by ONE TURN, not by a fraction of `t`. The step shrinks as the
    // spiral winds out, so far from the centre a "15% of t" back-off is dozens
    // of rings and tens of thousands of candidate points; one turn is one ring,
    // which is all a smaller pile needs to find the gap its predecessor left.
    const TURN: f32 = std::f32::consts::TAU;
    let mut resume = 0.0f32;
    for (rank, &i) in order.iter().enumerate() {
        let r = unit[i];
        if rank == 0 {
            placed[i] = (0.0, 0.0, r);
            grid.entry(cell_of(0.0, 0.0)).or_default().push(i);
            continue;
        }
        let mut t = (resume - TURN).max(0.0);
        loop {
            let rad = 0.9 * t;
            let (x, y) = (aspect * rad * t.cos(), rad * t.sin());
            // A pile can only touch one in its own cell or the eight around it,
            // because no pile is wider than a cell.
            let (cx, cy) = cell_of(x, y);
            let ok = (-1..=1).all(|dx| {
                (-1..=1).all(|dy| {
                    grid.get(&(cx + dx, cy + dy)).is_none_or(|bucket| {
                        bucket.iter().all(|&j| {
                            let (px, py, pr) = placed[j];
                            (x - px).hypot(y - py) >= r + pr + gap
                        })
                    })
                })
            });
            if ok {
                placed[i] = (x, y, r);
                grid.entry((cx, cy)).or_default().push(i);
                resume = t;
                break;
            }
            // Keep the sampling density along the (longer) elliptical arc.
            t += 0.35 / ((1.0 + rad * 0.15) * aspect.sqrt());
            if t > 4000.0 {
                placed[i] = (x, y, r);
                grid.entry((cx, cy)).or_default().push(i);
                resume = t;
                break;
            }
        }
    }
    placed
}

/// Fit a unit-space placement into the rect: uniform scale, centred.
fn fit(placed: Vec<(f32, f32, f32)>, field: Rect) -> Packed {
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
    Packed {
        centres: placed
            .iter()
            .map(|&(x, y, _)| pos2(ox + x * scale, oy + y * scale))
            .collect(),
        radii: placed.iter().map(|&(_, _, r)| r * scale).collect(),
        spacing: scale,
    }
}

fn arc_control(from: Pos2, to: Pos2, fan: f32) -> Pos2 {
    let mid = pos2((from.x + to.x) * 0.5, (from.y + to.y) * 0.5);
    let d = to - from;
    let len = d.length().max(1.0);
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

/// Keep `v` inside `[lo, hi]` — and survive `hi < lo`.
///
/// `f32::clamp` PANICS when the bounds invert, and here they invert for a
/// real reason: `hi` is "right edge minus the label's own width", so any
/// label wider than its container makes it smaller than `lo`. That crashed
/// the app on a narrow centre panel — a layout accident taking down the
/// process. Pinning to `lo` is the honest answer: the label starts at the
/// left edge and whatever does not fit is clipped, which is what a reader
/// would expect from a too-narrow pane.
fn clamp_into(v: f32, lo: f32, hi: f32) -> f32 {
    v.clamp(lo, hi.max(lo))
}

/// Lay out a single-line label that FITS, truncating with an ellipsis.
///
/// A label wider than the field is unreadable however it is positioned, so
/// the fix is to shorten it rather than to place it cleverly. Names got
/// longer when handle harvesting landed (S1 went from 4,718 to 23,057
/// aliases), which is what made this reachable.
fn fitted_label(
    painter: &egui::Painter,
    text: String,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap.max_width = max_width.max(24.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    painter.layout_job(job)
}

/// Trail length in samples, and their spacing in progress-space. Together
/// they span the last ~35% of a dot's flight.
const TRAIL_SAMPLES: usize = 10;
const TRAIL_STEP: f32 = 0.035;
/// How much of the flight window is spent LAUNCHING a batch — the spread of
/// per-dot start beats. Each dot still flies the remainder, so the batch
/// leaves as a stream and lands as one.
const BATCH_SPREAD: f32 = 0.4;

/// Re-time one dot's progress so a batch departs as a stream.
///
/// The dot starts somewhere in the first [`BATCH_SPREAD`] of the window and
/// takes the rest to fly. Two properties matter and are tested:
/// - **deterministic** in the asset key, so a dot's beat never changes;
/// - **exactly 1.0 when the batch is done** — a scrubbed (not playing)
///   field feeds in `1.0`, and any dot left below 1.0 there would be frozen
///   mid-flight over a static field.
fn stagger_progress(t: f32, asset: &str) -> f32 {
    let s = (jitter(asset, 0x5747).x * 0.5 + 0.5).clamp(0.0, 1.0) * BATCH_SPREAD;
    ((t - s) / (1.0 - BATCH_SPREAD)).clamp(0.0, 1.0)
}

fn jitter(asset: &str, k: u32) -> Vec2 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in asset.bytes() {
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

    /// A label wider than its pane must not take the process down.
    ///
    /// Reproduces the real crash: the field started at x=260 (just past the
    /// parties panel) while the label's width pushed the right-hand bound to
    /// 257 — `f32::clamp` panics on inverted bounds, so a narrow window was
    /// a hard abort on startup rather than an ugly label.
    #[test]
    fn a_label_wider_than_the_pane_pins_instead_of_panicking() {
        // The exact numbers from the panic: min = 260.0, max = 257.25.
        assert_eq!(clamp_into(300.0, 260.0, 257.25), 260.0);
        assert_eq!(clamp_into(0.0, 260.0, 257.25), 260.0);
        // Ordinary bounds behave exactly like `clamp`.
        assert_eq!(clamp_into(50.0, 10.0, 100.0), 50.0);
        assert_eq!(clamp_into(5.0, 10.0, 100.0), 10.0);
        assert_eq!(clamp_into(150.0, 10.0, 100.0), 100.0);
        // Degenerate but valid: zero-width window pins to the edge.
        assert_eq!(clamp_into(7.0, 42.0, 42.0), 42.0);
    }

    /// A dot leaves from the seat it SAT IN, not from its pile's centre.
    ///
    /// The seat allocator frees a slot on departure, and the vacated slot is
    /// exactly the flight's origin — so this pins the bookkeeping the render
    /// depends on. Before it, origin was `centres[prev]`, and on a 500-dot
    /// pile the teleport to the middle was larger than the flight itself.
    #[test]
    fn a_departing_dot_remembers_the_seat_it_vacated() {
        // alice holds two, then hands the SECOND one to bob: the vacated seat
        // must be the one that asset occupied, not slot 0 and not a centre.
        let moves = vec![
            AssetMove::mint(10, "a1", "alice"),
            AssetMove::mint(20, "a2", "alice"),
            AssetMove::transfer(30, "a2", "alice", "bob"),
        ];
        // Mirrors the allocator in `show`.
        let assets = ["a1", "a2"];
        let idx = |a: &str| assets.iter().position(|x| *x == a).unwrap();
        let mut held: Vec<Option<usize>> = vec![None; 2];
        let mut slot_of = [0u32; 2];
        let mut left_seat: Vec<Option<(usize, u32)>> = vec![None; 2];
        let mut next = [0u32; 2]; // per party: alice=0, bob=1
        let party = |p: &str| if p == "alice" { 0 } else { 1 };
        for m in &moves {
            let ai = idx(m.asset);
            if let Some(old) = held[ai] {
                left_seat[ai] = Some((old, slot_of[ai]));
            }
            held[ai] = m.to.map(party);
            if let Some(p) = held[ai] {
                slot_of[ai] = next[p];
                next[p] += 1;
            }
        }
        assert_eq!(
            left_seat[idx("a2")],
            Some((0, 1)),
            "a2 vacated alice's SECOND seat"
        );
        assert_eq!(
            left_seat[idx("a1")],
            None,
            "a1 never left, so never vacated"
        );
    }

    /// The stagger turns a batch into a stream, and must not strand a dot.
    ///
    /// The load-bearing case is SCRUBBING: a paused field feeds progress
    /// 1.0, and any dot whose re-timed progress landed below 1.0 there would
    /// hang mid-air over a static field — a bug that would only show up as
    /// "some dots are in the wrong place", with no obvious cause.
    #[test]
    fn the_batch_stagger_streams_without_stranding_anyone() {
        let assets: Vec<String> = (0..500).map(|i| format!("MD{i:04}")).collect();

        for a in &assets {
            assert_eq!(
                stagger_progress(1.0, a),
                1.0,
                "{a} is stranded mid-flight on a paused field"
            );
            // Deterministic: the same key gives the same beat, always.
            assert_eq!(stagger_progress(0.5, a), stagger_progress(0.5, a));
            // Never runs past its target, never goes backwards.
            assert!((0.0..=1.0).contains(&stagger_progress(0.5, a)));
            assert!(stagger_progress(0.0, a) <= f32::EPSILON);
        }

        // Mid-flight the batch is SPREAD OUT — that is the whole point. Some
        // dots have not left, some are well along.
        let mid: Vec<f32> = assets.iter().map(|a| stagger_progress(0.5, a)).collect();
        let lo = mid.iter().copied().fold(f32::MAX, f32::min);
        let hi = mid.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            hi - lo > 0.5,
            "a batch should read as a stream, not a clump: spread {lo}..{hi}"
        );
    }

    fn ctx() -> egui::Context {
        let c = egui::Context::default();
        crate::motion::tests::step(&c, 0.0);
        c
    }

    #[test]
    fn peak_is_the_high_water_mark_not_the_final_holding() {
        // alice takes 3, then hands 2 to bob: alice's PEAK is 3 even though she
        // ends with 1 — the packing must reserve room for the high-water mark
        // or her pile would overflow its slot mid-series.
        let moves = vec![
            AssetMove::mint(10, "a1", "alice"),
            AssetMove::mint(11, "a2", "alice"),
            AssetMove::mint(12, "a3", "alice"),
            AssetMove::transfer(20, "a1", "alice", "bob"),
            AssetMove::transfer(21, "a2", "alice", "bob"),
        ];
        let m = Model::build(&moves, &ctx(), Id::new("t"));
        let ai = m.parties.iter().position(|p| p == "alice").unwrap();
        let bi = m.parties.iter().position(|p| p == "bob").unwrap();
        assert_eq!(m.peak[ai], 3);
        assert_eq!(m.peak[bi], 2);
        assert_eq!((m.gained[ai], m.lost[ai]), (3, 2));
        assert_eq!((m.gained[bi], m.lost[bi]), (2, 0));
        assert_eq!(m.assets.len(), 3, "assets are interned, not duplicated");
    }

    #[test]
    fn burn_removes_the_asset_from_every_pile() {
        let moves = vec![
            AssetMove::mint(10, "a1", "alice"),
            AssetMove::burn(20, "a1", "alice"),
        ];
        let m = Model::build(&moves, &ctx(), Id::new("t"));
        assert_eq!(m.timeline.last().unwrap().to, None);
        assert_eq!(m.peak[0], 1, "held one before the burn");
    }

    #[test]
    fn packing_reserves_peak_and_never_overlaps() {
        let ctx = ctx();
        let field = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 300.0));
        let mut peak = vec![340u32, 210, 160, 120, 90];
        peak.extend((0..160).map(|i| 3 + (i % 5)));
        let p = pack(&peak, field, Id::new("t"), &ctx);
        for i in 0..peak.len() {
            for j in (i + 1)..peak.len() {
                let d = (p.centres[i] - p.centres[j]).length();
                assert!(d + 1e-3 >= p.radii[i] + p.radii[j], "piles {i}/{j} overlap");
            }
        }
        for (c, r) in p.centres.iter().zip(&p.radii) {
            assert!(c.x - r >= field.left() - 1.0 && c.x + r <= field.right() + 1.0);
            assert!(c.y - r >= field.top() - 1.0 && c.y + r <= field.bottom() + 1.0);
        }
    }

    /// **Piles do not move when the panel does.**
    ///
    /// Pinning a wallet makes the watched row appear, which shortens the
    /// viewport. If the packing depended on the rect's aspect, that shifted
    /// every pile on screen — and selecting something is the most common
    /// action in the tool. The placement must survive any rect change, so the
    /// two layouts can only differ by a uniform scale and a translation.
    #[test]
    fn a_panel_resize_does_not_move_the_piles() {
        let ctx = ctx();
        let peak = vec![340u32, 210, 160, 120, 90, 40, 12, 7, 3];
        let id = Id::new("stable");

        let wide = pack(
            &peak,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 420.0)),
            id,
            &ctx,
        );
        // The exact change a pin causes: same width, less height.
        let shorter = pack(
            &peak,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 360.0)),
            id,
            &ctx,
        );
        // And a drastic one, for good measure.
        let narrow = pack(
            &peak,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 500.0)),
            id,
            &ctx,
        );

        for other in [&shorter, &narrow] {
            // Every inter-pile distance must scale by the SAME factor — that is
            // exactly "the arrangement is unchanged, only fitted differently".
            let d0 = (wide.centres[0] - wide.centres[1]).length();
            let o0 = (other.centres[0] - other.centres[1]).length();
            let ratio = o0 / d0;
            for i in 0..peak.len() {
                for j in (i + 1)..peak.len() {
                    let a = (wide.centres[i] - wide.centres[j]).length();
                    let b = (other.centres[i] - other.centres[j]).length();
                    assert!(
                        (b - a * ratio).abs() < 0.5,
                        "pile {i}/{j} moved relative to the others: {a} -> {b} (ratio {ratio})"
                    );
                }
            }
        }
    }

    /// Slot allocation, in isolation: an asset that STAYS keeps its slot when
    /// a neighbour leaves the same pile, and the vacated slot is reused rather
    /// than the pile growing.
    ///
    /// This is the property the whole chart's stability rests on. Numbering the
    /// held set 1..n per party instead renumbers everything after a departure,
    /// which moved every dot in a 500-dot pile whenever one left.
    #[test]
    fn a_departure_does_not_move_the_dots_that_stayed() {
        // alice holds a1, a2, a3; a1 leaves; a4 arrives.
        let moves = vec![
            AssetMove::mint(10, "a1", "alice"),
            AssetMove::mint(11, "a2", "alice"),
            AssetMove::mint(12, "a3", "alice"),
            AssetMove::transfer(20, "a1", "alice", "bob"),
            AssetMove::mint(30, "a4", "alice"),
        ];
        let at = |t: i64| -> std::collections::HashMap<String, u32> { slots_at(&moves, t) };

        let before = at(15);
        assert_eq!(before["a1"], 0);
        assert_eq!(before["a2"], 1);
        assert_eq!(before["a3"], 2);

        // a1 has gone to bob. a2 and a3 must NOT have moved.
        let after = at(25);
        assert_eq!(after["a2"], 1, "a2 stayed put");
        assert_eq!(after["a3"], 2, "a3 stayed put");
        assert_eq!(after["a1"], 0, "a1 takes slot 0 in bob's empty pile");

        // The seat a1 vacated is reused, so the pile does not sprawl.
        let refilled = at(35);
        assert_eq!(refilled["a4"], 0, "a4 reuses alice's freed slot 0");
        assert_eq!(refilled["a2"], 1, "and still nothing else moved");
        assert_eq!(refilled["a3"], 2);
    }

    /// Reproduce the widget's slot allocator over a move list, returning
    /// `asset -> slot` at time `t`. Mirrors `show`'s scan exactly.
    fn slots_at(moves: &[AssetMove<'_>], t: i64) -> std::collections::HashMap<String, u32> {
        let mut parties: Vec<&str> = Vec::new();
        let mut assets: Vec<&str> = Vec::new();
        for m in moves {
            for p in [m.from, m.to].into_iter().flatten() {
                if !parties.contains(&p) {
                    parties.push(p);
                }
            }
            if !assets.contains(&m.asset) {
                assets.push(m.asset);
            }
        }
        let idx = |v: &Vec<&str>, s: &str| v.iter().position(|x| *x == s).unwrap();
        let mut held: Vec<Option<usize>> = vec![None; assets.len()];
        let mut slot_of: Vec<u32> = vec![0; assets.len()];
        let mut next: Vec<u32> = vec![0; parties.len()];
        let mut free: Vec<std::collections::BinaryHeap<std::cmp::Reverse<u32>>> = (0..parties
            .len())
            .map(|_| std::collections::BinaryHeap::new())
            .collect();
        for m in moves {
            if m.timestamp > t {
                break;
            }
            let ai = idx(&assets, m.asset);
            if let Some(old) = held[ai] {
                free[old].push(std::cmp::Reverse(slot_of[ai]));
            }
            held[ai] = m.to.map(|p| idx(&parties, p));
            if let Some(p) = held[ai] {
                slot_of[ai] = match free[p].pop() {
                    Some(std::cmp::Reverse(s)) => s,
                    None => {
                        let s = next[p];
                        next[p] += 1;
                        s
                    }
                };
            }
        }
        assets
            .iter()
            .enumerate()
            .map(|(i, a)| (a.to_string(), slot_of[i]))
            .collect()
    }

    /// The reshuffle, headless: hold state follows the playhead, and a transfer
    /// moves the count from one pile to the other.
    #[test]
    fn holdings_follow_the_playhead_and_transfers_reshuffle() {
        let moves = vec![
            AssetMove::mint(10, "a1", "alice"),
            AssetMove::mint(11, "a2", "alice"),
            AssetMove::mint(12, "a3", "bob"),
            AssetMove::transfer(30, "a1", "alice", "carol"),
            AssetMove::transfer(31, "a2", "alice", "carol"),
        ];
        let mut spine = SpineState::new((0, 40));
        let mut sel = Selection::default();
        let ctx = egui::Context::default();
        let run = |ctx: &egui::Context, spine: &SpineState, sel: &mut Selection| {
            let mut out = None;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
                ..Default::default()
            };
            ctx.begin_pass(raw);
            egui::Area::new(Id::new("a")).show(ctx, |ui| {
                ui.set_min_size(vec2(600.0, 400.0));
                out = Some(HolderField::new(&moves, spine, sel).show(ui));
            });
            let _ = ctx.end_pass();
            out.unwrap()
        };

        // Before the transfers: 3 assets across alice + bob.
        spine.set_playhead(20);
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 3);
        assert_eq!(r.holders_shown, 2);

        // After: still 3 assets, but now spread over THREE holders — alice 0,
        // bob 1, carol 2. The count moved; nothing was created or lost.
        spine.set_playhead(40);
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 3);
        assert_eq!(r.holders_shown, 2, "alice is empty; bob + carol hold");

        // Brush to the transfer window: only the assets that moved are shown.
        spine.set_brush(Some((25, 35)));
        let r = run(&ctx, &spine, &mut sel);
        assert_eq!(r.assets_shown, 2, "a1 + a2 changed hands in the window");
        assert_eq!(r.moves_in_window, 2);
        assert_eq!(r.holders_shown, 1, "both landed on carol");
    }
}
