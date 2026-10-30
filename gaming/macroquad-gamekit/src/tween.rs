//! `Animated` — a value that moves towards a target over time: the tween, held
//! by whatever owns the value.
//!
//! The owner calls [`Animated::update`] with the frame's `dt`, reads
//! [`Animated::get`], and asks [`Animated::settled`] to know when a step of a
//! sequence may proceed. Retargeting mid-flight starts the new tween from where
//! the value is now, so an interrupted move never jumps.
//!
//! ```ignore
//! let mut x = Animated::new(0.0);
//! x.to(100.0, 0.15, Ease::ExpoOut);
//! x.update(dt);
//! draw_at(x.get());
//! ```

use macroquad::prelude::{Color, Rect, Vec2};

use crate::ease::Ease;

/// Interpolation between two values of a type.
pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl Lerp for Vec2 {
    fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
        a + (b - a) * t
    }
}

/// Position and size together: a tile sliding and growing in one tween.
impl Lerp for Rect {
    fn lerp(a: Rect, b: Rect, t: f32) -> Rect {
        Rect::new(
            f32::lerp(a.x, b.x, t),
            f32::lerp(a.y, b.y, t),
            f32::lerp(a.w, b.w, t),
            f32::lerp(a.h, b.h, t),
        )
    }
}

impl Lerp for Color {
    fn lerp(a: Color, b: Color, t: f32) -> Color {
        Color::new(
            f32::lerp(a.r, b.r, t),
            f32::lerp(a.g, b.g, t),
            f32::lerp(a.b, b.b, t),
            f32::lerp(a.a, b.a, t),
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct Tween<T> {
    from: T,
    to: T,
    /// Seconds to wait before moving.
    delay: f32,
    duration: f32,
    ease: Ease,
    elapsed: f32,
}

impl<T: Lerp> Tween<T> {
    fn value(&self) -> T {
        let t = if self.duration <= 0.0 {
            1.0
        } else {
            ((self.elapsed - self.delay) / self.duration).clamp(0.0, 1.0)
        };
        T::lerp(self.from, self.to, self.ease.apply(t))
    }

    fn done(&self) -> bool {
        self.elapsed >= self.delay + self.duration
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Animated<T> {
    value: T,
    tween: Option<Tween<T>>,
}

impl<T: Lerp> Animated<T> {
    pub fn new(value: T) -> Self {
        Self { value, tween: None }
    }

    /// The value this frame.
    pub fn get(&self) -> T {
        self.value
    }

    /// Where it is heading, or where it is if still.
    pub fn target(&self) -> T {
        self.tween.map_or(self.value, |t| t.to)
    }

    /// Jump, cancelling any tween.
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.tween = None;
    }

    /// Move to `to` over `duration` seconds.
    pub fn to(&mut self, to: T, duration: f32, ease: Ease) {
        self.to_after(to, 0.0, duration, ease);
    }

    /// Move to `to`, starting after `delay` seconds: a stagger.
    pub fn to_after(&mut self, to: T, delay: f32, duration: f32, ease: Ease) {
        self.tween = Some(Tween {
            from: self.value,
            to,
            delay: delay.max(0.0),
            duration: duration.max(0.0),
            ease,
            elapsed: 0.0,
        });
        if duration <= 0.0 && delay <= 0.0 {
            self.set(to);
        }
    }

    pub fn update(&mut self, dt: f32) {
        let Some(t) = &mut self.tween else { return };
        t.elapsed += dt.max(0.0);
        self.value = t.value();
        if t.done() {
            self.value = t.to;
            self.tween = None;
        }
    }

    /// No tween running: it is where it is going.
    pub fn settled(&self) -> bool {
        self.tween.is_none()
    }

    /// Seconds until it settles.
    pub fn remaining(&self) -> f32 {
        self.tween
            .map_or(0.0, |t| (t.delay + t.duration - t.elapsed).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_arrives_exactly_on_time_and_then_settles() {
        // Binary-exact times (0.25 s in 1/16 s steps), so float accumulation
        // cannot move the boundary.
        let mut a = Animated::new(0.0f32);
        a.to(10.0, 0.25, Ease::ExpoOut);
        assert!(!a.settled());
        for _ in 0..3 {
            a.update(0.0625);
        }
        assert!(!a.settled(), "not there one step early");
        a.update(0.0625);
        assert_eq!(a.get(), 10.0);
        assert!(a.settled());
    }

    #[test]
    fn a_delay_holds_the_value_still() {
        let mut a = Animated::new(1.0f32);
        a.to_after(2.0, 0.1, 0.1, Ease::Linear);
        a.update(0.05);
        assert_eq!(a.get(), 1.0);
        a.update(0.1);
        assert!((a.get() - 1.5).abs() < 1e-5);
    }

    #[test]
    fn retargeting_mid_flight_starts_from_where_it_is() {
        let mut a = Animated::new(0.0f32);
        a.to(100.0, 1.0, Ease::Linear);
        a.update(0.5);
        let mid = a.get();
        a.to(0.0, 1.0, Ease::Linear);
        assert_eq!(a.get(), mid, "no jump on retarget");
        a.update(0.5);
        assert!((a.get() - mid / 2.0).abs() < 1e-3);
    }

    #[test]
    fn a_zero_duration_move_is_a_jump() {
        let mut a = Animated::new(Vec2::ZERO);
        a.to(Vec2::new(3.0, 4.0), 0.0, Ease::Linear);
        assert_eq!(a.get(), Vec2::new(3.0, 4.0));
        assert!(a.settled());
    }

    #[test]
    fn remaining_counts_down() {
        let mut a = Animated::new(0.0f32);
        a.to_after(1.0, 0.2, 0.3, Ease::Linear);
        assert!((a.remaining() - 0.5).abs() < 1e-6);
        a.update(0.4);
        assert!((a.remaining() - 0.1).abs() < 1e-5);
    }
}
