//! `OrderFulfilment` — the buyer-facing "what's happening to my order" widget.
//!
//! Captures the **1 order → N fulfilment txs** model explicitly: an order has
//! ONE payment (shown muted, distinct), a live status heartbeat + minted/total
//! progress, and a list of the N mint transactions that fulfilled it (each
//! tappable to view on-chain). The payment tx is deliberately NOT the mint
//! link — that conflation is the egui bug this replaces.
//!
//! VM-driven and stateless: the host polls its fulfilment endpoint, projects a
//! [`OrderFulfilmentVm`], and the widget returns a [`FulfilmentAction`] on tap.

use macroquad::prelude::*;

use crate::painter::Painter;
use crate::theme;

/// Order lifecycle status. Mirrors `shared_types::mint::MintOrderStatus`, kept
/// local so the widget crate carries no backend dependency.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Fulfilling,
    Submitted,
    Confirmed,
    Delivered,
    Unfulfilled,
    Failed,
}

impl OrderStatus {
    fn label(self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Fulfilling => "fulfilling",
            OrderStatus::Submitted => "submitted",
            OrderStatus::Confirmed => "confirmed",
            OrderStatus::Delivered => "delivered",
            OrderStatus::Unfulfilled => "sold out",
            OrderStatus::Failed => "failed",
        }
    }

    fn color(self) -> Color {
        match self {
            OrderStatus::Confirmed | OrderStatus::Delivered => theme::ACCENT,
            OrderStatus::Unfulfilled | OrderStatus::Failed => theme::DANGER,
            _ => theme::FG,
        }
    }

    /// Still working — drives the pulsing heartbeat dot.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            OrderStatus::Pending | OrderStatus::Fulfilling | OrderStatus::Submitted
        )
    }
}

/// Per-tx confirmation state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FulfilmentStatus {
    Submitted,
    Confirmed,
    Failed,
}

impl FulfilmentStatus {
    fn word(self) -> &'static str {
        match self {
            FulfilmentStatus::Submitted => "submitted",
            FulfilmentStatus::Confirmed => "confirmed",
            FulfilmentStatus::Failed => "failed",
        }
    }

    fn color(self) -> Color {
        match self {
            FulfilmentStatus::Submitted => theme::LINK,
            FulfilmentStatus::Confirmed => theme::ACCENT,
            FulfilmentStatus::Failed => theme::DANGER,
        }
    }
}

/// One fulfilment (mint) transaction — the N side of the model.
#[derive(Clone)]
pub struct FulfilmentTx {
    pub tx_hash: String,
    /// Units minted in THIS tx (slots it claimed).
    pub minted: u32,
    pub status: FulfilmentStatus,
}

/// Everything the widget renders — projected by the host from its polled
/// fulfilment view.
#[derive(Clone)]
pub struct OrderFulfilmentVm {
    pub status: OrderStatus,
    /// Units ordered (the entitlement / denominator).
    pub quantity: u32,
    /// Units minted so far (numerator) — slots in `minting`/`minted`.
    pub minted: u32,
    /// The payment tx (== order id). Shown as "paid", NOT as a mint link.
    pub payment_tx: String,
    /// The N fulfilment txs (empty until the first mint lands).
    pub fulfilments: Vec<FulfilmentTx>,
    /// Heartbeat freshness — seconds since the last poll, if known.
    pub updated_secs_ago: Option<u32>,
}

/// What the host should act on after `order_fulfilment`.
pub enum FulfilmentAction {
    /// View this fulfilment tx on-chain (host opens its tx viewer).
    OpenTx(String),
}

pub struct FulfilmentResponse {
    /// Bottom y after rendering — hosts stack content below.
    pub bottom: f32,
    pub action: Option<FulfilmentAction>,
}

/// Render the order heartbeat + 1→N fulfilment list in the column
/// `[x, x + w]`, starting at baseline `y`.
pub fn order_fulfilment(
    p: &Painter,
    vm: &OrderFulfilmentVm,
    x: f32,
    mut y: f32,
    w: f32,
) -> FulfilmentResponse {
    let mut action = None;

    // ── Heartbeat: a pulsing dot for active orders, solid for terminal ──
    if vm.status.is_active() {
        let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
        draw_circle(x + 6.0, y - 6.0, 5.0, theme::with_alpha(theme::ACCENT, 0.3 + 0.7 * pulse));
    } else {
        draw_circle(x + 6.0, y - 6.0, 5.0, vm.status.color());
    }
    p.text(vm.status.label(), x + 22.0, y, 22.0, vm.status.color());
    if let Some(s) = vm.updated_secs_ago {
        let t = format!("updated {}", ago(s));
        let dim = p.measure(&t, 14.0);
        p.text(&t, x + w - dim.width, y - 2.0, 14.0, theme::MUTED);
    }
    y += 24.0;

    // ── Progress: minted X / N + bar ──
    let frac = if vm.quantity > 0 {
        vm.minted as f32 / vm.quantity as f32
    } else {
        0.0
    };
    p.text(
        &format!("minted {} / {}", vm.minted, vm.quantity),
        x,
        y,
        16.0,
        theme::FG,
    );
    y += 8.0;
    p.progress(Rect::new(x, y, w, 8.0), frac, theme::ACCENT);
    y += 22.0;

    // ── Payment line — DISTINCT from fulfilment (this is the bug fix) ──
    // Label proportional, hash monospace.
    let paid = "paid · ";
    p.text(paid, x, y, 14.0, theme::MUTED);
    let pw = p.measure(paid, 14.0).width;
    p.mono(&short(&vm.payment_tx), x + pw, y, 14.0, theme::MUTED);
    y += 24.0;

    // ── Fulfilment list (the N side) ──
    let n = vm.fulfilments.len();
    p.text(
        &format!("fulfilment · {n} tx{}", if n == 1 { "" } else { "s" }),
        x,
        y,
        15.0,
        theme::FG,
    );
    y += 22.0;

    if vm.fulfilments.is_empty() {
        let msg = match vm.status {
            OrderStatus::Unfulfilled => "sold out — refund queued",
            OrderStatus::Failed => "failed — refund queued",
            _ => "waiting for first mint...",
        };
        p.text(msg, x + 8.0, y, 14.0, theme::MUTED);
        y += 22.0;
    } else {
        for f in &vm.fulfilments {
            let row = Rect::new(x, y - 14.0, w, 26.0);
            draw_circle(x + 6.0, y - 5.0, 4.0, f.status.color());
            // Count + hash are fixed-width data → monospace.
            p.mono(
                &format!("{}x  {}", f.minted, short(&f.tx_hash)),
                x + 18.0,
                y,
                14.0,
                theme::LINK,
            );
            let word = f.status.word();
            let dim = p.measure(word, 13.0);
            p.text(word, x + w - dim.width, y - 1.0, 13.0, f.status.color());
            if p.tapped(row) {
                action = Some(FulfilmentAction::OpenTx(f.tx_hash.clone()));
            }
            y += 26.0;
        }
        p.text("tap a tx to view on-chain", x + 8.0, y, 12.0, theme::MUTED);
        y += 18.0;
    }

    FulfilmentResponse { bottom: y, action }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn short(s: &str) -> String {
    if s.len() <= 18 {
        s.to_string()
    } else {
        format!("{}...{}", &s[..10], &s[s.len() - 6..])
    }
}

/// Compact "time ago" — `3s` / `4m` / `2h` / `5d`.
fn ago(secs: u32) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
