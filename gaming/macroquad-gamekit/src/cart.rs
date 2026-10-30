//! `Cart` — the things a reader means to buy, over the page: what each is, what it costs, the totals, and one button to go ahead, as a sheet on a phone and a card on a desk.
//!
//! Domain-free, like [`crate::wallet_modal::WalletModal`]. The host owns the
//! items and every word: it hands in a [`CartVm`] of rows (a picture, a title,
//! a line of detail, a figure), totals, the action's label and a
//! [`CartStage`], and acts on the [`CartAction`] the cart reports. The cart
//! owns only what it takes to show them: the sheet's slide and the list's
//! scroll.
//!
//! ## Stages
//!
//! Buying is the host's: building a transaction, the wallet's signature,
//! submitting, waiting for the chain. The cart shows where that stands,
//! in the host's own words:
//!
//! - **Editing**: rows can be removed, and the action is offered unless the
//!   host names a `blocker` ("Connect a wallet first").
//! - **Working**: a spinner and the step ("Approve in Eternl"). Rows cannot be
//!   removed, and the action waits.
//! - **Failed**: the reason, in the error colour; the action is offered again.
//! - **Done**: the outcome, in the success colour.
//!
//! So a transaction pipeline drives the cart without knowing how it is drawn,
//! and a story drives it with no pipeline at all.
//!
//! A host with more to show than words (a block train under a submitted
//! purchase) asks for a band of its own with [`CartVm::band`]: the card grows
//! to fit it under the stage line, and [`Cart::band`] says where it landed so
//! the host can draw there after [`Cart::show`].
//!
//! ## Input
//!
//! While [`Cart::visible`] the host should give the screen behind it no tap
//! and no drag. A tap on the dimmed screen, or on Close, reports
//! [`CartAction::Close`]; the host decides whether to close (a cart mid-way
//! through a signature might not).
//!
//! [`cart_button`] is the control that opens it: a pill with the count.

use macroquad::prelude::*;
use macroquad_widgets::painter::with_alpha;
use macroquad_widgets::{
    Align, Button, ButtonVariant, Painter, Scroll, ScrollInput, SheetForm, SheetState, TextSize,
    draw_rounded_rect, sheet_card, sheet_chrome,
};

use crate::clip::with_clip;
use crate::shape::{fill_rounded, fill_texture_rounded};

/// How a row's figure, or a total, is inked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CartTone {
    #[default]
    Normal,
    /// The figure that matters: a gain, the total to pay.
    Accent,
    Good,
    Warning,
}

/// One thing in the cart.
#[derive(Clone, Debug, PartialEq)]
pub struct CartRow {
    /// The host's key for it: what [`CartAction::Remove`] names.
    pub id: String,
    pub title: String,
    /// One line under the title ("+40 $PERP a day · ×3.99").
    pub detail: String,
    /// What it costs, as the host spells it ("₳85").
    pub figure: String,
    /// Its picture, once loaded; a rounded ground until then.
    pub image: Option<Texture2D>,
    pub detail_tone: CartTone,
}

/// One line of the totals.
#[derive(Clone, Debug, PartialEq)]
pub struct CartTotal {
    pub label: String,
    pub value: String,
    pub tone: CartTone,
    /// The line the action pays: larger, last.
    pub strong: bool,
}

/// Where buying stands, in the host's words.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CartStage {
    #[default]
    Editing,
    Working {
        step: String,
    },
    Failed {
        message: String,
    },
    Done {
        message: String,
    },
}

/// What the cart draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CartVm {
    pub title: String,
    pub rows: Vec<CartRow>,
    pub totals: Vec<CartTotal>,
    pub stage: CartStage,
    /// The action's label ("Buy 3 Perps").
    pub action: String,
    /// Why the action cannot be taken now, if it cannot; shown above it.
    pub blocker: Option<String>,
    /// What an empty cart says.
    pub empty: String,
    /// Height of a band under the stage line that the host draws into itself,
    /// after [`Cart::show`], at [`Cart::band`]: a block train under a
    /// submitted purchase, say. `0.0` for none.
    pub band: f32,
}

/// Where the host draws its band this frame, and the card's fade to draw it
/// with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartBand {
    pub rect: Rect,
    pub alpha: f32,
}

/// What the reader asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CartAction {
    Remove(String),
    /// The action: buy, or try again after a failure.
    Go,
    Close,
}

/// A row's height.
const ROW_H: f32 = 60.0;
const THUMB: f32 = 46.0;
const TITLE_BAND: f32 = 34.0;
const TOTAL_H: f32 = 24.0;
const STAGE_H: f32 = 26.0;
/// A remove button's hit area: a thumb, not a pixel.
const REMOVE_HIT: f32 = 40.0;
/// Screen the card leaves uncovered: more than the sheet chrome's own clamp
/// (40 on a desk), so a tall cart never has its totals cut.
const SCREEN_MARGIN: f32 = 72.0;

/// The cart's own state: the sheet's slide and the list's scroll.
#[derive(Debug, Default)]
pub struct Cart {
    sheet: SheetState,
    scroll: Scroll,
    band: Option<CartBand>,
}

impl Cart {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.sheet.open();
    }

    pub fn close(&mut self) {
        self.sheet.close();
    }

    /// Open, already all the way: for a capture of it.
    pub fn opened(mut self) -> Self {
        self.sheet = SheetState::new().opened();
        self
    }

    pub fn is_open(&self) -> bool {
        self.sheet.is_open()
    }

    /// Whether any of it is on screen: the host's cue to give the screen
    /// behind no input.
    pub fn visible(&self) -> bool {
        self.sheet.visible()
    }

    /// The host's band from the last [`Cart::show`], when the cart is showing
    /// and its view asked for one.
    pub fn band(&self) -> Option<CartBand> {
        self.band
    }

    /// Draw the cart over `area` (the whole screen) if any of it is showing,
    /// and report what the reader asked for. `input` scrolls a list longer
    /// than the sheet; `dt` moves the slide.
    pub fn show(
        &mut self,
        p: &Painter,
        area: Rect,
        vm: &CartVm,
        input: &ScrollInput,
        dt: f32,
    ) -> Option<CartAction> {
        // A cart that was not on screen when this frame began takes none of
        // its tap. The tap that opened it (on `cart_button`, earlier this
        // same frame) lands outside the card and would otherwise read as a
        // tap on the dimmed screen: the cart would close the moment it
        // opened.
        let was_visible = self.sheet.visible();
        self.band = None;
        self.sheet.update(dt);
        if !self.sheet.visible() {
            return None;
        }
        let quiet = p.untapped();
        let p = if was_visible { p } else { &quiet };
        let form = SheetForm::for_area(area);
        let fixed = fixed_height(form, area, vm);
        let wanted = vm.rows.len().max(1) as f32 * ROW_H;
        // A desk's card is clamped to the screen less its margins (and a
        // phone's sheet keeps a strip of page above it), so the list is sized
        // to what the card will really be, not to the screen.
        let room = (area.h - SCREEN_MARGIN - fixed).max(ROW_H);
        let list_h = wanted.min(room);
        let card = sheet_card(p, &self.sheet, area, fixed + list_h)?;
        let t = p.theme.color;
        let a = card.alpha;
        let c = card.content;
        let working = matches!(vm.stage, CartStage::Working { .. });
        let mut action = None;

        // The title, and the count beside it.
        let band = Rect::new(c.x, c.y, c.w, TITLE_BAND);
        p.text_in(
            &vm.title,
            band,
            p.size(TextSize::Lg),
            with_alpha(t.text_primary, a),
            Align::Start,
        );
        if !vm.rows.is_empty() {
            let count = format!("{}", vm.rows.len());
            p.mono_in(
                &count,
                band,
                p.size(TextSize::Sm),
                with_alpha(t.text_muted, a),
                Align::End,
            );
        }

        // The rows, scrolled when there are more than the sheet holds.
        let list = Rect::new(c.x, band.bottom(), c.w, list_h);
        let max = (wanted - list_h).max(0.0);
        self.scroll.update(list, input, Some(max), dt);
        let offset = self.scroll.offset();
        let tap = self.scroll.filter_tap(p.tap);
        if vm.rows.is_empty() {
            p.text_in(
                &vm.empty,
                Rect::new(list.x, list.y, list.w, ROW_H),
                p.size(TextSize::Sm),
                with_alpha(t.text_secondary, a),
                Align::Start,
            );
        }
        with_clip(list, || {
            for (i, row) in vm.rows.iter().enumerate() {
                let r = Rect::new(list.x, list.y + i as f32 * ROW_H - offset, list.w, ROW_H);
                if r.bottom() < list.y || r.y > list.bottom() {
                    continue;
                }
                if draw_row(p, r, row, a, !working, tap) {
                    action = Some(CartAction::Remove(row.id.clone()));
                }
            }
        });
        if max > 0.0 {
            // A thin bar: there is more below (or above).
            let track = list.h - 8.0;
            let thumb = (track * list.h / wanted).max(20.0);
            let y = list.y + 4.0 + (track - thumb) * (offset / max);
            draw_rounded_rect(
                list.right() - 2.0,
                y,
                3.0,
                thumb,
                1.5,
                with_alpha(t.text_muted, 0.5 * a),
            );
        }

        // The totals.
        let mut y = list.bottom() + 8.0;
        draw_line(c.x, y, c.right(), y, 1.0, with_alpha(t.bg_highlight, a));
        y += 6.0;
        for total in &vm.totals {
            let size = p.size(if total.strong {
                TextSize::Md
            } else {
                TextSize::Sm
            });
            let r = Rect::new(c.x, y, c.w, TOTAL_H);
            let label_ink = if total.strong {
                t.text_primary
            } else {
                t.text_secondary
            };
            p.text_in(
                &total.label,
                r,
                size,
                with_alpha(label_ink, a),
                Align::Start,
            );
            p.mono_in(
                &total.value,
                r,
                size,
                with_alpha(ink(p, total.tone), a),
                Align::End,
            );
            y += TOTAL_H;
        }

        // Where buying stands, or why it cannot start.
        let stage_line = match (&vm.stage, &vm.blocker) {
            (CartStage::Working { step }, _) => Some((step.as_str(), t.text_secondary, true)),
            (CartStage::Failed { message }, _) => Some((message.as_str(), t.error, false)),
            (CartStage::Done { message }, _) => Some((message.as_str(), t.success, false)),
            (CartStage::Editing, Some(blocker)) => Some((blocker.as_str(), t.text_muted, false)),
            (CartStage::Editing, None) => None,
        };
        if let Some((text, color, spinning)) = stage_line {
            let r = Rect::new(c.x, y + 4.0, c.w, STAGE_H - 4.0);
            let xs = p.size(TextSize::Sm);
            let mut text_r = r;
            if spinning {
                spinner(
                    vec2(r.x + 8.0, r.center().y),
                    6.0,
                    p.time() as f32,
                    with_alpha(t.accent, a),
                );
                text_r.x += 22.0;
                text_r.w -= 22.0;
            }
            p.text_in(
                &p.fit(text, xs, text_r.w),
                text_r,
                xs,
                with_alpha(color, a),
                Align::Start,
            );
            y += STAGE_H;
        }

        // The host's band: reserved here, drawn by the host after this call.
        if vm.band > 0.0 {
            self.band = Some(CartBand {
                rect: Rect::new(c.x, y + 4.0, c.w, vm.band),
                alpha: a,
            });
        }

        // Close, and the action.
        let gap = 10.0;
        let close_w = (card.close.w * 0.34).max(90.0);
        let close_r = Rect::new(card.close.x, card.close.y, close_w, card.close.h);
        let go_r = Rect::new(
            close_r.right() + gap,
            card.close.y,
            card.close.w - close_w - gap,
            card.close.h,
        );
        if Button::new("Close")
            .variant(ButtonVariant::Tonal)
            .show(p, close_r)
        {
            action = Some(CartAction::Close);
        }
        let can_go = !working
            && !vm.rows.is_empty()
            && vm.blocker.is_none()
            && !matches!(vm.stage, CartStage::Done { .. });
        if Button::new(&vm.action).enabled(can_go).show(p, go_r) {
            action = Some(CartAction::Go);
        }
        if action.is_none() && tap.is_some_and(|at| !card.rect.contains(at)) {
            action = Some(CartAction::Close);
        }
        action
    }
}

/// Everything but the list: the chrome, the title, the totals, the stage.
fn fixed_height(form: SheetForm, area: Rect, vm: &CartVm) -> f32 {
    let stage = if matches!(vm.stage, CartStage::Editing) && vm.blocker.is_none() {
        0.0
    } else {
        STAGE_H
    };
    let band = if vm.band > 0.0 { vm.band + 8.0 } else { 0.0 };
    sheet_chrome(form, area) + TITLE_BAND + 14.0 + vm.totals.len() as f32 * TOTAL_H + stage + band
}

fn ink(p: &Painter, tone: CartTone) -> Color {
    let t = p.theme.color;
    match tone {
        CartTone::Normal => t.text_primary,
        CartTone::Accent => t.accent,
        CartTone::Good => t.success,
        CartTone::Warning => t.warning,
    }
}

/// One row: the picture, the title over its detail, the figure, and a remove
/// button when `removable`. `true` when that button was tapped.
fn draw_row(
    p: &Painter,
    r: Rect,
    row: &CartRow,
    a: f32,
    removable: bool,
    tap: Option<Vec2>,
) -> bool {
    let t = p.theme.color;
    let thumb = Rect::new(r.x, r.center().y - THUMB * 0.5, THUMB, THUMB);
    match &row.image {
        Some(texture) => fill_texture_rounded(texture, thumb, 8.0, Color::new(1.0, 1.0, 1.0, a)),
        None => fill_rounded(thumb, 8.0, with_alpha(t.bg_highlight, a)),
    }
    let sm = p.size(TextSize::Sm);
    let xs = p.size(TextSize::Xs);
    let remove = Rect::new(
        r.right() - REMOVE_HIT,
        r.center().y - REMOVE_HIT * 0.5,
        REMOVE_HIT,
        REMOVE_HIT,
    );
    let figure_w = p.measure_mono(&row.figure, sm).width;
    let figure_r = Rect::new(remove.x - figure_w - 6.0, r.y, figure_w, r.h);
    p.mono_in(
        &row.figure,
        figure_r,
        sm,
        with_alpha(t.text_primary, a),
        Align::End,
    );
    let words_x = thumb.right() + 12.0;
    let words_w = (figure_r.x - 10.0 - words_x).max(0.0);
    let title = Rect::new(words_x, r.y + 10.0, words_w, sm + 4.0);
    let detail = Rect::new(words_x, title.bottom() + 3.0, words_w, xs + 4.0);
    p.text_in(
        &p.fit(&row.title, sm, title.w),
        title,
        sm,
        with_alpha(t.text_primary, a),
        Align::Start,
    );
    p.text_in(
        &p.fit(&row.detail, xs, detail.w),
        detail,
        xs,
        with_alpha(ink(p, row.detail_tone), a),
        Align::Start,
    );
    // The remove mark: a small cross, quiet until it can be used.
    let ink = if removable {
        t.text_secondary
    } else {
        with_alpha(t.text_muted, 0.4)
    };
    let c = remove.center();
    let k = 5.0;
    draw_line(c.x - k, c.y - k, c.x + k, c.y + k, 1.6, with_alpha(ink, a));
    draw_line(c.x - k, c.y + k, c.x + k, c.y - k, 1.6, with_alpha(ink, a));
    draw_line(
        r.x,
        r.bottom() - 0.5,
        r.right(),
        r.bottom() - 0.5,
        1.0,
        with_alpha(t.bg_highlight, 0.6 * a),
    );
    removable && tap.is_some_and(|at| remove.contains(at))
}

/// A turning arc of dots at `c`, on the clock `time`.
fn spinner(c: Vec2, radius: f32, time: f32, color: Color) {
    const DOTS: usize = 8;
    let head = (time * 10.0) as usize % DOTS;
    for i in 0..DOTS {
        let angle = i as f32 / DOTS as f32 * std::f32::consts::TAU;
        let behind = (head + DOTS - i) % DOTS;
        let k = 1.0 - behind as f32 / DOTS as f32;
        draw_circle(
            c.x + angle.cos() * radius,
            c.y + angle.sin() * radius,
            1.6,
            Color {
                a: color.a * (0.15 + 0.85 * k),
                ..color
            },
        );
    }
}

/// The control that opens the cart: a pill with the count, and `label` (a
/// total, say) when there is room. Quiet while empty. `true` when tapped.
pub fn cart_button(p: &Painter, rect: Rect, count: usize, label: &str) -> bool {
    let t = p.theme.color;
    let hit = p.interact(rect, true);
    let r = rect.h * 0.5;
    let filled = count > 0;
    let ground = if filled {
        with_alpha(t.accent, if hit.hover { 0.3 } else { 0.22 })
    } else {
        with_alpha(t.bg_highlight, if hit.hover { 0.9 } else { 0.7 })
    };
    draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, r, ground);
    let sm = p.size(TextSize::Sm);
    let xs = p.size(TextSize::Xs);
    // The count in a disc at the start.
    let disc = vec2(rect.x + r, rect.center().y);
    draw_circle(
        disc.x,
        disc.y,
        r - 4.0,
        if filled { t.accent } else { t.text_muted },
    );
    let n = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    p.text_centred(
        &n,
        Rect::new(disc.x - r, rect.y, r * 2.0, rect.h),
        xs,
        t.bg_primary,
    );
    let words = Rect::new(
        disc.x + r + 2.0,
        rect.y,
        rect.right() - disc.x - r - 12.0,
        rect.h,
    );
    let text = if label.is_empty() { "Cart" } else { label };
    let ink = if filled { t.accent } else { t.text_secondary };
    p.text_in(&p.fit(text, sm, words.w), words, sm, ink, Align::Start);
    hit.clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(rows: usize, stage: CartStage) -> CartVm {
        CartVm {
            title: "Cart".into(),
            rows: (0..rows)
                .map(|i| CartRow {
                    id: format!("p{i}"),
                    title: format!("Perp #{i:04}"),
                    detail: "+40 a day".into(),
                    figure: "₳85".into(),
                    image: None,
                    detail_tone: CartTone::Good,
                })
                .collect(),
            totals: vec![CartTotal {
                label: "Total".into(),
                value: "₳255".into(),
                tone: CartTone::Accent,
                strong: true,
            }],
            stage,
            action: "Buy".into(),
            blocker: None,
            empty: "Nothing yet".into(),
            band: 0.0,
        }
    }

    /// A host's band is room the card makes for it, below everything else.
    #[test]
    fn a_band_adds_its_height_and_a_gap() {
        let area = Rect::new(0.0, 0.0, 1200.0, 800.0);
        let form = SheetForm::for_area(area);
        let plain = fixed_height(form, area, &vm(2, CartStage::Editing));
        let mut banded = vm(2, CartStage::Editing);
        banded.band = 90.0;
        assert_eq!(fixed_height(form, area, &banded) - plain, 98.0);
    }

    /// The stage line takes room only when it has something to say.
    #[test]
    fn the_stage_line_takes_room_only_when_there_is_one() {
        let area = Rect::new(0.0, 0.0, 1200.0, 800.0);
        let form = SheetForm::for_area(area);
        let editing = fixed_height(form, area, &vm(2, CartStage::Editing));
        let working = fixed_height(
            form,
            area,
            &vm(
                2,
                CartStage::Working {
                    step: "Signing".into(),
                },
            ),
        );
        assert_eq!(working - editing, STAGE_H);
        let mut blocked = vm(2, CartStage::Editing);
        blocked.blocker = Some("Connect a wallet first".into());
        assert_eq!(fixed_height(form, area, &blocked) - editing, STAGE_H);
    }

    #[test]
    fn a_shut_cart_is_not_visible_and_an_opened_one_is() {
        let mut cart = Cart::new();
        assert!(!cart.visible());
        cart.open();
        assert!(cart.is_open());
        let cart = Cart::new().opened();
        assert!(cart.visible() && cart.is_open());
    }
}
