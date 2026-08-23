//! Host-owned CIP-30 connect flow for macroquad apps.
//!
//! `macroquad-widgets` is deliberately stateless — it renders a
//! [`WalletConnectVm`] and reports an action, but owns no connection. This
//! crate is the other half: it holds the in-flight [`ReqId`]s from
//! `wallet-miniquad`'s request→poll bridge and advances them across frames,
//! so every macroquad surface connects a wallet identically.
//!
//! Keep it free of game logic and of I/O beyond the wallet bridge.

use macroquad::prelude::Vec2;
use macroquad_widgets::{
    wallet_connect, Painter, Theme, WalletAction, WalletConnectVm, WalletItem, WalletState,
};
use wallet_miniquad::{self as wallet, PollResult, ReqId};

/// CIP-30 connect flow, bridged across frames (request → poll).
///
/// On native builds the bridge is stubbed (no providers, poll errors),
/// so the panel just shows "no wallet detected" — the flow is web-only.
pub struct WalletFlow {
    vm: WalletConnectVm,
    /// In-flight enable() request + wallet display name.
    connecting: Option<(ReqId, String)>,
    /// In-flight getRewardAddresses() request + wallet display name.
    fetching_addr: Option<(ReqId, String)>,
    /// Connected stake address (bech32) — the player identity that will
    /// accompany score submissions to the score-api.
    stake_address: Option<String>,
}

impl Default for WalletFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletFlow {
    pub fn new() -> Self {
        Self {
            vm: WalletConnectVm {
                state: WalletState::Disconnected(Self::providers()),
            },
            connecting: None,
            fetching_addr: None,
            stake_address: None,
        }
    }

    fn providers() -> Vec<WalletItem> {
        wallet::list_providers()
            .into_iter()
            .map(|p| WalletItem::new(p.key, p.name))
            .collect()
    }

    /// Connected stake address (bech32), once the flow completes.
    pub fn stake_address(&self) -> Option<&str> {
        self.stake_address.as_deref()
    }

    /// Poll in-flight requests. Call once per frame.
    pub fn update(&mut self) {
        if let Some((id, name)) = &self.connecting {
            match wallet::poll(*id) {
                PollResult::Pending => {}
                PollResult::Ok { .. } => {
                    self.fetching_addr = Some((wallet::reward_address(), name.clone()));
                    self.connecting = None;
                }
                PollResult::Err { data } => {
                    self.vm.state = WalletState::Error(data);
                    self.connecting = None;
                }
            }
        }
        if let Some((id, name)) = &self.fetching_addr {
            match wallet::poll(*id) {
                PollResult::Pending => {}
                PollResult::Ok { data } => {
                    let address = wallet::hex_to_bech32(&data);
                    self.stake_address = Some(address.clone());
                    self.vm.state = WalletState::Connected {
                        name: name.clone(),
                        address,
                    };
                    self.fetching_addr = None;
                }
                PollResult::Err { data } => {
                    self.vm.state = WalletState::Error(data);
                    self.fetching_addr = None;
                }
            }
        }
    }

    /// Render the connect panel and dispatch any resulting action.
    ///
    /// `tap` comes from the host's `Gestures::update()` — see
    /// [`macroquad_widgets::Gestures`]. Passing it in rather than reading the
    /// input state here is what makes a drag off a button *not* activate it:
    /// a tap is resolved on release, within a small slop, and a longer travel
    /// is a swipe instead.
    pub fn draw(&mut self, x: f32, y: f32, w: f32, tap: Option<Vec2>) {
        let painter = Painter::new(None, None, Theme::tokyo_night(), tap);
        let resp = wallet_connect(&painter, &self.vm, x, y, w);
        match resp.action {
            Some(WalletAction::Connect(key)) => {
                let name = match &self.vm.state {
                    WalletState::Disconnected(items) => items
                        .iter()
                        .find(|i| i.key == key)
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| key.clone()),
                    _ => key.clone(),
                };
                self.connecting = Some((wallet::connect(&key), name));
                self.vm.state = WalletState::Connecting;
            }
            Some(WalletAction::Disconnect) => {
                wallet::disconnect();
                self.stake_address = None;
                self.vm.state = WalletState::Disconnected(Self::providers());
            }
            Some(WalletAction::Retry) => {
                self.vm.state = WalletState::Disconnected(Self::providers());
            }
            None => {}
        }
    }
}
