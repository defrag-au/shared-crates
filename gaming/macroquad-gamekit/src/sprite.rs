//! `Sprite` — a retained, animated display object: where a card is, how it is
//! turned and scaled, how visible, and whether a point is on it.
//!
//! macroquad redraws everything every frame and remembers nothing; a card
//! game needs cards that keep their place between frames and glide to new
//! ones. A [`Sprite`] carries an animated [`Pose`] (position of its centre,
//! rotation in radians, uniform scale, alpha) and its unscaled size, so one
//! tween moves all four together.
//!
//! Hit testing is in the sprite's own frame, so a rotated card in a fanned hand
//! is hit where it is drawn, not where its unrotated box would be.
//! [`topmost`] picks the highest sprite under a point when cards overlap.

use macroquad::prelude::{Rect, Vec2, vec2};

use crate::ease::Ease;
use crate::tween::{Animated, Lerp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Centre, in screen space.
    pub pos: Vec2,
    /// Radians, clockwise in screen space (y down).
    pub rotation: f32,
    pub scale: f32,
    pub alpha: f32,
}

impl Pose {
    pub fn at(pos: Vec2) -> Self {
        Self {
            pos,
            rotation: 0.0,
            scale: 1.0,
            alpha: 1.0,
        }
    }

    pub fn rotated(self, rotation: f32) -> Self {
        Self { rotation, ..self }
    }

    pub fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    pub fn faded(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }
}

impl Default for Pose {
    fn default() -> Self {
        Pose::at(Vec2::ZERO)
    }
}

impl Lerp for Pose {
    fn lerp(a: Pose, b: Pose, t: f32) -> Pose {
        Pose {
            pos: Vec2::lerp(a.pos, b.pos, t),
            rotation: f32::lerp(a.rotation, b.rotation, t),
            scale: f32::lerp(a.scale, b.scale, t),
            alpha: f32::lerp(a.alpha, b.alpha, t),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub pose: Animated<Pose>,
    /// Unscaled size.
    pub size: Vec2,
    /// Draw order: higher is on top.
    pub z: i32,
}

impl Sprite {
    pub fn new(size: Vec2, pose: Pose) -> Self {
        Self {
            pose: Animated::new(pose),
            size,
            z: 0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.pose.update(dt);
    }

    /// Glide to `pose`.
    pub fn go(&mut self, pose: Pose, duration: f32, ease: Ease) {
        self.pose.to(pose, duration, ease);
    }

    pub fn settled(&self) -> bool {
        self.pose.settled()
    }

    /// `p` in the sprite's own frame: origin at its centre, unrotated,
    /// unscaled.
    pub fn local(&self, p: Vec2) -> Vec2 {
        let pose = self.pose.get();
        let d = p - pose.pos;
        let (s, c) = (-pose.rotation).sin_cos();
        let r = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
        if pose.scale.abs() < f32::EPSILON {
            return Vec2::splat(f32::INFINITY);
        }
        r / pose.scale
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let l = self.local(p);
        l.x.abs() <= self.size.x / 2.0 && l.y.abs() <= self.size.y / 2.0
    }

    /// The four corners on screen, clockwise from top-left.
    pub fn corners(&self) -> [Vec2; 4] {
        let pose = self.pose.get();
        let h = self.size * pose.scale / 2.0;
        let (s, c) = pose.rotation.sin_cos();
        [
            vec2(-h.x, -h.y),
            vec2(h.x, -h.y),
            vec2(h.x, h.y),
            vec2(-h.x, h.y),
        ]
        .map(|v| pose.pos + vec2(v.x * c - v.y * s, v.x * s + v.y * c))
    }

    /// The unrotated rectangle at the sprite's current scale: for layout, not
    /// for hits.
    pub fn rect(&self) -> Rect {
        let pose = self.pose.get();
        let s = self.size * pose.scale;
        Rect::new(pose.pos.x - s.x / 2.0, pose.pos.y - s.y / 2.0, s.x, s.y)
    }
}

/// The index of the highest sprite (by `z`, then by position in the slice)
/// containing `p`.
pub fn topmost<'a>(sprites: impl IntoIterator<Item = &'a Sprite>, p: Vec2) -> Option<usize> {
    sprites
        .into_iter()
        .enumerate()
        .filter(|(_, s)| s.contains(p))
        .max_by_key(|(i, s)| (s.z, *i))
        .map(|(i, _)| i)
}

/// Indices in draw order: lowest `z` first, ties in slice order.
pub fn draw_order(sprites: &[Sprite]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..sprites.len()).collect();
    idx.sort_by_key(|&i| (sprites[i].z, i));
    idx
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn card() -> Sprite {
        Sprite::new(vec2(100.0, 140.0), Pose::at(vec2(200.0, 200.0)))
    }

    #[test]
    fn an_upright_card_is_hit_inside_its_box() {
        let c = card();
        assert!(c.contains(vec2(200.0, 200.0)));
        assert!(c.contains(vec2(249.0, 269.0)));
        assert!(!c.contains(vec2(251.0, 200.0)));
    }

    #[test]
    fn a_turned_card_is_hit_where_it_is_drawn() {
        let mut c = card();
        c.pose.set(Pose::at(vec2(200.0, 200.0)).rotated(FRAC_PI_2));
        // Turned a quarter: now wide (140) and short (100).
        assert!(c.contains(vec2(265.0, 200.0)));
        assert!(!c.contains(vec2(200.0, 265.0)));
    }

    #[test]
    fn scale_grows_the_hit_area() {
        let mut c = card();
        c.pose.set(Pose::at(vec2(200.0, 200.0)).scaled(2.0));
        assert!(c.contains(vec2(290.0, 200.0)));
    }

    #[test]
    fn corners_follow_rotation() {
        let mut c = card();
        c.pose.set(Pose::at(Vec2::ZERO).rotated(FRAC_PI_2));
        let [tl, ..] = c.corners();
        // Top-left (-50, -70) turned clockwise a quarter lands at (70, -50).
        assert!((tl - vec2(70.0, -50.0)).length() < 1e-3);
    }

    #[test]
    fn the_topmost_overlapping_sprite_wins() {
        let a = card();
        let mut b = card();
        b.z = 1;
        let c = card();
        assert_eq!(topmost([&a, &b, &c], vec2(200.0, 200.0)), Some(1));
        assert_eq!(
            topmost([&a, &c], vec2(200.0, 200.0)),
            Some(1),
            "ties go to the later one"
        );
        assert_eq!(topmost([&a], vec2(0.0, 0.0)), None);
    }

    #[test]
    fn a_pose_tween_moves_everything_together() {
        let mut c = card();
        c.go(
            Pose::at(vec2(300.0, 200.0)).rotated(0.5).scaled(1.2),
            0.2,
            Ease::Linear,
        );
        c.update(0.1);
        let p = c.pose.get();
        assert!((p.pos.x - 250.0).abs() < 1e-3);
        assert!((p.rotation - 0.25).abs() < 1e-4);
        assert!((p.scale - 1.1).abs() < 1e-4);
    }
}
