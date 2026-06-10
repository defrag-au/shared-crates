//! `WalletConnect` organism — the buyer-facing wallet selector + connected
//! state, the front door of the mint flow.
//!
//! VM-driven across the CIP-30 lifecycle: pick a detected wallet → connecting →
//! connected (name + address + disconnect) → error/retry. Each wallet row shows
//! its icon when the host supplies one (a decoded CIP-30 `icon` texture), else a
//! monogram avatar — so it looks right whether or not an icon is available
//! (e.g. SVG icons we can't rasterise yet fall back cleanly).

use macroquad::prelude::*;

use crate::button::{Button, ButtonVariant};
use crate::painter::{draw_rounded_rect, with_alpha, Painter};

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
    let t = p.theme;
    let mut action = None;
    match &vm.state {
        WalletState::Disconnected(items) => {
            p.text_top("Connect a wallet", x, y, 16.0, t.fg);
            y += 28.0;
            if items.is_empty() {
                p.text_top(
                    "No Cardano wallet detected — open in your wallet's dApp browser.",
                    x,
                    y,
                    13.0,
                    t.muted,
                );
                y += 22.0;
            } else {
                for it in items {
                    let row = Rect::new(x, y, w, 48.0);
                    let hit = p.interact(row, true);
                    let fill = if hit.pressed {
                        with_alpha(t.accent, 0.30)
                    } else if hit.hover {
                        with_alpha(t.accent, 0.20)
                    } else {
                        with_alpha(t.accent, 0.12)
                    };
                    draw_rounded_rect(row.x, row.y, row.w, row.h, 10.0, fill);
                    draw_avatar(p, x + 8.0, y + 8.0, 32.0, it);
                    let baseline = p.centre_baseline(y, 48.0, 16.0);
                    p.text(&it.name, x + 52.0, baseline, 16.0, t.accent);
                    if hit.clicked {
                        action = Some(WalletAction::Connect(it.key.clone()));
                    }
                    y += 56.0;
                }
            }
        }
        WalletState::Connecting => {
            let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
            draw_circle(
                x + 6.0,
                y + 9.0,
                5.0,
                with_alpha(t.accent, 0.3 + 0.7 * pulse),
            );
            p.text_top("connecting...", x + 22.0, y, 16.0, t.fg);
            y += 30.0;
        }
        WalletState::Connected { name, address } => {
            draw_circle(x + 6.0, y + 9.0, 5.0, t.accent);
            p.text_top(name, x + 22.0, y, 16.0, t.fg);
            if Button::new("disconnect")
                .variant(ButtonVariant::Ghost)
                .font_size(14.0)
                .show(p, Rect::new(x + w - 118.0, y - 4.0, 118.0, 30.0))
            {
                action = Some(WalletAction::Disconnect);
            }
            y += 26.0;
            p.mono(
                &short(address),
                x + 22.0,
                p.top_baseline(y, 13.0),
                13.0,
                t.muted,
            );
            y += 22.0;
        }
        WalletState::Error(msg) => {
            p.text_top(msg, x, y, 14.0, t.danger);
            y += 26.0;
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
        draw_circle(x + r, y + r, r, with_alpha(p.theme.accent, 0.18));
        let ch = item
            .name
            .chars()
            .next()
            .unwrap_or('?')
            .to_ascii_uppercase()
            .to_string();
        let dim = p.measure(&ch, size * 0.5);
        let baseline = p.centre_baseline(y, size, size * 0.5);
        p.text(
            &ch,
            x + r - dim.width * 0.5,
            baseline,
            size * 0.5,
            p.theme.accent,
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
