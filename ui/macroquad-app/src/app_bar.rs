//! `AppBar` — the bar across the top of every screen: a way back, where the
//! reader is, and the screen's few actions.
//!
//! Drawn into the two rects [`crate::shell_layout`] gives: the band, painted
//! from the screen's top edge so its colour fills the notch, and the row of
//! controls inside the safe area. The title is cut with an ellipsis before it
//! reaches the actions; a subtitle sits under it in the muted ink.
//!
//! `raised` is how far the page under the bar has scrolled, 0 to 1: at rest
//! the bar is the page's own colour, and once content passes beneath it a
//! hairline and a lifted fill separate the two, as a phone's own bars do.

use macroquad::prelude::*;
use macroquad_widgets::{Align, Line, Painter, fill_rounded, stroke_rounded, theme::with_alpha};
use ui_theme::TextSize;

use crate::glyph::{Glyph, draw_glyph};

/// The square a bar's icon button takes, in points: a finger's minimum.
pub const BUTTON: f32 = 44.0;
/// The glyph's size inside it.
const GLYPH: f32 = 24.0;

/// What sits at the bar's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leading {
    /// Nothing: the app's top level.
    None,
    /// A chevron back to where the reader came from.
    Back,
    /// A cross, for a screen that is dismissed rather than left.
    Close,
}

/// One of the bar's actions, at its end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarAction<'a> {
    /// An icon button.
    Icon(Glyph),
    /// An icon button with a dot on it, for "something here is set" (a filter
    /// in force).
    Marked(Glyph),
    /// A labelled chip, for an action whose state is worth reading (which
    /// wallet, how many).
    Chip {
        glyph: Option<Glyph>,
        label: &'a str,
    },
}

/// What a tap on the bar hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarHit {
    Leading,
    /// The action at this index of [`AppBar::actions`].
    Action(usize),
}

/// See the [module docs](self).
#[derive(Clone, Copy, Debug)]
pub struct AppBar<'a> {
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub leading: Leading,
    pub actions: &'a [BarAction<'a>],
    /// 0 at rest, 1 once the page has scrolled under the bar.
    pub raised: f32,
}

impl<'a> AppBar<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            subtitle: None,
            leading: Leading::None,
            actions: &[],
            raised: 0.0,
        }
    }

    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    pub fn leading(mut self, leading: Leading) -> Self {
        self.leading = leading;
        self
    }

    pub fn actions(mut self, actions: &'a [BarAction<'a>]) -> Self {
        self.actions = actions;
        self
    }

    /// How far the page has scrolled under the bar, 0 to 1. A page passes
    /// its scroll offset over a short distance: `(offset / 24.0).min(1.0)`.
    pub fn raised(mut self, raised: f32) -> Self {
        self.raised = raised.clamp(0.0, 1.0);
        self
    }

    /// Draw the bar in `band` (its background) with its controls in `row`.
    pub fn show(&self, p: &Painter, band: Rect, row: Rect) -> Option<BarHit> {
        let c = &p.theme.color;
        let fill = macroquad_widgets::shape::mix(c.bg_primary, c.bg_secondary, self.raised);
        draw_rectangle(band.x, band.y, band.w, band.h, fill);
        if self.raised > 0.0 {
            let edge = with_alpha(c.border, self.raised);
            draw_rectangle(band.x, band.bottom() - 1.0, band.w, 1.0, edge);
        }

        let mut hit = None;
        let pad = 4.0;
        let mid = row.y + row.h * 0.5;
        let mut start = row.x + pad;
        if self.leading != Leading::None {
            let r = Rect::new(start, mid - BUTTON * 0.5, BUTTON, BUTTON);
            let glyph = match self.leading {
                Leading::Close => Glyph::Close,
                _ => Glyph::Back,
            };
            if icon_button(p, r, glyph, false) {
                hit = Some(BarHit::Leading);
            }
            start = r.right() + 4.0;
        } else {
            start += 12.0;
        }

        // The actions, laid out from the end.
        let mut end = row.right() - pad;
        let chip_size = p.size(TextSize::Base);
        for (i, action) in self.actions.iter().enumerate().rev() {
            let w = match action {
                BarAction::Icon(_) | BarAction::Marked(_) => BUTTON,
                BarAction::Chip { glyph, label } => {
                    let text = p.measure(label, chip_size).width;
                    text + 24.0 + if glyph.is_some() { 22.0 } else { 0.0 }
                }
            };
            let r = Rect::new(end - w, mid - BUTTON * 0.5, w, BUTTON);
            let tapped = match action {
                BarAction::Icon(g) => icon_button(p, r, *g, false),
                BarAction::Marked(g) => icon_button(p, r, *g, true),
                BarAction::Chip { glyph, label } => chip_button(p, r, *glyph, label, chip_size),
            };
            if tapped {
                hit = Some(BarHit::Action(i));
            }
            end = r.x - 2.0;
        }

        let title_box = Rect::new(start, row.y, (end - start - 8.0).max(0.0), row.h);
        let title_size = p.size(TextSize::Xl2);
        let title = p.fit(self.title, title_size, title_box.w);
        match self.subtitle {
            None => p.text_in(&title, title_box, title_size, c.text_primary, Align::Start),
            Some(sub) => {
                let sub_size = p.size(TextSize::Sm);
                let sub = p.fit(sub, sub_size, title_box.w);
                let lines = [
                    Line::new(&title, title_size, c.text_primary),
                    Line::new(&sub, sub_size, c.text_muted),
                ];
                p.stack_in(&lines, title_box, Align::Start, 7.0);
            }
        }
        hit
    }
}

/// A round icon button, `rect` its hit area. Returns whether it was tapped.
pub fn icon_button(p: &Painter, rect: Rect, glyph: Glyph, marked: bool) -> bool {
    let c = &p.theme.color;
    let hit = p.interact(rect, true);
    if hit.pressed || hit.hover {
        let a = if hit.pressed { 0.16 } else { 0.08 };
        let o = rect.center();
        draw_circle(o.x, o.y, rect.w.min(rect.h) * 0.5, with_alpha(c.text_primary, a));
    }
    let g = Rect::new(
        rect.center().x - GLYPH * 0.5,
        rect.center().y - GLYPH * 0.5,
        GLYPH,
        GLYPH,
    );
    draw_glyph(glyph, g, c.text_primary);
    if marked {
        draw_circle(g.right() - 1.0, g.y + 2.0, 4.0, c.accent);
    }
    hit.clicked
}

/// A labelled chip in the bar. Returns whether it was tapped.
fn chip_button(p: &Painter, rect: Rect, glyph: Option<Glyph>, label: &str, size: f32) -> bool {
    let c = &p.theme.color;
    let hit = p.interact(rect, true);
    let chip = Rect::new(rect.x, rect.center().y - 16.0, rect.w, 32.0);
    let fill = if hit.pressed {
        c.bg_highlight
    } else {
        c.bg_secondary
    };
    fill_rounded(chip, 16.0, fill);
    stroke_rounded(chip, 16.0, 1.0, c.border);
    let mut x = chip.x + 12.0;
    if let Some(g) = glyph {
        draw_glyph(g, Rect::new(x, chip.center().y - 9.0, 18.0, 18.0), c.text_primary);
        x += 22.0;
    }
    let text = Rect::new(x, chip.y, chip.right() - x - 12.0, chip.h);
    p.text_in(label, text, size, c.text_primary, Align::Start);
    hit.clicked
}
