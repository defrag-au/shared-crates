//! `Button` atom — a rounded, accent button with idle / hover / pressed /
//! disabled states and three weights (filled / tonal / ghost).
//!
//! Stateless per the crate charter: hover and pressed are derived purely from
//! *this frame's* mouse position + button state (no retained widget state), and
//! "clicked" rides the host's [`Painter::tap`]. So it composes cleanly in the
//! immediate-mode loop — the host positions the rect, the button draws + reports.

use macroquad::prelude::*;

use crate::painter::{draw_rounded_rect, Painter};
use crate::theme;

/// Visual weight.
#[derive(Clone, Copy)]
pub enum ButtonVariant {
    /// Solid accent fill, dark label — primary CTA.
    Filled,
    /// Translucent accent fill, accent label — secondary / knobs.
    Tonal,
    /// Fill only on hover/press, accent label — quiet / inline.
    Ghost,
}

pub struct Button<'a> {
    label: &'a str,
    variant: ButtonVariant,
    accent: Color,
    enabled: bool,
    font_size: f32,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Filled,
            accent: theme::ACCENT,
            enabled: true,
            font_size: 18.0,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Draw into `rect`; returns true if tapped/clicked this frame.
    pub fn show(&self, p: &Painter, rect: Rect) -> bool {
        let (mx, my) = mouse_position();
        let over = rect.contains(vec2(mx, my));
        let pressed = self.enabled && over && is_mouse_button_down(MouseButton::Left);
        let hover = self.enabled && over && !pressed;
        let clicked = self.enabled && p.tap.is_some_and(|t| rect.contains(t));

        let a = self.accent;
        let (fill, label_col) = match self.variant {
            ButtonVariant::Filled => {
                let f = if !self.enabled {
                    theme::TRACK
                } else if pressed {
                    theme::shade(a, 0.82)
                } else if hover {
                    theme::shade(a, 1.12)
                } else {
                    a
                };
                (f, if self.enabled { theme::BG } else { theme::MUTED })
            }
            ButtonVariant::Tonal => {
                let f = if !self.enabled {
                    theme::with_alpha(theme::MUTED, 0.10)
                } else if pressed {
                    theme::with_alpha(a, 0.32)
                } else if hover {
                    theme::with_alpha(a, 0.22)
                } else {
                    theme::with_alpha(a, 0.13)
                };
                (f, if self.enabled { a } else { theme::MUTED })
            }
            ButtonVariant::Ghost => {
                let f = if pressed {
                    theme::with_alpha(a, 0.22)
                } else if hover {
                    theme::with_alpha(a, 0.12)
                } else {
                    theme::with_alpha(a, 0.0)
                };
                (f, if self.enabled { a } else { theme::MUTED })
            }
        };

        let radius = (rect.h * 0.22).min(10.0);
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, radius, fill);

        // Pressed nudges the label down a hair — tactile "push in".
        let nudge = if pressed { 1.0 } else { 0.0 };
        let dim = p.measure(self.label, self.font_size);
        let baseline = p.centre_baseline(rect.y, rect.h, self.font_size) + nudge;
        p.text(
            self.label,
            rect.x + (rect.w - dim.width) * 0.5,
            baseline,
            self.font_size,
            label_col,
        );

        clicked
    }
}
