//! `TransformDial` — one control with no modes: the puck moves the subject and
//! the rim around it shapes it.
//!
//! [`TransformStick`](crate::transform_stick::TransformStick) needs a mode row
//! because one stick has to do three jobs. That row is the part of it a reader
//! has to learn, and it costs a line of chrome and a hint underneath. The dial
//! puts the three jobs in two places on one control instead:
//!
//! - **The puck** is a handle. Drag it and the subject moves with it, directly,
//!   by a distance geared to the space ([`SLIDE_ACROSS`] points of drag cross
//!   the whole of it). Push it out past the rim and it becomes a stick: the
//!   subject keeps travelling at a rate for as long as it is held there, faster
//!   the further past, so reach is never limited by the screen or a trackpad.
//!   Short, exact moves are direct, and the joystick is the overflow.
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
//! ## Keys, while held
//!
//! A keyboard can re-aim the puck on the fly, like a gamepad's bumpers. Every
//! key is live only while it is held, so letting go is always the way back
//! to moving:
//!
//! - **R**: dragging the puck sideways turns, and the vertical is ignored.
//! - **S**: dragging the puck up grows and down shrinks, and the sideways is
//!   ignored.
//! - **R and S together**: the puck does both, sideways turning and up
//!   growing.
//! - **Shift**: the fine hand, on everything, the rim included.
//!
//! With a key held the puck stops being a rate and becomes a handle, geared
//! exactly like the rim. A sideways drag turns by what the same distance swept
//! round the rim would, and a vertical drag sizes by what the same distance
//! pulled would. A rate can't match a direct gesture at any setting — a flick
//! of the thumb is far faster than any comfortable rate — so the keys share
//! the rim's gesture rather than imitating its speed. Moving stays a rate,
//! because reach is the puck's whole job.
//!
//! Pressing or releasing a key mid-drag changes what the drag does from the
//! next frame without letting go of the puck. The keys are ignored while a
//! text field has the keyboard, so typing a value can't drive the dial, and
//! while Ctrl, Cmd or Alt is down, so Cmd-R and Cmd-S keep their usual
//! meanings.
//!
//! As with the stick, the caller owns the placement and its space, and the
//! subject is drawn wherever it actually appears.

use std::f32::consts::{PI, TAU};

use egui::{Align2, CursorIcon, Key, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::icons::PhosphorIcon;
use crate::placement::Placement;
use crate::theme::ThemeExt;
use crate::transform_stick::{FINE, Rates, clamp_unit};

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

/// Points of puck drag that move the subject across the whole of its space.
/// Finer than dragging the subject itself, which is the dial's job; Shift
/// gears it finer still. `theme-exempt`: a feel parameter.
pub const SLIDE_ACROSS: f32 = 400.0;

/// How far past the rim, as a fraction of the puck's travel, a push reaches
/// the full move rate. `theme-exempt`: a feel parameter.
const EDGE: f32 = 0.5;

/// How many ridges the rim carries. They turn with a sweep, which is what makes
/// the rim read as something that turns. `theme-exempt`: drawing detail.
const RIDGES: usize = 28;

/// Spring rate for the puck and the rim returning home, per second.
const SPRING: f32 = 18.0;

/// One line for the reader, for a caption or a first-run hint.
pub const HINT: &str =
    "drag the puck to move, past the rim to keep going · sweep the rim to turn · pull it to size";

/// The keys, in one line, for a host with a keyboard to say so.
pub const KEYS_HINT: &str = "hold R to turn, S to size, shift for fine";

/// What the puck drives. [`Move`](Self::Move) unless a key is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PuckMode {
    #[default]
    Move,
    /// R: sideways turns.
    Turn,
    /// S: up grows, down shrinks.
    Size,
    /// R and S: sideways turns and up grows, together.
    Shape,
}

impl PuckMode {
    /// The mode the held keys ask for.
    pub fn from_keys(turn: bool, size: bool) -> Self {
        match (turn, size) {
            (false, false) => PuckMode::Move,
            (true, false) => PuckMode::Turn,
            (false, true) => PuckMode::Size,
            (true, true) => PuckMode::Shape,
        }
    }

    /// A push with only the axes this mode reads. The puck is drawn from this
    /// too, so with a key held it rides the one line that does anything.
    pub fn constrain(self, push: Vec2) -> Vec2 {
        match self {
            PuckMode::Move | PuckMode::Shape => push,
            PuckMode::Turn => vec2(push.x, 0.0),
            PuckMode::Size => vec2(0.0, push.y),
        }
    }
}

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
    /// The puck, with R and S both held.
    Shape,
}

impl DialGesture {
    pub fn label(self) -> &'static str {
        match self {
            DialGesture::Move => "moving",
            DialGesture::Rim => "sweep to turn, pull to size",
            DialGesture::Turn => "turning",
            DialGesture::Size => "sizing",
            DialGesture::Shape => "turning and sizing",
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
    /// The puck, and where the pointer was last frame — what a keyed drag
    /// measures its step from.
    Puck {
        last: Pos2,
    },
    Rim(Rim),
}

/// A puck that moves and a rim that turns and sizes, on one control.
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
    /// Turn, growth and direct slide (in points) gathered since the last
    /// [`apply`](Self::apply).
    turn: f32,
    grow: f32,
    slide: Vec2,
    /// The rim's drawn turn: its ridges follow the thumb.
    ridges: f32,
    /// The rim's drawn pull, in points, springing home after release.
    pull: f32,
    /// What the puck drives this frame, from the keys held.
    mode: PuckMode,
    /// Shift is held: the fine hand, on top of whatever the host asks for.
    shift: bool,
    /// The rim's middle radius as last drawn, in points — what a sweep's
    /// gearing is, and so what a held R is geared to.
    rim: f32,
}

impl Default for TransformDial {
    fn default() -> Self {
        Self {
            push: Vec2::ZERO,
            puck: Vec2::ZERO,
            grab: Vec2::ZERO,
            hold: None,
            turn: 0.0,
            grow: 0.0,
            slide: Vec2::ZERO,
            ridges: 0.0,
            pull: 0.0,
            mode: PuckMode::Move,
            shift: false,
            // The rim at the dial's own size, until a draw says otherwise.
            rim: Geometry::new(Rect::from_min_size(Pos2::ZERO, Vec2::splat(WELL))).middle(),
        }
    }
}

/// What a keyed puck drag of `delta` points does: the turn in radians and the
/// growth in factors of *e*. Geared as the rim is — a sweep of `rim` points
/// is one radian, a pull of [`PULL_PER_E`] is one *e* — so the same distance
/// by either hand is the same change.
fn keyed_step(mode: PuckMode, delta: Vec2, rim: f32) -> (f32, f32) {
    let turns = matches!(mode, PuckMode::Turn | PuckMode::Shape);
    let sizes = matches!(mode, PuckMode::Size | PuckMode::Shape);
    (
        if turns { delta.x / rim.max(1.0) } else { 0.0 },
        // Up grows: the screen's y runs down.
        if sizes { -delta.y / PULL_PER_E } else { 0.0 },
    )
}

impl TransformDial {
    /// What the puck drives this frame — for a host that wants to show it.
    pub fn puck_mode(&self) -> PuckMode {
        self.mode
    }

    /// Is the fine hand on from the keyboard?
    pub fn fine_held(&self) -> bool {
        self.shift
    }

    /// Is a move rate running — the puck pushed past the rim? While it is,
    /// the caller must keep asking for frames, or the subject stops with the
    /// pointer.
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
            Hold::Puck { .. } => Some(match self.mode {
                PuckMode::Move => DialGesture::Move,
                PuckMode::Turn => DialGesture::Turn,
                PuckMode::Size => DialGesture::Size,
                PuckMode::Shape => DialGesture::Shape,
            }),
            Hold::Rim(rim) => Some(match rim.axis {
                None => DialGesture::Rim,
                Some(RimAxis::Sweep) => DialGesture::Turn,
                Some(RimAxis::Pull) => DialGesture::Size,
            }),
        }
    }

    /// The held puck, with the pointer at `at`. Every mode is the pointer's
    /// own step since last frame: with no key held it slides the subject, and
    /// with R or S held it turns or sizes, geared as the rim is and drawn as
    /// the rim draws it. The one rate is a move with the puck pushed out past
    /// the rim.
    fn track_puck(&mut self, at: Pos2, centre: Pos2, travel: f32) {
        let Some(Hold::Puck { last }) = &mut self.hold else {
            return;
        };
        let delta = at - *last;
        *last = at;
        let raw = (at + self.grab - centre) / travel;
        // The puck rides only the axis the held key reads.
        self.puck = self.mode.constrain(clamp_unit(raw));
        if self.mode == PuckMode::Move {
            let reach = raw.length();
            if reach <= 1.0 {
                self.slide += delta;
            } else {
                // Past the rim: a stick, at a rate that grows with how far.
                self.push = raw / reach * ((reach - 1.0) / EDGE).min(1.0);
            }
            return;
        }
        let (turn, grow) = keyed_step(self.mode, delta, self.rim);
        self.turn += turn;
        self.grow += grow;
        self.ridges += turn;
        if grow != 0.0 {
            // Points outward, as a pull's own are.
            self.pull = (self.pull - delta.y).clamp(-BAND, ROOM);
        }
    }

    /// Draw the dial and take this frame's gesture. Call [`apply`](Self::apply)
    /// after it, once a frame, to carry the gesture onto a placement.
    pub fn show(&mut self, ui: &mut Ui, dt: f32) -> egui::Response {
        let side = WELL.min(ui.available_width());
        let (rect, interaction) = ui.allocate_exact_size(Vec2::splat(side), Sense::drag());
        let geometry = Geometry::new(rect);
        let centre = geometry.centre;
        self.rim = geometry.middle();
        let pointer = ui.ctx().input(|input| input.pointer.interact_pos());

        // The keys, read fresh each frame so a press or a release lands
        // mid-push. A text field with the keyboard, or a chord, is not ours.
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (turn_key, size_key, shift) = ui.ctx().input(|input| {
            let chord = input.modifiers.ctrl || input.modifiers.command || input.modifiers.alt;
            let ours = !typing && !chord;
            (
                ours && input.key_down(Key::R),
                ours && input.key_down(Key::S),
                input.modifiers.shift,
            )
        });
        self.mode = PuckMode::from_keys(turn_key, size_key);
        self.shift = shift;

        if interaction.drag_started() {
            let press = ui
                .ctx()
                .input(|input| input.pointer.press_origin())
                .or(pointer);
            self.hold = press.and_then(|at| {
                let offset = at - centre;
                if offset.length() < geometry.inner {
                    self.grab = centre + self.puck * geometry.travel - at;
                    Some(Hold::Puck { last: at })
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
            Some(Hold::Puck { .. }) => {
                if let Some(at) = pointer {
                    self.track_puck(at, centre, geometry.travel);
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
        if !matches!(self.hold, Some(Hold::Puck { .. })) {
            self.puck *= spring;
            if self.puck.length() < 1e-3 {
                self.puck = Vec2::ZERO;
            }
        }
        // The ring holds its stretch while something is sizing — a pulled rim,
        // or a held puck with S down — and springs home otherwise.
        let keyed_sizing = matches!(self.hold, Some(Hold::Puck { .. }))
            && matches!(self.mode, PuckMode::Size | PuckMode::Shape);
        if !matches!(self.hold, Some(Hold::Rim(_))) && !keyed_sizing {
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
        // What is lit is what is changing, whichever hand is changing it: a
        // swept rim and a puck with R held light the same ridges, and a
        // pulled rim and a puck with S held stretch the same ring.
        let turning = matches!(gesture, Some(DialGesture::Turn | DialGesture::Shape));
        let sizing = matches!(gesture, Some(DialGesture::Size | DialGesture::Shape));

        // The well, then the rim band over its outer edge.
        painter.circle_filled(centre, outer, tokens.color.bg_secondary);
        painter.circle_stroke(
            centre,
            (outer + inner) * 0.5,
            Stroke::new(outer - inner, tokens.color.bg_highlight),
        );
        painter.circle_stroke(centre, outer, border);
        painter.circle_stroke(centre, inner, border);

        // The ridges, turning with a turn — the thumb's own on a sweep, the
        // rate's on a held R. They are left out under the two glyphs so the
        // glyphs stay legible as the ridges pass beneath.
        let ridge = if turning {
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
        let middle = geometry.middle();
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
            lit(turning),
        );
        PhosphorIcon::ArrowsOut.paint(
            painter,
            centre + vec2(0.0, middle),
            Align2::CENTER_CENTER,
            glyph,
            lit(sizing),
        );

        // A size shows as a ring that stretches — with the thumb on a pull,
        // and with the rate on a held S.
        if self.pull != 0.0 {
            let radius = (outer + self.pull).clamp(inner, outer + ROOM);
            painter.circle_stroke(
                centre,
                radius,
                Stroke::new(2.0_f32, tokens.color.accent.gamma_multiply(0.8)),
            );
        }

        // The handle's own furniture: the cross and the puck. With a key held
        // the puck rides only the axis that key reads, which is the drag being
        // honest about itself rather than a second signal.
        let guide = Stroke::new(1.0_f32, tokens.color.text_muted.gamma_multiply(0.3));
        let reach = inner - 4.0;
        painter.line_segment(
            [centre - vec2(reach, 0.0), centre + vec2(reach, 0.0)],
            guide,
        );
        painter.line_segment(
            [centre - vec2(0.0, reach), centre + vec2(0.0, reach)],
            guide,
        );

        // Pushed past the rim, the puck is a stick: the stretch of rim it is
        // pushing against lights, brighter the faster it runs.
        if self.push != Vec2::ZERO {
            let heading = bearing(self.push);
            let arc: Vec<Pos2> = (0..=12)
                .map(|step| {
                    let angle = heading + (step as f32 / 12.0 - 0.5) * 0.7;
                    centre + vec2(angle.cos(), angle.sin()) * outer
                })
                .collect();
            painter.add(egui::Shape::line(
                arc,
                Stroke::new(
                    3.0_f32,
                    tokens
                        .color
                        .accent
                        .gamma_multiply(0.4 + 0.6 * self.push.length()),
                ),
            ));
        }

        let at = centre + clamp_unit(self.puck) * travel;
        painter.circle_filled(at, PUCK, tokens.color.accent);
        painter.circle_stroke(at, PUCK, Stroke::new(1.0_f32, tokens.color.bg_primary));
    }

    /// Carry this frame's gesture onto `placement`, and clear it.
    ///
    /// A puck drag moves the subject directly: [`SLIDE_ACROSS`] points cross
    /// the larger side of `space`, whatever its units. Pushed past the rim, it
    /// moves at up to [`Rates::pan`] units per second instead. Either way the
    /// centre is kept inside `space`. Turning and sizing — by the rim, or by
    /// the puck with R or S held — are direct too, so `Rates::turn` and
    /// `Rates::size` are not read. `fine`, or Shift held, gears everything down
    /// by the same factor.
    pub fn apply(
        &mut self,
        placement: &mut Placement,
        space: Rect,
        rates: Rates,
        dt: f32,
        fine: bool,
    ) -> bool {
        let scale = if fine || self.shift { FINE } else { 1.0 };
        let mut changed = false;

        let span = space.width().max(space.height());
        let travel = self.slide * (span / SLIDE_ACROSS) + self.push * rates.pan * dt;
        if travel != Vec2::ZERO {
            let next = placement.center + travel * scale;
            placement.center = pos2(
                next.x.clamp(space.min.x, space.max.x),
                next.y.clamp(space.min.y, space.max.y),
            );
            changed = true;
        }
        self.slide = Vec2::ZERO;
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

    /// The rim band's middle radius — where a thumb on the rim sits, and so
    /// the radius a sweep is geared at.
    fn middle(&self) -> f32 {
        (self.outer + self.inner) * 0.5
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

    fn space() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
    }

    /// A dial holding its puck, pressed at its centre, with `mode` held.
    fn holding(mode: PuckMode) -> TransformDial {
        TransformDial {
            hold: Some(Hold::Puck { last: Pos2::ZERO }),
            mode,
            ..Default::default()
        }
    }

    /// Drag the held puck from the centre by `to`, a frame at a time, applying
    /// each frame as a host would.
    fn drag(dial: &mut TransformDial, placement: &mut Placement, to: Vec2, fine: bool) {
        let steps = 50;
        for step in 1..=steps {
            let at = Pos2::ZERO + to * (step as f32 / steps as f32);
            dial.track_puck(at, Pos2::ZERO, 60.0);
            dial.apply(placement, space(), Rates::default(), 1.0 / 60.0, fine);
        }
    }

    fn start() -> Placement {
        Placement::new(pos2(0.5, 0.5), vec2(0.2, 0.1))
    }

    #[test]
    fn the_keys_pick_the_mode() {
        assert_eq!(PuckMode::from_keys(false, false), PuckMode::Move);
        assert_eq!(PuckMode::from_keys(true, false), PuckMode::Turn);
        assert_eq!(PuckMode::from_keys(false, true), PuckMode::Size);
        assert_eq!(PuckMode::from_keys(true, true), PuckMode::Shape);
    }

    #[test]
    fn a_held_key_keeps_only_its_axis() {
        let push = vec2(0.6, -0.7);
        assert_eq!(PuckMode::Move.constrain(push), push);
        assert_eq!(PuckMode::Shape.constrain(push), push);
        assert_eq!(PuckMode::Turn.constrain(push), vec2(0.6, 0.0));
        assert_eq!(PuckMode::Size.constrain(push), vec2(0.0, -0.7));
    }

    /// The feel this exists for: the same distance by either hand is the same
    /// change. A sideways drag with R held turns exactly as far as the same
    /// distance swept round the rim, and a drag up with S held sizes exactly
    /// as much as the same distance pulled outward.
    #[test]
    fn a_keyed_drag_is_geared_exactly_like_the_rim() {
        let travel = 90.0;
        let rim = TransformDial::default().rim;
        let steps = 200;

        // The rim, swept `travel` points round its middle, and pulled
        // `travel` points outward.
        let mut sweep = Rim::new(vec2(rim, 0.0));
        let swept: f32 = (1..=steps)
            .map(|step| {
                sweep
                    .advance(at_bearing(step as f32 / steps as f32 * travel / rim, rim))
                    .0
            })
            .sum();
        let mut pull = Rim::new(vec2(rim, 0.0));
        let pulled: f32 = (1..=steps)
            .map(|step| {
                pull.advance(vec2(rim + step as f32 / steps as f32 * travel, 0.0))
                    .1
            })
            .sum();

        // The puck, dragged the same distances with R and with S.
        let mut turned = start();
        drag(
            &mut holding(PuckMode::Turn),
            &mut turned,
            vec2(travel, 0.0),
            false,
        );
        let mut sized = start();
        drag(
            &mut holding(PuckMode::Size),
            &mut sized,
            vec2(0.0, -travel),
            false,
        );

        assert!(
            (turned.angle - swept).abs() < 1e-3,
            "R turned {} where the rim turned {swept}",
            turned.angle
        );
        let grown = (sized.size.x / start().size.x).ln();
        assert!(
            (grown - pulled).abs() < 1e-3,
            "S grew {grown} where the rim grew {pulled}"
        );
    }

    #[test]
    fn a_keyed_turn_follows_the_rim_as_drawn() {
        // A dial squeezed smaller has a tighter rim, so a sweep turns more per
        // point — and a held R keeps step with it.
        let mut roomy = TransformDial {
            rim: 80.0,
            ..holding(PuckMode::Turn)
        };
        let mut tight = TransformDial {
            rim: 40.0,
            ..holding(PuckMode::Turn)
        };
        let (mut a, mut b) = (start(), start());
        drag(&mut roomy, &mut a, vec2(40.0, 0.0), false);
        drag(&mut tight, &mut b, vec2(40.0, 0.0), false);
        assert!((b.angle - 2.0 * a.angle).abs() < 1e-5);
    }

    #[test]
    fn a_held_key_runs_no_rate() {
        // Held still after a drag, a keyed puck changes nothing: it is a
        // handle, not a stick.
        let mut dial = holding(PuckMode::Turn);
        let mut placement = start();
        drag(&mut dial, &mut placement, vec2(30.0, 0.0), false);
        assert!(!dial.engaged());
        let settled = placement;
        for _ in 0..30 {
            dial.track_puck(pos2(30.0, 0.0), Pos2::ZERO, 60.0);
            dial.apply(&mut placement, space(), Rates::default(), 1.0 / 60.0, false);
        }
        assert_eq!(placement, settled);
    }

    #[test]
    fn a_puck_drag_moves_the_subject_directly() {
        // 40 points of drag, inside the well, in a unit space: 40 / 400 of it.
        let mut placement = start();
        let mut dial = holding(PuckMode::Move);
        drag(&mut dial, &mut placement, vec2(40.0, -20.0), false);
        let moved = placement.center - start().center;
        // f32 steps added onto 0.5: a few ulps of drift over fifty frames.
        assert!(
            (moved.x - 40.0 / SLIDE_ACROSS).abs() < 1e-5,
            "moved {moved:?}"
        );
        assert!(
            (moved.y + 20.0 / SLIDE_ACROSS).abs() < 1e-5,
            "moved {moved:?}"
        );
        assert_eq!(placement.angle, 0.0);
        assert_eq!(placement.size, start().size);

        // And held still there, it stays: inside the well it's a handle.
        assert!(!dial.engaged());
        let settled = placement;
        for _ in 0..30 {
            dial.track_puck(pos2(40.0, -20.0), Pos2::ZERO, 60.0);
            dial.apply(&mut placement, space(), Rates::default(), 1.0 / 60.0, false);
        }
        assert_eq!(placement, settled);
    }

    #[test]
    fn a_slide_is_geared_to_the_space_not_its_units() {
        // The same drag crosses the same share of a space a thousand times
        // larger.
        let mut dial = holding(PuckMode::Move);
        dial.track_puck(pos2(40.0, 0.0), Pos2::ZERO, 60.0);
        let big = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 1000.0));
        let mut placement = Placement::new(pos2(500.0, 500.0), vec2(100.0, 100.0));
        dial.apply(&mut placement, big, Rates::default(), 1.0 / 60.0, false);
        assert!((placement.center.x - (500.0 + 1000.0 * 40.0 / SLIDE_ACROSS)).abs() < 1e-3);
    }

    #[test]
    fn past_the_rim_the_puck_is_a_stick() {
        // Pushed half a travel past the rim: the full rate, held still or not.
        let mut dial = holding(PuckMode::Move);
        dial.track_puck(pos2(90.0, 0.0), Pos2::ZERO, 60.0);
        assert!(dial.engaged());
        let mut placement = Placement::new(pos2(0.2, 0.5), vec2(0.1, 0.1));
        let before = placement.center.x;
        for _ in 0..10 {
            dial.track_puck(pos2(90.0, 0.0), Pos2::ZERO, 60.0);
            dial.apply(&mut placement, space(), Rates::default(), 0.1, false);
        }
        // One second at the full pan rate. The step that carried the pointer
        // out there landed past the rim, so it slid nothing: outside the well
        // is the rate's, and only the rate's.
        let ran = placement.center.x - before;
        assert!(
            (ran - Rates::default().pan).abs() < 1e-3,
            "ran {ran} against the pan rate"
        );
    }

    #[test]
    fn the_rate_past_the_rim_grows_with_how_far() {
        let rate = |x: f32| {
            let mut dial = holding(PuckMode::Move);
            dial.hold = Some(Hold::Puck { last: pos2(x, 0.0) });
            dial.track_puck(pos2(x, 0.0), Pos2::ZERO, 60.0);
            dial.push.length()
        };
        assert_eq!(rate(60.0), 0.0, "at the rim, still a handle");
        assert!(rate(66.0) > 0.0);
        assert!(rate(66.0) < rate(78.0));
        assert_eq!(rate(90.0), 1.0);
        assert_eq!(rate(200.0), 1.0, "and no faster than the full rate");
    }

    #[test]
    fn shift_gears_a_slide_down() {
        let (mut coarse, mut fine) = (start(), start());
        drag(
            &mut holding(PuckMode::Move),
            &mut coarse,
            vec2(40.0, 0.0),
            false,
        );
        let mut dial = TransformDial {
            shift: true,
            ..holding(PuckMode::Move)
        };
        drag(&mut dial, &mut fine, vec2(40.0, 0.0), false);
        let a = coarse.center.x - start().center.x;
        let b = fine.center.x - start().center.x;
        assert!((b - a * FINE).abs() < 1e-6);
    }

    /// One look for one change: a held key moves the rim's ridges and ring
    /// exactly as the rim itself would, rather than drawing something of its
    /// own.
    #[test]
    fn keys_draw_what_the_rim_draws() {
        // R: the ridges turn by the turn applied, and the ring stays put.
        let mut dial = holding(PuckMode::Turn);
        let mut placement = start();
        drag(&mut dial, &mut placement, vec2(30.0, 0.0), false);
        assert!(dial.ridges > 0.0);
        assert!((dial.ridges - placement.angle).abs() < 1e-5);
        assert_eq!(dial.pull, 0.0, "a turn does not stretch the ring");

        // S, dragged up: the ring stretches outward, as a pull outward does.
        let mut dial = holding(PuckMode::Size);
        drag(&mut dial, &mut start(), vec2(0.0, -6.0), false);
        assert!((dial.pull - 6.0).abs() < 1e-4, "pull {}", dial.pull);
        assert_eq!(dial.ridges, 0.0, "a size does not turn the ridges");

        // And down, inward.
        let mut dial = holding(PuckMode::Size);
        drag(&mut dial, &mut start(), vec2(0.0, 6.0), false);
        assert!(dial.pull < 0.0);
    }

    #[test]
    fn r_turns_and_nothing_else() {
        let mut placement = start();
        drag(
            &mut holding(PuckMode::Turn),
            &mut placement,
            vec2(30.0, -40.0),
            false,
        );
        let rim = TransformDial::default().rim;
        assert!(
            (placement.angle - 30.0 / rim).abs() < 1e-5,
            "the vertical is ignored"
        );
        assert_eq!(placement.center, start().center, "a turn does not move it");
        assert_eq!(placement.size, start().size, "and does not size it");
    }

    #[test]
    fn s_sizes_and_nothing_else() {
        // Up grows, exponentially, and the sideways is ignored.
        let mut placement = start();
        drag(
            &mut holding(PuckMode::Size),
            &mut placement,
            vec2(25.0, -PULL_PER_E),
            false,
        );
        let expected = start().size.x * 1.0_f32.exp();
        assert!(
            (placement.size.x - expected).abs() < 1e-4,
            "size {}",
            placement.size.x
        );
        assert!((placement.size.y / placement.size.x - 0.5).abs() < 1e-5);
        assert_eq!(placement.center, start().center);
        assert_eq!(placement.angle, 0.0);

        // Down shrinks.
        let mut shrunk = start();
        drag(
            &mut holding(PuckMode::Size),
            &mut shrunk,
            vec2(0.0, 20.0),
            false,
        );
        assert!(shrunk.size.x < start().size.x);
    }

    #[test]
    fn r_and_s_together_turn_and_size_and_do_not_move() {
        let mut placement = start();
        drag(
            &mut holding(PuckMode::Shape),
            &mut placement,
            vec2(30.0, -30.0),
            false,
        );
        assert!(placement.angle > 0.0);
        assert!(placement.size.x > start().size.x);
        assert_eq!(placement.center, start().center);
    }

    #[test]
    fn shift_gears_a_keyed_drag_down_and_not_its_ridges() {
        // As on the rim, where the ridges follow the thumb whatever the gearing.
        let mut coarse = holding(PuckMode::Turn);
        let mut fine = TransformDial {
            shift: true,
            ..holding(PuckMode::Turn)
        };
        let (mut a, mut b) = (start(), start());
        drag(&mut coarse, &mut a, vec2(30.0, 0.0), false);
        drag(&mut fine, &mut b, vec2(30.0, 0.0), false);
        assert!((b.angle - a.angle * FINE).abs() < 1e-6);
        assert_eq!(coarse.ridges, fine.ridges);
    }

    #[test]
    fn a_held_puck_reaches_the_rim() {
        let geometry = Geometry::new(Rect::from_min_size(Pos2::ZERO, Vec2::splat(WELL)));
        assert!(geometry.travel > geometry.inner);
        assert!(geometry.travel + PUCK <= geometry.outer);
    }
}
