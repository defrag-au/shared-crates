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
//! [`Measure`] trait ([`MeasureAt`](crate::MeasureAt) on a screen, a
//! fixed-width fake in tests), so wrapping is unit-tested without a window, and
//! a parley-backed layout could replace this module without its callers
//! changing.
//!
//! What it does not do yet, and parley would: shaping for complex scripts,
//! Unicode line-break rules beyond spaces and newlines, and bidi. A single word
//! wider than the box overflows rather than breaking mid-word.

use macroquad::prelude::*;

use crate::gate::admits;
use crate::measure::Measure;
use crate::raster::{atlas_size, draw_raster};
use crate::run::paint;
use crate::style::TextStyle;

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
    /// With the [glyph gate](crate::enable_glyph_gate) on, a layout whose
    /// glyphs are not all cached draws nothing this frame, icons included,
    /// rather than appearing piece by piece.
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
                ready &= admits(s, st.font.as_ref(), atlas_size(px, dpi));
            }
        }
        if !ready {
            return;
        }
        for item in &self.items {
            match item {
                Placed::Text {
                    text: s,
                    style,
                    pos,
                } => {
                    let st = &text.styles[style.0];
                    paint(s, st, origin + *pos * scale, k, scale, alpha);
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

#[cfg(test)]
mod tests {
    use super::*;

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
