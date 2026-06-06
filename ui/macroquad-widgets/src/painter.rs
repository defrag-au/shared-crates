//! `Painter` — the thin draw surface widgets render through. Carries the host's
//! font (so widgets don't bake their own) and this frame's tap, and offers the
//! handful of primitives the widgets need (text, button, progress, hit-test).
//! Deliberately tiny — this is a widget toolkit, not a UI framework.

use macroquad::prelude::*;

use crate::theme;

pub struct Painter<'a> {
    /// Proportional font — UI chrome (headings, labels, buttons).
    pub font: Option<&'a Font>,
    /// Monospace font — hashes / fixed-width data, so columns of hex align.
    pub mono: Option<&'a Font>,
    /// Where the user tapped/clicked this frame, if anywhere (see [`frame_tap`]).
    pub tap: Option<Vec2>,
}

impl<'a> Painter<'a> {
    pub fn new(font: Option<&'a Font>, mono: Option<&'a Font>, tap: Option<Vec2>) -> Self {
        Self { font, mono, tap }
    }

    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.draw_in(self.font, s, x, y, size, color);
    }

    /// Draw fixed-width data (a tx hash, an amount) in the monospace font.
    pub fn mono(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.draw_in(self.mono, s, x, y, size, color);
    }

    fn draw_in(&self, font: Option<&Font>, s: &str, x: f32, y: f32, size: f32, color: Color) {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font,
                font_size: size as u16,
                color,
                ..Default::default()
            },
        );
    }

    pub fn measure(&self, s: &str, size: f32) -> TextDimensions {
        measure_text(s, self.font, size as u16, 1.0)
    }

    /// Baseline `y` that vertically centres text (at `size`) within a band of
    /// `height` starting at `top` — **independent of the string's own glyphs**.
    ///
    /// Centring on the *measured string* would drop caps-only labels (`Mint`)
    /// lower than descender labels (`tap`), since their glyph boxes differ. So
    /// we centre a fixed reference line with both a cap AND a descender (`"Ag"`)
    /// — every label then shares one baseline. The `+ size * OPTICAL_NUDGE`
    /// drops it a hair below the line-box centre (the eye expects descender
    /// space below the baseline); tune it if labels feel high (raise) or low
    /// (lower).
    pub fn centre_baseline(&self, top: f32, height: f32, size: f32) -> f32 {
        const OPTICAL_NUDGE: f32 = 0.10;
        let line = self.measure("Ag", size);
        top + height * 0.5 + line.offset_y - line.height * 0.5 + size * OPTICAL_NUDGE
    }

    /// Did the user tap inside `rect` this frame?
    pub fn tapped(&self, rect: Rect) -> bool {
        self.tap.is_some_and(|p| rect.contains(p))
    }

    /// A default filled accent button — convenience wrapper over [`Button`].
    /// Returns true on a tap inside. For variants/accent see [`Button`].
    pub fn button(&self, label: &str, rect: Rect, enabled: bool) -> bool {
        crate::button::Button::new(label).enabled(enabled).show(self, rect)
    }

    /// A horizontal progress bar: `frac` (0..1) of `rect` filled with `fill`
    /// over the muted track (rounded).
    pub fn progress(&self, rect: Rect, frac: f32, fill: Color) {
        let r = rect.h * 0.5;
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, r, theme::TRACK);
        let w = rect.w * frac.clamp(0.0, 1.0);
        if w > r {
            draw_rounded_rect(rect.x, rect.y, w, rect.h, r, fill);
        }
    }
}

/// Filled rounded rectangle (macroquad has no high-level rounded rect).
///
/// Built as a single convex polygon (four corner arcs joined by straight edges)
/// filled by a triangle fan from the centre, so every pixel is drawn EXACTLY
/// once. A rects-plus-corner-discs approach double-draws the overlaps, which is
/// invisible for opaque fills but compounds alpha into dark corner blobs for
/// translucent ones (tonal / ghost buttons). `r` is clamped to half the
/// smaller side.
pub fn draw_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    use std::f32::consts::PI;
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    if r < 0.75 {
        draw_rectangle(x, y, w, h, color);
        return;
    }
    // Corner centre + arc range (screen space, y-down), walked clockwise.
    const SEG: usize = 4;
    let corners = [
        (x + r, y + r, PI, 1.5 * PI),           // top-left
        (x + w - r, y + r, 1.5 * PI, 2.0 * PI), // top-right
        (x + w - r, y + h - r, 0.0, 0.5 * PI),  // bottom-right
        (x + r, y + h - r, 0.5 * PI, PI),       // bottom-left
    ];
    let mut pts: Vec<Vec2> = Vec::with_capacity(4 * (SEG + 1));
    for (cx, cy, a0, a1) in corners {
        for i in 0..=SEG {
            let t = a0 + (a1 - a0) * (i as f32 / SEG as f32);
            pts.push(vec2(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    let centre = vec2(x + w * 0.5, y + h * 0.5);
    for i in 0..pts.len() {
        draw_triangle(centre, pts[i], pts[(i + 1) % pts.len()], color);
    }
}

/// A tap this frame — a fresh touch (mobile) OR a mouse press (desktop). Touch
/// is checked first so wallet-webview runs exercise the real touch path.
pub fn frame_tap() -> Option<Vec2> {
    for t in touches() {
        if matches!(t.phase, TouchPhase::Started) {
            return Some(t.position);
        }
    }
    if is_mouse_button_pressed(MouseButton::Left) {
        let (x, y) = mouse_position();
        return Some(vec2(x, y));
    }
    None
}
