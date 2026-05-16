//! Wallet identity header — the big "this is who we're showing" strip at
//! the top of a wallet-profile view.
//!
//! Displays a handle (e.g. `$djo`) prominently when present, falling back
//! to a shortened stake address. The full stake address sits underneath
//! as smaller secondary text. A copy button on the right copies the full
//! stake address to the clipboard.
//!
//! Stateless. Returns the optional click action so the caller decides what
//! to do (e.g. show a "copied!" toast).
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::wallet_identity_header::{WalletIdentityHeader, WalletIdentityAction};
//!
//! let action = WalletIdentityHeader::new(stake_address)
//!     .handle(Some("$djo"))
//!     .show(ui);
//! if matches!(action, Some(WalletIdentityAction::CopyStake)) {
//!     ui.output_mut(|o| o.copied_text = stake_address.to_string());
//! }
//! ```

use egui::{Color32, RichText, Ui};

use crate::theme;
use crate::PhosphorIcon;

/// Click events the header can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletIdentityAction {
    /// The copy button was clicked. Caller should copy the stake address.
    CopyStake,
}

/// Layout + theming knobs.
pub struct WalletIdentityConfig {
    pub primary_size: f32,
    pub secondary_size: f32,
    pub primary_color: Color32,
    pub secondary_color: Color32,
    pub copy_icon_size: f32,
    pub copy_icon_color: Color32,
    /// How many characters of the stake address to keep at each end when
    /// truncating for display (used when no handle is present).
    pub stake_truncate_lead: usize,
    pub stake_truncate_tail: usize,
}

impl Default for WalletIdentityConfig {
    fn default() -> Self {
        Self {
            primary_size: 22.0,
            secondary_size: 11.0,
            primary_color: theme::TEXT_PRIMARY,
            secondary_color: theme::TEXT_SECONDARY,
            copy_icon_size: 14.0,
            copy_icon_color: theme::TEXT_SECONDARY,
            stake_truncate_lead: 12,
            stake_truncate_tail: 6,
        }
    }
}

/// The header widget. Caller-owned builder.
pub struct WalletIdentityHeader<'a> {
    stake_address: &'a str,
    handle: Option<&'a str>,
    config: WalletIdentityConfig,
    show_copy: bool,
}

impl<'a> WalletIdentityHeader<'a> {
    /// Create a new header for the given stake address.
    pub fn new(stake_address: &'a str) -> Self {
        Self {
            stake_address,
            handle: None,
            config: WalletIdentityConfig::default(),
            show_copy: true,
        }
    }

    /// Set the ADA Handle (e.g. `$djo`). When set, the handle is the primary
    /// label and the stake address moves to the secondary line.
    pub fn handle(mut self, handle: Option<&'a str>) -> Self {
        self.handle = handle;
        self
    }

    /// Hide the copy button.
    pub fn no_copy(mut self) -> Self {
        self.show_copy = false;
        self
    }

    /// Override the config.
    pub fn with_config(mut self, config: WalletIdentityConfig) -> Self {
        self.config = config;
        self
    }

    /// Render the header.
    pub fn show(self, ui: &mut Ui) -> Option<WalletIdentityAction> {
        let cfg = &self.config;
        let mut action: Option<WalletIdentityAction> = None;

        // Primary line: handle if present, otherwise short stake.
        let primary = match self.handle {
            Some(h) => h.to_string(),
            None => truncate_stake(
                self.stake_address,
                cfg.stake_truncate_lead,
                cfg.stake_truncate_tail,
            ),
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(primary)
                    .size(cfg.primary_size)
                    .strong()
                    .color(cfg.primary_color),
            );

            if self.show_copy {
                ui.add_space(6.0);
                let resp = PhosphorIcon::Copy.show(ui, cfg.copy_icon_size, cfg.copy_icon_color);
                if resp.clicked() {
                    action = Some(WalletIdentityAction::CopyStake);
                }
                resp.on_hover_text("Copy stake address");
            }
        });

        // Secondary line: stake address (full when handle present, otherwise omitted).
        if self.handle.is_some() {
            ui.label(
                RichText::new(self.stake_address)
                    .size(cfg.secondary_size)
                    .color(cfg.secondary_color)
                    .monospace(),
            );
        }

        action
    }
}

/// Public helper — short-form a bech32 stake address as `lead…tail`.
pub fn truncate_stake(stake: &str, lead: usize, tail: usize) -> String {
    if stake.len() <= lead + tail + 1 {
        return stake.to_string();
    }
    let end = stake.len().saturating_sub(tail);
    format!("{}…{}", &stake[..lead.min(stake.len())], &stake[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_stake_unchanged() {
        assert_eq!(truncate_stake("stake1abc", 12, 6), "stake1abc");
    }

    #[test]
    fn truncate_long_stake() {
        let stake = "stake1u9pnzqcrvnt6njaqkexglkqtcktxrdc4lt7mdtsxafjzdfsgm5ex2";
        let out = truncate_stake(stake, 12, 6);
        assert_eq!(out, "stake1u9pnz…gm5ex2");
    }
}
