//! `Toasts` — short messages about what just happened, stacked above the
//! navigation, each leaving on its own.
//!
//! A plain toast says one thing and goes after a few seconds. A keyed toast
//! stands for a job ("naming wallets 12/40") that the app updates as it goes
//! and settles with [`Toasts::resolve`]: it stays while the job runs, then
//! says how it ended and goes. A tap dismisses any toast.
//!
//! Time is a parameter ([`Toasts::update`]), so a capture shows a toast at an
//! exact point in its entrance.

use std::collections::VecDeque;

use macroquad::prelude::*;
use macroquad_gamekit::ease::Ease;
use macroquad_widgets::{Align, Painter, fill_rounded, theme::with_alpha};
use ui_theme::TextSize;

/// How a toast's news went, which is the colour of its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Error,
}

/// One toast.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    /// A job's key, while it runs: [`Toasts::resolve`] finds it by this.
    pub key: Option<String>,
    /// A job's progress, 0 to 1, drawn as a bar along the foot.
    pub progress: Option<f32>,
    /// Seconds shown.
    age: f32,
    /// Seconds to show, from now; `None` while a job runs.
    ttl: Option<f32>,
    /// Seconds into leaving, once it has begun to.
    leaving: Option<f32>,
}

/// How long a plain toast stays.
pub const TOAST_SECS: f32 = 3.5;
const ENTER_SECS: f32 = 0.22;
const LEAVE_SECS: f32 = 0.18;
/// The most on screen at once: older ones leave to make room.
const MOST: usize = 3;
const TOAST_H: f32 = 48.0;
const GAP: f32 = 8.0;

/// See the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct Toasts {
    items: VecDeque<Toast>,
}

impl Toasts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Say `text`, for a few seconds.
    pub fn push(&mut self, text: impl Into<String>, tone: Tone) {
        self.add(Toast {
            text: text.into(),
            tone,
            key: None,
            progress: None,
            age: 0.0,
            ttl: Some(TOAST_SECS),
            leaving: None,
        });
    }

    /// Show or update the job `key`: it stays until resolved.
    pub fn job(&mut self, key: &str, text: impl Into<String>, progress: Option<f32>) {
        let text = text.into();
        if let Some(t) = self.find(key) {
            t.text = text;
            t.progress = progress;
            return;
        }
        self.add(Toast {
            text,
            tone: Tone::Info,
            key: Some(key.to_string()),
            progress,
            age: 0.0,
            ttl: None,
            leaving: None,
        });
    }

    /// Settle the job `key`: say how it ended, then go. A job no longer on
    /// screen is said as a plain toast.
    pub fn resolve(&mut self, key: &str, text: impl Into<String>, tone: Tone) {
        let text = text.into();
        match self.find(key) {
            Some(t) => {
                t.text = text;
                t.tone = tone;
                t.key = None;
                t.progress = None;
                t.ttl = Some(TOAST_SECS);
            }
            None => self.push(text, tone),
        }
    }

    /// Whether anything is on screen.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The toasts on screen, oldest first.
    pub fn items(&self) -> impl Iterator<Item = &Toast> {
        self.items.iter()
    }

    fn find(&mut self, key: &str) -> Option<&mut Toast> {
        self.items
            .iter_mut()
            .find(|t| t.key.as_deref() == Some(key) && t.leaving.is_none())
    }

    fn add(&mut self, toast: Toast) {
        self.items.push_back(toast);
        let staying = self.items.iter().filter(|t| t.leaving.is_none()).count();
        if staying > MOST
            && let Some(old) = self.items.iter_mut().find(|t| t.leaving.is_none())
        {
            old.leaving = Some(0.0);
        }
    }

    /// Advance by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        for t in &mut self.items {
            t.age += dt;
            match (&mut t.leaving, &mut t.ttl) {
                (Some(l), _) => *l += dt,
                (None, Some(ttl)) => {
                    *ttl -= dt;
                    if *ttl <= 0.0 {
                        t.leaving = Some(0.0);
                    }
                }
                (None, None) => {}
            }
        }
        self.items
            .retain(|t| t.leaving.is_none_or(|l| l < LEAVE_SECS));
    }

    /// Draw the toasts stacked up from `foot` (the top of the navigation, or
    /// the safe area's foot), centred in `width` from `x`. A tap on one
    /// dismisses it; returns whether a tap was taken, so the screen under it
    /// does not take it too.
    pub fn show(&mut self, p: &Painter, x: f32, width: f32, foot: f32) -> bool {
        let c = &p.theme.color;
        let w = width.min(480.0) - 24.0;
        let left = x + (width - w) * 0.5;
        let size = p.size(TextSize::Base);
        let mut y = foot - GAP;
        let mut taken = false;
        for t in self.items.iter_mut().rev() {
            let enter = Ease::CubicOut.apply((t.age / ENTER_SECS).min(1.0));
            let leave = t
                .leaving
                .map_or(0.0, |l| Ease::CubicIn.apply((l / LEAVE_SECS).min(1.0)));
            let shown = enter * (1.0 - leave);
            let rise = (1.0 - enter) * 16.0;
            let rect = Rect::new(left, y - TOAST_H + rise, w, TOAST_H);
            let alpha = shown;
            fill_rounded(rect, 12.0, with_alpha(c.bg_highlight, 0.97 * alpha));
            let edge = match t.tone {
                Tone::Info => c.accent_blue,
                Tone::Success => c.success,
                Tone::Warning => c.warning,
                Tone::Error => c.error,
            };
            fill_rounded(
                Rect::new(rect.x, rect.y, 4.0, rect.h),
                2.0,
                with_alpha(edge, alpha),
            );
            let text_box = Rect::new(rect.x + 16.0, rect.y, rect.w - 28.0, rect.h);
            let text = p.fit(&t.text, size, text_box.w);
            p.text_in(
                &text,
                text_box,
                size,
                with_alpha(c.text_primary, alpha),
                Align::Start,
            );
            if let Some(progress) = t.progress {
                let bar = Rect::new(rect.x + 12.0, rect.bottom() - 6.0, rect.w - 24.0, 3.0);
                p.progress(bar, progress, with_alpha(c.accent, alpha));
            }
            if t.leaving.is_none() && p.tapped(rect) {
                t.leaving = Some(0.0);
                taken = true;
            }
            y -= (TOAST_H + GAP) * shown;
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_toast_leaves_on_its_own() {
        let mut t = Toasts::new();
        t.push("Copied", Tone::Success);
        t.update(TOAST_SECS - 0.1);
        assert!(!t.is_empty());
        t.update(0.2);
        t.update(LEAVE_SECS + 0.01);
        assert!(t.is_empty());
    }

    #[test]
    fn a_job_stays_until_resolved_then_says_how_it_ended() {
        let mut t = Toasts::new();
        t.job("names", "Naming wallets 1/40", Some(0.025));
        t.update(60.0);
        assert_eq!(t.items().count(), 1, "a running job never times out");
        t.job("names", "Naming wallets 40/40", Some(1.0));
        assert_eq!(t.items().next().unwrap().text, "Naming wallets 40/40");
        t.resolve("names", "Named 40 wallets", Tone::Success);
        let shown = t.items().next().unwrap();
        assert_eq!((shown.tone, shown.key.as_deref()), (Tone::Success, None));
        t.update(TOAST_SECS + 0.1);
        t.update(LEAVE_SECS + 0.01);
        assert!(t.is_empty());
    }

    #[test]
    fn the_oldest_makes_room() {
        let mut t = Toasts::new();
        for i in 0..4 {
            t.push(format!("{i}"), Tone::Info);
        }
        t.update(LEAVE_SECS + 0.01);
        let left: Vec<&str> = t.items().map(|t| t.text.as_str()).collect();
        assert_eq!(left, ["1", "2", "3"]);
    }
}
