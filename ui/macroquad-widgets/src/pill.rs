//! `Pill` — a modal's buttons: fully rounded, drawn in the sheet's own fade so
//! they leave with it.
//!
//! Three styles, as the wallet modal had them:
//!
//! - **Filled**: the step's primary action, a gradient plate in its ink (the
//!   theme's accent unless the host names one), the label in the page colour.
//! - **Ghost**: an outlined way out or across ("Switch wallet", "View on
//!   Wayup"), filled only under the pointer.
//! - **Danger**: outlined and inked in the error colour, an exit that is plainly
//!   there without being the primary action ("Disconnect").
//!
//! [`crate::Button`] stays the page's button. A pill is for a sheet's footer
//! and a modal's body, where every surface around it is rounded and layered.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::Painter;
use crate::painter::with_alpha;
use crate::shape::{Gradient, fill_gradient, fill_rounded, mix, stroke_rounded};

/// How a pill is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PillStyle {
    #[default]
    Filled,
    Ghost,
    Danger,
}

/// See the [module docs](self).
#[derive(Clone, Debug)]
pub struct Pill<'a> {
    label: &'a str,
    style: PillStyle,
    ink: Option<Color>,
    enabled: bool,
    live: bool,
    alpha: f32,
}

impl<'a> Pill<'a> {
    /// The primary action.
    pub fn filled(label: &'a str) -> Self {
        Self::styled(label, PillStyle::Filled)
    }

    pub fn ghost(label: &'a str) -> Self {
        Self::styled(label, PillStyle::Ghost)
    }

    pub fn danger(label: &'a str) -> Self {
        Self::styled(label, PillStyle::Danger)
    }

    pub fn styled(label: &'a str, style: PillStyle) -> Self {
        Self {
            label,
            style,
            ink: None,
            enabled: true,
            live: true,
            alpha: 1.0,
        }
    }

    /// The colour a Filled pill is filled with, a Danger pill inked with, or a
    /// Ghost pill's label is written in.
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// A disabled pill is drawn faint and takes no tap.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Whether it takes a tap this frame, without looking any different: a
    /// sheet still sliding in, or already leaving, holds its pills back.
    pub fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    /// The sheet's fade.
    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Draw into `rect`. `true` when tapped.
    pub fn show(&self, p: &Painter, rect: Rect) -> bool {
        let c = p.theme.color;
        let takes = self.enabled && self.live;
        let hit = p.interact(rect, takes);
        let a = self.alpha * if self.enabled { 1.0 } else { 0.42 };
        let r = rect.h * 0.5;
        let size = p.size(TextSize::Lg);
        let label = p.fit(self.label, size, rect.w - 24.0);
        let (hover, pressed) = (hit.hover && takes, hit.pressed && takes);
        match self.style {
            PillStyle::Filled => {
                let fill = self.ink.unwrap_or(c.accent);
                let base = if pressed {
                    mix(fill, c.bg_primary, 0.15)
                } else if hover {
                    mix(fill, c.text_primary, 0.12)
                } else {
                    fill
                };
                fill_gradient(
                    rect,
                    r,
                    Gradient::Vertical {
                        top: with_alpha(mix(base, c.text_primary, 0.14), a),
                        bottom: with_alpha(base, a),
                    },
                );
                p.text_centred(&label, rect, size, with_alpha(c.bg_primary, a));
            }
            PillStyle::Ghost => {
                let k = if pressed {
                    0.7
                } else if hover {
                    0.45
                } else {
                    0.0
                };
                if k > 0.0 {
                    fill_rounded(rect, r, with_alpha(c.bg_highlight, k * a));
                }
                stroke_rounded(rect, r, 1.0, with_alpha(c.border, a));
                let ink = self.ink.unwrap_or(c.text_primary);
                p.text_centred(&label, rect, size, with_alpha(ink, a));
            }
            PillStyle::Danger => {
                let fill = self.ink.unwrap_or(c.error);
                let k = if pressed {
                    0.28
                } else if hover {
                    0.18
                } else {
                    0.08
                };
                fill_rounded(rect, r, with_alpha(fill, k * a));
                stroke_rounded(rect, r, 1.0, with_alpha(fill, 0.7 * a));
                p.text_centred(&label, rect, size, with_alpha(fill, a));
            }
        }
        takes && hit.clicked
    }
}

/// `n` pills side by side across `rect`, `gap` apart, the first `lead` share
/// of the width for the first when there are two (a quieter action beside the
/// primary one). Rects in order.
pub fn pill_row(rect: Rect, n: usize, gap: f32, lead: Option<f32>) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    if n == 2
        && let Some(share) = lead
    {
        let first = ((rect.w - gap) * share.clamp(0.1, 0.9)).floor();
        return vec![
            Rect::new(rect.x, rect.y, first, rect.h),
            Rect::new(rect.x + first + gap, rect.y, rect.w - first - gap, rect.h),
        ];
    }
    let w = (rect.w - gap * (n - 1) as f32) / n as f32;
    (0..n)
        .map(|i| Rect::new(rect.x + i as f32 * (w + gap), rect.y, w, rect.h))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_of_pills_fills_its_width_with_even_gaps() {
        let rects = pill_row(Rect::new(0.0, 0.0, 300.0, 44.0), 3, 10.0, None);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].x, 0.0);
        assert!((rects[2].right() - 300.0).abs() < 1e-3);
        assert!((rects[1].x - rects[0].right() - 10.0).abs() < 1e-3);
    }

    #[test]
    fn two_pills_can_give_the_primary_the_larger_share() {
        let rects = pill_row(Rect::new(0.0, 0.0, 310.0, 44.0), 2, 10.0, Some(0.4));
        assert_eq!(rects[0].w, 120.0);
        assert!((rects[1].right() - 310.0).abs() < 1e-3);
    }
}
