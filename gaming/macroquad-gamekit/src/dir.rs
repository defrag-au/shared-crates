//! `Dir4` — up, down, left and right: a swipe's direction, a hop, a step on a
//! grid. The widgets crate's `SwipeDir` is left and right only, because a page
//! turns sideways; a game moves four ways.

use macroquad::prelude::{Vec2, vec2};

/// One of four directions, in stage units: y runs down, so `Up` is −y.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir4 {
    Up,
    Down,
    Left,
    Right,
}

impl Dir4 {
    pub const ALL: [Dir4; 4] = [Dir4::Up, Dir4::Down, Dir4::Left, Dir4::Right];

    /// The direction `v` mostly points: its longer axis. `None` for a zero
    /// vector. An exact diagonal reads as horizontal.
    pub fn of(v: Vec2) -> Option<Self> {
        if v == Vec2::ZERO || !v.is_finite() {
            return None;
        }
        Some(if v.x.abs() >= v.y.abs() {
            if v.x < 0.0 { Dir4::Left } else { Dir4::Right }
        } else if v.y < 0.0 {
            Dir4::Up
        } else {
            Dir4::Down
        })
    }

    /// A unit step this way, y down.
    pub fn vec(self) -> Vec2 {
        match self {
            Dir4::Up => vec2(0.0, -1.0),
            Dir4::Down => vec2(0.0, 1.0),
            Dir4::Left => vec2(-1.0, 0.0),
            Dir4::Right => vec2(1.0, 0.0),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Dir4::Up => Dir4::Down,
            Dir4::Down => Dir4::Up,
            Dir4::Left => Dir4::Right,
            Dir4::Right => Dir4::Left,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longer_axis_wins_and_y_runs_down() {
        assert_eq!(Dir4::of(vec2(3.0, -10.0)), Some(Dir4::Up));
        assert_eq!(Dir4::of(vec2(3.0, 10.0)), Some(Dir4::Down));
        assert_eq!(Dir4::of(vec2(-10.0, 3.0)), Some(Dir4::Left));
        assert_eq!(Dir4::of(vec2(10.0, -3.0)), Some(Dir4::Right));
        assert_eq!(
            Dir4::of(vec2(5.0, 5.0)),
            Some(Dir4::Right),
            "a tie is horizontal"
        );
        assert_eq!(Dir4::of(Vec2::ZERO), None);
        assert_eq!(Dir4::of(vec2(f32::NAN, 1.0)), None);
    }

    #[test]
    fn each_direction_round_trips_through_its_vector() {
        for d in Dir4::ALL {
            assert_eq!(Dir4::of(d.vec()), Some(d));
            assert_eq!(d.vec() + d.opposite().vec(), Vec2::ZERO);
        }
    }
}
