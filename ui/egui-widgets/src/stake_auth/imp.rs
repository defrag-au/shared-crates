//! Browser half of [`super`] — the connector, the round trips, the trait impl.
//!
//! Split from the parent so the parent's decisions ([`super::classify`] above
//! all) stay testable without a browser. Nothing in here makes a decision that
//! is not delegated upward.

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::{StakeAuthConfig, WalletBinding};
use crate::auth::{AuthPhase, AuthStrategy, Session};
use crate::stake_session::{
    StakeSessionAction, StakeSessionPanel, StakeSessionPhase, StoredStakeSession, sign_challenge,
};
use crate::wallet::{self, WalletApi, WalletConnector, WalletProvider};

// ─── wire types ──────────────────────────────────────────────────────────────
//
// Mirrors of `shared_types::wallet_auth`, which lives in another repo and cannot
// be depended on from here. **The field names ARE the contract** — a rename
// here silently breaks sign-in on every consumer, which is why each one is
// spelled out rather than flattened into a `Value`.

#[derive(Deserialize)]
struct StakeChallenge {
    challenge_id: String,
    payload_to_sign_hex: String,
    /// `cardano:preprod` / `cardano:mainnet` — so the wallet can be refused
    /// before it signs, rather than after the worker rejects it.
    #[serde(default)]
    network: String,
}

#[derive(Serialize)]
struct StakeVerifyRequest {
    challenge_id: String,
    cose_sig_hex: String,
    cose_key_hex: String,
}

#[derive(Deserialize)]
struct StakeSessionResponse {
    token: String,
    stake_address: String,
    tier: String,
    expires_at_ms: i64,
}

/// Async results landing back on the strategy.
enum StakeMsg {
    /// A wallet finished connecting. Carries the handle, which is the whole
    /// point — a connect that does not yield an api is useless here.
    Connected {
        result: Box<crate::wallet::WalletConnectResult>,
        api: Box<WalletApi>,
    },
    ConnectFailed(String),
    SignedIn(Box<StoredStakeSession>),
    SignInFailed(String),
    /// A fresh read of the live wallet's stake address — drift detection.
    ActiveStake(Option<String>),
}

pub struct StakeAuthStrategy {
    config: StakeAuthConfig,
    phase: AuthPhase,
    /// The panel's view model. Kept alongside [`Self::phase`] rather than
    /// derived on the fly because `StakeSessionPanel` borrows it.
    session_phase: StakeSessionPhase,
    /// ⚠️ **Owned by the strategy, not the app.** The wallet IS the identity on
    /// this path, so a connector the app owned would put sign-in in the
    /// business of reaching into app state — and would make the binding check
    /// advisory rather than enforceable.
    connector: WalletConnector,
    /// Last known stake address of the *live* wallet. Distinct from
    /// `connector.stake_address`, which is whatever was true at connect time;
    /// this one is refreshed by [`Self::recheck_binding`].
    active_stake: Option<String>,
    binding: WalletBinding,
    /// A queue, not a slot: a connect and a stake re-read can land in the same
    /// frame and dropping either would leave the binding stale.
    inbox: Rc<RefCell<Vec<StakeMsg>>>,
}

impl StakeAuthStrategy {
    pub fn new(config: StakeAuthConfig) -> Self {
        Self {
            config,
            phase: super::initial_phase(),
            session_phase: StakeSessionPhase::SignedOut,
            connector: WalletConnector::new(),
            active_stake: None,
            binding: WalletBinding::Unauthenticated,
            inbox: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Whether the wallet in the browser is the one the session was issued to.
    pub fn binding(&self) -> &WalletBinding {
        &self.binding
    }

    /// The CIP-30 handle for signing — **`None` unless the binding is
    /// [`WalletBinding::Bound`]**.
    ///
    /// This is the enforcement, not a convenience. An app cannot sign with the
    /// wrong wallet by forgetting to check, because the check is the only way
    /// to obtain the handle. If you find yourself wanting a variant that
    /// bypasses it, you are reopening the hole described in the module header.
    pub fn wallet_api(&self) -> Option<WalletApi> {
        match self.binding.may_sign() {
            true => self.connector.api.clone(),
            false => None,
        }
    }

    /// Read-only view of the connector, for display (balances, icon, name).
    ///
    /// Deliberately not `&mut`: mutating the connection is the strategy's
    /// business, because the session is bound to it.
    pub fn connector(&self) -> &WalletConnector {
        &self.connector
    }

    /// Re-read the live wallet's stake address and recompute the binding.
    ///
    /// ⚠️ **Call this before building or submitting a transaction.** CIP-30 has
    /// no account-changed event, so a switch inside the extension is invisible
    /// until something asks. A timer cannot close the window between "user
    /// clicks sign" and "wallet signs"; this can.
    pub fn recheck_binding(&mut self) {
        let Some(api) = self.connector.api.clone() else {
            self.active_stake = None;
            self.recompute_binding();
            return;
        };
        let inbox = Rc::clone(&self.inbox);
        wasm_bindgen_futures::spawn_local(async move {
            // `reward_addresses` rather than re-deriving from the change
            // address: it is the stake key directly, and it is what
            // `sign_challenge` itself reads — so a mismatch here cannot be an
            // artefact of two different derivations disagreeing.
            let stake = api
                .reward_addresses()
                .await
                .ok()
                .and_then(|addrs| addrs.first().cloned())
                .and_then(|hex| {
                    wallet_pallas::Address::from_hex(&hex)
                        .ok()
                        .and_then(|a| a.stake_address_bech32())
                });
            inbox.borrow_mut().push(StakeMsg::ActiveStake(stake));
        });
    }

    /// The stake address the current session was issued to.
    fn signed_in_stake(&self) -> Option<&str> {
        self.session_phase
            .session()
            .map(|s| s.stake_address.as_str())
    }

    fn recompute_binding(&mut self) {
        // Prefer the freshly-read address; fall back to the connect-time one so
        // a binding exists before the first recheck completes.
        let active = self
            .active_stake
            .as_deref()
            .or(self.connector.stake_address.as_deref());
        self.binding = super::classify(self.signed_in_stake(), active);
    }

    /// Rebuild [`Self::phase`] from the session, then the binding from both.
    fn adopt(&mut self) {
        self.phase = match &self.session_phase {
            StakeSessionPhase::SignedOut => AuthPhase::Anonymous,
            StakeSessionPhase::SigningIn => AuthPhase::Pending,
            StakeSessionPhase::Failed(e) => AuthPhase::Failed(e.clone()),
            StakeSessionPhase::SignedIn(s) => AuthPhase::authenticated(Session {
                claims: super::claims_for(&s.stake_address, &s.tier),
                credentials: Some(("X-Session-Token".to_string(), s.token.clone())),
                // Milliseconds on the wire, seconds in `Session`.
                expires_at: Some((s.expires_at_ms / 1000).max(0) as u64),
            }),
        };
        self.recompute_binding();
    }

    fn connect(&mut self, provider: WalletProvider) {
        self.connector.set_connecting();
        let inbox = Rc::clone(&self.inbox);
        wasm_bindgen_futures::spawn_local(async move {
            let msg = match wallet::connect_wallet(provider).await {
                Ok((result, api)) => StakeMsg::Connected {
                    result: Box::new(result),
                    api: Box::new(api),
                },
                Err(e) => StakeMsg::ConnectFailed(e),
            };
            inbox.borrow_mut().push(msg);
        });
    }

    fn start_sign_in(&mut self) {
        let Some(api) = self.connector.api.clone() else {
            self.session_phase = StakeSessionPhase::Failed("Connect a wallet first.".to_string());
            self.adopt();
            return;
        };
        let wallet_name = self
            .connector
            .provider_name()
            .unwrap_or("wallet")
            .to_string();
        self.session_phase = StakeSessionPhase::SigningIn;
        self.adopt();

        let inbox = Rc::clone(&self.inbox);
        wasm_bindgen_futures::spawn_local(async move {
            let msg = match run_sign_in(&api, wallet_name).await {
                Ok(session) => StakeMsg::SignedIn(Box::new(session)),
                Err(e) => StakeMsg::SignInFailed(e),
            };
            inbox.borrow_mut().push(msg);
        });
    }
}

/// Challenge → `signData` → verify → a session worth storing.
///
/// Was duplicated verbatim in all three admin frontends, along with the two
/// `api.rs` calls it makes.
async fn run_sign_in(api: &WalletApi, wallet_name: String) -> Result<StoredStakeSession, String> {
    let challenge: StakeChallenge = post_json("/auth/challenge", &Empty {}).await?;
    let network = (!challenge.network.is_empty()).then_some(challenge.network.as_str());
    let signed = sign_challenge(api, &challenge.payload_to_sign_hex, network).await?;
    let session: StakeSessionResponse = post_json(
        "/auth/verify",
        &StakeVerifyRequest {
            challenge_id: challenge.challenge_id,
            cose_sig_hex: signed.cose_sig_hex,
            cose_key_hex: signed.cose_key_hex,
        },
    )
    .await?;
    Ok(StoredStakeSession {
        token: session.token,
        stake_address: session.stake_address,
        tier: session.tier,
        expires_at_ms: session.expires_at_ms,
        wallet_name,
    })
}

#[derive(Serialize)]
struct Empty {}

/// Same-origin POST. Relative paths are deliberate: every consumer serves these
/// two routes from its own worker, so there is no base URL to get wrong.
async fn post_json<B: Serialize, R: for<'de> Deserialize<'de>>(
    path: &str,
    body: &B,
) -> Result<R, String> {
    let payload = serde_json::to_vec(body).map_err(|e| format!("encode {path}: {e}"))?;
    let mut request = ehttp::Request::post(path, payload);
    request
        .headers
        .insert("Content-Type".to_string(), "application/json".to_string());

    let response = ehttp::fetch_async(request)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    if !response.ok {
        return Err(format!("{path}: HTTP {}", response.status));
    }
    serde_json::from_slice(&response.bytes).map_err(|e| format!("decode {path}: {e}"))
}

impl AuthStrategy for StakeAuthStrategy {
    fn restore(&mut self) {
        let now_ms = js_sys::Date::now() as i64;
        self.session_phase = match StoredStakeSession::load(&self.config.session_key, now_ms) {
            Some(s) => StakeSessionPhase::SignedIn(s),
            None => StakeSessionPhase::SignedOut,
        };
        self.adopt();

        // Reconnect the wallet the session was made with, silently. Without
        // this a restored session sits in `Disconnected` for ever and the user
        // sees a signed-in header above a dead Sign button.
        if let Some(provider) = WalletConnector::last_wallet() {
            self.connect(provider);
        }
    }

    fn tick(&mut self) {
        let drained: Vec<StakeMsg> = self.inbox.borrow_mut().drain(..).collect();
        if drained.is_empty() {
            return;
        }
        for msg in drained {
            match msg {
                StakeMsg::Connected { result, api } => {
                    // A fresh connection is the one moment the live address is
                    // known for certain, so adopt it rather than waiting for a
                    // recheck.
                    self.active_stake = result.stake_address.clone();
                    self.connector.apply_connect_result(*result);
                    self.connector.api = Some(*api);
                }
                StakeMsg::ConnectFailed(e) => self.connector.set_error(e),
                StakeMsg::SignedIn(session) => {
                    session.save(&self.config.session_key);
                    self.session_phase = StakeSessionPhase::SignedIn(*session);
                }
                StakeMsg::SignInFailed(e) => self.session_phase = StakeSessionPhase::Failed(e),
                StakeMsg::ActiveStake(stake) => self.active_stake = stake,
            }
        }
        self.adopt();
    }

    fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    fn gate_ui(&mut self, ui: &mut egui::Ui) {
        let action = StakeSessionPanel::new(
            &self.connector,
            &self.session_phase,
            &self.config.realm_label,
        )
        .show(ui);

        match action {
            StakeSessionAction::Connect(provider) => self.connect(provider),
            StakeSessionAction::Disconnect => {
                self.connector.disconnect();
                self.active_stake = None;
                self.adopt();
            }
            StakeSessionAction::SignIn => self.start_sign_in(),
            StakeSessionAction::SignOut => self.sign_out(),
            StakeSessionAction::None => {}
        }
    }

    fn sign_out(&mut self) {
        StoredStakeSession::clear(&self.config.session_key);
        self.session_phase = StakeSessionPhase::SignedOut;
        // The wallet stays connected: signing out of the worker is not a
        // reason to make the user re-approve the extension.
        self.adopt();
    }
}
