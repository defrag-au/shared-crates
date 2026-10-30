//! `DetailSheet` — the detail behind a headline, out of the way until asked for: a modal on a wide screen, a sheet sliding up from the bottom on a phone.
//!
//! A buyer-facing screen leads with the one number that matters and keeps the
//! breakdown (fees, impact, how it settles) one tap away. [`info_button`] is
//! the tap: a small circled `i` with a thumb-sized hit area. The sheet is what
//! it opens.
//!
//! ## The chrome every modal shares
//!
//! [`sheet_card`] is the modal: the scrim, the card and its motion, a header
//! with a title and a close cross, and a footer band for the sheet's actions.
//! Every sheet in the kit draws through it — the cart, a Perp's detail, a
//! purchase — and the wallet modal paints its card with the same
//! [`sheet_surface`] and closes with the same [`sheet_close`]. So a sheet looks
//! like the others without trying: a soft shadow under it, a gradient plate
//! with a glow of its tint across the top, a hairline border, and rounded
//! corners that a bottom sheet runs off the edge.
//!
//! What goes inside is the caller's. The facts go in a [`crate::well`], the
//! actions are [`crate::Pill`]s in [`SheetCard::footer`].
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
//! The slide is host-owned in a [`SheetState`] (like `TrainState`), stepped with
//! the frame's `dt`, so the widget stays a function of its inputs. While the
//! sheet is up the host should draw the screen behind it with no tap and no
//! drag — a tap meant for the scrim must not also press what lies under it.
//! A tap on the scrim or on the cross reports [`SheetCard::dismissed`]; whether
//! the sheet then closes is the host's call (a cart mid-signature might not).

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::with_alpha;
use crate::shape::{Gradient, fill_gradient, fill_rounded, mix, stroke_rounded};
use crate::well::{WellRow, well, well_height};
use crate::{Painter, draw_rounded_rect};

/// Narrower than this, the sheet rises from the bottom edge.
pub const BOTTOM_SHEET_BELOW: f32 = 600.0;
/// Seconds the sheet takes to open or close.
const SLIDE_SECS: f32 = 0.24;
/// The widest a modal card gets.
const MODAL_MAX_W: f32 = 420.0;
/// The widest a bottom sheet gets, on a wide-ish narrow window.
const SHEET_MAX_W: f32 = 560.0;
/// The strip of page a bottom sheet leaves at either side, so its border and
/// rounded corners read as a card rising rather than a slab glued to the edges.
pub const SHEET_SIDE_INSET: f32 = 4.0;
const PAD: f32 = 20.0;
const RADIUS: f32 = 20.0;
const NOTE_H: f32 = 17.0;
/// The footer's band: the height of a pill.
const FOOTER_H: f32 = 44.0;
/// Room between a caller's content and the footer.
const FOOTER_GAP: f32 = 16.0;
/// The header's title row: the close cross's square.
const ICON_BUTTON: f32 = 32.0;
/// A subtitle's line under the title.
const SUBTITLE_H: f32 = 22.0;
/// Room between the header and the content.
const HEADER_GAP: f32 = 14.0;
/// How tall the tint's glow across the card's top runs.
const GLOW_H: f32 = 140.0;
/// How far a modal card settles as it fades in.
const SETTLE_PX: f32 = 24.0;
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
/// How far open a sheet must be before it takes a tap: one still arriving, or
/// already leaving, is not there to press.
const LIVE_AT: f32 = 0.85;

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

/// What a sheet's chrome carries besides the card: a title and its subtitle,
/// the tint of its glow, whether it has a close cross, and whether it keeps a
/// footer band for actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet<'a> {
    pub title: Option<&'a str>,
    pub subtitle: Option<&'a str>,
    /// The glow across the card's top, and the colour a host's own accents
    /// should agree with. The theme's accent when `None`.
    pub tint: Option<Color>,
    /// A close cross in the header. On by default; a sheet that must not be
    /// dismissed mid-step still gets one, and ignores what it reports.
    pub closable: bool,
    /// Keep [`SheetCard::footer`] for the sheet's actions.
    pub footer: bool,
}

impl<'a> Sheet<'a> {
    pub const fn new() -> Self {
        Self {
            title: None,
            subtitle: None,
            tint: None,
            closable: true,
            footer: true,
        }
    }

    pub const fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub const fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    pub const fn tint(mut self, tint: Color) -> Self {
        self.tint = Some(tint);
        self
    }

    pub const fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    pub const fn footer(mut self, footer: bool) -> Self {
        self.footer = footer;
        self
    }

    /// Whether there is a header row at all.
    fn has_header(&self) -> bool {
        self.title.is_some() || self.subtitle.is_some() || self.closable
    }

    /// The header's height: the title row, a subtitle, the gap under it.
    fn header_h(&self) -> f32 {
        if !self.has_header() {
            return 0.0;
        }
        let subtitle = if self.subtitle.is_some() {
            SUBTITLE_H
        } else {
            0.0
        };
        ICON_BUTTON + subtitle + HEADER_GAP
    }
}

impl Default for Sheet<'_> {
    fn default() -> Self {
        Self::new()
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
        1.0 - t * t * t * t
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SheetResponse {
    /// The cross or the scrim was tapped.
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
    /// Whether the sheet is open far enough to take a tap. A caller's own
    /// controls should be enabled only while it is.
    pub live: bool,
    /// The title row, cross included: a caller may draw beside the title (a
    /// count at its end) inside it.
    pub header: Rect,
    /// Where a caller draws: inside the padding, under the header, with the
    /// footer's band left out at the bottom.
    pub content: Rect,
    /// The footer's band, for the sheet's actions. Zero-height when the sheet
    /// asked for none.
    pub footer: Rect,
    /// The cross, or the scrim, was tapped.
    pub dismissed: bool,
}

/// Vertical room a card's chrome takes outside [`SheetCard::content`]: the top
/// padding, a bottom sheet's handle, the header, the footer and its gap, and
/// the bottom padding.
///
/// A caller sizing a card from the height of its own content adds this once,
/// so the two halves of the height cannot drift apart.
pub fn sheet_chrome(form: SheetForm, area: Rect, sheet: &Sheet) -> f32 {
    let handle = if form == SheetForm::Bottom {
        HANDLE_SPACE
    } else {
        0.0
    };
    let footer = if sheet.footer {
        FOOTER_GAP + FOOTER_H
    } else {
        0.0
    };
    (handle + PAD + footer + PAD) * density(area) + sheet.header_h()
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

/// Paint a modal's surface: the scrim over `area`, then the card at `rect` —
/// a shadow falling under it, a gradient plate, a glow of `tint` across its
/// top, a hairline border, and a bottom sheet's grab handle. `e` is how far in
/// it is, 0..=1.
///
/// Public so a modal with its own motion and layout (the wallet modal) paints
/// the same card as every [`sheet_card`].
pub fn sheet_surface(p: &Painter, area: Rect, rect: Rect, form: SheetForm, e: f32, tint: Color) {
    let c = &p.theme.color;
    draw_rectangle(
        area.x,
        area.y,
        area.w,
        area.h,
        with_alpha(c.bg_primary, 0.74 * e),
    );
    // The bottom corners of a sheet are pushed off the edge, so it reads as
    // rising out of it rather than floating above it.
    let shape = match form {
        SheetForm::Modal => rect,
        SheetForm::Bottom => Rect::new(rect.x, rect.y, rect.w, rect.h + RADIUS),
    };
    let a = if form == SheetForm::Modal { e } else { 1.0 };
    // One shadow plate: overlapping translucent plates stack into a halo.
    fill_rounded(
        Rect::new(shape.x + 2.0, shape.y + 10.0, shape.w - 4.0, shape.h),
        RADIUS,
        with_alpha(c.bg_primary, 0.8 * e),
    );
    fill_gradient(
        shape,
        RADIUS,
        Gradient::Vertical {
            top: with_alpha(mix(c.bg_secondary, c.bg_highlight, 0.3), a),
            bottom: with_alpha(c.bg_secondary, a),
        },
    );
    fill_gradient(
        Rect::new(shape.x, shape.y, shape.w, GLOW_H.min(shape.h)),
        RADIUS,
        Gradient::Vertical {
            top: with_alpha(tint, 0.16 * a),
            bottom: with_alpha(tint, 0.0),
        },
    );
    stroke_rounded(shape, RADIUS, 1.0, with_alpha(c.border, a));
    if form == SheetForm::Bottom {
        draw_rounded_rect(
            rect.center().x - HANDLE_W * 0.5,
            rect.y + 8.0,
            HANDLE_W,
            4.0,
            2.0,
            with_alpha(c.text_muted, 0.6 * a),
        );
    }
}

/// The header's close cross in its square `rect`, lit under the pointer.
/// `true` when tapped.
pub fn sheet_close(p: &Painter, rect: Rect, live: bool, a: f32) -> bool {
    let c = &p.theme.color;
    let hit = p.interact(rect, live);
    let at = rect.center();
    if hit.hover || hit.pressed {
        draw_circle(
            at.x,
            at.y,
            rect.w * 0.5,
            with_alpha(c.bg_highlight, 0.8 * a),
        );
    }
    let half = 5.5;
    let ink = with_alpha(c.text_muted, a);
    draw_line(at.x - half, at.y - half, at.x + half, at.y + half, 2.0, ink);
    draw_line(at.x - half, at.y + half, at.x + half, at.y - half, 2.0, ink);
    hit.clicked
}

/// Draw a modal — the scrim, the card, its header and this frame's motion —
/// and return the room left inside it.
///
/// A sheet's *content* is the caller's: the price breakdown fills it with
/// rows, a Perp's detail with its art and traits, the cart with what is in it.
/// The scrim, the card, the header, the footer's band and the motion are one
/// widget.
///
/// `height` is the card's height once open, chrome included (add
/// [`sheet_chrome`] to the content's height); [`SheetState`] decides where it
/// is.
pub fn sheet_card(
    painter: &Painter,
    state: &SheetState,
    area: Rect,
    height: f32,
    sheet: &Sheet,
) -> Option<SheetCard> {
    if !state.visible() {
        return None;
    }
    let c = &painter.theme.color;
    let e = state.progress();
    let form = SheetForm::for_area(area);
    let rect = sheet_rect(form, area, height, e);
    let k = density(area);
    let tint = sheet.tint.unwrap_or(c.accent);
    sheet_surface(painter, area, rect, form, e, tint);
    let alpha = if form == SheetForm::Modal { e } else { 1.0 };
    let live = state.is_open() && e > LIVE_AT;

    let handle = if form == SheetForm::Bottom {
        HANDLE_SPACE * k
    } else {
        0.0
    };
    let x = rect.x + PAD;
    let w = rect.w - PAD * 2.0;
    let top = rect.y + handle + PAD * k;
    let header = Rect::new(x, top, w, ICON_BUTTON);
    let mut dismissed = false;
    if sheet.has_header() {
        let mid = header.center().y;
        let close_w = if sheet.closable {
            ICON_BUTTON + 8.0
        } else {
            0.0
        };
        if let Some(title) = sheet.title {
            let size = painter.size(TextSize::Xl2);
            let fitted = painter.fit(title, size, w - close_w);
            painter.text_mid(&fitted, x, mid, size, with_alpha(c.text_primary, alpha));
        }
        if sheet.closable {
            let cross = Rect::new(header.right() - ICON_BUTTON, top, ICON_BUTTON, ICON_BUTTON);
            dismissed |= sheet_close(painter, cross, live, alpha);
        }
        if let Some(subtitle) = sheet.subtitle {
            let size = painter.size(TextSize::Md);
            let fitted = painter.fit(subtitle, size, w);
            painter.text_mid(
                &fitted,
                x,
                header.bottom() + SUBTITLE_H * 0.5 + 2.0,
                size,
                with_alpha(c.text_secondary, alpha),
            );
        }
    }
    let content_top = top + sheet.header_h();
    let footer = if sheet.footer {
        Rect::new(x, rect.bottom() - PAD * k - FOOTER_H * k, w, FOOTER_H * k)
    } else {
        Rect::new(x, rect.bottom() - PAD * k, w, 0.0)
    };
    let gap = if sheet.footer { FOOTER_GAP * k } else { 0.0 };
    // A tap outside the card, while it is all the way in, is a tap on the scrim.
    dismissed |= live && painter.tap.is_some_and(|t| !rect.contains(t));
    Some(SheetCard {
        rect,
        form,
        alpha,
        live,
        header,
        content: Rect::new(x, content_top, w, (footer.y - gap - content_top).max(0.0)),
        footer,
        dismissed,
    })
}

/// Room between the figure and the rows.
const FIGURE_GAP: f32 = 12.0;

/// How tall the sheet's content is, for a form.
fn content_height(
    p: &Painter,
    form: SheetForm,
    area: Rect,
    figure_h: f32,
    rows: &[WellRow],
    notes: usize,
) -> f32 {
    let figure = if figure_h > 0.0 {
        figure_h + FIGURE_GAP
    } else {
        0.0
    };
    let notes_h = if notes > 0 {
        10.0 + notes as f32 * NOTE_H
    } else {
        0.0
    };
    sheet_chrome(form, area, &detail_chrome("")) + figure + well_height(p, rows) + notes_h
}

/// The detail sheet's chrome: a title and the cross, no footer.
fn detail_chrome(title: &str) -> Sheet<'_> {
    Sheet::new().title(title).footer(false)
}

/// Where the sheet sits at `progress` (0 shut, 1 open).
///
/// A bottom sheet slides up from below the edge; a modal settles down a few
/// points into the centre as it fades in.
pub fn sheet_rect(form: SheetForm, area: Rect, height: f32, progress: f32) -> Rect {
    let p = progress.clamp(0.0, 1.0);
    match form {
        SheetForm::Bottom => {
            let w = (area.w - SHEET_SIDE_INSET * 2.0).min(SHEET_MAX_W);
            let h = height.min(area.h);
            let x = area.x + (area.w - w) * 0.5;
            let y = area.bottom() - h * p;
            Rect::new(x, y, w, h)
        }
        SheetForm::Modal => {
            let w = (area.w - PAD * 2.0).min(MODAL_MAX_W);
            let h = height.min(area.h - PAD * 2.0);
            let x = area.x + (area.w - w) * 0.5;
            let y = area.y + (area.h - h) * 0.5 + SETTLE_PX * (1.0 - p);
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

/// The breakdown's rows as facts in a well, each value in its tone's ink.
fn well_rows<'a>(p: &Painter, rows: &'a [DetailRow]) -> Vec<WellRow<'a>> {
    let c = &p.theme.color;
    rows.iter()
        .map(|row| {
            let ink = match row.tone {
                RowTone::Normal => c.text_primary,
                RowTone::Warn => c.warning,
                RowTone::Strong => c.accent,
            };
            WellRow::inline(&row.label, &row.value).ink(ink)
        })
        .collect()
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
    let form = SheetForm::for_area(area);
    let rows = well_rows(p, vm.rows);
    let h = content_height(p, form, area, vm.figure_h, &rows, vm.notes.len());
    let Some(card) = sheet_card(p, state, area, h, &detail_chrome(vm.title)) else {
        return out;
    };
    let alpha = card.alpha;
    out.rect = Some(card.rect);
    let body = card.content;
    let mut y = body.y;
    if vm.figure_h > 0.0 {
        out.figure = Some(Rect::new(body.x, y, body.w, vm.figure_h));
        y += vm.figure_h + FIGURE_GAP;
    }
    let well_h = well_height(p, &rows);
    well(
        p,
        Rect::new(body.x, y, body.w, well_h),
        &rows,
        card.live,
        alpha,
    );
    y += well_h;
    if !vm.notes.is_empty() {
        y += 10.0;
        let note = p.size(TextSize::Xs);
        for line in vm.notes {
            p.text_top(line, body.x, y, note, with_alpha(c.text_muted, alpha));
            y += NOTE_H;
        }
    }
    out.close = card.dismissed;
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
        assert_eq!(
            open.w,
            PHONE.w - SHEET_SIDE_INSET * 2.0,
            "the phone's width, less a strip at either side"
        );
        assert_eq!(open.x, PHONE.x + SHEET_SIDE_INSET, "centred");
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

    /// A caller that sizes a card from its own content adds [`sheet_chrome`]
    /// once, so the helper has to be the same money the card actually spends.
    /// Written out independently of the helper: the point is to catch a change
    /// to a constant that the helper was not told about.
    #[test]
    fn the_chrome_helper_is_the_paddings_the_handle_the_header_and_the_footer() {
        let roomy = Rect::new(0.0, 0.0, 390.0, 844.0);
        let plain = Sheet::new().closable(false);
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, roomy, &plain),
            HANDLE_SPACE + PAD + FOOTER_GAP + FOOTER_H + PAD
        );
        assert_eq!(
            sheet_chrome(SheetForm::Modal, roomy, &plain),
            PAD + FOOTER_GAP + FOOTER_H + PAD,
        );
        let titled = Sheet::new().title("Cart");
        assert_eq!(
            sheet_chrome(SheetForm::Modal, roomy, &titled),
            PAD + ICON_BUTTON + HEADER_GAP + FOOTER_GAP + FOOTER_H + PAD,
        );
        let subtitled = titled.subtitle("3 Perps");
        assert_eq!(
            sheet_chrome(SheetForm::Modal, roomy, &subtitled)
                - sheet_chrome(SheetForm::Modal, roomy, &titled),
            SUBTITLE_H
        );
        let bare = Sheet::new().closable(false).footer(false);
        assert_eq!(sheet_chrome(SheetForm::Modal, roomy, &bare), PAD + PAD);
    }

    /// A 320x224 canvas is a whole screen in a pixel game, and a sheet spaced for
    /// a phone would spend half of it on chrome.
    #[test]
    fn a_short_surface_compresses_the_chrome_but_a_phone_does_not() {
        let canvas = Rect::new(0.0, 0.0, 320.0, 224.0);
        let plain = Sheet::new().closable(false);
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, canvas, &plain),
            sheet_chrome(SheetForm::Bottom, PHONE, &plain) * COMPACT
        );
        assert_eq!(
            sheet_chrome(SheetForm::Bottom, PHONE, &plain),
            sheet_chrome(SheetForm::Bottom, DESK, &plain)
        );
    }
}
