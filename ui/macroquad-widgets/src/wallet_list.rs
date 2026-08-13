//! `WalletList` organism — a user's linked wallets, in their own order.
//!
//! Sibling to [`crate::wallet_connect`], and deliberately a separate widget:
//! connecting is a *session* ("which wallet am I acting as right now"), while
//! linking is an *identity claim* that outlives the tab. Collapsing the two
//! into one widget would mean one `Connected` state standing for two very
//! different commitments.
//!
//! Design notes:
//!
//! - **Handle first, address second.** `$alice` is recognisable and checkable
//!   at a glance; a truncated `stake1u…c5rhq9` is not, and its visible ends
//!   collide often enough that people confirm the wrong wallet. The address is
//!   always shown underneath, so nothing is hidden — it just isn't the label.
//! - **Order is the user's**, not a ranking we invent. Rows render in the order
//!   given and the move actions report intent; the host owns persistence.
//! - **No "primary" badge.** Every linked wallet counts equally for ownership,
//!   so marking one would imply a hierarchy that doesn't exist.

use macroquad::prelude::*;

use crate::button::{Button, ButtonVariant};
use crate::painter::{draw_rounded_rect, with_alpha, Painter};

/// One linked wallet.
pub struct WalletRow {
    /// Bech32 stake address — the identity that was actually proved.
    pub stake_address: String,
    /// ADA Handle, when the wallet has one.
    pub handle: Option<String>,
}

impl WalletRow {
    pub fn new(stake_address: impl Into<String>) -> Self {
        Self {
            stake_address: stake_address.into(),
            handle: None,
        }
    }

    pub fn with_handle(mut self, handle: impl Into<String>) -> Self {
        self.handle = Some(handle.into());
        self
    }

    /// How to name this wallet to a human — handle if there is one.
    pub fn display(&self) -> String {
        match &self.handle {
            Some(h) => format!("${h}"),
            None => short(&self.stake_address),
        }
    }
}

pub enum WalletListState {
    /// Fetching. Distinct from `Empty` so a slow load never reads as "you have
    /// no wallets", which would prompt someone to link a duplicate.
    Loading,
    /// Loaded, and genuinely none.
    Empty,
    Ready(Vec<WalletRow>),
    Error(String),
}

pub struct WalletListVm {
    pub state: WalletListState,
    /// Index of a row with an operation in flight; its controls are disabled so
    /// a second click can't race the first.
    pub busy: Option<usize>,
}

impl WalletListVm {
    pub fn new(state: WalletListState) -> Self {
        Self { state, busy: None }
    }

    pub fn busy(mut self, index: usize) -> Self {
        self.busy = Some(index);
        self
    }
}

pub enum WalletListAction {
    /// Start the link flow for an additional wallet.
    LinkAnother,
    /// Release a wallet. Carries the address rather than the index, because the
    /// list can be reordered between render and dispatch.
    Unlink(String),
    /// Move a row one place toward the front / back.
    MoveUp(usize),
    MoveDown(usize),
    Retry,
}

pub struct WalletListResponse {
    pub bottom: f32,
    pub action: Option<WalletListAction>,
}

const ROW_H: f32 = 56.0;

pub fn wallet_list(
    p: &Painter,
    vm: &WalletListVm,
    x: f32,
    mut y: f32,
    w: f32,
) -> WalletListResponse {
    let t = p.theme;
    let mut action = None;

    match &vm.state {
        WalletListState::Loading => {
            let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
            draw_circle(
                x + 6.0,
                y + 9.0,
                5.0,
                with_alpha(t.accent, 0.3 + 0.7 * pulse),
            );
            p.text_top("loading wallets...", x + 22.0, y, 16.0, t.muted);
            y += 30.0;
        }

        WalletListState::Empty => {
            p.text_top("No wallets linked yet", x, y, 16.0, t.fg);
            y += 24.0;
            p.text_top(
                "Link one to prove ownership of your assets.",
                x,
                y,
                13.0,
                t.muted,
            );
            y += 28.0;
            if Button::new("Link a wallet")
                .variant(ButtonVariant::Tonal)
                .show(p, Rect::new(x, y, 160.0, 40.0))
            {
                action = Some(WalletListAction::LinkAnother);
            }
            y += 48.0;
        }

        WalletListState::Ready(rows) => {
            p.text_top("Linked wallets", x, y, 16.0, t.fg);
            y += 28.0;

            let last = rows.len().saturating_sub(1);
            for (i, row) in rows.iter().enumerate() {
                let enabled = vm.busy != Some(i);
                let rect = Rect::new(x, y, w, ROW_H);
                draw_rounded_rect(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    10.0,
                    with_alpha(t.accent, if enabled { 0.12 } else { 0.06 }),
                );

                // Ordinal, so the user's arrangement is legible rather than
                // implied purely by vertical position.
                p.mono(
                    &format!("{}", i + 1),
                    x + 12.0,
                    p.centre_baseline(y, ROW_H, 13.0),
                    13.0,
                    t.muted,
                );

                let label_colour = if enabled { t.accent } else { t.muted };
                p.text_top(&row.display(), x + 38.0, y + 10.0, 16.0, label_colour);
                p.mono(
                    &short(&row.stake_address),
                    x + 38.0,
                    p.top_baseline(y + 32.0, 12.0),
                    12.0,
                    t.muted,
                );

                // Reorder controls. The ends are disabled rather than hidden so
                // the row's controls don't shift position between rows.
                let ctl_x = x + w - 150.0;
                if Button::new("↑")
                    .variant(ButtonVariant::Ghost)
                    .font_size(14.0)
                    .enabled(enabled && i > 0)
                    .show(p, Rect::new(ctl_x, y + 12.0, 32.0, 32.0))
                {
                    action = Some(WalletListAction::MoveUp(i));
                }
                if Button::new("↓")
                    .variant(ButtonVariant::Ghost)
                    .font_size(14.0)
                    .enabled(enabled && i < last)
                    .show(p, Rect::new(ctl_x + 36.0, y + 12.0, 32.0, 32.0))
                {
                    action = Some(WalletListAction::MoveDown(i));
                }
                if Button::new("unlink")
                    .variant(ButtonVariant::Ghost)
                    .font_size(13.0)
                    .enabled(enabled)
                    .show(p, Rect::new(ctl_x + 74.0, y + 12.0, 68.0, 32.0))
                {
                    action = Some(WalletListAction::Unlink(row.stake_address.clone()));
                }

                y += ROW_H + 8.0;
            }

            y += 8.0;
            if Button::new("Link another")
                .variant(ButtonVariant::Tonal)
                .show(p, Rect::new(x, y, 160.0, 40.0))
            {
                action = Some(WalletListAction::LinkAnother);
            }
            y += 48.0;
        }

        WalletListState::Error(msg) => {
            p.text_top(msg, x, y, 14.0, t.danger);
            y += 26.0;
            if Button::new("Retry")
                .variant(ButtonVariant::Tonal)
                .show(p, Rect::new(x, y, 120.0, 40.0))
            {
                action = Some(WalletListAction::Retry);
            }
            y += 48.0;
        }
    }

    WalletListResponse { bottom: y, action }
}

/// Middle-truncate an address so both ends stay visible.
///
/// Keeping the head and tail is what makes two addresses distinguishable at a
/// glance; truncating one end only would leave near-identical strings.
fn short(addr: &str) -> String {
    if addr.len() <= 24 {
        return addr.to_string();
    }
    format!("{}…{}", &addr[..14], &addr[addr.len() - 8..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_prefers_the_handle() {
        let row = WalletRow::new("stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9")
            .with_handle("alice");
        assert_eq!(row.display(), "$alice");
    }

    #[test]
    fn display_falls_back_to_a_truncated_address() {
        let row = WalletRow::new("stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9");
        let d = row.display();
        assert!(d.starts_with("stake1u8962x3"));
        assert!(d.ends_with("gc5rhq9"));
        assert!(d.contains('…'));
    }

    #[test]
    fn short_addresses_are_left_intact() {
        assert_eq!(short("stake1short"), "stake1short");
    }

    /// Both ends must survive truncation — a head-only cut leaves two different
    /// wallets looking identical.
    #[test]
    fn truncation_keeps_head_and_tail() {
        let a = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";
        let b = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgcAAAA";
        assert_ne!(short(a), short(b));
    }
}
