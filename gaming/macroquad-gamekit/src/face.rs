//! `face` — the parts of a printed card face: dark plates, cut gems, stat emblems and tabs, and names that shrink before they are cut.
//!
//! [`crate::paint`] has the materials (gradient metal, grain, bevels, inner
//! shadows); this has the parts built from them that every card face in the
//! estate repeats. `CARD-FACES.md` is how they go together. They were written
//! once in the fight spike's `card.rs` and needed again by the NFT cards in
//! `macroquad-cnft`; a gem drawn two ways in one estate is how two games stop
//! looking like one.
//!
//! Text goes through `macroquad-text`, so a capture's report sees it. Each
//! part that carries text takes it as an `Option`: `None` draws the part
//! bare, for a face whose words would be covered (half under a header, off
//! the screen) and so should not be drawn and reported.

use macroquad::prelude::*;

use crate::icon::{Icon, draw_icon};
use crate::paint::{Gradient, bevel, fill_circle_gradient, fill_gradient};
use crate::shape::{fill_rounded, stroke_rounded};
use macroquad_text::{Align, Measure, MeasureAt, RichText, TextStyle};

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// One line of text in one style, its box placed by `anchor` (0,0 top-left,
/// 0.5,0.5 centre) at `at`, laid out at `px` pixels per unit. Returns the
/// box.
pub fn line(s: &str, style: TextStyle, at: Vec2, anchor: Vec2, px: f32) -> Rect {
    let mut t = RichText::new();
    let id = t.style(style);
    t.text(s, id);
    let l = t.layout(&MeasureAt(px), f32::INFINITY, Align::Start, 0.0);
    let origin = at - l.size * anchor;
    l.draw(&t, origin, px, 1.0, |_, _| {});
    Rect::new(origin.x, origin.y, l.size.x, l.size.y)
}

/// The largest size from `max` down to `min` at which `s` fits `width` in
/// `font` at `px`, else `s` cut with an ellipsis at `min`.
///
/// What a printed card's title does: a long name sets smaller before it is
/// ever cut. Steps by a quarter point, so the size is one of a few, not a new
/// glyph size per name.
pub fn shrink_to_fit(
    font: Option<&Font>,
    s: &str,
    (max, min): (f32, f32),
    width: f32,
    px: f32,
) -> (String, f32) {
    let m = MeasureAt(px);
    fit_with(s, (max, min), width, |text, size| {
        m.width(text, &TextStyle::new(font.cloned(), size, WHITE))
    })
}

/// [`shrink_to_fit`] with the measuring passed in, so the choice of size and
/// cut is tested without a window (measuring real glyphs needs one).
fn fit_with(
    s: &str,
    (max, min): (f32, f32),
    width: f32,
    measure: impl Fn(&str, f32) -> f32,
) -> (String, f32) {
    let mut size = max;
    while size > min {
        if measure(s, size) <= width {
            return (s.to_string(), size);
        }
        size -= 0.25;
    }
    if measure(s, min) <= width {
        return (s.to_string(), min);
    }
    let chars: Vec<char> = s.chars().collect();
    for keep in (0..chars.len()).rev() {
        let cut = format!("{}…", chars[..keep].iter().collect::<String>().trim_end());
        if keep == 0 || measure(&cut, min) <= width {
            return (cut, min);
        }
    }
    (String::new(), min)
}

/// A dark plate, for a title or a type line: a vertical gradient and a
/// bevel that catches the light.
pub fn plate(r: Rect, radius: f32, (top, bottom): (Color, Color)) {
    fill_gradient(r, radius, Gradient::Vertical { top, bottom });
    bevel(
        r,
        radius,
        1.0,
        with_alpha(WHITE, 0.18),
        Color::new(0.0, 0.0, 0.0, 0.5),
    );
}

/// A cut gem at `at`, `r` across the body: a drop shadow, a dark setting, a
/// radial body from `body.0` at the centre to `body.1` at the rim, a facet
/// ring and a specular glint, and `label` on it.
pub fn gem(at: Vec2, r: f32, body: (Color, Color), label: Option<(&str, TextStyle)>, px: f32) {
    draw_circle(
        at.x + 0.8,
        at.y + 1.4,
        r + 2.2,
        Color::new(0.0, 0.0, 0.0, 0.5),
    );
    fill_circle_gradient(
        at,
        r + 2.0,
        Color::from_rgba(0x3a, 0x3f, 0x4d, 255),
        Color::from_rgba(0x0b, 0x0c, 0x10, 255),
    );
    fill_circle_gradient(at, r, body.0, body.1);
    draw_circle_lines(at.x, at.y, r * 0.78, 0.8, with_alpha(WHITE, 0.25));
    draw_ellipse(
        at.x - r * 0.32,
        at.y - r * 0.42,
        r * 0.38,
        r * 0.2,
        -25.0,
        with_alpha(WHITE, 0.55),
    );
    if let Some((s, style)) = label {
        line(s, style, at + vec2(0.0, r * 0.04), Vec2::splat(0.5), px);
    }
}

/// Where an emblem's parts go, as fractions of its height `h`. Pure, so the
/// spacing is tested without a window.
///
/// The first version set the number at nine-tenths of the pill's height with
/// the icon hard against the rim: legible, but crammed. These leave air
/// round both: a number about two-thirds of the height, an icon half of it,
/// a third of the height before the icon, and a little more after the number
/// than before the icon, because numerals sit visually left of their box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EmblemLayout {
    w: f32,
    icon_x: f32,
    icon_s: f32,
    num_x: f32,
    num_size: f32,
}

impl EmblemLayout {
    /// The number's size, which its measured width depends on.
    fn num_size(h: f32) -> f32 {
        h * 0.64
    }

    fn new(h: f32, num_w: f32) -> Self {
        let icon_s = h * 0.48;
        let icon_x = h * 0.32;
        let num_x = icon_x + icon_s + h * 0.2;
        // At least a short capsule, so a single digit does not sit in what
        // reads as a circle with an icon jammed into it.
        let w = (num_x + num_w + h * 0.38).max(h * 1.9);
        Self {
            w,
            icon_x,
            icon_s,
            num_x,
            num_size: Self::num_size(h),
        }
    }
}

/// A stat emblem: a dark pill `h` tall edged in `color`, with `icon` and its
/// number, room round both. Returns the pill, whose width follows the number.
///
/// What a card shows at a glance, in the strip a fanned hand leaves showing:
/// stack them down the art's left edge, a quarter of their height apart.
pub fn emblem(
    top_left: Vec2,
    h: f32,
    icon: Icon,
    color: Color,
    number: (&str, Option<&Font>),
    show_number: bool,
    px: f32,
) -> Rect {
    let (number, font) = number;
    let num_size = EmblemLayout::num_size(h);
    let num_w = MeasureAt(px).width(number, &TextStyle::new(font.cloned(), num_size, WHITE));
    let l = EmblemLayout::new(h, num_w);
    let pill = Rect::new(top_left.x, top_left.y, l.w, h);
    fill_rounded(
        Rect::new(pill.x + 1.0, pill.y + 1.5, pill.w, pill.h),
        h / 2.0,
        Color::new(0.0, 0.0, 0.0, 0.45),
    );
    fill_gradient(
        pill,
        h / 2.0,
        Gradient::Vertical {
            top: Color::from_rgba(38, 42, 54, 240),
            bottom: Color::from_rgba(10, 11, 16, 240),
        },
    );
    stroke_rounded(pill, h / 2.0, 1.4, color);
    stroke_rounded(
        Rect::new(pill.x + 1.6, pill.y + 1.6, pill.w - 3.2, pill.h - 3.2),
        h / 2.0 - 1.6,
        0.8,
        with_alpha(WHITE, 0.12),
    );
    let mid = pill.y + h / 2.0;
    draw_icon(
        icon,
        Rect::new(pill.x + l.icon_x, mid - l.icon_s / 2.0, l.icon_s, l.icon_s),
        color,
    );
    if show_number {
        let style = TextStyle::new(font.cloned(), l.num_size, WHITE)
            .outline(Color::new(0.0, 0.0, 0.0, 0.7), l.num_size * 0.05);
        line(
            number,
            style,
            vec2(pill.x + l.num_x, mid),
            vec2(0.0, 0.5),
            px,
        );
    }
    pill
}

/// Where a stat tab's parts go: the icon after the rounded end, the number
/// against the flat one. Fractions of the height `h`, as for the emblem.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TabLayout {
    icon_x: f32,
    icon_s: f32,
    /// The number's right edge, measured from the tab's left.
    num_right: f32,
    num_size: f32,
}

impl TabLayout {
    fn new(h: f32, w: f32) -> Self {
        Self {
            icon_x: h * 0.34,
            icon_s: h * 0.48,
            num_right: w - h * 0.3,
            num_size: h * 0.64,
        }
    }

    /// The width that holds `num_w` with the same air as an emblem.
    fn width(h: f32, num_w: f32) -> f32 {
        (h * 0.34 + h * 0.48 + h * 0.24 + num_w + h * 0.3).max(h * 1.9)
    }
}

/// The width of a [`tab`] `h` tall that holds `widest`, the longest of the
/// numbers it will show. Give every tab in a column this width, so they line
/// up as one column rather than a ragged one.
pub fn tab_width(h: f32, widest: &str, font: Option<&Font>, px: f32) -> f32 {
    let size = h * 0.64;
    let w = MeasureAt(px).width(widest, &TextStyle::new(font.cloned(), size, WHITE));
    TabLayout::width(h, w)
}

/// A stat tab: an emblem rounded on its left and flat on its right, the flat
/// side set hard against an edge (an art window's border) at `top_right`.
/// The icon sits after the rounded end; the number against the flat one.
///
/// The emblem floats over art; a tab belongs to the frame. Stack them up
/// from the bottom of the edge, a quarter of their height apart.
pub fn tab(
    top_right: Vec2,
    (w, h): (f32, f32),
    icon: Icon,
    color: Color,
    number: (&str, Option<&Font>),
    show_number: bool,
    px: f32,
) -> Rect {
    let (number, font) = number;
    let r = Rect::new(top_right.x - w, top_right.y, w, h);
    let l = TabLayout::new(h, w);
    let gradient = Gradient::Vertical {
        top: Color::from_rgba(38, 42, 54, 245),
        bottom: Color::from_rgba(10, 11, 16, 245),
    };
    // A capsule whose right half is squared off: the same gradient over the
    // rounded end, so the join does not show.
    fill_gradient(r, h / 2.0, gradient);
    fill_gradient(
        Rect::new(r.right() - h / 2.0, r.y, h / 2.0, h),
        0.0,
        gradient,
    );
    // The edge in the stat's colour round the rounded end and along the top
    // and bottom; none on the flat side, which meets the border.
    let t = 1.4;
    let c = vec2(r.x + h / 2.0, r.y + h / 2.0);
    draw_line(c.x, r.y + t / 2.0, r.right(), r.y + t / 2.0, t, color);
    draw_line(
        c.x,
        r.bottom() - t / 2.0,
        r.right(),
        r.bottom() - t / 2.0,
        t,
        color,
    );
    // `draw_arc` grows outward from its radius, so start half a line in.
    draw_arc(c.x, c.y, 32, h / 2.0 - t / 2.0, 90.0, t, 180.0, color);
    let mid = r.y + h / 2.0;
    draw_icon(
        icon,
        Rect::new(r.x + l.icon_x, mid - l.icon_s / 2.0, l.icon_s, l.icon_s),
        color,
    );
    if show_number {
        let style = TextStyle::new(font.cloned(), l.num_size, WHITE);
        line(
            number,
            style,
            vec2(r.x + l.num_right, mid),
            vec2(1.0, 0.5),
            px,
        );
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_keeps_its_number_clear_of_its_icon_and_its_flat_edge() {
        let h = 20.0;
        let num_w = 12.0;
        let w = TabLayout::width(h, num_w);
        let l = TabLayout::new(h, w);
        // Air between the icon and where the number starts.
        assert!((l.num_right - num_w) - (l.icon_x + l.icon_s) >= h * 0.2);
        // And between the number and the flat edge.
        assert!(w - l.num_right >= h * 0.25);
        // A single digit still makes a tab, not a stub.
        assert!(TabLayout::width(h, 2.0) >= h * 1.9);
    }

    #[test]
    fn an_emblem_leaves_room_round_its_icon_and_number() {
        let h = 20.0;
        let l = EmblemLayout::new(h, 10.0);
        // Air before the icon, between icon and number, and after the number.
        assert!(l.icon_x >= h * 0.3);
        assert!(l.num_x - (l.icon_x + l.icon_s) >= h * 0.2);
        assert!(l.w - (l.num_x + 10.0) >= h * 0.35);
        // The number is clearly smaller than the pill it sits in.
        assert!(l.num_size <= h * 0.7);
    }

    #[test]
    fn a_one_digit_emblem_is_still_a_capsule() {
        let h = 20.0;
        assert!(EmblemLayout::new(h, 2.0).w >= h * 1.9);
        // And a long number widens it rather than squeezing the padding.
        let wide = EmblemLayout::new(h, 40.0);
        assert!(wide.w - (wide.num_x + 40.0) >= h * 0.35);
    }

    /// A monospace stand-in: every character half its size wide.
    fn mono(s: &str, size: f32) -> f32 {
        s.chars().count() as f32 * size * 0.5
    }

    #[test]
    fn a_name_that_fits_keeps_the_largest_size() {
        // "Ok" at 20 is 20 wide.
        assert_eq!(
            fit_with("Ok", (20.0, 10.0), 20.0, mono),
            ("Ok".to_string(), 20.0)
        );
    }

    #[test]
    fn a_name_too_wide_at_the_largest_size_shrinks_before_it_is_cut() {
        // Ten characters fit 60 at size 12 (10 × 6), not at anything larger.
        assert_eq!(
            fit_with("Toolhead 1", (20.0, 10.0), 60.0, mono),
            ("Toolhead 1".to_string(), 12.0)
        );
    }

    #[test]
    fn a_name_too_wide_at_the_smallest_size_is_cut_there() {
        // At size 10 a character is 5 wide: 40 holds eight, ellipsis
        // included, so seven characters and the ellipsis.
        let (s, size) = fit_with("A very long name", (20.0, 10.0), 40.0, mono);
        assert_eq!(size, 10.0);
        assert_eq!(s, "A very…", "trailing space trimmed before the ellipsis");
    }
}
