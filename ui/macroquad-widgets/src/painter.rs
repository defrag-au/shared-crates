//! `Painter` — the thin draw surface widgets render through. Carries the host's
//! fonts (proportional + mono), the active [`Theme`], and this frame's tap, and
//! offers the primitives widgets need (text, button, progress, rounded rect,
//! hit-test). Deliberately tiny — a widget toolkit, not a UI framework.

use macroquad::prelude::*;
use macroquad_text::{Align, TextStyle, draw_run, measure_run};
use ui_theme::{TextRole, TextSize};

use crate::theme::{self, Theme};

pub struct Painter<'a> {
    /// Proportional font — UI chrome (headings, labels, buttons).
    pub font: Option<&'a Font>,
    /// Monospace font — hashes / fixed-width data, so columns of hex align.
    pub mono: Option<&'a Font>,
    /// Active palette — widgets read `p.theme.*`; swap to re-skin everything.
    pub theme: Theme,
    /// Where the user tapped/clicked this frame, if anywhere (see [`frame_tap`]).
    pub tap: Option<Vec2>,
    /// Device pixels per unit where widgets land; see [`Painter::pixel_scale`].
    px: f32,
    /// Seconds on the host's clock; see [`Painter::at_time`].
    now: f64,
}

/// Per-frame interaction state for a rect — for building custom interactive
/// elements (icon rows etc.) without re-deriving it. See [`Painter::interact`].
pub struct Hit {
    pub hover: bool,
    pub pressed: bool,
    pub clicked: bool,
}

impl<'a> Painter<'a> {
    pub fn new(
        font: Option<&'a Font>,
        mono: Option<&'a Font>,
        theme: Theme,
        tap: Option<Vec2>,
    ) -> Self {
        Self {
            font,
            mono,
            theme,
            tap,
            px: screen_dpi_scale(),
            now: get_time(),
        }
    }

    /// Device pixels per unit where these widgets are drawn: the screen's DPI
    /// scale (the default) for widgets drawn in screen points, times the
    /// camera's zoom for widgets drawn in a game's design units. Text
    /// rasterises at that size, so it stays sharp under any camera.
    pub fn pixel_scale(mut self, px: f32) -> Self {
        self.px = px;
        self
    }

    /// The time animated widgets (a pending pulse, a spinner) read, in
    /// seconds. Defaults to macroquad's clock; a host on a fixed clock — a
    /// capture at an exact moment — passes its own, so the pulse is the same
    /// in every run.
    pub fn at_time(mut self, seconds: f64) -> Self {
        self.now = seconds;
        self
    }

    /// The time animated widgets read: see [`Painter::at_time`].
    pub fn time(&self) -> f64 {
        self.now
    }

    /// Point size for a step of the type ramp — **the one way a widget gets a
    /// size**.
    ///
    /// `p.text(label, x, y, p.size(TextSize::Base), col)` is the blessed shape;
    /// `tests/text_sizes.rs` rejects a bare literal in that position. The ramp
    /// is the theme's, so a host that opens it out
    /// ([`Theme::with_text_scale`](crate::Theme::with_text_scale)) moves every
    /// widget at once — which a literal can never do.
    pub fn size(&self, step: TextSize) -> f32 {
        self.theme.text_size(step)
    }

    /// Point size for a text ROLE. Prefer this when the call site knows what
    /// its text *is* — that is information a step cannot carry.
    pub fn role(&self, role: TextRole) -> f32 {
        self.theme.role_size(role)
    }

    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.draw_in(self.font, s, x, y, size, color);
    }

    /// Draw fixed-width data (a tx hash, an amount) in the monospace font.
    pub fn mono(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.draw_in(self.mono, s, x, y, size, color);
    }

    /// Baseline for text whose visual top sits at `top` — for top-anchored
    /// (box) layouts. Uses the `"Ag"` reference so it's string-independent.
    pub fn top_baseline(&self, top: f32, size: f32) -> f32 {
        top + self.measure("Ag", size).offset_y
    }

    /// Draw proportional text with its top at `top` (left-aligned at `x`).
    pub fn text_top(&self, s: &str, x: f32, top: f32, size: f32, color: Color) {
        self.text(s, x, self.top_baseline(top, size), size, color);
    }

    /// Through `macroquad-text`: drawn at exactly `size` (macroquad alone
    /// would truncate 10.5 to 10), rasterised at [`Painter::pixel_scale`],
    /// held back a frame by a host's glyph gate, and recorded for capture
    /// reports.
    fn draw_in(&self, font: Option<&Font>, s: &str, x: f32, y: f32, size: f32, color: Color) {
        let style = TextStyle::new(font.cloned(), size, color);
        draw_run(s, &style, vec2(x, y), self.px, 1.0);
    }

    fn measure_in(&self, font: Option<&Font>, s: &str, size: f32) -> TextDimensions {
        measure_run(s, &TextStyle::new(font.cloned(), size, WHITE), self.px)
    }

    /// Measure in the PROPORTIONAL face — what [`Painter::text`] draws.
    ///
    /// Named explicitly because the asymmetry bites: anything drawn with
    /// [`Painter::mono`] must be measured with [`Painter::measure_mono`], and
    /// getting it wrong fails SILENTLY. Monospace is wider per character, so
    /// measuring it here under-reports, and two independently placed strings
    /// quietly overlap instead of erroring.
    pub fn measure(&self, s: &str, size: f32) -> TextDimensions {
        self.measure_in(self.font, s, size)
    }

    /// Measure in the MONOSPACE face — what [`Painter::mono`] draws.
    ///
    /// Without this a widget can only measure half the text it can draw, which
    /// makes right-aligning or fitting a hash, a price or any fixed-width
    /// figure a guess. See [`Painter::measure`] for why the guess is silent.
    pub fn measure_mono(&self, s: &str, size: f32) -> TextDimensions {
        self.measure_in(self.mono, s, size)
    }

    /// `s`, cut with an ellipsis to fit `max_w` in the proportional face.
    ///
    /// The one cut every host was writing for itself — seven copies across the
    /// two estates, each with its own idea of where the ellipsis goes. A label
    /// too long for its box is shortened, never allowed to run over its
    /// neighbour; an empty result is a label with no room at all.
    pub fn fit(&self, s: &str, size: f32, max_w: f32) -> String {
        self.fit_in(self.font, s, size, max_w)
    }

    /// [`Painter::fit`] in the monospace face.
    pub fn fit_mono(&self, s: &str, size: f32, max_w: f32) -> String {
        self.fit_in(self.mono, s, size, max_w)
    }

    fn fit_in(&self, font: Option<&Font>, s: &str, size: f32, max_w: f32) -> String {
        if self.measure_in(font, s, size).width <= max_w {
            return s.to_string();
        }
        let mut out = String::new();
        for ch in s.chars() {
            let mut candidate = out.clone();
            candidate.push(ch);
            candidate.push('…');
            if self.measure_in(font, &candidate, size).width > max_w {
                break;
            }
            out.push(ch);
        }
        out.push('…');
        out
    }

    /// Baseline `y` that vertically centres text (at `size`) within a band of
    /// `height` starting at `top` — **independent of the string's own glyphs**.
    ///
    /// For capitals-and-digits labels in a tight box (a stamp, a badge, a
    /// pill), prefer [`Painter::text_centred`] or [`Painter::mid_baseline`]:
    /// they centre the cap band rather than a line box with a tuned nudge, so
    /// they hold in every face.
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

    // ── Vertical placement by cap height ────────────────────────────────────
    //
    // UI labels are mostly capitals and digits (`FILED 7/14`, `#13,358,880`,
    // `MEAN GAP`), and what the eye centres is their INK: the band from the
    // baseline up to the top of a capital. The point size is much taller than
    // that band (VT323's capitals are about half its size), and a line box
    // from ascender to descender hangs below it. Centre either of those in a
    // box and capitals sit visibly high, by a different amount per face.
    //
    // These place text by the cap band, so a label is centred in a stamp, on a
    // row's midline or beside an icon whatever the face, with no per-face
    // nudge. Descenders hang below the band, as they do in print.

    /// Height of a capital above the baseline in the proportional face.
    pub fn cap_height(&self, size: f32) -> f32 {
        self.measure("H", size).offset_y
    }

    /// Height of a capital above the baseline in the monospace face.
    pub fn cap_height_mono(&self, size: f32) -> f32 {
        self.measure_mono("H", size).offset_y
    }

    /// Baseline that puts the middle of the capitals on `mid`.
    pub fn mid_baseline(&self, mid: f32, size: f32) -> f32 {
        mid + self.cap_height(size) * 0.5
    }

    /// [`Painter::mid_baseline`] for the monospace face.
    pub fn mid_baseline_mono(&self, mid: f32, size: f32) -> f32 {
        mid + self.cap_height_mono(size) * 0.5
    }

    /// Proportional text whose capitals are centred on `mid`: a row's
    /// midline, an icon's centre. Pair it with the icon's own centre and the
    /// two line up in any face.
    pub fn text_mid(&self, s: &str, x: f32, mid: f32, size: f32, color: Color) {
        self.text(s, x, self.mid_baseline(mid, size), size, color);
    }

    /// Monospace text whose capitals are centred on `mid`.
    pub fn mono_mid(&self, s: &str, x: f32, mid: f32, size: f32, color: Color) {
        self.mono(s, x, self.mid_baseline_mono(mid, size), size, color);
    }

    /// Monospace text with the top of its capitals at `top`: the monospace
    /// twin of [`Painter::text_top`].
    pub fn mono_top(&self, s: &str, x: f32, top: f32, size: f32, color: Color) {
        let base = top + self.cap_height_mono(size);
        self.mono(s, x, base, size, color);
    }

    /// Proportional text centred in `rect` both ways, by its capitals: a
    /// stamp, a badge, a button's label.
    pub fn text_centred(&self, s: &str, rect: Rect, size: f32, color: Color) {
        let w = self.measure(s, size).width;
        let x = rect.x + (rect.w - w) * 0.5;
        self.text_mid(s, x, rect.y + rect.h * 0.5, size, color);
    }

    /// Monospace text centred in `rect` both ways, by its capitals.
    pub fn mono_centred(&self, s: &str, rect: Rect, size: f32, color: Color) {
        let w = self.measure_mono(s, size).width;
        let x = rect.x + (rect.w - w) * 0.5;
        self.mono_mid(s, x, rect.y + rect.h * 0.5, size, color);
    }

    /// Proportional text in `rect` with its capitals on the rect's midline,
    /// against its start, its centre or its end: a row's label, a figure
    /// right against a tile's edge, a chip's word. Inset `rect` for padding.
    ///
    /// The one call for "a label in a box". Placing it with
    /// [`Painter::text_top`] and an offset tuned by eye is how labels end up
    /// a point or two off centre, differently per face and per size.
    pub fn text_in(&self, s: &str, rect: Rect, size: f32, color: Color, align: Align) {
        let x = aligned_x(rect, self.measure(s, size).width, align);
        self.text_mid(s, x, rect.y + rect.h * 0.5, size, color);
    }

    /// [`Painter::text_in`] in the monospace face.
    pub fn mono_in(&self, s: &str, rect: Rect, size: f32, color: Color, align: Align) {
        let x = aligned_x(rect, self.measure_mono(s, size).width, align);
        self.mono_mid(s, x, rect.y + rect.h * 0.5, size, color);
    }

    /// Lines centred in `rect` as one block, each against `align`: a title
    /// over its subtitle in a list row, a figure over its caption in a tile.
    ///
    /// Placed by their capitals, `gap` apart (cap band to cap band), and the
    /// block's capitals are centred on the rect's midline, so a two-line row
    /// sits in its box the way a one-line label does. Descenders hang below
    /// their line, as in print; leave room for them in `gap`.
    pub fn stack_in(&self, lines: &[Line<'_>], rect: Rect, align: Align, gap: f32) {
        let cap = |l: &Line<'_>| match l.mono {
            true => self.cap_height_mono(l.size),
            false => self.cap_height(l.size),
        };
        let caps: Vec<f32> = lines.iter().map(cap).collect();
        let block = caps.iter().sum::<f32>() + gap * lines.len().saturating_sub(1) as f32;
        let mut top = rect.y + (rect.h - block) * 0.5;
        for (line, cap) in lines.iter().zip(caps) {
            let w = match line.mono {
                true => self.measure_mono(line.text, line.size).width,
                false => self.measure(line.text, line.size).width,
            };
            let x = aligned_x(rect, w, align);
            match line.mono {
                true => self.mono(line.text, x, top + cap, line.size, line.color),
                false => self.text(line.text, x, top + cap, line.size, line.color),
            }
            top += cap + gap;
        }
    }

    /// Did the user tap inside `rect` this frame?
    pub fn tapped(&self, rect: Rect) -> bool {
        self.tap.is_some_and(|p| rect.contains(p))
    }

    /// Hover / pressed / clicked state for `rect` this frame (current-frame
    /// input only — no retained state). `enabled = false` reports all false.
    pub fn interact(&self, rect: Rect, enabled: bool) -> Hit {
        let (mx, my) = mouse_position();
        let over = enabled && rect.contains(vec2(mx, my));
        let pressed = over && is_mouse_button_down(MouseButton::Left);
        Hit {
            hover: over && !pressed,
            pressed,
            clicked: enabled && self.tap.is_some_and(|t| rect.contains(t)),
        }
    }

    /// A default filled accent button — convenience wrapper over [`Button`].
    /// Returns true on a tap inside. For variants/accent see [`Button`].
    pub fn button(&self, label: &str, rect: Rect, enabled: bool) -> bool {
        crate::button::Button::new(label)
            .enabled(enabled)
            .show(self, rect)
    }

    /// A horizontal progress bar: `frac` (0..1) of `rect` filled with `fill`
    /// over the muted track (rounded).
    pub fn progress(&self, rect: Rect, frac: f32, fill: Color) {
        let r = rect.h * 0.5;
        draw_rounded_rect(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            r,
            self.theme.color.bg_highlight,
        );
        let w = rect.w * frac.clamp(0.0, 1.0);
        if w > r {
            draw_rounded_rect(rect.x, rect.y, w, rect.h, r, fill);
        }
    }
}

/// One line of a [`Painter::stack_in`] block.
#[derive(Clone, Copy, Debug)]
pub struct Line<'s> {
    pub text: &'s str,
    pub size: f32,
    pub color: Color,
    /// In the monospace face.
    pub mono: bool,
}

impl<'s> Line<'s> {
    pub fn new(text: &'s str, size: f32, color: Color) -> Self {
        Self {
            text,
            size,
            color,
            mono: false,
        }
    }

    /// The same line in the monospace face.
    pub fn mono(self) -> Self {
        Self { mono: true, ..self }
    }
}

/// Where text `w` wide starts in `rect` for `align`.
fn aligned_x(rect: Rect, w: f32, align: Align) -> f32 {
    match align {
        Align::Start => rect.x,
        Align::Center => rect.x + (rect.w - w) * 0.5,
        Align::End => rect.right() - w,
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

// Re-export the colour ops widgets commonly need alongside the Painter.
pub use theme::{shade, with_alpha};

/// A tap this frame — a fresh touch (mobile) OR a mouse press (desktop). Touch
/// is checked first so wallet-webview runs exercise the real touch path.
///
/// # Deprecated: fires on touch-*down*
///
/// This reports a tap the instant a finger lands, so a finger that touches a
/// control and slides away still activates it — not how a native control
/// behaves, and on a mint button the difference between a mis-tap the buyer
/// can take back and one they can't. It also makes swipe impossible: a drag
/// across a grid would select whatever it started on *and* scroll.
///
/// [`crate::Gestures`] resolves a tap on release, within a small slop, and
/// recognises swipes as mutually exclusive with taps. Hold one on your app
/// state, call `update()` once per frame, and pass `.tap` here instead:
///
/// ```ignore
/// let gesture = self.gestures.update();
/// let p = Painter::new(font, mono, theme, gesture.tap);
/// ```
///
/// Kept (rather than removed) so any out-of-tree caller gets a warning and
/// this explanation instead of a broken build.
#[deprecated(
    note = "fires on touch-down, so dragging off a control still activates it; use `Gestures` and pass `Gesture::tap`"
)]
pub fn frame_tap() -> Option<Vec2> {
    for t in crate::gesture::touch_points() {
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
