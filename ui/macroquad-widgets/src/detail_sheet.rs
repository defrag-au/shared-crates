//! `DetailSheet` — the detail behind a headline, out of the way until asked for: a modal on a wide screen, a sheet sliding up from the bottom on a phone.
//!
//! A buyer-facing screen leads with the one number that matters and keeps the
//! breakdown (fees, impact, how it settles) one tap away. [`info_button`] is
//! the tap: a small circled `i` with a thumb-sized hit area. The sheet is what
//! it opens.
//!
//! ## Which form
//!
//! [`SheetForm::for_area`] picks by width: under [`BOTTOM_SHEET_BELOW`] points
//! the sheet rises from the bottom edge, where a thumb already is; wider, it is
//! a centred card over a scrim. Width rather than "is this a touch device",
//! because a narrow desktop window has the same problem a phone does.
//!
//! ## Who owns what
//!
//! Domain-free: the host hands in rows of label and value, already worded. The
//! slide is host-owned in a [`SheetState`] (like `TrainState`), stepped with
//! the frame's `dt`, so the widget stays a function of its inputs. While the
//! sheet is up the host should draw the screen behind it with no tap and no
//! drag — a tap meant for the scrim must not also press what lies under it.
//! A tap on the scrim or on Close reports [`SheetResponse::close`].

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::button::{Button, ButtonVariant};
use crate::{Painter, draw_rounded_rect, painter::with_alpha};

/// Narrower than this, the sheet rises from the bottom edge.
pub const BOTTOM_SHEET_BELOW: f32 = 600.0;
/// Seconds the sheet takes to open or close.
const SLIDE_SECS: f32 = 0.22;
/// The widest a modal card gets.
const MODAL_MAX_W: f32 = 420.0;
/// The widest a bottom sheet gets, on a wide-ish narrow window.
const SHEET_MAX_W: f32 = 560.0;
const PAD: f32 = 20.0;
const RADIUS: f32 = 18.0;
const ROW_H: f32 = 26.0;
const NOTE_H: f32 = 17.0;
const CLOSE_H: f32 = 46.0;
/// Room between a caller's content and the close band.
const CLOSE_GAP: f32 = 14.0;
/// A surface at least this tall affords the sheet's full spacing.
const ROOMY_H: f32 = 600.0;
/// The least of its spacing a sheet keeps, on a canvas a few hundred pixels
/// tall.
const COMPACT: f32 = 0.4;
/// The grab handle a bottom sheet shows, and the room it takes.
const HANDLE_W: f32 = 40.0;
const HANDLE_SPACE: f32 = 16.0;
const INFO_R: f32 = 10.0;
/// The hit area round the info button: a thumb, not a pixel.
const INFO_HIT: f32 = 44.0;

/// How the sheet is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetForm {
    /// A card centred over a scrim.
    Modal,
    /// Full width, rising from the bottom edge.
    Bottom,
}

impl SheetForm {
    pub fn for_area(area: Rect) -> Self {
        if area.w < BOTTOM_SHEET_BELOW {
            Self::Bottom
        } else {
            Self::Modal
        }
    }
}

/// How a row's value is inked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowTone {
    #[default]
    Normal,
    /// Something the reader should look at twice.
    Warn,
    /// The headline the rows explain.
    Strong,
}

/// One line of the breakdown.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailRow {
    pub label: String,
    pub value: String,
    pub tone: RowTone,
}

impl DetailRow {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            tone: RowTone::Normal,
        }
    }

    pub fn tone(mut self, tone: RowTone) -> Self {
        self.tone = tone;
        self
    }
}

pub struct DetailSheetVm<'a> {
    pub title: &'a str,
    /// Height of a figure between the title and the rows; 0 for none. The
    /// widget only makes the room: the host draws into
    /// [`SheetResponse::figure`], since what the figure is (a chart, a route
    /// graph) is the host's to know.
    pub figure_h: f32,
    pub rows: &'a [DetailRow],
    /// Short lines under the rows, already split to fit: the widget does not
    /// wrap, so a host words each line for the narrowest form.
    pub notes: &'a [String],
}

/// Open or shut, and how far along the way.
#[derive(Clone, Copy, Debug, Default)]
pub struct SheetState {
    open: bool,
    shown: f32,
}

impl SheetState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Open, already all the way: for a review capture of the open sheet.
    pub fn opened(mut self) -> Self {
        self.open = true;
        self.shown = 1.0;
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether any of it is on screen — the host's cue to hold back input
    /// from the screen behind.
    pub fn visible(&self) -> bool {
        self.shown > 0.0
    }

    /// Step towards open or shut. Lands exactly, so a settled sheet is 0 or 1.
    pub fn update(&mut self, dt: f32) {
        let target = if self.open { 1.0 } else { 0.0 };
        let step = dt / SLIDE_SECS;
        if (target - self.shown).abs() <= step {
            self.shown = target;
        } else if target > self.shown {
            self.shown += step;
        } else {
            self.shown -= step;
        }
    }

    /// 0..=1, eased: fast out, gentle landing.
    pub fn progress(&self) -> f32 {
        let t = 1.0 - self.shown.clamp(0.0, 1.0);
        1.0 - t * t * t
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SheetResponse {
    /// Close or the scrim was tapped.
    pub close: bool,
    /// Where the sheet is drawn this frame.
    pub rect: Option<Rect>,
    /// The room made for the figure this frame, when one was asked for.
    pub figure: Option<Rect>,
}

/// What [`sheet_card`] drew, and the room it left for a caller's content.
#[derive(Clone, Copy, Debug)]
pub struct SheetCard {
    /// The card this frame, in the motion the state is in.
    pub rect: Rect,
    pub form: SheetForm,
    /// Fades a caller's own content with the sheet: nothing should be drawn at
    /// full strength on a card that is still transparent.
    pub alpha: f32,
    /// Where a caller draws: inside the padding, below the handle, with the
    /// close button's band left out at the bottom.
    pub content: Rect,
    /// The band the chrome kept for Close, at the card's foot. The chrome owns
    /// the room; the caller owns the button, because whether a sheet closes on
    /// a press is the host's decision.
    pub close: Rect,
}

/// Vertical room a card's chrome takes outside [`SheetCard::content`]: the top
/// padding, a bottom sheet's handle, the gap above the close band and the band
/// itself.
///
/// A caller sizing a card from the height of its own content adds this once,
/// so the two halves of the height cannot drift apart.
pub fn sheet_chrome(form: SheetForm, area: Rect) -> f32 {
    let handle = if form == SheetForm::Bottom {
        HANDLE_SPACE
    } else {
        0.0
    };
    (handle + PAD + CLOSE_GAP + CLOSE_H + PAD) * density(area)
}

/// How much of its vertical spacing a sheet keeps on this surface.
///
/// The numbers above are written for a phone's several hundred points. A
/// 320x224 pixel-art canvas is a whole screen too, and there that padding would
/// be half of it — so the vertical chrome compresses with the surface. Width is
/// rarely the scarce dimension, so horizontal padding does not move.
fn density(area: Rect) -> f32 {
    (area.h / ROOMY_H).clamp(COMPACT, 1.0)
}

/// Draw the modal's chrome — the scrim, the card and this frame's motion — and
/// return the room left inside it.
///
/// Split out of [`detail_sheet`] because a sheet's *content* is the caller's:
/// the price breakdown fills it with rows, the operator panel with a live
/// readout and its buttons. The scrim, the shape (a bottom sheet on a phone, a
/// modal on a desk) and the motion are one widget, and were written twice before
/// this existed.
///
/// `height` is the card's height once open; [`SheetState`] decides where it is.
pub fn sheet_card(
    painter: &Painter,
    state: &SheetState,
    area: Rect,
    height: f32,
) -> Option<SheetCard> {
    if !state.visible() {
        return None;
    }
    let c = &painter.theme.color;
    let e = state.progress();
    let form = SheetForm::for_area(area);
    let rect = sheet_rect(form, area, height, e);
    let k = density(area);

    // The scrim: the screen behind reads as out of reach.
    draw_rectangle(
        area.x,
        area.y,
        area.w,
        area.h,
        Color::new(0.0, 0.0, 0.0, 0.6 * e),
    );
    let alpha = if form == SheetForm::Modal { e } else { 1.0 };
    let panel = with_alpha(c.bg_secondary, alpha);
    match form {
        // Square bottom corners: the sheet runs off the edge.
        SheetForm::Bottom => {
            draw_rounded_rect(rect.x, rect.y, rect.w, rect.h + RADIUS, RADIUS, panel)
        }
        SheetForm::Modal => draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, RADIUS, panel),
    }
    draw_line(
        rect.x + RADIUS,
        rect.y + 0.5,
        rect.right() - RADIUS,
        rect.y + 0.5,
        1.0,
        with_alpha(c.accent, 0.35 * alpha),
    );
    let handle = if form == SheetForm::Bottom {
        draw_rounded_rect(
            rect.center().x - HANDLE_W * 0.5,
            rect.y + 8.0,
            HANDLE_W,
            4.0,
            2.0,
            with_alpha(c.text_muted, 0.5),
        );
        HANDLE_SPACE * k
    } else {
        0.0
    };

    let top = rect.y + handle + PAD * k;
    let close = Rect::new(
        rect.x + PAD,
        rect.bottom() - PAD * k - CLOSE_H * k,
        rect.w - PAD * 2.0,
        CLOSE_H * k,
    );
    Some(SheetCard {
        rect,
        form,
        alpha,
        content: Rect::new(
            rect.x + PAD,
            top,
            rect.w - PAD * 2.0,
            (close.y - CLOSE_GAP * k - top).max(0.0),
        ),
        close,
    })
}

/// Room between the figure and the first row.
const FIGURE_GAP: f32 = 10.0;

/// How tall the sheet's content is, for a form.
fn content_height(form: SheetForm, area: Rect, figure_h: f32, rows: usize, notes: usize) -> f32 {
    let figure = if figure_h > 0.0 {
        figure_h + FIGURE_GAP
    } else {
        0.0
    };
    let notes_h = if notes > 0 {
        8.0 + notes as f32 * NOTE_H
    } else {
        0.0
    };
    sheet_chrome(form, area) + 22.0 + 10.0 + figure + rows as f32 * ROW_H + notes_h
}

/// Where the sheet sits at `progress` (0 shut, 1 open).
///
/// A bottom sheet slides up from below the edge; a modal settles down a few
/// points into the centre as it fades in.
pub fn sheet_rect(form: SheetForm, area: Rect, height: f32, progress: f32) -> Rect {
    let p = progress.clamp(0.0, 1.0);
    match form {
        SheetForm::Bottom => {
            let w = area.w.min(SHEET_MAX_W);
            let h = height.min(area.h);
            let x = area.x + (area.w - w) * 0.5;
            let y = area.bottom() - h * p;
            Rect::new(x, y, w, h)
        }
        SheetForm::Modal => {
            let w = (area.w - PAD * 2.0).min(MODAL_MAX_W);
            let h = height.min(area.h - PAD * 2.0);
            let x = area.x + (area.w - w) * 0.5;
            let y = area.y + (area.h - h) * 0.5 + 16.0 * (1.0 - p);
            Rect::new(x, y, w, h)
        }
    }
}

/// The width a caller's content gets in a card of this form, before its height
/// is known.
///
/// Enough for a caller to choose its own layout — one column or two — and then
/// size the card in a single pass, instead of guessing at the card's padding.
pub fn sheet_content_width(form: SheetForm, area: Rect) -> f32 {
    sheet_rect(form, area, 0.0, 1.0).w - PAD * 2.0
}

/// A circled `i` centred at `at`. Returns true when tapped.
pub fn info_button(p: &Painter, at: Vec2, enabled: bool) -> bool {
    let c = &p.theme.color;
    let ink = if enabled {
        c.text_secondary
    } else {
        with_alpha(c.text_muted, 0.4)
    };
    draw_circle_lines(at.x, at.y, INFO_R, 1.5, ink);
    let size = p.size(TextSize::Sm);
    let w = p.measure("i", size).width;
    p.text(
        "i",
        at.x - w * 0.5,
        p.centre_baseline(at.y - INFO_R, INFO_R * 2.0, size),
        size,
        ink,
    );
    let hit = Rect::new(
        at.x - INFO_HIT * 0.5,
        at.y - INFO_HIT * 0.5,
        INFO_HIT,
        INFO_HIT,
    );
    enabled && p.tapped(hit)
}

/// Draw the sheet over `area`, if any of it is showing.
pub fn detail_sheet(
    p: &Painter,
    vm: &DetailSheetVm,
    state: &SheetState,
    area: Rect,
) -> SheetResponse {
    let mut out = SheetResponse::default();
    if !state.visible() {
        return out;
    }
    let c = &p.theme.color;
    let e = state.progress();
    let form = SheetForm::for_area(area);
    let h = content_height(form, area, vm.figure_h, vm.rows.len(), vm.notes.len());
    let Some(card) = sheet_card(p, state, area, h) else {
        return out;
    };
    let r = card.rect;
    let alpha = card.alpha;
    out.rect = Some(r);

    let x = r.x + PAD;
    let right = r.right() - PAD;
    let mut y = card.content.y;
    p.text_top(
        vm.title,
        x,
        y,
        p.size(TextSize::Lg),
        with_alpha(c.text_primary, alpha),
    );
    y += 22.0 + 10.0;
    if vm.figure_h > 0.0 {
        out.figure = Some(Rect::new(x, y, right - x, vm.figure_h));
        y += vm.figure_h + FIGURE_GAP;
    }

    let size = p.size(TextSize::Base);
    for row in vm.rows {
        let ink = match row.tone {
            RowTone::Normal => c.text_secondary,
            RowTone::Warn => c.warning,
            RowTone::Strong => c.accent,
        };
        p.text_top(&row.label, x, y, size, with_alpha(c.text_muted, alpha));
        let w = p.measure(&row.value, size).width;
        p.text_top(&row.value, right - w, y, size, with_alpha(ink, alpha));
        y += ROW_H;
    }
    if !vm.notes.is_empty() {
        y += 8.0;
        let note = p.size(TextSize::Xs);
        for line in vm.notes {
            p.text_top(line, x, y, note, with_alpha(c.text_muted, alpha));
            y += NOTE_H;
        }
    }
    // Only an open sheet answers taps: one closing is already on its way.
    let live = state.is_open() && e > 0.6;
    let pressed = Button::new("Close")
        .variant(ButtonVariant::Tonal)
        .enabled(live)
        .show(p, card.close);
    let scrim = live && p.tap.is_some_and(|t| !r.contains(t));
    out.close = pressed || scrim;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 390.0,
        h: 844.0,
    };
    const DESK: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1280.0,
        h: 800.0,
    };

    #[test]
    fn a_phone_gets_a_bottom_sheet_and_a_desk_a_modal() {
        assert_eq!(SheetForm::for_area(PHONE), SheetForm::Bottom);
        assert_eq!(SheetForm::for_area(DESK), SheetForm::Modal);
    }

    #[test]
    fn a_bottom_sheet_rises_from_below_the_edge_and_lands_flush() {
        let shut = sheet_rect(SheetForm::Bottom, PHONE, 300.0, 0.0);
        assert_eq!(shut.y, PHONE.bottom(), "shut is wholly below the edge");
        let open = sheet_rect(SheetForm::Bottom, PHONE, 300.0, 1.0);
        assert_eq!(open.bottom(), PHONE.bottom(), "open sits on the edge");
        assert_eq!(open.w, PHONE.w, "full width on a phone");
    }

    #[test]
    fn a_modal_is_centred_and_capped() {
        let open = sheet_rect(SheetForm::Modal, DESK, 300.0, 1.0);
        assert_eq!(open.w, MODAL_MAX_W);
        assert!((open.center().x - DESK.center().x).abs() < 0.01);
        assert!((open.center().y - DESK.center().y).abs() < 0.01);
    }

    #[test]
    fn the_slide_lands_exactly_both_ways() {
        let mut s = SheetState::new();
        s.open();
        for _ in 0..30 {
            s.update(1.0 / 60.0);
        }
        assert_eq!(s.progress(), 1.0);
        s.close();
        for _ in 0..30 {
            s.update(1.0 / 60.0);
        }
        assert!(!s.visible(), "a closed sheet is gone, not nearly gone");
    }

    #[test]
    fn a_figure_makes_room_and_none_makes_none() {
        let plain = content_height(SheetForm::Bottom, PHONE, 0.0, 4, 2);
        let with = content_height(SheetForm::Bottom, PHONE, 120.0, 4, 2);
        assert_eq!(with - plain, 120.0 + FIGURE_GAP);
    }

    #[test]
    fn a_tall_sheet_never_outgrows_the_screen() {
        let r = sheet_rect(SheetForm::Bottom, PHONE, 2000.0, 1.0);
        assert!(r.y >= PHONE.y);
    }

    /// A caller that sizes a card from its own content adds [`sheet_chrome`]
    /// once, so the helper has to be the same money the card actually spends.
    /// Written out independently of the helper: the point is to catch a change
    /// to a constant that the helper was not told about.
    #[test]
    fn the_chrome_helper_is_the_paddings_the_handle_and_the_close_band() {
        let roomy = Rect::new(0.0, 0.0, 390.0, 844.0);
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, roomy),
            HANDLE_SPACE + PAD + CLOSE_GAP + CLOSE_H + PAD
        );
        assert_eq!(
            sheet_chrome(SheetForm::Modal, roomy),
            PAD + CLOSE_GAP + CLOSE_H + PAD,
        );
    }

    /// A 320x224 canvas is a whole screen in a pixel game, and a sheet spaced for
    /// a phone would spend half of it on chrome.
    #[test]
    fn a_short_surface_compresses_the_chrome_but_a_phone_does_not() {
        let canvas = Rect::new(0.0, 0.0, 320.0, 224.0);
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, canvas),
            sheet_chrome(SheetForm::Bottom, PHONE) * COMPACT
        );
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, PHONE),
            sheet_chrome(SheetForm::Bottom, DESK)
        );
    }
}
