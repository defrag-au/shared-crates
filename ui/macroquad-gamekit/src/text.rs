//! `RichText` — wrapped text in several styles with inline icon boxes, laid out
//! once and drawn crisp at any pixel scale.
//!
//! Card text is the motivating case: "**Deal 4** ⟨energy icon⟩ then *Draw 1*",
//! wrapped to a card's width, centred, legible on a phone. macroquad draws a
//! single line in a single style; this adds the layout.
//!
//! ## Shaped like parley
//!
//! The model is parley's: styled runs over text, inline boxes for icons, and a
//! layout that hands back positioned pieces. Measurement goes through the
//! [`Measure`] trait ([`MacroquadMeasure`] in a game, a fixed-width fake in
//! tests), so wrapping is unit-tested without a window, and a parley-backed
//! layout could replace this module without its callers changing.
//!
//! What it does not do yet, and parley would: shaping for complex scripts,
//! Unicode line-break rules beyond spaces and newlines, and bidi. A single word
//! wider than the box overflows rather than breaking mid-word.
//!
//! ## Sizes
//!
//! A [`TextStyle`]'s size is in points and comes from the caller, resolved
//! through the theme's ramp (`Theme::text_size(TextSize::…)`); this module
//! never writes one down.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};

use macroquad::prelude::*;

/// A line drawn around each glyph, for text over art.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
}

/// A copy drawn underneath, offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    /// `None` draws in macroquad's built-in font.
    pub font: Option<Font>,
    /// Points; resolve through the theme's ramp.
    pub size: f32,
    pub color: Color,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
}

impl TextStyle {
    pub fn new(font: Option<Font>, size: f32, color: Color) -> Self {
        Self {
            font,
            size,
            color,
            outline: None,
            shadow: None,
        }
    }

    pub fn outline(mut self, color: Color, width: f32) -> Self {
        self.outline = Some(Outline { color, width });
        self
    }

    pub fn shadow(mut self, color: Color, offset: Vec2) -> Self {
        self.shadow = Some(Shadow { color, offset });
        self
    }
}

/// Index of a style within a [`RichText`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleId(pub usize);

#[derive(Clone, Debug, PartialEq)]
enum Span {
    Text {
        text: String,
        style: StyleId,
    },
    /// A box the caller draws: an icon in the flow of the text.
    Icon {
        id: u32,
        size: Vec2,
        style: StyleId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// Text widths and vertical metrics for a style.
pub trait Measure {
    fn width(&self, text: &str, style: &TextStyle) -> f32;
    /// Distance from the baseline up to the top of tall glyphs.
    fn ascent(&self, style: &TextStyle) -> f32;
    /// Distance from the baseline down to the bottom of descenders.
    fn descent(&self, style: &TextStyle) -> f32;
}

/// Measures with macroquad's own font rasteriser, so layout and drawing agree.
pub struct MacroquadMeasure;

impl Measure for MacroquadMeasure {
    fn width(&self, text: &str, style: &TextStyle) -> f32 {
        measure(text, style).width
    }

    fn ascent(&self, style: &TextStyle) -> f32 {
        measure("Mg", style).offset_y
    }

    fn descent(&self, style: &TextStyle) -> f32 {
        let d = measure("Mg", style);
        (d.height - d.offset_y).max(0.0)
    }
}

/// Measured at a fine raster, scaled to the exact size: the same size the text
/// is drawn at, whatever the pixel scale.
fn measure(text: &str, style: &TextStyle) -> TextDimensions {
    let (px, scale) = raster(style.size, MEASURE_SCALE);
    gated_measure(text, style, px, scale)
}

/// Measure through macroquad, unless the glyph gate is holding these glyphs
/// back: then an estimate, so measuring never caches mid-frame.
fn gated_measure(text: &str, style: &TextStyle, font_size: u16, scale: f32) -> TextDimensions {
    let size = atlas_size(font_size, screen_dpi_scale());
    if gate_admits(text, style.font.as_ref(), size) {
        measure_text(text, style.font.as_ref(), font_size, scale)
    } else {
        estimate(text, style)
    }
}

/// A plausible box for text whose glyphs are not cached yet. It is only used
/// for a frame in which the text itself is not drawn.
fn estimate(text: &str, style: &TextStyle) -> TextDimensions {
    TextDimensions {
        width: text.chars().count() as f32 * style.size * 0.55,
        height: style.size,
        offset_y: style.size * 0.8,
    }
}

/// Pixels per point measurements rasterise at: fine enough that rounding the
/// raster size moves a width by well under a pixel.
const MEASURE_SCALE: f32 = 4.0;

/// The glyph size to rasterise `size` points at `k` pixels per point, and the
/// scale that brings it back to exactly `size`.
///
/// macroquad rasterises at whole pixel sizes. Scaling by `1 / k` would draw
/// at `round(size * k) / k` points, which is not the size the layout measured
/// at (fractional sizes, such as a title shrunk to fit, come out wider than
/// measured and swallow the spaces between words); scaling by
/// `size / round(size * k)` draws at exactly `size`, whatever `k` is.
fn raster(size: f32, k: f32) -> (u16, f32) {
    let px = (size * k).round().clamp(1.0, u16::MAX as f32);
    (px as u16, size.max(0.0) / px)
}

/// The font size and scale to hand macroquad so `size` points rasterise at
/// `pixel_scale` device pixels per point, on a window whose DPI scale is `dpi`.
///
/// macroquad multiplies the font size it is given by the window's DPI scale
/// before it rasterises (`ceil(font_size * dpi)`). `pixel_scale` already
/// includes that scale, so it comes out here: passing `size * pixel_scale`
/// straight through rasterised every glyph at `dpi` times the size it lands
/// at, four times the atlas area on a 2x display, and downsampled it.
fn draw_raster(size: f32, pixel_scale: f32, dpi: f32) -> (u16, f32) {
    raster(size, pixel_scale.max(0.01) / dpi.max(0.01))
}

/// The size macroquad caches a glyph under for a font size it was given: the
/// key its atlas is filled by.
fn atlas_size(font_size: u16, dpi: f32) -> u16 {
    (font_size as f32 * dpi).ceil().clamp(1.0, u16::MAX as f32) as u16
}

/// Measures at the size text will be drawn at, `pixel_scale` device pixels per
/// point: the same glyphs [`TextLayout::draw`] rasterises, so layout and drawing
/// share one set of atlas entries and agree to the pixel.
///
/// [`MacroquadMeasure`] measures at a fixed fine raster instead, which is
/// independent of the screen but caches a second, much larger, copy of every
/// glyph. Prefer this one when the pixel scale is known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureAt(pub f32);

impl MeasureAt {
    fn measure(&self, text: &str, style: &TextStyle) -> TextDimensions {
        let (px, scale) = draw_raster(style.size, self.0, screen_dpi_scale());
        gated_measure(text, style, px, scale)
    }
}

impl Measure for MeasureAt {
    fn width(&self, text: &str, style: &TextStyle) -> f32 {
        self.measure(text, style).width
    }

    fn ascent(&self, style: &TextStyle) -> f32 {
        self.measure("Mg", style).offset_y
    }

    fn descent(&self, style: &TextStyle) -> f32 {
        let d = self.measure("Mg", style);
        (d.height - d.offset_y).max(0.0)
    }
}

/// The characters a size is warmed with in one go.
fn printable_ascii() -> impl Iterator<Item = char> {
    (0x20u8..=0x7e).map(char::from)
}

/// Cache every printable ASCII glyph `style` will draw at `pixel_scale`.
///
/// See [`enable_glyph_gate`] for why caching ahead of drawing matters, and for
/// a way to do it without listing the styles up front.
pub fn warm_font_cache(style: &TextStyle, pixel_scale: f32) {
    let Some(font) = style.font.as_ref() else {
        return;
    };
    let dpi = screen_dpi_scale();
    let (px, _) = draw_raster(style.size, pixel_scale, dpi);
    let characters: Vec<char> = printable_ascii().collect();
    font.populate_font_cache(&characters, atlas_size(px, dpi));
}

/// Turn on the glyph gate: from now on, text whose glyphs are not cached yet is
/// held back for a frame, and [`flush_glyphs`] caches it all at the end of the
/// frame, so each font atlas uploads at most once per frame.
///
/// ## Why
///
/// macroquad keeps each font's glyphs in one CPU image and one texture. Caching
/// a glyph marks the image dirty, and the next `draw_text_ex` uploads the
/// *whole* image again (`Atlas::texture`). Draws are batched to the end of the
/// frame, but uploads are not: a frame that lays out and draws a hundred runs,
/// each bringing a new glyph or size, uploads the atlas a hundred times. On
/// macOS's OpenGL each upload is staged in its own buffer until the frame is
/// submitted, and the driver keeps the buffers afterwards; on the web each one
/// is a stall. A 2048² atlas is 16 MB, so one text-heavy first frame left
/// gigabytes of graphics memory behind (measured in solada-ledger: 166 uploads
/// in one frame, 129 retained 16 MB regions).
///
/// ## How
///
/// Call this once, before the first frame, and load fonts with [`load_font`]
/// (or pass them to [`register_font`]). Then call [`flush_glyphs`] once per
/// frame, after the last text is drawn. Everything else is automatic: the
/// measures and [`TextLayout::draw`] consult the gate, so a layout whose
/// glyphs are missing is measured with an estimate and not drawn, and appears
/// the next frame from one upload.
///
/// A new size is warmed with all of printable ASCII, so numbers that change
/// every frame never miss; other characters are cached as they appear. The
/// cost is that text at a size or character never seen before appears one
/// frame late. A [`Preloader`](crate::Preloader) hides the first of those.
///
/// Until this is called the gate admits everything and text caches as it
/// draws, as macroquad does on its own.
///
/// The gate keys on raster sizes, not fonts: [`flush_glyphs`] warms every
/// registered font at each held-back size. A font that is used without being
/// registered still draws, but caches its glyphs mid-frame.
pub fn enable_glyph_gate() {
    GATE.with_borrow_mut(|gate| gate.on = true);
}

/// Load a TTF font and register it with the glyph gate.
pub fn load_font(bytes: &[u8]) -> Result<Font, macroquad::Error> {
    let font = load_ttf_font_from_bytes(bytes)?;
    register_font(&font);
    Ok(font)
}

/// Tell the glyph gate about a font loaded some other way. Register each font
/// once: the gate cannot tell two handles to one font apart.
pub fn register_font(font: &Font) {
    GATE.with_borrow_mut(|gate| gate.fonts.push(font.clone()));
}

/// Whether the gate is holding text back for glyphs it has not cached yet.
pub fn glyphs_pending() -> bool {
    GATE.with_borrow(|gate| !gate.state.is_empty())
}

/// What one [`flush_glyphs`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphFlush {
    /// Sizes and characters cached this call.
    pub cached: usize,
    /// Left for the next call because the budget ran out.
    pub remaining: usize,
}

/// Cache what the gate held back this frame. Call once per frame, after the
/// last text is drawn.
///
/// `budget` is seconds of work to allow: rasterising a new size is a few
/// milliseconds per font in a release build, several times that in debug, and
/// on the web it all happens on the one thread that also draws. With a budget,
/// at least one size is cached and the rest wait for the next frame, so a
/// screen that suddenly needs many sizes fills in over a few frames rather
/// than pausing. `None` caches everything now: right behind a preloader.
pub fn flush_glyphs(budget: Option<f64>) -> GlyphFlush {
    let start = get_time();
    GATE.with_borrow_mut(|gate| {
        let ascii: Vec<char> = printable_ascii().collect();
        let default_font = gate.default_font.then(Font::default);
        let mut cached = 0;
        while let Some(job) = gate.state.next_job() {
            let fonts = gate.fonts.iter().chain(default_font.as_ref());
            match job {
                Job::Size(size) => fonts.for_each(|font| font.populate_font_cache(&ascii, size)),
                Job::Char(size, ch) => fonts.for_each(|font| font.populate_font_cache(&[ch], size)),
            }
            gate.state.done(job);
            cached += 1;
            if budget.is_some_and(|budget| get_time() - start >= budget) {
                break;
            }
        }
        GlyphFlush {
            cached,
            remaining: gate.state.len(),
        }
    })
}

/// Whether the gate lets `text` through at atlas size `size`; records what is
/// missing when it does not.
fn gate_admits(text: &str, font: Option<&Font>, size: u16) -> bool {
    GATE.with_borrow_mut(|gate| {
        if !gate.on {
            return true;
        }
        if font.is_none() {
            gate.default_font = true;
        }
        gate.state.admit(text, size)
    })
}

thread_local! {
    /// macroquad draws from one thread, so the gate is per thread.
    static GATE: RefCell<Gate> = RefCell::new(Gate::default());
}

#[derive(Default)]
struct Gate {
    on: bool,
    fonts: Vec<Font>,
    /// A style without a font was seen: warm macroquad's own too.
    default_font: bool,
    state: GateState,
}

/// One unit of caching work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// All of printable ASCII at an atlas size.
    Size(u16),
    /// One other character at an atlas size.
    Char(u16, char),
}

#[derive(Debug, Default)]
struct GateState {
    /// Atlas sizes whose printable ASCII is cached in every font.
    warm: HashSet<u16>,
    /// Other characters cached at an atlas size.
    extra: HashSet<(u16, char)>,
    pending_sizes: BTreeSet<u16>,
    pending_chars: BTreeSet<(u16, char)>,
}

impl GateState {
    /// Whether every glyph `text` needs at atlas size `size` is cached;
    /// otherwise remember what is missing.
    fn admit(&mut self, text: &str, size: u16) -> bool {
        let mut ready = true;
        if !self.warm.contains(&size) {
            self.pending_sizes.insert(size);
            ready = false;
        }
        for ch in text.chars() {
            // Layout breaks on whitespace, so it never reaches the atlas.
            if ch.is_ascii_graphic() || ch.is_whitespace() {
                continue;
            }
            if !self.extra.contains(&(size, ch)) {
                self.pending_chars.insert((size, ch));
                ready = false;
            }
        }
        ready
    }

    /// The next thing to cache: odd characters first, as they are cheap.
    fn next_job(&self) -> Option<Job> {
        self.pending_chars
            .first()
            .map(|&(size, ch)| Job::Char(size, ch))
            .or_else(|| self.pending_sizes.first().map(|&size| Job::Size(size)))
    }

    /// Mark a job cached.
    fn done(&mut self, job: Job) {
        match job {
            Job::Size(size) => {
                self.pending_sizes.remove(&size);
                self.warm.insert(size);
            }
            Job::Char(size, ch) => {
                self.pending_chars.remove(&(size, ch));
                self.extra.insert((size, ch));
            }
        }
    }

    fn len(&self) -> usize {
        self.pending_sizes.len() + self.pending_chars.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One positioned piece of a laid-out [`RichText`], relative to its top-left.
#[derive(Clone, Debug, PartialEq)]
pub enum Placed {
    /// A run of text; `pos` is the left end of its baseline.
    Text {
        text: String,
        style: StyleId,
        pos: Vec2,
    },
    Icon {
        id: u32,
        rect: Rect,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub items: Vec<Placed>,
    /// Width of the widest line, and total height.
    pub size: Vec2,
    pub lines: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RichText {
    styles: Vec<TextStyle>,
    spans: Vec<Span>,
}

/// An unbreakable stretch: the pieces between two spaces.
#[derive(Clone, Debug)]
struct Word {
    parts: Vec<Part>,
    width: f32,
    /// Width of the space before it, in the style that space was written in.
    space_before: f32,
    /// Starts a new line (it followed a `\n`).
    hard_break: bool,
}

#[derive(Clone, Debug)]
enum Part {
    Text {
        text: String,
        style: StyleId,
        width: f32,
    },
    Icon {
        id: u32,
        size: Vec2,
        style: StyleId,
    },
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style(&mut self, style: TextStyle) -> StyleId {
        self.styles.push(style);
        StyleId(self.styles.len() - 1)
    }

    pub fn styles(&self) -> &[TextStyle] {
        &self.styles
    }

    /// Append text in `style`. Spaces and newlines are break points; a run
    /// that starts mid-word joins the word before it.
    pub fn text(&mut self, text: &str, style: StyleId) -> &mut Self {
        self.spans.push(Span::Text {
            text: text.to_string(),
            style,
        });
        self
    }

    /// Append an inline box the caller draws. It sits in the flow like a word
    /// and is centred on the text it rides with; `style` sets which text that
    /// is, for spacing and alignment.
    pub fn icon(&mut self, id: u32, size: Vec2, style: StyleId) -> &mut Self {
        self.spans.push(Span::Icon { id, size, style });
        self
    }

    fn words(&self, m: &impl Measure) -> Vec<Word> {
        let mut words = Vec::new();
        let mut cur = Word {
            parts: Vec::new(),
            width: 0.0,
            space_before: 0.0,
            hard_break: false,
        };
        let mut pending_space = 0.0;
        let mut pending_break = false;
        let flush = |cur: &mut Word, words: &mut Vec<Word>| {
            if !cur.parts.is_empty() {
                words.push(std::mem::replace(
                    cur,
                    Word {
                        parts: Vec::new(),
                        width: 0.0,
                        space_before: 0.0,
                        hard_break: false,
                    },
                ));
            }
        };
        for span in &self.spans {
            match span {
                Span::Icon { id, size, style } => {
                    if cur.parts.is_empty() {
                        cur.space_before = pending_space;
                        cur.hard_break = pending_break;
                        pending_space = 0.0;
                        pending_break = false;
                    }
                    cur.parts.push(Part::Icon {
                        id: *id,
                        size: *size,
                        style: *style,
                    });
                    cur.width += size.x;
                }
                Span::Text { text, style } => {
                    let s = &self.styles[style.0];
                    let mut chunk = String::new();
                    let push_chunk = |chunk: &mut String, cur: &mut Word| {
                        if !chunk.is_empty() {
                            let width = m.width(chunk, s);
                            cur.width += width;
                            cur.parts.push(Part::Text {
                                text: std::mem::take(chunk),
                                style: *style,
                                width,
                            });
                        }
                    };
                    for ch in text.chars() {
                        if ch == '\n' || ch.is_whitespace() {
                            push_chunk(&mut chunk, &mut cur);
                            flush(&mut cur, &mut words);
                            if ch == '\n' {
                                pending_break = true;
                                pending_space = 0.0;
                            } else if !pending_break {
                                pending_space = m.width(" ", s);
                            }
                        } else {
                            if cur.parts.is_empty() && chunk.is_empty() {
                                cur.space_before = pending_space;
                                cur.hard_break = pending_break;
                                pending_space = 0.0;
                                pending_break = false;
                            }
                            chunk.push(ch);
                        }
                    }
                    push_chunk(&mut chunk, &mut cur);
                }
            }
        }
        flush(&mut cur, &mut words);
        words
    }

    /// Lay out within `max_width`, aligning each line. `line_gap` is extra
    /// space between lines.
    pub fn layout(
        &self,
        m: &impl Measure,
        max_width: f32,
        align: Align,
        line_gap: f32,
    ) -> TextLayout {
        // Break into lines of words.
        let mut lines: Vec<Vec<Word>> = vec![Vec::new()];
        let mut x = 0.0;
        for w in self.words(m) {
            let line = lines.last_mut().expect("at least one line");
            let fits = x + w.space_before + w.width <= max_width;
            if !line.is_empty() && (w.hard_break || !fits) {
                lines.push(Vec::new());
                x = 0.0;
            }
            let line = lines.last_mut().expect("at least one line");
            x += if line.is_empty() { 0.0 } else { w.space_before } + w.width;
            line.push(w);
        }

        let mut items = Vec::new();
        let mut top = 0.0;
        let mut widest: f32 = 0.0;
        let n = lines.len();
        for (li, line) in lines.iter().enumerate() {
            // Metrics from the text styles on the line, or the first style.
            let mut ascent: f32 = 0.0;
            let mut descent: f32 = 0.0;
            let mut icon_h: f32 = 0.0;
            for w in line {
                for p in &w.parts {
                    match p {
                        Part::Text { style, .. } => {
                            let s = &self.styles[style.0];
                            ascent = ascent.max(m.ascent(s));
                            descent = descent.max(m.descent(s));
                        }
                        Part::Icon { size, style, .. } => {
                            let s = &self.styles[style.0];
                            ascent = ascent.max(m.ascent(s));
                            descent = descent.max(m.descent(s));
                            icon_h = icon_h.max(size.y);
                        }
                    }
                }
            }
            if line.is_empty()
                && let Some(s) = self.styles.first()
            {
                ascent = m.ascent(s);
                descent = m.descent(s);
            }
            // An icon taller than the text pushes the line open evenly.
            let text_h = ascent + descent;
            let extra = ((icon_h - text_h) / 2.0).max(0.0);
            let baseline = top + extra + ascent;
            let mid = baseline - (ascent - descent) / 2.0;

            let width: f32 = line
                .iter()
                .enumerate()
                .map(|(i, w)| w.width + if i == 0 { 0.0 } else { w.space_before })
                .sum();
            widest = widest.max(width);
            let mut x = match align {
                Align::Start => 0.0,
                Align::Center => (max_width - width) / 2.0,
                Align::End => max_width - width,
            };
            for (i, w) in line.iter().enumerate() {
                if i > 0 {
                    x += w.space_before;
                }
                for p in &w.parts {
                    match p {
                        Part::Text { text, style, width } => {
                            items.push(Placed::Text {
                                text: text.clone(),
                                style: *style,
                                pos: vec2(x, baseline),
                            });
                            x += width;
                        }
                        Part::Icon { id, size, .. } => {
                            items.push(Placed::Icon {
                                id: *id,
                                rect: Rect::new(x, mid - size.y / 2.0, size.x, size.y),
                            });
                            x += size.x;
                        }
                    }
                }
            }
            top += text_h + 2.0 * extra;
            if li + 1 < n {
                top += line_gap;
            }
        }
        TextLayout {
            items,
            size: vec2(widest, top),
            lines: n,
        }
    }
}

impl TextLayout {
    /// Draw at `origin` (top-left). `pixel_scale` is how many device pixels a
    /// point covers where this lands (the screen's DPI scale, times any
    /// camera zoom or bake scale): glyphs rasterise at that size, so text stays
    /// sharp rather than being stretched. `icon` draws each inline box.
    ///
    /// With the [glyph gate](enable_glyph_gate) on, a layout whose glyphs are
    /// not all cached draws nothing this frame, icons included, rather than
    /// appearing piece by piece.
    pub fn draw(
        &self,
        text: &RichText,
        origin: Vec2,
        pixel_scale: f32,
        alpha: f32,
        icon: impl FnMut(u32, Rect),
    ) {
        self.draw_scaled(text, origin, pixel_scale, 1.0, alpha, icon);
    }

    /// Draw scaled by `scale` about `origin`, for text that pops, pulses or
    /// zooms. Glyphs are rasterised once, at the laid-out size, and the quads
    /// are scaled: animating the style's size instead would cache a new set
    /// of glyphs every frame of the animation.
    pub fn draw_scaled(
        &self,
        text: &RichText,
        origin: Vec2,
        pixel_scale: f32,
        scale: f32,
        alpha: f32,
        mut icon: impl FnMut(u32, Rect),
    ) {
        let k = pixel_scale.max(0.01);
        let dpi = screen_dpi_scale();
        // Every run is checked, so a frame records all that is missing.
        let mut ready = true;
        for item in &self.items {
            if let Placed::Text { text: s, style, .. } = item {
                let st = &text.styles[style.0];
                let (px, _) = draw_raster(st.size, k, dpi);
                ready &= gate_admits(s, st.font.as_ref(), atlas_size(px, dpi));
            }
        }
        if !ready {
            return;
        }
        let fade = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
        for item in &self.items {
            match item {
                Placed::Text {
                    text: s,
                    style,
                    pos,
                } => {
                    let st = &text.styles[style.0];
                    let at = origin + *pos * scale;
                    let raster = draw_raster(st.size, k, dpi);
                    let params = |color: Color| TextParams {
                        font: st.font.as_ref(),
                        font_size: raster.0,
                        font_scale: raster.1 * scale,
                        color: fade(color),
                        ..Default::default()
                    };
                    if let Some(sh) = st.shadow {
                        let off = sh.offset * scale;
                        draw_text_ex(s, at.x + off.x, at.y + off.y, params(sh.color));
                    }
                    if let Some(o) = st.outline {
                        for (dx, dy) in OUTLINE_DIRS {
                            draw_text_ex(
                                s,
                                at.x + dx * o.width * scale,
                                at.y + dy * o.width * scale,
                                params(o.color),
                            );
                        }
                    }
                    draw_text_ex(s, at.x, at.y, params(st.color));
                    if crate::harness::report::on() {
                        // At the draw raster: the box of what was drawn, and
                        // no second set of glyphs cached mid-frame to get it.
                        let m = MeasureAt(k);
                        let ascent = m.ascent(st) * scale;
                        let rect = Rect::new(
                            at.x,
                            at.y - ascent,
                            m.width(s, st) * scale,
                            ascent + m.descent(st) * scale,
                        );
                        crate::harness::report::line(s, rect, st.size * scale, alpha);
                    }
                }
                Placed::Icon { id, rect } => {
                    icon(
                        *id,
                        Rect::new(
                            origin.x + rect.x * scale,
                            origin.y + rect.y * scale,
                            rect.w * scale,
                            rect.h * scale,
                        ),
                    );
                }
            }
        }
    }
}

const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Eight directions for an outline drawn by offset copies.
const OUTLINE_DIRS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (D, D),
    (-D, D),
    (D, -D),
    (-D, -D),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raster_draws_at_exactly_the_size_asked_at_any_pixel_scale() {
        for size in [9.0, 10.25, 11.6, 12.9, 31.5] {
            for k in [1.0, 1.5, 2.0, 2.625, 3.0, MEASURE_SCALE] {
                let (px, scale) = raster(size, k);
                assert_eq!(px, (size * k).round() as u16);
                let drawn = px as f32 * scale;
                assert!((drawn - size).abs() < 1e-4, "{size} at {k}x drew {drawn}");
            }
        }
        assert_eq!(raster(0.0, 2.0).0, 1, "never a zero-pixel raster");
    }

    #[test]
    fn a_draw_rasterises_at_the_device_pixels_it_lands_on_not_dpi_times_more() {
        // 10 points at 2.31 device px per point on a 2x display: about 23
        // device pixels, not the 46 the DPI-blind raster asked for.
        let (font_size, scale) = draw_raster(10.0, 2.31, 2.0);
        let atlas = atlas_size(font_size, 2.0);
        assert_eq!(font_size, 12);
        assert_eq!(atlas, 24);
        // What macroquad draws: the atlas raster back in points, scaled.
        let drawn = atlas as f32 / 2.0 * scale;
        assert!((drawn - 10.0).abs() < 1e-4, "drew {drawn}");
        // On a 1x display nothing changes.
        assert_eq!(draw_raster(10.0, 1.5, 1.0), raster(10.0, 1.5));
    }

    #[test]
    fn the_gate_holds_back_new_sizes_until_flushed_then_admits_them() {
        let mut gate = GateState::default();
        assert!(!gate.admit("12,345 ADA", 24), "a size never seen");
        assert!(!gate.admit("BUY", 24), "still pending this frame");
        assert_eq!(drain(&mut gate), vec![Job::Size(24)]);
        assert!(gate.admit("98,760 ADA", 24), "any ASCII at a warm size");
        assert!(!gate.admit("BUY", 18), "a different size is not warm");
    }

    /// Run every pending job, as an unbudgeted flush does.
    fn drain(gate: &mut GateState) -> Vec<Job> {
        let mut jobs = Vec::new();
        while let Some(job) = gate.next_job() {
            gate.done(job);
            jobs.push(job);
        }
        jobs
    }

    #[test]
    fn the_gate_caches_other_characters_one_by_one_at_a_warm_size() {
        let mut gate = GateState::default();
        gate.admit("x", 24);
        drain(&mut gate);
        assert!(!gate.admit("A → B", 24));
        assert_eq!(
            drain(&mut gate),
            vec![Job::Char(24, '→')],
            "the size was already warm"
        );
        assert!(gate.admit("C → D", 24));
        assert!(
            gate.admit("tab\tand\u{a0}nbsp", 24),
            "whitespace is never drawn"
        );
    }

    #[test]
    fn a_budgeted_flush_leaves_the_rest_held_back_until_it_is_cached() {
        let mut gate = GateState::default();
        gate.admit("a", 12);
        gate.admit("b", 18);
        gate.admit("é", 12);
        // Characters first, then sizes, smallest first.
        let first = gate.next_job().unwrap();
        assert_eq!(first, Job::Char(12, 'é'));
        gate.done(first);
        let second = gate.next_job().unwrap();
        assert_eq!(second, Job::Size(12));
        gate.done(second);
        assert_eq!(gate.len(), 1, "18 waits for the next frame");
        assert!(!gate.admit("b", 18), "and its text stays held back");
        assert!(gate.admit("é a", 12));
    }

    /// Every character is `size / 2` wide; ascent is 0.8 of the size and
    /// descent 0.2, so a line is exactly `size` tall.
    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, t: &str, s: &TextStyle) -> f32 {
            t.chars().count() as f32 * s.size / 2.0
        }
        fn ascent(&self, s: &TextStyle) -> f32 {
            s.size * 0.8
        }
        fn descent(&self, s: &TextStyle) -> f32 {
            s.size * 0.2
        }
    }

    fn style(size: f32) -> TextStyle {
        TextStyle::new(None, size, WHITE)
    }

    fn texts(l: &TextLayout) -> Vec<(String, f32, f32)> {
        l.items
            .iter()
            .filter_map(|i| match i {
                Placed::Text { text, pos, .. } => Some((text.clone(), pos.x, pos.y)),
                Placed::Icon { .. } => None,
            })
            .collect()
    }

    #[test]
    fn words_wrap_at_spaces_to_fit_the_width() {
        let mut t = RichText::new();
        let s = t.style(style(10.0)); // 5 per char, space 5
        t.text("aaaa bbbb cccc", s);
        // "aaaa bbbb" is 45 wide; with "cccc" it would be 70.
        let l = t.layout(&Fixed, 50.0, Align::Start, 0.0);
        assert_eq!(l.lines, 2);
        assert_eq!(
            texts(&l),
            vec![
                ("aaaa".into(), 0.0, 8.0),
                ("bbbb".into(), 25.0, 8.0),
                ("cccc".into(), 0.0, 18.0)
            ]
        );
        assert_eq!(l.size, vec2(45.0, 20.0));
    }

    #[test]
    fn a_style_change_mid_word_does_not_break_it() {
        let mut t = RichText::new();
        let a = t.style(style(10.0));
        let b = t.style(style(10.0));
        t.text("Deal ", a).text("4", b).text("x more", a);
        // "4x" is one word in two styles: never split across lines.
        let l = t.layout(&Fixed, 25.0, Align::Start, 0.0);
        let t = texts(&l);
        assert_eq!(t[1], ("4".into(), 0.0, 18.0));
        assert_eq!(t[2], ("x".into(), 5.0, 18.0));
    }

    #[test]
    fn centred_lines_are_centred_each_on_its_own() {
        let mut t = RichText::new();
        let s = t.style(style(10.0));
        t.text("aa\naaaa", s);
        let l = t.layout(&Fixed, 100.0, Align::Center, 0.0);
        let t = texts(&l);
        assert_eq!(t[0].1, 45.0);
        assert_eq!(t[1].1, 40.0);
    }

    #[test]
    fn an_icon_sits_in_the_flow_centred_on_the_text() {
        let mut t = RichText::new();
        let s = t.style(style(10.0));
        t.text("Gain ", s)
            .icon(7, vec2(10.0, 10.0), s)
            .text(" now", s);
        let l = t.layout(&Fixed, 200.0, Align::Start, 0.0);
        let icon = l
            .items
            .iter()
            .find_map(|i| match i {
                Placed::Icon { id, rect } => Some((*id, *rect)),
                _ => None,
            })
            .unwrap();
        assert_eq!(icon.0, 7);
        assert_eq!(icon.1.x, 25.0, "after 'Gain' and a space");
        // Text spans y 0..10 (baseline 8); the icon is centred on it.
        assert_eq!(icon.1.y, 0.0);
        assert_eq!(texts(&l)[1].1, 40.0, "'now' follows the icon and a space");
    }

    #[test]
    fn a_tall_icon_opens_the_line_evenly() {
        let mut t = RichText::new();
        let s = t.style(style(10.0));
        t.text("x ", s).icon(1, vec2(20.0, 20.0), s);
        let l = t.layout(&Fixed, 200.0, Align::Start, 0.0);
        assert_eq!(l.size.y, 20.0);
        assert_eq!(
            texts(&l)[0].2,
            13.0,
            "baseline pushed down by half the extra"
        );
    }

    #[test]
    fn line_gap_separates_lines_but_not_the_last() {
        let mut t = RichText::new();
        let s = t.style(style(10.0));
        t.text("a\nb\nc", s);
        let l = t.layout(&Fixed, 100.0, Align::Start, 4.0);
        assert_eq!(l.size.y, 38.0);
    }

    #[test]
    fn an_overlong_word_overflows_rather_than_vanishing() {
        let mut t = RichText::new();
        let s = t.style(style(10.0));
        t.text("abcdefghij", s);
        let l = t.layout(&Fixed, 20.0, Align::Start, 0.0);
        assert_eq!(l.lines, 1);
        assert_eq!(l.size.x, 50.0);
    }

    #[test]
    fn empty_text_lays_out_to_nothing() {
        let t = RichText::new();
        let l = t.layout(&Fixed, 100.0, Align::Start, 0.0);
        assert!(l.items.is_empty());
    }
}
