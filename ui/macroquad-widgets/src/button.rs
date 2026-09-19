//! `Button` atom — a rounded, accent button with idle / hover / pressed /
//! disabled states and three weights (filled / tonal / ghost).
//!
//! Stateless per the crate charter: hover and pressed are derived purely from
//! *this frame's* input (via [`Painter::interact`]), and "clicked" rides the
//! host's tap. The accent defaults to the active theme's, overridable per call.

use macroquad::prelude::*;
use ui_theme::{Ink, TextSize, Token};

use crate::painter::{Painter, draw_rounded_rect, shade, with_alpha};

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
    /// Which ink the fill takes. [`Ink`] rather than `Option<Color>` on
    /// purpose: `None` says *absent*, but a themed default is the opposite of
    /// absent — it is the considered answer, and naming it here means reading
    /// this struct tells you what the button will look like.
    accent: Ink<Color>,
    enabled: bool,
    /// The label's step on the type ramp. Resolved in [`Self::show`], where a
    /// `Painter` (and so a theme) finally exists.
    size: ButtonTextSize,
}

/// A button's label size: a ramp step, or a raw height for the callers that
/// genuinely size their glyph from their own geometry.
#[derive(Clone, Copy)]
enum ButtonTextSize {
    Step(TextSize),
    /// theme-exempt: a glyph scaled to the control's own height — see
    /// `quantity_stepper`, whose `−`/`+` must grow with the row.
    Px(f32),
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Filled,
            accent: Ink::Token(Token::Accent),
            enabled: true,
            size: ButtonTextSize::Step(TextSize::Xl2),
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Override the fill ink — a token, a wash, or a fixed colour.
    ///
    /// `impl Into<Ink<Color>>`, so `.accent(some_color)` and
    /// `.accent(Token::Success)` both still read naturally.
    pub fn accent(mut self, accent: impl Into<Ink<Color>>) -> Self {
        self.accent = accent.into();
        self
    }

    /// Size the label from the type ramp.
    pub fn text_size(mut self, size: TextSize) -> Self {
        self.size = ButtonTextSize::Step(size);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Size the label in raw pixels.
    ///
    /// For a glyph whose size is a proportion of the control's own geometry,
    /// not a step on the type ramp — `quantity_stepper` sizes its `−`/`+` from
    /// the row height so the whole control scales together. Prefer
    /// [`Self::text_size`] everywhere else.
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.size = ButtonTextSize::Px(font_size);
        self
    }

    /// Draw into `rect`; returns true if tapped/clicked this frame.
    pub fn show(&self, p: &Painter, rect: Rect) -> bool {
        let hit = p.interact(rect, self.enabled);
        let a = self.accent.resolve(&p.theme);
        let muted = p.theme.color.text_muted;

        let (fill, label_col) = match self.variant {
            ButtonVariant::Filled => {
                let f = if !self.enabled {
                    p.theme.color.bg_highlight
                } else if hit.pressed {
                    shade(a, 0.82)
                } else if hit.hover {
                    shade(a, 1.12)
                } else {
                    a
                };
                (
                    f,
                    if self.enabled {
                        p.theme.color.bg_primary
                    } else {
                        muted
                    },
                )
            }
            ButtonVariant::Tonal => {
                let f = if !self.enabled {
                    with_alpha(muted, 0.10)
                } else if hit.pressed {
                    with_alpha(a, 0.32)
                } else if hit.hover {
                    with_alpha(a, 0.22)
                } else {
                    with_alpha(a, 0.13)
                };
                (f, if self.enabled { a } else { muted })
            }
            ButtonVariant::Ghost => {
                let f = if hit.pressed {
                    with_alpha(a, 0.22)
                } else if hit.hover {
                    with_alpha(a, 0.12)
                } else {
                    with_alpha(a, 0.0)
                };
                (f, if self.enabled { a } else { muted })
            }
        };

        let radius = (rect.h * 0.22).min(10.0);
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, radius, fill);

        let size = match self.size {
            ButtonTextSize::Step(step) => p.size(step),
            ButtonTextSize::Px(px) => px,
        };

        // Pressed nudges the label down a hair — tactile "push in".
        let nudge = if hit.pressed { 1.0 } else { 0.0 };
        let dim = p.measure(self.label, size);
        let baseline = p.centre_baseline(rect.y, rect.h, size) + nudge;
        p.text(
            self.label,
            rect.x + (rect.w - dim.width) * 0.5,
            baseline,
            size,
            label_col,
        );

        hit.clicked
    }
}
