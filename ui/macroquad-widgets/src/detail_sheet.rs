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
}

/// How tall the sheet's content is, for a form.
fn content_height(form: SheetForm, rows: usize, notes: usize) -> f32 {
    let handle = if form == SheetForm::Bottom {
        HANDLE_SPACE
    } else {
        0.0
    };
    let notes_h = if notes > 0 {
        8.0 + notes as f32 * NOTE_H
    } else {
        0.0
    };
    handle + PAD + 22.0 + 10.0 + rows as f32 * ROW_H + notes_h + 14.0 + CLOSE_H + PAD
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
    let h = content_height(form, vm.rows.len(), vm.notes.len());
    let r = sheet_rect(form, area, h, e);
    out.rect = Some(r);

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
        SheetForm::Bottom => draw_rounded_rect(r.x, r.y, r.w, r.h + RADIUS, RADIUS, panel),
        SheetForm::Modal => draw_rounded_rect(r.x, r.y, r.w, r.h, RADIUS, panel),
    }
    draw_line(
        r.x + RADIUS,
        r.y + 0.5,
        r.right() - RADIUS,
        r.y + 0.5,
        1.0,
        with_alpha(c.accent, 0.35 * alpha),
    );

    let x = r.x + PAD;
    let right = r.right() - PAD;
    let mut y = r.y;
    if form == SheetForm::Bottom {
        draw_rounded_rect(
            r.center().x - HANDLE_W * 0.5,
            r.y + 8.0,
            HANDLE_W,
            4.0,
            2.0,
            with_alpha(c.text_muted, 0.5),
        );
        y += HANDLE_SPACE;
    }
    y += PAD;
    p.text_top(
        vm.title,
        x,
        y,
        p.size(TextSize::Lg),
        with_alpha(c.text_primary, alpha),
    );
    y += 22.0 + 10.0;

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
    y += 14.0;

    // Only an open sheet answers taps: one closing is already on its way.
    let live = state.is_open() && e > 0.6;
    let close = Rect::new(x, y, right - x, CLOSE_H);
    let pressed = Button::new("Close")
        .variant(ButtonVariant::Tonal)
        .enabled(live)
        .show(p, close);
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
    fn a_tall_sheet_never_outgrows_the_screen() {
        let r = sheet_rect(SheetForm::Bottom, PHONE, 2000.0, 1.0);
        assert!(r.y >= PHONE.y);
    }
}
