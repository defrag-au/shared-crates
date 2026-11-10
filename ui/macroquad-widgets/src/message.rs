//! `Message` — a whole-body message where a screen has nothing else to show
//! yet: a heading, a sentence or two of why, and at most one thing to do
//! about it.
//!
//! For loading ("Reading the collection"), empty ("Your wallets hold nothing
//! here") and failed ("The overview could not be read") states, so a screen
//! always says what is happening rather than showing a blank. The heading and
//! the wrapped detail are centred as one block; an action is a button under
//! them. Moved here once the staking site and the holder app each had one.
//!
//! A message about a wait carries a [`Spinner`] over its heading:
//! [`Message::busy`] when nobody can say how long, [`Message::progress`] when
//! the fraction is known. Without one, a slow load reads as a page that stopped.
//!
//! [`Message::show`] draws from the top of a rect (a page that puts it under
//! its header); [`Message::show_in`] places it a little above the middle of an
//! area (a body with nothing else in it), where the eye lands.

use macroquad::prelude::*;

use crate::button::Button;
use crate::spinner::Spinner;
use crate::{Align, Line, Painter, TextSize};

/// The widest a message's text runs, however wide the screen.
const MAX_W: f32 = 560.0;
/// The detail's line advance, in points.
const LINE_H: f32 = 22.0;
/// The action button's size, in points.
const ACTION: Vec2 = Vec2::new(220.0, 44.0);
/// Between the text and the action.
const ACTION_GAP: f32 = 20.0;
/// The spinner's side, in points.
const SPINNER: f32 = 48.0;
/// Between the spinner and the heading.
const SPINNER_GAP: f32 = 16.0;

/// See the [module docs](self).
#[derive(Clone, Copy, Debug)]
pub struct Message<'a> {
    pub heading: &'a str,
    pub detail: &'a str,
    /// One action's label, when there is something to do.
    pub action: Option<&'a str>,
    /// The wait this message is about, drawn over the heading.
    pub spinner: Option<Spinner>,
}

impl<'a> Message<'a> {
    pub fn new(heading: &'a str, detail: &'a str) -> Self {
        Self {
            heading,
            detail,
            action: None,
            spinner: None,
        }
    }

    pub fn action(mut self, label: &'a str) -> Self {
        self.action = Some(label);
        self
    }

    /// A wait of unknown length.
    pub fn busy(mut self) -> Self {
        self.spinner = Some(Spinner::busy());
        self
    }

    /// A wait `fraction` of the way through.
    pub fn progress(mut self, fraction: f32) -> Self {
        self.spinner = Some(Spinner::at(fraction));
        self
    }

    /// The room the spinner takes above the heading.
    fn spinner_h(&self) -> f32 {
        match self.spinner {
            Some(_) => SPINNER + SPINNER_GAP,
            None => 0.0,
        }
    }

    /// The text's width in a rect `width` wide.
    fn text_w(width: f32) -> f32 {
        width.min(MAX_W)
    }

    /// How tall the message is in a rect `width` wide.
    pub fn height(&self, p: &Painter, width: f32) -> f32 {
        let lines = p
            .wrap(self.detail, p.size(TextSize::Sm), Self::text_w(width))
            .len();
        let text = self.spinner_h() + 40.0 + LINE_H * lines as f32;
        match self.action {
            Some(_) => text + ACTION_GAP + ACTION.y,
            None => text,
        }
    }

    /// Draw from the top of `rect`, centred across it. Returns whether the
    /// action was tapped.
    pub fn show(&self, p: &Painter, rect: Rect) -> bool {
        if let Some(spinner) = self.spinner {
            let side = Rect::new(rect.x + (rect.w - SPINNER) * 0.5, rect.y, SPINNER, SPINNER);
            spinner.show(p, side);
        }
        let top = rect.y + self.spinner_h();
        let c = p.theme.color;
        let w = Self::text_w(rect.w);
        let x = rect.x + (rect.w - w) * 0.5;
        let sm = p.size(TextSize::Sm);
        let detail = p.wrap(self.detail, sm, w);
        let mut lines = vec![Line::new(
            self.heading,
            p.size(TextSize::Xl),
            c.text_primary,
        )];
        lines.extend(detail.iter().map(|d| Line::new(d, sm, c.text_secondary)));
        let text_h = 40.0 + LINE_H * detail.len() as f32;
        p.stack_in(&lines, Rect::new(x, top, w, text_h), Align::Center, 12.0);
        match self.action {
            Some(label) => {
                let bw = ACTION.x.min(rect.w);
                let b = Rect::new(
                    rect.x + (rect.w - bw) * 0.5,
                    top + text_h + ACTION_GAP,
                    bw,
                    ACTION.y,
                );
                Button::new(label).show(p, b)
            }
            None => false,
        }
    }

    /// Draw a little above the middle of `area`. Returns whether the action
    /// was tapped.
    pub fn show_in(&self, p: &Painter, area: Rect) -> bool {
        let h = self.height(p, area.w);
        let top = area.y + ((area.h - h) * 0.42).max(16.0);
        self.show(p, Rect::new(area.x, top, area.w, h))
    }
}
