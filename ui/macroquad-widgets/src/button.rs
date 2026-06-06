//! `Button` atom — a rounded, accent button with idle / hover / pressed /
//! disabled states and three weights (filled / tonal / ghost).
//!
//! Stateless per the crate charter: hover and pressed are derived purely from
//! *this frame's* input (via [`Painter::interact`]), and "clicked" rides the
//! host's tap. The accent defaults to the active theme's, overridable per call.

use macroquad::prelude::*;

use crate::painter::{draw_rounded_rect, with_alpha, shade, Painter};

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
    /// `None` → use the active theme's accent.
    accent: Option<Color>,
    enabled: bool,
    font_size: f32,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Filled,
            accent: None,
            enabled: true,
            font_size: 18.0,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = Some(accent);
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
        let hit = p.interact(rect, self.enabled);
        let a = self.accent.unwrap_or(p.theme.accent);

        let (fill, label_col) = match self.variant {
            ButtonVariant::Filled => {
                let f = if !self.enabled {
                    p.theme.track
                } else if hit.pressed {
                    shade(a, 0.82)
                } else if hit.hover {
                    shade(a, 1.12)
                } else {
                    a
                };
                (f, if self.enabled { p.theme.bg } else { p.theme.muted })
            }
            ButtonVariant::Tonal => {
                let f = if !self.enabled {
                    with_alpha(p.theme.muted, 0.10)
                } else if hit.pressed {
                    with_alpha(a, 0.32)
                } else if hit.hover {
                    with_alpha(a, 0.22)
                } else {
                    with_alpha(a, 0.13)
                };
                (f, if self.enabled { a } else { p.theme.muted })
            }
            ButtonVariant::Ghost => {
                let f = if hit.pressed {
                    with_alpha(a, 0.22)
                } else if hit.hover {
                    with_alpha(a, 0.12)
                } else {
                    with_alpha(a, 0.0)
                };
                (f, if self.enabled { a } else { p.theme.muted })
            }
        };

        let radius = (rect.h * 0.22).min(10.0);
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, radius, fill);

        // Pressed nudges the label down a hair — tactile "push in".
        let nudge = if hit.pressed { 1.0 } else { 0.0 };
        let dim = p.measure(self.label, self.font_size);
        let baseline = p.centre_baseline(rect.y, rect.h, self.font_size) + nudge;
        p.text(
            self.label,
            rect.x + (rect.w - dim.width) * 0.5,
            baseline,
            self.font_size,
            label_col,
        );

        hit.clicked
    }
}
