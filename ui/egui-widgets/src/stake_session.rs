//! `StakeSessionPanel` — connect a wallet, sign in to a worker by stake key, and stay signed in; the whole strip, with the session it holds.
//!
//! Every wallet-authed worker in the estate has the same front door: pick a
//! CIP-30 wallet, sign a server challenge with `signData`, receive an opaque
//! session token, carry it as `X-Session-Token`, forget it on sign-out or
//! expiry. The client portal hand-rolled that strip; the abandonware admin
//! needed it next. This widget is the strip, plus the two pure pieces every
//! host was re-writing beside it:
//!
//! - [`StoredStakeSession`] — what a session is, whether it has expired, and
//!   (on wasm) how it persists in `localStorage` under a host-chosen key.
//! - [`sign_challenge`] — the wallet half of the sign-in: reward address,
//!   network pre-check, `signData` over the challenge bytes. The HTTP halves
//!   (fetch the challenge, post the signature) stay with the host, since the
//!   host owns its URLs and wire types.
//!
//! The widget never spawns tasks. It draws the [`StakeSessionPhase`] the
//! host holds and hands back a [`StakeSessionAction`] to act on — the same
//! contract as [`WalletButton`](crate::wallet_button::WalletButton), which
//! it composes for the connect step.
//!
//! ```ignore
//! let action = StakeSessionPanel::new(&state.wallet, &state.session_phase, "abandonware")
//!     .network(Some("cardano:preprod"))
//!     .show(ui);
//! match action {
//!     StakeSessionAction::Connect(provider) => spawn(connect_wallet(provider)),
//!     StakeSessionAction::SignIn => spawn(async { challenge → sign_challenge → verify }),
//!     StakeSessionAction::SignOut => { StoredStakeSession::clear(KEY); phase = SignedOut }
//!     …
//! }
//! ```

use egui::RichText;
use serde::{Deserialize, Serialize};

use crate::error_note::ErrorNote;
use crate::icons::PhosphorIcon;
use crate::theme::ThemeExt;
use crate::user_badge::{UserBadge, UserBadgeAction};
use crate::wallet::{WalletApi, WalletConnector, WalletProvider};
use crate::wallet_button::{WalletAction, WalletButton};

// ============================================================================
// The session
// ============================================================================

/// A stake-key session with one worker, as the frontend keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredStakeSession {
    /// Opaque token; sent as `X-Session-Token`.
    pub token: String,
    /// Bech32 stake address the worker verified.
    pub stake_address: String,
    /// The worker's allowlist tier for that stake (`super_admin`, `whitelisted`).
    pub tier: String,
    pub expires_at_ms: i64,
    /// Which wallet signed in, so it can be reconnected on reload.
    pub wallet_name: String,
}

/// Safety margin before the server's expiry — a token presented in its last
/// minute would be rejected on arrival.
const EXPIRY_MARGIN_MS: i64 = 60_000;

impl StoredStakeSession {
    /// True once the session is past its TTL, with a one-minute margin.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at_ms - EXPIRY_MARGIN_MS < now_ms
    }

    /// Load a session persisted under `key`, dropping it if expired.
    pub fn load(key: &str, now_ms: i64) -> Option<Self> {
        let json = local_storage()?.get_item(key).ok()??;
        let session: Self = serde_json::from_str(&json).ok()?;
        if session.is_expired(now_ms) {
            Self::clear(key);
            return None;
        }
        Some(session)
    }

    /// Persist under `key`. Best effort: a browser without storage just
    /// forgets the session on reload.
    pub fn save(&self, key: &str) {
        let Some(storage) = local_storage() else {
            log::warn!("localStorage unavailable; stake session not persisted");
            return;
        };
        match serde_json::to_string(self) {
            Ok(json) => {
                if let Err(e) = storage.set_item(key, &json) {
                    log::warn!("save stake session: {e:?}");
                }
            }
            Err(e) => log::warn!("serialise stake session: {e}"),
        }
    }

    pub fn clear(key: &str) {
        if let Some(storage) = local_storage() {
            let _ = storage.remove_item(key);
        }
    }
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

// ============================================================================
// The wallet half of sign-in
// ============================================================================

/// What the wallet produced over the challenge: the two hex blobs a worker's
/// `POST /auth/verify` wants.
#[derive(Debug, Clone)]
pub struct SignedChallenge {
    pub cose_sig_hex: String,
    pub cose_key_hex: String,
}

/// Sign a worker's challenge with the connected wallet.
///
/// `expected_network` is the worker's `chain:network` string when the
/// challenge carried one. A wrong-network wallet is refused here with a
/// sentence, rather than being asked to sign and then bounced with an opaque
/// 401 because its mainnet stake matches no preprod allowlist.
pub async fn sign_challenge(
    api: &WalletApi,
    payload_to_sign_hex: &str,
    expected_network: Option<&str>,
) -> Result<SignedChallenge, String> {
    let reward_addrs = api
        .reward_addresses()
        .await
        .map_err(|e| format!("getRewardAddresses: {e}"))?;
    let stake_addr_hex = reward_addrs
        .first()
        .cloned()
        .ok_or("wallet returned no reward addresses (no stake key)")?;

    let expected = expected_network.filter(|n| !n.is_empty());
    if let (Some(expected), Ok(wallet_net)) = (expected, api.network_id().await) {
        let expects_mainnet = expected.contains("mainnet");
        if expects_mainnet != (wallet_net == 1) {
            let want = if expects_mainnet {
                "mainnet"
            } else {
                "preprod"
            };
            return Err(format!(
                "Wrong network — this worker runs on {want}; switch your wallet to a {want} \
                 account and reconnect."
            ));
        }
    }

    let signed = api
        .sign_data(&stake_addr_hex, payload_to_sign_hex)
        .await
        .map_err(|e| format!("signData: {e}"))?;
    Ok(SignedChallenge {
        cose_sig_hex: signed.signature,
        cose_key_hex: signed.key,
    })
}

// ============================================================================
// The panel
// ============================================================================

/// Where the sign-in is. The host holds this and moves it as async results
/// arrive; the session rides inside the variant that has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StakeSessionPhase {
    SignedOut,
    /// The challenge is being fetched or the wallet dialog is open.
    SigningIn,
    SignedIn(StoredStakeSession),
    Failed(String),
}

impl StakeSessionPhase {
    pub fn session(&self) -> Option<&StoredStakeSession> {
        match self {
            StakeSessionPhase::SignedIn(s) => Some(s),
            _ => None,
        }
    }

    pub fn token(&self) -> Option<&str> {
        self.session().map(|s| s.token.as_str())
    }
}

/// What the operator clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StakeSessionAction {
    None,
    /// Connect this wallet (from the embedded [`WalletButton`]).
    Connect(WalletProvider),
    /// Disconnect the wallet. A session for it no longer makes sense either.
    Disconnect,
    /// Start the challenge → `signData` → verify flow.
    SignIn,
    /// Forget the session (and tell the worker, if the host wants to).
    SignOut,
}

/// The strip. Composes the wallet button for the connect step and a user
/// badge for the signed-in state.
pub struct StakeSessionPanel<'a> {
    connector: &'a WalletConnector,
    phase: &'a StakeSessionPhase,
    /// What the operator is signing in to, e.g. "abandonware admin".
    realm_label: &'a str,
    network: Option<&'a str>,
    font_size: f32,
    /// The strip's width. It sizes itself rather than filling the parent, so
    /// it can sit in a header without claiming the whole row.
    max_width: f32,
    /// Salt for the badge popup's id, for hosts that draw more than one.
    id_salt: &'a str,
}

impl<'a> StakeSessionPanel<'a> {
    pub fn new(
        connector: &'a WalletConnector,
        phase: &'a StakeSessionPhase,
        realm_label: &'a str,
    ) -> Self {
        Self {
            connector,
            phase,
            realm_label,
            network: None,
            font_size: 11.0,
            max_width: 360.0,
            id_salt: "stake_session",
        }
    }

    pub fn id_salt(mut self, salt: &'a str) -> Self {
        self.id_salt = salt;
        self
    }

    /// The worker's network, shown beside the sign-in prompt so a wrong-network
    /// wallet is obvious before it signs.
    pub fn network(mut self, network: Option<&'a str>) -> Self {
        self.network = network;
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    /// Draw the strip. Lays itself out top-down at its own width whatever
    /// the parent's layout is: a host that places it in a right-to-left
    /// header row would otherwise see every inner row reversed and the
    /// error text wrapped a few characters wide, because egui's
    /// `horizontal` inherits the parent's direction.
    pub fn show(self, ui: &mut egui::Ui) -> StakeSessionAction {
        let max_width = self.max_width;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(max_width);
            self.draw_body(ui)
        })
        .inner
    }

    fn draw_body(self, ui: &mut egui::Ui) -> StakeSessionAction {
        crate::install_phosphor_font(ui.ctx());

        // Signed in: the badge is the whole strip. The wallet is implied.
        if let StakeSessionPhase::SignedIn(session) = self.phase {
            let subtitle = format!("{} · {}", session.tier, self.realm_label);
            let action = UserBadge::new(&session.wallet_name)
                .icon(PhosphorIcon::Wallet)
                .subtitle(&subtitle)
                .identifier("stake", &session.stake_address)
                .detail("Expires", expires_label(session.expires_at_ms))
                .id_salt(self.id_salt)
                .show(ui);
            return match action {
                UserBadgeAction::SignOut => StakeSessionAction::SignOut,
                _ => StakeSessionAction::None,
            };
        }

        // Not signed in: the connect step first.
        let mut action = match WalletButton::new().show(ui, self.connector) {
            WalletAction::None => StakeSessionAction::None,
            WalletAction::Connect(p) => StakeSessionAction::Connect(p),
            WalletAction::Disconnect => StakeSessionAction::Disconnect,
        };
        if !self.connector.is_connected() {
            return action;
        }

        ui.add_space(6.0);
        match self.phase {
            StakeSessionPhase::SignedOut => {
                ui.horizontal(|ui| {
                    let label = format!("Sign in to {}", self.realm_label);
                    if ui
                        .button(RichText::new(label).size(self.font_size))
                        .clicked()
                    {
                        action = StakeSessionAction::SignIn;
                    }
                    if let Some(network) = self.network {
                        ui.label(
                            RichText::new(network)
                                .color(ui.tokens().color.text_muted)
                                .size(self.font_size - 1.0),
                        );
                    }
                });
                ui.label(
                    RichText::new(
                        "Your wallet signs a challenge; nothing is spent. The worker checks \
                         the stake against its allowlist.",
                    )
                    .color(ui.tokens().color.text_muted)
                    .size(self.font_size - 1.0),
                );
            }
            StakeSessionPhase::SigningIn => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Check your wallet — sign the challenge there.")
                            .color(ui.tokens().color.accent_cyan)
                            .size(self.font_size),
                    );
                });
            }
            StakeSessionPhase::Failed(error) => {
                ErrorNote::new(error).show(ui);
                ui.add_space(4.0);
                if ui
                    .button(RichText::new("Try again").size(self.font_size))
                    .clicked()
                {
                    action = StakeSessionAction::SignIn;
                }
            }
            StakeSessionPhase::SignedIn(_) => unreachable!("handled above"),
        }
        action
    }
}

/// "in 7h 52m" / "expired" from a unix-ms deadline.
fn expires_label(expires_at_ms: i64) -> String {
    let now_ms = js_sys::Date::now() as i64;
    let left = expires_at_ms - now_ms;
    if left <= 0 {
        return "expired".to_string();
    }
    let mins = left / 60_000;
    if mins >= 60 {
        format!("in {}h {:02}m", mins / 60, mins % 60)
    } else {
        format!("in {mins}m")
    }
}
