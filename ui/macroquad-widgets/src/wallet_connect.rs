//! `WalletConnect` organism — the buyer-facing wallet selector + connected
//! state, the front door of the mint flow.
//!
//! VM-driven across the CIP-30 lifecycle: pick a detected wallet → connecting →
//! connected (name + address + disconnect) → error/retry. Each wallet row shows
//! its icon when the host supplies one (a decoded CIP-30 `icon` texture), else a
//! monogram avatar — so it looks right whether or not an icon is available
//! (e.g. SVG icons we can't rasterise yet fall back cleanly).
//!
//! ## Sentences wrap; labels do not
//!
//! An error is a wallet's or a bridge's own sentence and it is not bounded —
//! "Wrong network: the wallet reports 0, this app needs 1" is 374 units wide at
//! this widget's error size, which runs off a 350-wide phone column and hides
//! the half that says what to do. The empty-picker line has the same shape. Both
//! wrap to at most two lines, and only a message that still does not fit is cut
//! with an ellipsis.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::button::{Button, ButtonVariant};
use crate::painter::{Painter, draw_rounded_rect, with_alpha};

/// One detected wallet (`window.cardano.<key>`).
pub struct WalletItem {
    pub key: String,
    pub name: String,
    /// Decoded CIP-30 icon (host decodes the data-URI → texture); `None`
    /// renders a monogram avatar.
    pub icon: Option<Texture2D>,
}

impl WalletItem {
    /// Convenience for the common (no-icon) case.
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            icon: None,
        }
    }
}

pub enum WalletState {
    /// Detected wallets to choose from (empty = none injected).
    Disconnected(Vec<WalletItem>),
    Connecting,
    /// Connected: display name + (bech32) address.
    Connected {
        name: String,
        address: String,
    },
    Error(String),
}

pub struct WalletConnectVm {
    pub state: WalletState,
}

pub enum WalletAction {
    Connect(String),
    Disconnect,
    Retry,
}

pub struct WalletResponse {
    pub bottom: f32,
    pub action: Option<WalletAction>,
}

pub fn wallet_connect(
    p: &Painter,
    vm: &WalletConnectVm,
    x: f32,
    mut y: f32,
    w: f32,
) -> WalletResponse {
    let c = p.theme.color;
    let mut action = None;
    match &vm.state {
        WalletState::Disconnected(items) => {
            p.text_top(
                "Connect a wallet",
                x,
                y,
                p.size(TextSize::Xl),
                c.text_primary,
            );
            y += 28.0;
            if items.is_empty() {
                let size = p.size(TextSize::Base);
                for line in wrapped(p, NO_WALLETS, size, w, 2) {
                    p.text_top(&line, x, y, size, c.text_muted);
                    y += size + 3.0;
                }
                y += 4.0;
            } else {
                for it in items {
                    let row = Rect::new(x, y, w, 48.0);
                    let hit = p.interact(row, true);
                    let fill = if hit.pressed {
                        with_alpha(c.accent, 0.30)
                    } else if hit.hover {
                        with_alpha(c.accent, 0.20)
                    } else {
                        with_alpha(c.accent, 0.12)
                    };
                    draw_rounded_rect(row.x, row.y, row.w, row.h, 10.0, fill);
                    draw_avatar(p, x + 8.0, y + 8.0, 32.0, it);
                    let name_size = p.size(TextSize::Xl);
                    let baseline = p.centre_baseline(y, 48.0, name_size);
                    p.text(&it.name, x + 52.0, baseline, name_size, c.accent);
                    if hit.clicked {
                        action = Some(WalletAction::Connect(it.key.clone()));
                    }
                    y += 56.0;
                }
            }
        }
        WalletState::Connecting => {
            let pulse = (p.time() * 2.0).sin() as f32 * 0.5 + 0.5;
            draw_circle(
                x + 6.0,
                y + 9.0,
                5.0,
                with_alpha(c.accent, 0.3 + 0.7 * pulse),
            );
            p.text_top(
                "connecting...",
                x + 22.0,
                y,
                p.size(TextSize::Xl),
                c.text_primary,
            );
            y += 30.0;
        }
        WalletState::Connected { name, address } => {
            draw_circle(x + 6.0, y + 9.0, 5.0, c.accent);
            p.text_top(name, x + 22.0, y, p.size(TextSize::Xl), c.text_primary);
            if Button::new("disconnect")
                .variant(ButtonVariant::Ghost)
                .text_size(TextSize::Md)
                .show(p, Rect::new(x + w - 118.0, y - 4.0, 118.0, 30.0))
            {
                action = Some(WalletAction::Disconnect);
            }
            y += 26.0;
            let addr_size = p.size(TextSize::Base);
            p.mono(
                &short(address),
                x + 22.0,
                p.top_baseline(y, addr_size),
                addr_size,
                c.text_muted,
            );
            y += 22.0;
        }
        WalletState::Error(msg) => {
            let size = p.size(TextSize::Md);
            for line in wrapped(p, msg, size, w, 2) {
                p.text_top(&line, x, y, size, c.error);
                y += size + 3.0;
            }
            y += 6.0;
            if Button::new("Retry")
                .variant(ButtonVariant::Tonal)
                .show(p, Rect::new(x, y, 120.0, 40.0))
            {
                action = Some(WalletAction::Retry);
            }
            y += 48.0;
        }
    }
    WalletResponse { bottom: y, action }
}

/// The wallet icon (if supplied) or a monogram avatar in a tinted disc.
fn draw_avatar(p: &Painter, x: f32, y: f32, size: f32, item: &WalletItem) {
    if let Some(tex) = &item.icon {
        draw_texture_ex(
            tex,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(size, size)),
                ..Default::default()
            },
        );
    } else {
        let r = size * 0.5;
        draw_circle(x + r, y + r, r, with_alpha(p.theme.color.accent, 0.18));
        let ch = item
            .name
            .chars()
            .next()
            .unwrap_or('?')
            .to_ascii_uppercase()
            .to_string();
        // theme-exempt: a monogram fills the disc it sits in, so it is sized
        // from the avatar's diameter rather than from the type ramp.
        let glyph = size * 0.5;
        let dim = p.measure(&ch, glyph);
        let baseline = p.centre_baseline(y, size, glyph);
        p.text(
            &ch,
            x + r - dim.width * 0.5,
            baseline,
            glyph,
            p.theme.color.accent,
        );
    }
}

fn short(s: &str) -> String {
    if s.len() <= 20 {
        s.to_string()
    } else {
        format!("{}...{}", &s[..12], &s[s.len() - 6..])
    }
}

/// What the picker says when the page has injected no wallet at all.
const NO_WALLETS: &str = "No Cardano wallet detected — open in your wallet's dApp browser.";

/// `s` broken into at most `max_lines` lines that fit `w`, at word boundaries.
///
/// Only a message that still does not fit at the last line is cut, and then
/// with an ellipsis — which a capture's report flags as cut, because it is.
fn wrapped(p: &Painter, s: &str, size: f32, w: f32, max_lines: usize) -> Vec<String> {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if out.len() == max_lines {
            // Words are left and there is no line for them.
            if let Some(last) = out.last_mut() {
                *last = cut_to(p, last, size, w);
            }
            return out;
        }
        let mut line = words[i].to_string();
        i += 1;
        while i < words.len() {
            let candidate = format!("{line} {}", words[i]);
            if p.measure(&candidate, size).width > w {
                break;
            }
            line = candidate;
            i += 1;
        }
        out.push(if p.measure(&line, size).width > w {
            cut_to(p, &line, size, w)
        } else {
            line
        });
    }
    out
}

fn cut_to(p: &Painter, s: &str, size: f32, w: f32) -> String {
    let mut cut = s.to_string();
    while !cut.is_empty() && p.measure(&format!("{cut}…"), size).width > w {
        cut.pop();
    }
    format!("{cut}…")
}
