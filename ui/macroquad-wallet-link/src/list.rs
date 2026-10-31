//! The list view: the same wallets as plain rows, for a reader who would
//! rather read than look. A row is a small moon in its chain's colour, the
//! wallet's name, and its chain and short address; a tap opens the same
//! sheet the orbit does.

use macroquad::prelude::*;
use macroquad_gamekit::Frame;
use macroquad_gamekit::wallet_modal::{chain_mark, short_address};
use macroquad_gamekit::with_clip;
use macroquad_widgets::{
    Align, Line, Painter, Scroll, ScrollInput, TextSize, fill_rounded, stroke_rounded,
};
use wallet_link_flow::family_of;
use wallet_link_flow::wallet_link::LinkedWallet;

use crate::orbit::{moon_tint, wallet_label};

const ROW_H: f32 = 64.0;
const ROW_GAP: f32 = 8.0;
const ROW_R: f32 = 16.0;
const ICON_R: f32 = 15.0;
/// The widest the rows grow on a desk.
const MAX_W: f32 = 520.0;

/// See the [module docs](self).
#[derive(Default)]
pub struct ListView {
    scroll: Scroll,
}

impl ListView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw `wallets` into `field` and report the address of a row tapped.
    /// `live` is whether it takes input (not behind a sheet, not fading out).
    pub fn show(
        &mut self,
        frame: &Frame,
        p: &Painter,
        field: Rect,
        wallets: &[LinkedWallet],
        selected: Option<&str>,
        fade: f32,
        live: bool,
    ) -> Option<String> {
        let c = p.theme.color;
        let w = field.w.min(MAX_W);
        let x = field.center().x - w * 0.5;
        let content = wallets.len() as f32 * (ROW_H + ROW_GAP);
        let input = if live {
            ScrollInput::read(frame.input.gesture.drag, frame.pointer())
        } else {
            ScrollInput::none()
        };
        self.scroll
            .update(field, &input, Some((content - field.h).max(0.0)), frame.dt);
        let tap = if live {
            self.scroll.filter_tap(p.tap).filter(|t| field.contains(*t))
        } else {
            None
        };
        let offset = self.scroll.offset();
        let mut hit = None;
        with_clip(field, || {
            for (i, wallet) in wallets.iter().enumerate() {
                let row = Rect::new(
                    x,
                    field.y + i as f32 * (ROW_H + ROW_GAP) - offset,
                    w,
                    ROW_H,
                );
                if row.bottom() < field.y || row.y > field.bottom() {
                    continue;
                }
                let family = family_of(wallet);
                let tint = moon_tint(p, family);
                let is_sel = selected == Some(wallet.address.as_str());
                let pressed = tap.is_some_and(|t| row.contains(t));
                if pressed {
                    hit = Some(wallet.address.clone());
                }
                let ground = if is_sel { c.bg_highlight } else { c.bg_secondary };
                fill_rounded(row, ROW_R, alpha(ground, 0.92 * fade));
                stroke_rounded(
                    row,
                    ROW_R,
                    1.0,
                    alpha(if is_sel { tint } else { c.border }, fade),
                );
                let at = vec2(row.x + 16.0 + ICON_R, row.center().y);
                draw_circle(at.x, at.y, ICON_R, alpha(tint, 0.9 * fade));
                if let Some(f) = family {
                    chain_mark(f, at, ICON_R * 1.1, alpha(c.bg_primary, 0.8 * fade));
                }
                let text = Rect::new(
                    at.x + ICON_R + 14.0,
                    row.y,
                    row.right() - at.x - ICON_R - 44.0,
                    row.h,
                );
                let chain = family.map_or("Unknown chain", |f| f.display_name());
                let sub = format!("{chain} · {}", short_address(&wallet.address));
                let label = wallet_label(wallet);
                p.stack_in(
                    &[
                        Line::new(&label, p.size(TextSize::Lg), alpha(c.text_primary, fade)),
                        Line::new(&sub, p.size(TextSize::Sm), alpha(c.text_muted, fade)),
                    ],
                    text,
                    Align::Start,
                    4.0,
                );
                // A chevron: the row opens something.
                let cx = row.right() - 22.0;
                let cy = row.center().y;
                let ink = alpha(c.text_muted, fade);
                draw_line(cx - 3.0, cy - 6.0, cx + 3.0, cy, 2.0, ink);
                draw_line(cx + 3.0, cy, cx - 3.0, cy + 6.0, 2.0, ink);
            }
        });
        if wallets.is_empty() {
            p.text_in(
                "No wallets linked yet",
                Rect::new(field.x, field.y + 24.0, field.w, 24.0),
                p.size(TextSize::Md),
                alpha(c.text_muted, fade),
                Align::Center,
            );
        }
        hit
    }
}

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}
