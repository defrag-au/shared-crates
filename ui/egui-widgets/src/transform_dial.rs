//! `TransformDial` — one control with no modes: the puck moves the subject and
//! the rim around it shapes it.
//!
//! [`TransformStick`](crate::transform_stick::TransformStick) needs a mode row
//! because one stick has to do three jobs. That row is the part of it a reader
//! has to learn, and it costs a line of chrome and a hint underneath. The dial
//! puts the three jobs in two places on one control instead:
//!
//! - **The puck** is the stick. Push and hold to move, at a rate, as far as you
//!   like. Once it is held it can ride out over the rim, so the rim's edge is
//!   also the edge of a full push.
//! - **The rim** is a ring you can grab. Sweep it round to turn, or pull it out
//!   or in to size. Both are direct rather than rates. A turn is circular, so a
//!   ring never runs out of reach for it. A size rarely needs more than one pull,
//!   and a second grab carries on from the first.
//!
//! ## One grab does one thing
//!
//! A thumb sweeping round a ring never moves in a perfect circle, so a rim
//! gesture that both turned and sized would grow a little on every turn. The
//! first few points of travel decide it instead: mostly round the ring is a
//! **sweep**, mostly away from or towards the centre is a **pull**, and the
//! decision holds until release. Letting go and grabbing again is how a reader
//! changes their mind.
//!
//! ## Sizing is exponential
//!
//! A pull of [`PULL_PER_E`] points scales by *e*, outward or inward. Equal
//! pulls give equal ratios, the centre is not a singularity, and pulling out
//! and back in again returns exactly to where it started.
//!
//! As with the stick, the caller owns the placement and its space, and the
//! subject is drawn wherever it actually appears.

use std::f32::consts::{PI, TAU};

use egui::{Align2, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::icons::PhosphorIcon;
use crate::placement::Placement;
use crate::theme::ThemeExt;
use crate::transform_stick::{DEAD_ZONE, FINE, Rates, clamp_unit, response};

/// The dial's side, in points. `theme-exempt`: a thumb affordance, sized so the
/// rim band and the puck are each a fingertip.
const WELL: f32 = 184.0;

/// Room kept outside the rim for the pull's ghost ring and a forgiving grab.
/// `theme-exempt`: pointer affordance.
const ROOM: f32 = 10.0;

/// How wide the rim's grab band is. `theme-exempt`: a fingertip.
const BAND: f32 = 34.0;

/// The puck's radius. `theme-exempt`: pointer affordance.
const PUCK: f32 = 15.0;

/// How far a rim grab travels before it decides between a sweep and a pull.
/// `theme-exempt`: a feel parameter.
const LOCK: f32 = 8.0;

/// Below this distance from the centre a bearing is noise, so a sweep waits
/// rather than snapping. `theme-exempt`: geometry.
const FLOOR: f32 = 12.0;

/// Points of radial pull per factor of *e* in size. Pulling a whole rim band
/// outward is roughly a ×1.7.
pub const PULL_PER_E: f32 = 64.0;

/// How many ridges the rim carries. They turn with a sweep, which is what makes
/// the rim read as something that turns. `theme-exempt`: drawing detail.
const RIDGES: usize = 28;

/// Spring rate for the puck and the rim returning home, per second.
const SPRING: f32 = 18.0;

/// One line for the reader, for a caption or a first-run hint.
pub const HINT: &str = "push the puck to move · sweep the rim to turn · pull it to size";

/// What a rim grab became once it had travelled far enough to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RimAxis {
    /// Round the ring: turn.
    Sweep,
    /// Away from or towards the centre: size.
    Pull,
}

/// What the dial is doing right now, for a host that wants to say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialGesture {
    Move,
    /// The rim is held and has not yet travelled far enough to decide.
    Rim,
    Turn,
    Size,
}

impl DialGesture {
    pub fn label(self) -> &'static str {
        match self {
            DialGesture::Move => "moving",
            DialGesture::Rim => "sweep to turn, pull to size",
            DialGesture::Turn => "turning",
            DialGesture::Size => "sizing",
        }
    }
}

/// A rim grab in flight. Every point is relative to the dial's centre.
#[derive(Clone, Copy, Debug)]
struct Rim {
    start: Vec2,
    last: Vec2,
    axis: Option<RimAxis>,
}

impl Rim {
    fn new(at: Vec2) -> Self {
        Self {
            start: at,
            last: at,
            axis: None,
        }
    }

    /// The pointer moved to `at`. Returns this step's turn in radians and its
    /// growth in factors of *e*; at most one of the two is non-zero.
    fn advance(&mut self, at: Vec2) -> (f32, f32) {
        let axis = match self.axis {
            Some(axis) => axis,
            None => {
                let moved = at - self.start;
                if moved.length() < LOCK {
                    return (0.0, 0.0);
                }
                let out = self.start.normalized();
                let radial = moved.dot(out);
                let tangential = (moved - out * radial).length();
                let axis = if tangential >= radial.abs() {
                    RimAxis::Sweep
                } else {
                    RimAxis::Pull
                };
                self.axis = Some(axis);
                axis
            }
        };
        match axis {
            RimAxis::Sweep => {
                // Too near the centre to read a bearing: hold, and pick the
                // sweep up again from the last good one. `last` is always a
                // good one — a grab starts on the rim, and only a good bearing
                // replaces it.
                if at.length() < FLOOR {
                    return (0.0, 0.0);
                }
                let turn = shortest_turn(bearing(at) - bearing(self.last));
                self.last = at;
                (turn, 0.0)
            }
            RimAxis::Pull => {
                let grow = (at.length() - self.last.length()) / PULL_PER_E;
                self.last = at;
                (0.0, grow)
            }
        }
    }
}

/// What a press is holding.
#[derive(Clone, Copy, Debug)]
enum Hold {
    Puck,
    Rim(Rim),
}

/// A puck that moves and a rim that turns and sizes, on one control.
#[derive(Default)]
pub struct TransformDial {
    /// What the move rate reads, in `−1..=1`. Zero the instant the puck is let
    /// go, as with the stick.
    push: Vec2,
    /// Where the puck is drawn, springing home after release.
    puck: Vec2,
    /// The press's offset from the puck, so the puck does not jump under a
    /// thumb that lands off-centre.
    grab: Vec2,
    hold: Option<Hold>,
    /// Turn and growth gathered since the last [`apply`](Self::apply).
    turn: f32,
    grow: f32,
    /// The rim's drawn turn: its ridges follow the thumb.
    ridges: f32,
    /// The rim's drawn pull, in points, springing home after release.
    pull: f32,
}

impl TransformDial {
    /// Is a move rate running? While it is, the caller must keep asking for
    /// frames, or the subject stops with the pointer.
    pub fn engaged(&self) -> bool {
        self.push != Vec2::ZERO
    }

    /// Is anything still springing home? Worth a repaint, and nothing more.
    pub fn settling(&self) -> bool {
        self.hold.is_none() && (self.puck != Vec2::ZERO || self.pull != 0.0)
    }

    /// What the dial is doing, if anything is held.
    pub fn gesture(&self) -> Option<DialGesture> {
        match self.hold? {
            Hold::Puck => Some(DialGesture::Move),
            Hold::Rim(rim) => Some(match rim.axis {
                None => DialGesture::Rim,
                Some(RimAxis::Sweep) => DialGesture::Turn,
                Some(RimAxis::Pull) => DialGesture::Size,
            }),
        }
    }

    /// Draw the dial and take this frame's gesture. Call [`apply`](Self::apply)
    /// after it, once a frame, to carry the gesture onto a placement.
    pub fn show(&mut self, ui: &mut Ui, dt: f32) -> egui::Response {
        let side = WELL.min(ui.available_width());
        let (rect, interaction) = ui.allocate_exact_size(Vec2::splat(side), Sense::drag());
        let geometry = Geometry::new(rect);
        let centre = geometry.centre;
        let pointer = ui.ctx().input(|input| input.pointer.interact_pos());

        if interaction.drag_started() {
            let press = ui
                .ctx()
                .input(|input| input.pointer.press_origin())
                .or(pointer);
            self.hold = press.and_then(|at| {
                let offset = at - centre;
                if offset.length() < geometry.inner {
                    self.grab = centre + self.puck * geometry.travel - at;
                    Some(Hold::Puck)
                } else if offset.length() <= geometry.outer + ROOM {
                    Some(Hold::Rim(Rim::new(offset)))
                } else {
                    None
                }
            });
        }
        if interaction.drag_stopped() {
            self.hold = None;
        }

        self.push = Vec2::ZERO;
        match &mut self.hold {
            Some(Hold::Puck) => {
                if let Some(at) = pointer {
                    self.push = clamp_unit((at + self.grab - centre) / geometry.travel);
                    self.puck = self.push;
                }
            }
            Some(Hold::Rim(rim)) => {
                if let Some(at) = pointer {
                    let offset = at - centre;
                    let (turn, grow) = rim.advance(offset);
                    self.turn += turn;
                    self.grow += grow;
                    self.ridges += turn;
                    if rim.axis == Some(RimAxis::Pull) {
                        self.pull = offset.length() - rim.start.length();
                    }
                }
            }
            None => {}
        }
        let spring = (1.0 - dt * SPRING).clamp(0.0, 1.0);
        if !matches!(self.hold, Some(Hold::Puck)) {
            self.puck *= spring;
            if self.puck.length() < 1e-3 {
                self.puck = Vec2::ZERO;
            }
        }
        if !matches!(self.hold, Some(Hold::Rim(_))) {
            self.pull *= spring;
            if self.pull.abs() < 0.1 {
                self.pull = 0.0;
            }
        }

        self.paint(ui, &geometry);

        let hovered = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|at| (at - centre).length() <= geometry.outer + ROOM);
        if self.hold.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if hovered {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        interaction
    }

    fn paint(&self, ui: &Ui, geometry: &Geometry) {
        let tokens = ui.tokens();
        let painter = ui.painter();
        let Geometry {
            centre,
            outer,
            inner,
            travel,
            ..
        } = *geometry;
        let gesture = self.gesture();
        let border = Stroke::new(1.0_f32, tokens.color.border);

        // The well, then the rim band over its outer edge.
        painter.circle_filled(centre, outer, tokens.color.bg_secondary);
        painter.circle_stroke(
            centre,
            (outer + inner) * 0.5,
            Stroke::new(outer - inner, tokens.color.bg_highlight),
        );
        painter.circle_stroke(centre, outer, border);
        painter.circle_stroke(centre, inner, border);

        // The ridges, turning with a sweep. They are left out under the two
        // glyphs so the glyphs stay legible as the ridges pass beneath.
        let sweeping = gesture == Some(DialGesture::Turn);
        let ridge = if sweeping {
            tokens.color.accent.gamma_multiply(0.8)
        } else {
            tokens.color.text_muted.gamma_multiply(0.45)
        };
        for index in 0..RIDGES {
            let angle = self.ridges + index as f32 / RIDGES as f32 * TAU;
            let near_glyph = [-PI * 0.5, PI * 0.5]
                .iter()
                .any(|glyph| shortest_turn(angle - glyph).abs() < 0.32);
            if near_glyph {
                continue;
            }
            let direction = vec2(angle.cos(), angle.sin());
            painter.line_segment(
                [
                    centre + direction * (inner + 9.0),
                    centre + direction * (outer - 9.0),
                ],
                Stroke::new(1.5_f32, ridge),
            );
        }

        // The two glyphs say what the rim does before it is touched: turn at
        // the top, size at the bottom.
        let middle = (outer + inner) * 0.5;
        let glyph = ui.text_size(crate::theme::TextSize::Base);
        let lit = |on: bool| {
            if on {
                tokens.color.accent
            } else {
                tokens.color.text_secondary
            }
        };
        PhosphorIcon::ArrowsClockwise.paint(
            painter,
            centre - vec2(0.0, middle),
            Align2::CENTER_CENTER,
            glyph,
            lit(sweeping),
        );
        PhosphorIcon::ArrowsOut.paint(
            painter,
            centre + vec2(0.0, middle),
            Align2::CENTER_CENTER,
            glyph,
            lit(gesture == Some(DialGesture::Size)),
        );

        // A pull shows as a ring that stretches with the thumb.
        if self.pull != 0.0 {
            let radius = (outer + self.pull).clamp(inner, outer + ROOM);
            painter.circle_stroke(
                centre,
                radius,
                Stroke::new(2.0_f32, tokens.color.accent.gamma_multiply(0.8)),
            );
        }

        // The stick's own furniture: the cross, the dead zone, the puck.
        let guide = tokens.color.text_muted.gamma_multiply(0.3);
        let reach = inner - 4.0;
        painter.line_segment(
            [centre - vec2(reach, 0.0), centre + vec2(reach, 0.0)],
            Stroke::new(1.0_f32, guide),
        );
        painter.line_segment(
            [centre - vec2(0.0, reach), centre + vec2(0.0, reach)],
            Stroke::new(1.0_f32, guide),
        );
        painter.circle_stroke(centre, travel * DEAD_ZONE, Stroke::new(1.0_f32, guide));

        let at = centre + clamp_unit(self.puck) * travel;
        painter.circle_filled(at, PUCK, tokens.color.accent);
        painter.circle_stroke(at, PUCK, Stroke::new(1.0_f32, tokens.color.bg_primary));
    }

    /// Carry this frame's gesture onto `placement`, and clear it.
    ///
    /// The move is a rate, in [`Rates::pan`] units per second, and the centre is
    /// kept inside `space`. The turn and the size are direct, so `rates` does
    /// not gear them and `dt` does not scale them. `fine` gears all three down.
    pub fn apply(
        &mut self,
        placement: &mut Placement,
        space: Rect,
        rates: Rates,
        dt: f32,
        fine: bool,
    ) -> bool {
        let scale = if fine { FINE } else { 1.0 };
        let mut changed = false;

        let held = response(self.push);
        if held != Vec2::ZERO {
            let next = placement.center + held * rates.pan * scale * dt;
            placement.center = pos2(
                next.x.clamp(space.min.x, space.max.x),
                next.y.clamp(space.min.y, space.max.y),
            );
            changed = true;
        }
        if self.turn != 0.0 {
            placement.angle += self.turn * scale;
            changed = true;
        }
        if self.grow != 0.0 {
            placement.size *= (self.grow * scale).exp();
            changed = true;
        }
        self.turn = 0.0;
        self.grow = 0.0;
        changed
    }
}

/// Where the dial's parts sit, from the rect it was given.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    centre: Pos2,
    /// The rim's outer edge.
    outer: f32,
    /// The rim's inner edge: a press inside it takes the puck.
    inner: f32,
    /// How far the puck's centre travels at a full push. Further than the
    /// inner edge: a held puck rides out over the rim.
    travel: f32,
}

impl Geometry {
    fn new(rect: Rect) -> Self {
        let outer = (rect.width() * 0.5 - ROOM).max(PUCK * 3.0);
        let inner = (outer - BAND).max(PUCK * 1.5);
        Self {
            centre: rect.center(),
            outer,
            inner,
            travel: (outer - PUCK - 4.0).max(1.0),
        }
    }
}

/// The bearing of `v`, clockwise on screen (y grows down).
fn bearing(v: Vec2) -> f32 {
    v.y.atan2(v.x)
}

/// The shortest turn between two bearings, in (−π, π].
fn shortest_turn(radians: f32) -> f32 {
    (radians + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rim grab on the right-hand side, at the band's middle.
    fn grab() -> Rim {
        Rim::new(vec2(70.0, 0.0))
    }

    fn at_bearing(radians: f32, radius: f32) -> Vec2 {
        vec2(radians.cos(), radians.sin()) * radius
    }

    #[test]
    fn a_rim_grab_waits_before_it_decides() {
        let mut rim = grab();
        assert_eq!(rim.advance(vec2(72.0, 3.0)), (0.0, 0.0));
        assert_eq!(rim.axis, None);
    }

    #[test]
    fn round_the_ring_is_a_sweep_and_turns_by_the_angle_swept() {
        let mut rim = grab();
        let mut turned = 0.0;
        for step in 1..=10 {
            let (turn, grow) = rim.advance(at_bearing(step as f32 * PI / 20.0, 70.0));
            turned += turn;
            assert_eq!(grow, 0.0);
        }
        assert_eq!(rim.axis, Some(RimAxis::Sweep));
        assert!((turned - PI * 0.5).abs() < 1e-4, "turned {turned}");
        // Held still, it turns no further.
        assert_eq!(rim.advance(at_bearing(PI * 0.5, 70.0)), (0.0, 0.0));
    }

    #[test]
    fn a_sweep_ignores_the_thumb_wandering_in_and_out() {
        let mut rim = grab();
        let mut grown = 0.0;
        for (step, radius) in [(1, 74.0), (2, 62.0), (3, 80.0), (4, 66.0)] {
            grown += rim.advance(at_bearing(step as f32 * 0.2, radius)).1;
        }
        assert_eq!(rim.axis, Some(RimAxis::Sweep));
        assert_eq!(grown, 0.0);
    }

    #[test]
    fn a_sweep_carries_on_past_a_full_turn() {
        // Round more than once, across the ±π seam: a dial never runs out.
        let mut rim = grab();
        let mut turned = 0.0;
        for step in 1..=60 {
            turned += rim.advance(at_bearing(step as f32 * TAU / 40.0, 70.0)).0;
        }
        assert!((turned - 1.5 * TAU).abs() < 1e-3, "turned {turned}");
    }

    #[test]
    fn straight_out_is_a_pull_and_sizes_exponentially() {
        let mut rim = grab();
        let mut grown = 0.0;
        for step in 1..=8 {
            let (turn, grow) = rim.advance(vec2(70.0 + step as f32 * PULL_PER_E / 8.0, 1.0));
            assert_eq!(turn, 0.0);
            grown += grow;
        }
        assert_eq!(rim.axis, Some(RimAxis::Pull));
        assert!((grown - 1.0).abs() < 1e-2, "grew {grown} e-folds");
    }

    #[test]
    fn pulling_out_and_back_in_returns_to_the_start() {
        let mut rim = grab();
        let mut grown = 0.0;
        for x in [80.0, 110.0, 140.0, 100.0, 40.0, 70.0] {
            grown += rim.advance(vec2(x, 0.0)).1;
        }
        assert!(grown.abs() < 1e-5, "net growth {grown}");
    }

    #[test]
    fn inward_shrinks() {
        let mut rim = grab();
        let grow: f32 = [60.0, 40.0]
            .iter()
            .map(|x| rim.advance(vec2(*x, 0.0)).1)
            .sum();
        assert!(grow < 0.0);
    }

    #[test]
    fn apply_takes_a_gesture_once() {
        let mut dial = TransformDial {
            turn: 0.5,
            grow: 1.0,
            ..Default::default()
        };
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut placement = Placement::new(pos2(0.5, 0.5), vec2(0.2, 0.2));
        assert!(dial.apply(&mut placement, space, Rates::default(), 1.0 / 60.0, false));
        assert!((placement.angle - 0.5).abs() < 1e-6);
        assert!((placement.size.x - 0.2 * 1.0_f32.exp()).abs() < 1e-5);

        let settled = placement;
        assert!(!dial.apply(&mut placement, space, Rates::default(), 1.0 / 60.0, false));
        assert_eq!(placement, settled);
    }

    #[test]
    fn the_fine_hand_gears_the_rim_down_too() {
        let mut dial = TransformDial {
            turn: 1.0,
            ..Default::default()
        };
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut placement = Placement::new(pos2(0.5, 0.5), vec2(0.2, 0.2));
        dial.apply(&mut placement, space, Rates::default(), 1.0 / 60.0, true);
        assert!((placement.angle - FINE).abs() < 1e-6);
    }

    #[test]
    fn a_held_puck_reaches_the_rim() {
        let geometry = Geometry::new(Rect::from_min_size(Pos2::ZERO, Vec2::splat(WELL)));
        assert!(geometry.travel > geometry.inner);
        assert!(geometry.travel + PUCK <= geometry.outer);
    }
}
