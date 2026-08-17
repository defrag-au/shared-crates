//! Motion — keyed, eased tweens over egui's Id-addressed memory.
//!
//! This is the `d3-transition` idea for immediate mode: **object constancy** comes
//! from keying animation state by *entity* (an asset name, a holder key, a
//! destination) rather than by draw order. Every frame you ask "where is
//! entity K right now, given it wants to be at P?" and get back a position that
//! is on its way there. Nothing is retained between frames except this small
//! per-key record, which lives in egui's own `ctx.data` store — the same place
//! `Context::animate_value_with_time` keeps its state.
//!
//! Why not just call `animate_value_with_time`? It interpolates *linearly* and
//! retargets from wherever it is, which is right for a slider and wrong for a
//! dot settling into a pile: settling wants ease-out, arrival wants ease-in-out,
//! and a retarget mid-flight must start from the *eased* position, not snap.
//! [`tween`] keeps `(from, to, start, duration)` per key and applies the easing
//! curve to progress, so retargeting is continuous. It requests a repaint while
//! anything is still moving and goes quiet the instant everything has arrived.
//!
//! Determinism: given the same sequence of targets and frame times, the same
//! positions come out. Nothing here uses randomness or wall-clock beyond
//! `ctx.input(|i| i.time)`, which a test harness controls.

use egui::{Color32, Context, Id, Pos2, Vec2};

/// The easing curve applied to tween progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    Linear,
    /// Fast start, decelerating arrival — the "settle" curve. Use for things
    /// landing (dots into piles, bars into rank).
    #[default]
    OutCubic,
    /// Slow start, slow end — use for things *moving between* two resting
    /// states where both ends should look deliberate.
    InOutCubic,
    /// Slight overshoot then settle. Use sparingly; good for a rank change
    /// you want the eye to catch.
    OutBack,
}

impl Easing {
    /// Map linear progress `t ∈ [0,1]` to eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::OutCubic => 1.0 - (1.0 - t).powi(3),
            Self::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::OutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Per-key tween record. Lives in `ctx.data` under the caller's `Id`.
#[derive(Clone, Copy, Debug)]
struct TweenState {
    from: f32,
    to: f32,
    start: f64,
    duration: f32,
    easing: Easing,
}

impl TweenState {
    fn value_at(&self, now: f64) -> f32 {
        if self.duration <= 0.0 {
            return self.to;
        }
        let t = ((now - self.start) as f32 / self.duration).clamp(0.0, 1.0);
        let e = self.easing.apply(t);
        self.from + (self.to - self.from) * e
    }

    fn done_at(&self, now: f64) -> bool {
        self.duration <= 0.0 || (now - self.start) as f32 >= self.duration
    }
}

/// Tween a scalar toward `target` over `duration` seconds with `easing`.
///
/// First call for a key snaps to `target` (a newly-appearing thing has no
/// "from"). A changed `target` retargets from the current *eased* value, so a
/// dot re-aimed mid-flight bends rather than jumps. Requests a repaint while
/// still moving.
pub fn tween(ctx: &Context, id: Id, target: f32, duration: f32, easing: Easing) -> f32 {
    let now = ctx.input(|i| i.time);
    let mut still_moving = false;
    let value = ctx.data_mut(|d| {
        let st = d.get_temp_mut_or_insert_with(id, || TweenState {
            from: target,
            to: target,
            start: now,
            duration: 0.0,
            easing,
        });
        if (st.to - target).abs() > f32::EPSILON {
            let current = st.value_at(now);
            *st = TweenState {
                from: current,
                to: target,
                start: now,
                duration,
                easing,
            };
        }
        let v = st.value_at(now);
        still_moving = !st.done_at(now);
        v
    });
    if still_moving {
        ctx.request_repaint();
    }
    value
}

/// Tween a scalar starting FROM a given value the first time the key is seen —
/// for things that *arrive* (a dot flying in from the mint) rather than things
/// that merely change.
pub fn tween_from(
    ctx: &Context,
    id: Id,
    initial: f32,
    target: f32,
    duration: f32,
    easing: Easing,
) -> f32 {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        if d.get_temp::<TweenState>(id).is_none() {
            d.insert_temp(
                id,
                TweenState {
                    from: initial,
                    to: target,
                    start: now,
                    duration,
                    easing,
                },
            );
        }
    });
    tween(ctx, id, target, duration, easing)
}

/// Tween a position (each axis is its own keyed scalar).
pub fn tween_pos(ctx: &Context, id: Id, target: Pos2, duration: f32, easing: Easing) -> Pos2 {
    Pos2::new(
        tween(ctx, id.with("x"), target.x, duration, easing),
        tween(ctx, id.with("y"), target.y, duration, easing),
    )
}

/// Tween a position that ARRIVES from `initial` the first time the key is seen.
pub fn tween_pos_from(
    ctx: &Context,
    id: Id,
    initial: Pos2,
    target: Pos2,
    duration: f32,
    easing: Easing,
) -> Pos2 {
    Pos2::new(
        tween_from(ctx, id.with("x"), initial.x, target.x, duration, easing),
        tween_from(ctx, id.with("y"), initial.y, target.y, duration, easing),
    )
}

/// Tween a size / offset.
pub fn tween_vec(ctx: &Context, id: Id, target: Vec2, duration: f32, easing: Easing) -> Vec2 {
    Vec2::new(
        tween(ctx, id.with("x"), target.x, duration, easing),
        tween(ctx, id.with("y"), target.y, duration, easing),
    )
}

/// Tween a colour in premultiplied-RGBA space (each channel keyed).
pub fn tween_color(
    ctx: &Context,
    id: Id,
    target: Color32,
    duration: f32,
    easing: Easing,
) -> Color32 {
    let ch = |k: &str, v: u8| tween(ctx, id.with(k), v as f32, duration, easing).round() as u8;
    Color32::from_rgba_premultiplied(
        ch("r", target.r()),
        ch("g", target.g()),
        ch("b", target.b()),
        ch("a", target.a()),
    )
}

/// Progress `0..=1` of a boolean flip — the eased twin of
/// `Context::animate_bool_with_time`. Handy for emphasis fades.
pub fn tween_bool(ctx: &Context, id: Id, on: bool, duration: f32, easing: Easing) -> f32 {
    tween(ctx, id, if on { 1.0 } else { 0.0 }, duration, easing)
}

/// Forget a key's tween state (e.g. when the entity leaves the dataset), so a
/// later re-appearance is treated as a fresh arrival.
pub fn forget(ctx: &Context, id: Id) {
    ctx.data_mut(|d| {
        d.remove::<TweenState>(id);
        for k in ["x", "y", "r", "g", "b", "a"] {
            d.remove::<TweenState>(id.with(k));
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn easing_curves_are_anchored_and_monotone() {
        for e in [
            Easing::Linear,
            Easing::OutCubic,
            Easing::InOutCubic,
            Easing::OutBack,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-6, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-5, "{e:?} at 1");
        }
        // The non-overshooting curves never leave [0,1].
        for e in [Easing::Linear, Easing::OutCubic, Easing::InOutCubic] {
            let mut prev = 0.0;
            for i in 0..=100 {
                let v = e.apply(i as f32 / 100.0);
                assert!((0.0..=1.0).contains(&v));
                assert!(v >= prev - 1e-6, "{e:?} not monotone at {i}");
                prev = v;
            }
        }
        // OutBack overshoots past 1 on the way in — that is the point of it.
        assert!((0..100).any(|i| Easing::OutBack.apply(i as f32 / 100.0) > 1.0));
        // OutCubic decelerates: more than half the distance in the first half.
        assert!(Easing::OutCubic.apply(0.5) > 0.5);
    }

    /// Begin a pass at wall-clock `t`: egui reads `input.time` from the raw
    /// input handed to `begin_pass`, so tests can step time deterministically.
    pub(crate) fn step(ctx: &Context, t: f64) {
        ctx.begin_pass(egui::RawInput {
            time: Some(t),
            ..Default::default()
        });
    }

    fn ctx_at(t: f64) -> Context {
        let ctx = Context::default();
        step(&ctx, t);
        ctx
    }

    #[test]
    fn first_call_snaps_then_retarget_eases_and_settles() {
        let ctx = Context::default();
        let id = Id::new("dot");

        // t=0: first sight → snaps to target.
        step(&ctx, 0.0);
        assert_eq!(tween(&ctx, id, 10.0, 1.0, Easing::Linear), 10.0);
        let _ = ctx.end_pass();

        // t=0.5: retarget to 20 → begins moving from 10.
        step(&ctx, 0.5);
        assert_eq!(tween(&ctx, id, 20.0, 1.0, Easing::Linear), 10.0);
        let _ = ctx.end_pass();

        // t=1.0: halfway (linear).
        step(&ctx, 1.0);
        let mid = tween(&ctx, id, 20.0, 1.0, Easing::Linear);
        assert!((mid - 15.0).abs() < 1e-4, "got {mid}");
        let _ = ctx.end_pass();

        // t=1.5: arrived and settled — the recorded state reports done, which
        // is exactly the condition under which no repaint is requested.
        step(&ctx, 1.5);
        assert_eq!(tween(&ctx, id, 20.0, 1.0, Easing::Linear), 20.0);
        let settled = ctx.data(|d| d.get_temp::<TweenState>(id).map(|s| s.done_at(1.5)));
        assert_eq!(settled, Some(true), "settled tween must report done");
        let _ = ctx.end_pass();
    }

    #[test]
    fn retarget_mid_flight_is_continuous() {
        let ctx = Context::default();
        let id = Id::new("dot");
        step(&ctx, 0.0);
        tween(&ctx, id, 0.0, 1.0, Easing::Linear);
        let _ = ctx.end_pass();
        step(&ctx, 0.0);
        tween(&ctx, id, 100.0, 1.0, Easing::Linear); // start flight 0→100
        let _ = ctx.end_pass();
        step(&ctx, 0.5);
        let before = tween(&ctx, id, 100.0, 1.0, Easing::Linear); // 50
        let _ = ctx.end_pass();
        step(&ctx, 0.5);
        // Re-aim to 0 at the same instant: must start from ~50, not jump.
        let after = tween(&ctx, id, 0.0, 1.0, Easing::Linear);
        assert!((after - before).abs() < 1e-3, "{before} -> {after}");
    }

    #[test]
    fn tween_from_starts_at_initial() {
        let ctx = ctx_at(0.0);
        let id = Id::new("arrival");
        // Arrives from -100 toward 0; at t=0 it is AT -100, not snapped to 0.
        let v = tween_from(&ctx, id, -100.0, 0.0, 1.0, Easing::Linear);
        assert!((v + 100.0).abs() < 1e-4, "got {v}");
    }
}
