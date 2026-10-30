//! `Well` — facts in a panel of their own: a recessed, bordered plate with a
//! row per fact and a hairline between them.
//!
//! Lifted from the wallet modal's account view, where the handle, the
//! addresses and the network sit in one, so every sheet can group its facts
//! the same way instead of scattering them across the card. A row is one of:
//!
//! - **inline**: the label at the start, the value at the end ("Network ·
//!   Mainnet", "Total · 65 ADA");
//! - **stacked**: a small muted label over its value ("Receiving address" over
//!   the address, "Clothes" over the trait), with something at the end — a
//!   figure ("×2.0"), or a Copy button.
//!
//! A row can carry a **meter**: a thin bar under it for its share of a whole
//! (a trait's share of a boost).
//!
//! Copy feedback is the host's state, because the panel is a function of its
//! inputs: the host remembers which value was copied and when, and hands the
//! age back in [`Trailing::Copy`] for the check and the word "Copied".

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::with_alpha;
use crate::shape::{fill_rounded, mix, stroke_rounded};
use crate::{Align, Painter};

/// How long a copied value's row says so.
pub const COPIED_SECS: f32 = 1.8;
/// Between rows, and round the panel's edge.
const ROW_PAD: f32 = 12.0;
/// The panel's inside, left and right.
const INSET: f32 = 12.0;
const RADIUS: f32 = 14.0;
const COPY_W: f32 = 30.0;
/// A meter's bar and the room above it.
const METER_H: f32 = 3.0;
const METER_GAP: f32 = 7.0;

/// How a row lays out its label and value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WellLayout {
    /// The label at the start, the value at the end, one line.
    Inline,
    /// A small label over its value.
    Stacked,
}

/// What sits at the end of a stacked row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Trailing<'a> {
    #[default]
    None,
    /// A figure in its own ink ("×2.0").
    Text(&'a str, Color),
    /// A Copy button. `copied` is the age, in seconds, of a copy of this row's
    /// value, while [`COPIED_SECS`] has not run out.
    Copy { copied: Option<f32> },
}

/// One fact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WellRow<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub layout: WellLayout,
    /// Draw the value in the monospace face: an address, a hash, an amount.
    pub mono: bool,
    pub value_ink: Option<Color>,
    pub trailing: Trailing<'a>,
    /// A share of a whole, 0..=1, and its colour, drawn as a bar under the row.
    pub meter: Option<(f32, Color)>,
    /// The fact the others lead to (a total): the label in the primary ink,
    /// the value a size larger.
    pub strong: bool,
}

impl<'a> WellRow<'a> {
    pub fn inline(label: &'a str, value: &'a str) -> Self {
        Self::new(label, value, WellLayout::Inline)
    }

    pub fn stacked(label: &'a str, value: &'a str) -> Self {
        Self::new(label, value, WellLayout::Stacked)
    }

    fn new(label: &'a str, value: &'a str, layout: WellLayout) -> Self {
        Self {
            label,
            value,
            layout,
            mono: false,
            value_ink: None,
            trailing: Trailing::None,
            meter: None,
            strong: false,
        }
    }

    pub fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }

    /// The value's size: a size up for a strong row.
    fn value_size(&self, p: &Painter) -> f32 {
        p.size(if self.strong {
            TextSize::Lg
        } else {
            TextSize::Md
        })
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn ink(mut self, ink: Color) -> Self {
        self.value_ink = Some(ink);
        self
    }

    pub fn trailing(mut self, trailing: Trailing<'a>) -> Self {
        self.trailing = trailing;
        self
    }

    pub fn meter(mut self, share: f32, color: Color) -> Self {
        self.meter = Some((share.clamp(0.0, 1.0), color));
        self
    }

    /// The row's own height, without the padding between rows.
    pub fn height(&self, p: &Painter) -> f32 {
        let label = p.size(TextSize::Sm);
        let value = self.value_size(p);
        let body = match self.layout {
            WellLayout::Inline => value.max(label) + 2.0,
            WellLayout::Stacked => label + 4.0 + value + 3.0,
        };
        let meter = if self.meter.is_some() {
            METER_GAP + METER_H
        } else {
            0.0
        };
        body + meter
    }
}

/// What the reader did in the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WellResponse {
    /// The row whose Copy button was tapped.
    pub copy: Option<usize>,
}

/// The panel's height for `rows`: each row, the padding between them, and the
/// padding round the edge. `0.0` for none.
pub fn well_height(p: &Painter, rows: &[WellRow]) -> f32 {
    if rows.is_empty() {
        return 0.0;
    }
    ROW_PAD + rows.iter().map(|r| r.height(p) + ROW_PAD).sum::<f32>()
}

/// The width a row's value gets in a panel `w` wide, for a host that shortens
/// a long value (an address) before handing it in.
pub fn well_value_width(w: f32, trailing: Trailing) -> f32 {
    let end = match trailing {
        Trailing::Copy { .. } => COPY_W + 8.0,
        _ => 0.0,
    };
    w - INSET * 2.0 - end
}

/// Draw the panel and its rows into `rect` (its height from [`well_height`]),
/// in the sheet's fade `a`. `live` is whether its Copy buttons take a tap.
pub fn well(p: &Painter, rect: Rect, rows: &[WellRow], live: bool, a: f32) -> WellResponse {
    let mut out = WellResponse::default();
    if rows.is_empty() {
        return out;
    }
    let c = p.theme.color;
    fill_rounded(rect, RADIUS, with_alpha(c.bg_primary, 0.65 * a));
    stroke_rounded(rect, RADIUS, 1.0, with_alpha(c.border, a));
    let label_size = p.size(TextSize::Sm);
    let inner = rect.x + INSET;
    let inner_w = rect.w - INSET * 2.0;
    let mut y = rect.y + ROW_PAD;
    for (i, row) in rows.iter().enumerate() {
        let h = row.height(p);
        let value_size = row.value_size(p);
        let label_ink = with_alpha(
            if row.strong {
                c.text_primary
            } else {
                c.text_muted
            },
            a,
        );
        if i > 0 {
            let rule = y - ROW_PAD * 0.5;
            draw_line(
                inner,
                rule,
                rect.right() - INSET,
                rule,
                1.0,
                with_alpha(c.border, 0.6 * a),
            );
        }
        let ink = with_alpha(row.value_ink.unwrap_or(c.text_primary), a);
        let value = |text: &str, r: Rect, align: Align| {
            if row.mono {
                p.mono_in(
                    &p.fit_mono(text, value_size, r.w),
                    r,
                    value_size,
                    ink,
                    align,
                );
            } else {
                p.text_in(&p.fit(text, value_size, r.w), r, value_size, ink, align);
            }
        };
        match row.layout {
            WellLayout::Inline => {
                let line = Rect::new(inner, y, inner_w, value_size.max(label_size) + 2.0);
                let label_w = p.measure(row.label, label_size).width;
                p.text_in(row.label, line, label_size, label_ink, Align::Start);
                let room = Rect::new(
                    inner + label_w + 12.0,
                    line.y,
                    (inner_w - label_w - 12.0).max(0.0),
                    line.h,
                );
                value(row.value, room, Align::End);
            }
            WellLayout::Stacked => {
                let end_w = match row.trailing {
                    Trailing::Text(text, _) => p.measure_mono(text, value_size).width + 10.0,
                    Trailing::Copy { .. } => COPY_W + 8.0,
                    Trailing::None => 0.0,
                };
                let label_r = Rect::new(inner, y, inner_w - end_w, label_size + 4.0);
                p.text_in(
                    &p.fit(row.label, label_size, label_r.w),
                    label_r,
                    label_size,
                    label_ink,
                    Align::Start,
                );
                let value_r = Rect::new(inner, label_r.bottom(), inner_w - end_w, value_size + 3.0);
                value(row.value, value_r, Align::Start);
                let body = Rect::new(inner, y, inner_w, label_size + 4.0 + value_size + 3.0);
                match row.trailing {
                    Trailing::None => {}
                    Trailing::Text(text, color) => p.mono_in(
                        text,
                        Rect::new(body.x, value_r.y, body.w, value_r.h),
                        value_size,
                        with_alpha(color, a),
                        Align::End,
                    ),
                    Trailing::Copy { copied } => {
                        let button = Rect::new(
                            rect.right() - INSET - COPY_W,
                            body.center().y - COPY_W * 0.5,
                            COPY_W,
                            COPY_W,
                        );
                        let hit = p.interact(button, live);
                        if hit.hover || hit.pressed {
                            fill_rounded(button, 8.0, with_alpha(c.bg_highlight, 0.8 * a));
                        }
                        match copied.filter(|age| *age < COPIED_SECS) {
                            Some(age) => {
                                let fade =
                                    (1.0 - (age - (COPIED_SECS - 0.4)) / 0.4).clamp(0.0, 1.0);
                                let pop = back_out(age / 0.25);
                                check_badge(
                                    button.center(),
                                    9.0 * pop,
                                    c.success,
                                    c.bg_primary,
                                    a * fade,
                                );
                                let word = "Copied";
                                let ww = p.measure(word, label_size).width;
                                p.text_mid(
                                    word,
                                    button.x - ww - 6.0,
                                    y + 6.0,
                                    label_size,
                                    with_alpha(c.success, a * fade),
                                );
                            }
                            None => {
                                let ink = if hit.hover {
                                    mix(c.text_muted, c.accent, 0.8)
                                } else {
                                    c.text_muted
                                };
                                copy_glyph(button.center(), with_alpha(ink, a));
                            }
                        }
                        if hit.clicked {
                            out.copy = Some(i);
                        }
                    }
                }
            }
        }
        if let Some((share, color)) = row.meter {
            let bar_y = y + h - METER_H;
            fill_rounded(
                Rect::new(inner, bar_y, inner_w, METER_H),
                METER_H * 0.5,
                with_alpha(c.bg_highlight, a),
            );
            if share > 0.0 {
                fill_rounded(
                    Rect::new(inner, bar_y, (inner_w * share).max(METER_H), METER_H),
                    METER_H * 0.5,
                    with_alpha(color, 0.9 * a),
                );
            }
        }
        y += h + ROW_PAD;
    }
    out
}

/// A filled disc with a check: done.
pub fn check_badge(at: Vec2, r: f32, fill: Color, ink: Color, a: f32) {
    draw_circle(at.x, at.y, r, with_alpha(fill, a));
    let k = r / 7.0;
    let th = 1.8 * k;
    let p0 = at + vec2(-3.2, 0.2) * k;
    let p1 = at + vec2(-0.8, 2.6) * k;
    let p2 = at + vec2(3.4, -2.4) * k;
    draw_line(p0.x, p0.y, p1.x, p1.y, th, with_alpha(ink, a));
    draw_line(p1.x, p1.y, p2.x, p2.y, th, with_alpha(ink, a));
}

/// Two overlapping sheets: the Copy mark.
pub fn copy_glyph(at: Vec2, color: Color) {
    stroke_rounded(
        Rect::new(at.x - 5.0, at.y - 3.0, 9.0, 10.0),
        2.0,
        1.6,
        color,
    );
    stroke_rounded(
        Rect::new(at.x - 2.0, at.y - 7.0, 9.0, 10.0),
        2.0,
        1.6,
        color,
    );
}

/// A pop that overshoots and settles: a badge arriving.
fn back_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_leaves_room_for_a_copy_button() {
        assert_eq!(well_value_width(300.0, Trailing::None), 276.0);
        assert_eq!(
            well_value_width(300.0, Trailing::Copy { copied: None }),
            276.0 - COPY_W - 8.0
        );
    }

    #[test]
    fn the_pop_lands_on_one() {
        assert_eq!(back_out(0.0), 0.0);
        assert!((back_out(1.0) - 1.0).abs() < 1e-6);
        assert!(back_out(0.6) > 1.0, "it overshoots on the way");
    }
}
