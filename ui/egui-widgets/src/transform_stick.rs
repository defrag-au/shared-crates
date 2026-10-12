//! `TransformStick` — a spring-centred stick that pans, sizes and turns a
//! subject that lives somewhere else on screen.
//!
//! The control and the thing it moves are deliberately apart. A subject shown
//! through a projection — a decal on a mug — has a screen shape that is not its
//! value's shape, so a control drawn *on* it can only ever approximate the thing
//! it is showing. A stick takes the other road: a small, fixed affordance that
//! says "push me", and the subject elsewhere, moving.
//!
//! ## Why a rate and not a position
//!
//! A stick that mapped its own deflection straight to a position could only move
//! the subject as far as the stick is wide — the same reach problem, one size
//! smaller. Held deflection is a *rate* instead: the subject keeps travelling for
//! as long as the stick is pushed, so a 150pt control can put a decal anywhere on
//! a surface it could never be dragged across. That is the whole reason a gamepad
//! is the right shape for placing something on a curved body.
//!
//! ## What the caller owns
//!
//! The placement, the space it lives in, and what the rates are in: [`Rates`]
//! says how fast a full push moves the value, in the caller's own units per
//! second. The stick holds only its own deflection and which transform is being
//! driven.
//!
//! The subject is not drawn here at all — that is the point. Draw it wherever it
//! actually appears, and let this be the hand.

use egui::{Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::placement::Placement;
use crate::theme::ThemeExt;

/// What a full push does, in the caller's own units per second.
///
/// A pan rate is in the space's units — a decal's unit square crosses in about
/// two seconds at full push. A turn rate is radians; a size rate is the fraction
/// of the width per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub pan: f32,
    pub size: f32,
    pub turn: f32,
}

impl Default for Rates {
    fn default() -> Self {
        Self {
            pan: 0.55,
            size: 0.7,
            turn: 1.8,
        }
    }
}

/// How much of the travel the fine hand gets, on the fine modifier.
const FINE: f32 = 0.15;

/// The transform the stick is driving. One stick, three jobs — the mode is what
/// makes a single small control compound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StickMode {
    /// Both axes: the subject travels with the push.
    #[default]
    Move,
    /// Up grows, down shrinks.
    Size,
    /// Sideways turns, either way.
    Turn,
    /// Sideways turns *and* up grows, at once — what a second stick drives when
    /// the hand prefers two sticks to a mode row. Not in [`ALL`](Self::ALL): it
    /// is chosen by a layout, not picked from a row.
    Shape,
}

impl StickMode {
    pub const ALL: [StickMode; 3] = [StickMode::Move, StickMode::Size, StickMode::Turn];

    pub fn label(self) -> &'static str {
        match self {
            StickMode::Move => "move",
            StickMode::Size => "size",
            StickMode::Turn => "turn",
            StickMode::Shape => "shape",
        }
    }

    /// One line for the reader, since a stick says nothing about itself.
    pub fn hint(self) -> &'static str {
        match self {
            StickMode::Move => "push to move it — it keeps going while you hold",
            StickMode::Size => "push up to grow, down to shrink",
            StickMode::Turn => "push sideways to turn",
            StickMode::Shape => "sideways turns it, up grows it",
        }
    }
}

/// Below this fraction of the stick's travel nothing happens: a released stick
/// must not drift, and a resting thumb must not creep.
pub const DEAD_ZONE: f32 = 0.12;

/// A push short of the rim is the finest rate, so the last few percent of travel
/// are not the most violent ones. `theme-exempt`: a feel parameter, not a size.
const FINE_FLOOR: f32 = 0.10;

/// The stick's well and puck, in points. `theme-exempt`: pointer affordances,
/// sized like the knobs in `placement`.
const WELL: f32 = 162.0;
const PUCK: f32 = 17.0;

/// Where a push lands, in `−1..=1` on both axes, dead zone removed and curved
/// for a fine hand.
///
/// Split out because it is the whole feel of the thing: the dead zone keeps a
/// release clean, and the square curve makes the middle of the travel the precise
/// part rather than the twitchy part.
pub fn response(push: Vec2) -> Vec2 {
    let length = push.length();
    if length <= DEAD_ZONE {
        return Vec2::ZERO;
    }
    let travel = ((length - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0);
    let curved = FINE_FLOOR + (1.0 - FINE_FLOOR) * travel * travel;
    push / length * curved
}

/// One frame of a rate: `deflection × rate × dt`.
pub fn step(deflection: f32, rate: f32, dt: f32) -> f32 {
    deflection * rate * dt
}

/// A spring-centred stick, and the transform it drives.
#[derive(Default)]
pub struct TransformStick {
    /// What the rate reads, in `−1..=1`. Zero the instant the pointer lets go —
    /// a rate that coasted would overshoot whatever it was aimed at.
    push: Vec2,
    /// Where the puck is drawn, which springs back to the middle after the rate
    /// has already stopped.
    puck: Vec2,
    /// The pointer's offset from the puck when the grab began, so the puck does
    /// not jump under a thumb that lands off-centre.
    grab: Vec2,
    grabbing: bool,
    mode: StickMode,
}

impl TransformStick {
    pub fn mode(&self) -> StickMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: StickMode) {
        self.mode = mode;
    }

    /// Is a rate running? A caller that owns the motion must keep asking for
    /// frames while this is true, or the subject stops with the pointer.
    pub fn engaged(&self) -> bool {
        self.push != Vec2::ZERO
    }

    /// Draw the stick — a well, the dead zone, the puck — and return what the
    /// rate should be driven by this frame, in the same units as a push.
    pub fn show(&mut self, ui: &mut Ui, dt: f32) -> Vec2 {
        let side = WELL.min(ui.available_width());
        let (rect, interaction) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
        let tokens = ui.tokens();
        let painter = ui.painter().clone();
        let centre = rect.center();
        let travel = (side * 0.5 - PUCK - 6.0).max(1.0);

        let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
        if interaction.drag_started() {
            self.grabbing = true;
            self.grab = pointer.map_or(Vec2::ZERO, |at| centre + self.puck * travel - at);
        }
        if interaction.drag_stopped() {
            self.grabbing = false;
        }
        if self.grabbing {
            if let Some(at) = pointer {
                self.push = clamp_unit((at + self.grab - centre) / travel);
                self.puck = self.push;
            }
        } else {
            // Let go and the rate stops dead; only the puck travels home, so a
            // release is a statement and not a drift.
            self.push = Vec2::ZERO;
            self.puck *= (1.0 - dt * 18.0).clamp(0.0, 1.0);
        }

        painter.circle_filled(centre, side * 0.5, tokens.color.bg_secondary);
        painter.circle_stroke(
            centre,
            side * 0.5,
            Stroke::new(1.0_f32, tokens.color.border),
        );
        painter.circle_stroke(
            centre,
            travel * DEAD_ZONE,
            Stroke::new(1.0_f32, tokens.color.border),
        );
        // The cross, so "up" and "sideways" mean something before the push.
        let guide = tokens.color.text_muted.gamma_multiply(0.3);
        painter.line_segment(
            [centre - vec2(travel, 0.0), centre + vec2(travel, 0.0)],
            Stroke::new(1.0_f32, guide),
        );
        painter.line_segment(
            [centre - vec2(0.0, travel), centre + vec2(0.0, travel)],
            Stroke::new(1.0_f32, guide),
        );

        let at = centre + clamp_unit(self.puck) * travel;
        painter.circle_filled(at, PUCK, tokens.color.accent);
        painter.circle_stroke(
            at,
            PUCK,
            Stroke::new(1.0_f32, tokens.color.bg_primary),
        );

        if self.grabbing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if interaction.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        response(self.push)
    }

    /// Move `placement` by this frame's travel, in the caller's own units.
    ///
    /// `space` is the rectangle the value lives in — the centre is kept inside
    /// it, because a subject whose centre has left the surface is not a placement
    /// any more. `dt` is the frame; `fine` asks for the fine hand.
    pub fn apply(
        &self,
        placement: &mut Placement,
        space: Rect,
        rates: Rates,
        dt: f32,
        fine: bool,
    ) -> bool {
        let held = response(self.push);
        if held == Vec2::ZERO {
            return false;
        }
        let scale = if fine { FINE } else { 1.0 };
        match self.mode {
            StickMode::Move => {
                let travel = held * rates.pan * scale * dt;
                let next = placement.center + travel;
                placement.center = pos2(
                    next.x.clamp(space.min.x, space.max.x),
                    next.y.clamp(space.min.y, space.max.y),
                );
                true
            }
            StickMode::Size => {
                // Up grows. The rate is proportional, so a bigger subject grows
                // faster in absolute terms and both ends of the travel feel the
                // same.
                let growth = 1.0 + step(-held.y, rates.size * scale, dt);
                placement.size *= growth.max(0.5);
                true
            }
            StickMode::Turn => {
                placement.angle += step(held.x, rates.turn * scale, dt);
                true
            }
            StickMode::Shape => {
                placement.angle += step(held.x, rates.turn * scale, dt);
                let growth = 1.0 + step(-held.y, rates.size * scale, dt);
                placement.size *= growth.max(0.5);
                true
            }
        }
    }

    /// The mode row: three jobs, one stick.
    pub fn mode_row(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for mode in StickMode::ALL {
                if ui
                    .selectable_label(self.mode == mode, mode.label())
                    .clicked()
                {
                    self.mode = mode;
                }
            }
        });
        ui.weak(self.mode.hint());
    }
}

/// `v` clamped into the unit disc — the puck rides a circle, not a square, so a
/// diagonal push is not a longer one than a straight push.
fn clamp_unit(v: Vec2) -> Vec2 {
    let length = v.length();
    if length <= 1.0 { v } else { v / length }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};

    #[test]
    fn a_released_stick_does_nothing() {
        assert_eq!(response(Vec2::ZERO), Vec2::ZERO);
        assert_eq!(response(vec2(0.05, -0.05)), Vec2::ZERO);
    }

    #[test]
    fn the_middle_of_the_travel_is_the_fine_part() {
        // Half a push is under half the rate, and a full push is exactly it.
        let half = response(vec2(DEAD_ZONE + (1.0 - DEAD_ZONE) * 0.5, 0.0)).length();
        assert!((0.2..0.45).contains(&half), "half push gave {half}");
        let full = response(vec2(1.0, 0.0)).length();
        assert!((full - 1.0).abs() < 1e-4, "full push gave {full}");
    }

    #[test]
    fn the_direction_is_the_pushes_own() {
        let push = response(vec2(0.0, 0.8));
        assert!(push.y > 0.0 && push.x.abs() < 1e-5);
    }

    #[test]
    fn a_rate_is_time_shaped_not_frame_shaped() {
        // One second at full push is the rate, whatever the frames are made of.
        let second = step(1.0, 0.55, 0.5) + step(1.0, 0.55, 0.5);
        assert!((second - 0.55).abs() < 1e-6);
    }

    #[test]
    fn the_puck_rides_a_circle() {
        let out = clamp_unit(vec2(2.0, 2.0));
        assert!((out.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_move_keeps_the_centre_inside_the_space() {
        let mut stick = TransformStick::default();
        stick.push = vec2(1.0, 0.0);
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut placement = Placement {
            center: pos2(0.98, 0.5),
            size: vec2(0.3, 0.3),
            angle: 0.0,
        };
        for _ in 0..30 {
            stick.apply(&mut placement, space, Rates::default(), 1.0 / 30.0, false);
        }
        assert!((placement.center.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn size_up_grows_and_size_down_shrinks() {
        let mut stick = TransformStick::default();
        stick.mode = StickMode::Size;
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

        stick.push = vec2(0.0, -1.0);
        let mut grown = Placement {
            center: pos2(0.5, 0.5),
            size: vec2(0.3, 0.3),
            angle: 0.0,
        };
        stick.apply(&mut grown, space, Rates::default(), 1.0, false);
        assert!(grown.size.x > 0.3, "grew to {}", grown.size.x);

        stick.push = vec2(0.0, 1.0);
        let mut shrunk = grown;
        stick.apply(&mut shrunk, space, Rates::default(), 1.0, false);
        assert!(shrunk.size.x < grown.size.x);
    }

    #[test]
    fn turn_uses_the_sideways_axis() {
        let mut stick = TransformStick::default();
        stick.mode = StickMode::Turn;
        stick.push = vec2(1.0, 0.0);
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut placement = Placement {
            center: pos2(0.5, 0.5),
            size: vec2(0.3, 0.3),
            angle: 0.0,
        };
        stick.apply(&mut placement, space, Rates::default(), 1.0, false);
        assert!((placement.angle - Rates::default().turn).abs() < 1e-5);
    }

    #[test]
    fn the_shape_stick_turns_and_grows_in_one_push() {
        let mut stick = TransformStick::default();
        stick.mode = StickMode::Shape;
        stick.push = vec2(1.0, -1.0);
        let space = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut placement = Placement {
            center: pos2(0.5, 0.5),
            size: vec2(0.3, 0.3),
            angle: 0.0,
        };
        stick.apply(&mut placement, space, Rates::default(), 1.0, false);
        assert!(placement.angle > 0.0, "angle {}", placement.angle);
        assert!(placement.size.x > 0.3, "size {}", placement.size.x);
        // A diagonal push is one push, not two: the same rates, one frame.
        assert!((placement.angle - Rates::default().turn).abs() < 1e-5);
    }
}
