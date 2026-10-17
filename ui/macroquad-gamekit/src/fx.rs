//! `Shake`, `Popups` and `Particles` — the effects that make a hit land:
//! screen shake that decays, numbers that pop and drift, and bursts of bits.
//!
//! All three are host-owned and stepped by `dt`. None draws from a random
//! source: shake is a sum of sines and particles come from a seeded generator,
//! so a replayed fight shakes the same way and a captured frame is repeatable.

use std::f32::consts::TAU;

use macroquad::prelude::*;

use crate::ease::Ease;
use crate::text::{Align, MacroquadMeasure, RichText, TextStyle};

/// Trauma-based shake: add trauma on a hit, read an offset every frame, and it
/// dies away. The offset grows with trauma squared, so small hits stay small.
#[derive(Clone, Copy, Debug)]
pub struct Shake {
    trauma: f32,
    /// Trauma lost per second.
    pub decay: f32,
    /// Offset at full trauma, in points.
    pub max: f32,
    t: f32,
}

impl Shake {
    pub fn new(max: f32, decay: f32) -> Self {
        Self {
            trauma: 0.0,
            decay,
            max,
            t: 0.0,
        }
    }

    /// Add trauma (0..1; the total is capped at 1).
    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount.max(0.0)).min(1.0);
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        self.trauma = (self.trauma - self.decay * dt).max(0.0);
    }

    pub fn offset(&self) -> Vec2 {
        let k = self.max * self.trauma * self.trauma;
        let t = self.t;
        vec2(
            (t * 47.0).sin() * 0.6 + (t * 89.0 + 1.3).sin() * 0.4,
            (t * 53.0 + 2.1).sin() * 0.6 + (t * 97.0).sin() * 0.4,
        ) * k
    }

    pub fn active(&self) -> bool {
        self.trauma > 0.0
    }
}

/// A number or word that pops out, drifts up and fades.
#[derive(Clone, Debug)]
struct Popup {
    text: String,
    style: TextStyle,
    at: Vec2,
    age: f32,
}

/// Floating text such as damage numbers.
#[derive(Clone, Debug)]
pub struct Popups {
    items: Vec<Popup>,
    /// Seconds each lives.
    pub life: f32,
    /// Points it rises over its life.
    pub rise: f32,
}

/// Seconds a popup takes to pop up to full size.
const POP_IN: f32 = 0.15;

impl Popups {
    pub fn new(life: f32, rise: f32) -> Self {
        Self {
            items: Vec::new(),
            life,
            rise,
        }
    }

    /// Show `text` centred on `at`.
    pub fn spawn(&mut self, text: impl Into<String>, style: TextStyle, at: Vec2) {
        self.items.push(Popup {
            text: text.into(),
            style,
            at,
            age: 0.0,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.items {
            p.age += dt;
        }
        let life = self.life;
        self.items.retain(|p| p.age < life);
    }

    pub fn active(&self) -> bool {
        !self.items.is_empty()
    }

    /// Where a popup of `age` is drawn relative to where it spawned, its scale
    /// and its opacity.
    fn pose(&self, age: f32) -> (f32, f32, f32) {
        let t = (age / self.life).clamp(0.0, 1.0);
        let lift = -self.rise * Ease::ExpoOut.apply(t);
        let scale = Ease::BackOut
            .apply((age / POP_IN).clamp(0.0, 1.0))
            .max(0.01);
        // Hold full, then fade over the last 40% of its life.
        let alpha = if t < 0.6 { 1.0 } else { 1.0 - (t - 0.6) / 0.4 };
        (lift, scale, alpha)
    }

    pub fn draw(&self, pixel_scale: f32) {
        for p in &self.items {
            let (lift, scale, alpha) = self.pose(p.age);
            let mut text = RichText::new();
            let style = text.style(TextStyle {
                size: p.style.size * scale,
                ..p.style.clone()
            });
            text.text(&p.text, style);
            let l = text.layout(&MacroquadMeasure, f32::INFINITY, Align::Start, 0.0);
            let origin = p.at + vec2(-l.size.x / 2.0, lift - l.size.y / 2.0);
            l.draw(&text, origin, pixel_scale, alpha, |_, _| {});
        }
    }
}

/// A small seeded generator: deterministic, no dependency.
#[derive(Clone, Copy, Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    pos: Vec2,
    vel: Vec2,
    age: f32,
    life: f32,
    size: f32,
    color: Color,
}

/// Bursts of small squares thrown outward, falling and fading.
#[derive(Clone, Debug)]
pub struct Particles {
    items: Vec<Particle>,
    rng: Rng,
    /// Points per second squared, downward.
    pub gravity: f32,
}

impl Particles {
    pub fn new(seed: u64, gravity: f32) -> Self {
        Self {
            items: Vec::new(),
            rng: Rng(seed.max(1)),
            gravity,
        }
    }

    /// Throw `count` bits from `at` at up to `speed` points per second, each
    /// `size` points across, living about `life` seconds.
    pub fn burst(
        &mut self,
        at: Vec2,
        count: usize,
        speed: f32,
        size: f32,
        life: f32,
        color: Color,
    ) {
        for _ in 0..count {
            let angle = self.rng.next() * TAU;
            let v = speed * (0.35 + 0.65 * self.rng.next());
            let l = life * (0.6 + 0.4 * self.rng.next());
            self.items.push(Particle {
                pos: at,
                vel: vec2(angle.cos(), angle.sin()) * v,
                age: 0.0,
                life: l,
                size: size * (0.6 + 0.8 * self.rng.next()),
                color,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.items {
            p.age += dt;
            p.vel.y += self.gravity * dt;
            p.pos += p.vel * dt;
        }
        self.items.retain(|p| p.age < p.life);
    }

    pub fn active(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn draw(&self) {
        for p in &self.items {
            let a = 1.0 - p.age / p.life;
            let s = p.size * (0.5 + 0.5 * a);
            draw_rectangle(
                p.pos.x - s / 2.0,
                p.pos.y - s / 2.0,
                s,
                s,
                Color::new(p.color.r, p.color.g, p.color.b, p.color.a * a),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shake_dies_away() {
        let mut s = Shake::new(20.0, 1.5);
        s.add(0.8);
        s.update(0.05);
        assert!(s.offset().length() > 0.0);
        for _ in 0..60 {
            s.update(1.0 / 60.0);
        }
        assert!(!s.active());
        assert_eq!(s.offset(), Vec2::ZERO);
    }

    #[test]
    fn small_hits_shake_much_less_than_big_ones() {
        let mut small = Shake::new(20.0, 0.0);
        let mut big = Shake::new(20.0, 0.0);
        small.add(0.2);
        big.add(1.0);
        // Same clock, so the same direction: only the size differs.
        small.update(0.3);
        big.update(0.3);
        let ratio = small.offset().length() / big.offset().length();
        assert!((ratio - 0.04).abs() < 1e-4, "trauma squared: 0.2² = 0.04");
    }

    #[test]
    fn a_popup_pops_rises_fades_and_goes() {
        let p = Popups::new(1.0, 40.0);
        let (lift0, scale0, a0) = p.pose(0.0);
        assert_eq!(lift0, 0.0);
        assert!(scale0 < 0.1);
        assert_eq!(a0, 1.0);
        let (_, scale_mid, _) = p.pose(POP_IN);
        assert!((scale_mid - 1.0).abs() < 1e-5);
        let (lift_end, _, a_end) = p.pose(1.0);
        assert!((lift_end + 40.0).abs() < 1e-3);
        assert!(a_end.abs() < 1e-6);
    }

    #[test]
    fn particles_are_repeatable_and_burn_out() {
        let run = || {
            let mut p = Particles::new(7, 400.0);
            p.burst(Vec2::ZERO, 20, 200.0, 4.0, 0.5, WHITE);
            p.update(0.1);
            p.items.iter().map(|q| q.pos).collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "same seed, same burst");
        let mut p = Particles::new(7, 400.0);
        p.burst(Vec2::ZERO, 20, 200.0, 4.0, 0.5, WHITE);
        assert_eq!(p.count(), 20);
        for _ in 0..60 {
            p.update(1.0 / 60.0);
        }
        assert!(!p.active());
    }
}
