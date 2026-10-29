//! Host-owned CIP-30 connect flow for macroquad apps.
//!
//! `macroquad-widgets` is deliberately stateless — it renders a
//! [`WalletConnectVm`] and reports an action, but owns no connection. This
//! crate is the other half: it holds the in-flight [`ReqId`]s from
//! `wallet-miniquad`'s request→poll bridge and advances them across frames,
//! so every macroquad surface connects a wallet identically.
//!
//! ## What a connect establishes
//!
//! `enable()` → `getNetworkId()` → `getChangeAddress()` →
//! `getRewardAddresses()`. The change address is what a transaction builder
//! funds from; the network id is what lets a mainnet-only surface refuse a
//! testnet wallet before it builds; the reward address is optional (a wallet
//! may have none) and falls back to the stake part of the change address.
//!
//! A reader declining `enable()` is not an error: it returns to the picker and
//! leaves a [`WalletFlow::take_notice`] line, because a UI that paints a
//! refusal in red teaches people not to press the button.
//!
//! ## The wallet is remembered
//!
//! A connect that reaches [`Stage::Ready`] is written to the host's durable
//! store through [`Bridge::remember`], and the next page load connects it
//! before the first frame — silently, because CIP-30 keeps the permission
//! grant. One attempt per load: a refusal forgets the key and lands back on the
//! picker, an explicit [`WalletFlow::disconnect`] forgets it too, and a wallet
//! that is not installed is remembered but not connected.
//!
//! ## The account can change under the page
//!
//! CIP-30 has no account-changed event, so a wallet switched to another
//! account while the tab was hidden goes unnoticed. [`WalletFlow::recheck`] is
//! the quiet re-read, which the [`WalletConnector`] runs each time the page
//! comes back into view; [`WalletFlow::refresh_address`] is the stricter one a
//! builder runs before funding a transaction.
//!
//! ## Drawing
//!
//! [`WalletFlow::draw`] takes the host's `Painter`, so the panel uses the
//! host's fonts, theme, DPI scale and clock. A host that wants to place the
//! widget itself projects [`WalletFlow::vm`] and draws it with its own
//! `Painter` — the same shape `frontend-pump` uses.
//!
//! ## The modal
//!
//! [`WalletConnector`] is the one-button connector a game holds: every chain
//! it connects, behind `macroquad_gamekit`'s `WalletModal`. Each chain is a
//! [`ChainHost`] — [`CardanoHost`] puts this same flow behind it, with the
//! wallets' own icons; [`EvmHost`] does the same for EIP-6963 wallets. It
//! sits beside [`WalletFlow::draw`] rather than replacing it.
//!
//! Keep it free of game logic and of I/O beyond the wallet bridge. The
//! [`Bridge`] trait is the seam a soak drives with a fake wallet, so every
//! stage is testable without a browser.

pub mod cardano;
pub mod connector;
pub mod evm;
pub mod icons;

pub use cardano::{CardanoHost, cardano_account};
pub use connector::{ChainHost, WalletConnector, copy_text};
pub use evm::{
    AddChain, ChainTarget, EvmBridge, EvmHost, MiniquadEvmBridge, NativeCurrency, evm_account,
    robinhood_chain,
};
pub use macroquad_widgets::WalletState;
use macroquad_widgets::{Painter, WalletAction, WalletConnectVm, WalletItem, wallet_connect};
use wallet_miniquad::{self as wallet, FailureKind, PollResultDetailed, Provider, ReqId};

/// What the flow needs from a wallet bridge.
///
/// `wallet-miniquad` is the production implementation; tests drive a fake one
/// through every stage. Every request is fire-and-poll, never awaited.
pub trait Bridge {
    /// Whether a browser wallet can exist at all here. Native builds answer
    /// `false`, and the screen says so instead of offering a dead picker.
    fn available(&self) -> bool {
        wallet::AVAILABLE
    }
    fn list_providers(&self) -> Vec<Provider>;
    fn connect(&mut self, key: &str) -> ReqId;
    fn network_id(&mut self) -> ReqId;
    fn change_address(&mut self) -> ReqId;
    fn reward_address(&mut self) -> ReqId;
    fn poll(&mut self, id: ReqId) -> PollResultDetailed;
    fn cancel(&mut self, id: ReqId);
    fn disconnect(&mut self);

    /// The wallet key this host last connected, if it keeps one between runs.
    ///
    /// A host with no durable store answers `None` — which is the right
    /// answer for a native run, a test, and a page with `localStorage`
    /// disabled.
    fn remembered(&self) -> Option<String> {
        None
    }

    /// Keep `Some(key)`, or forget (`None`).
    ///
    /// Called only when the answer is known: a connect that reached
    /// [`Stage::Ready`], or an `enable()` that was refused. Never on a mere
    /// attempt — a remembered wallet should mean "this worked here".
    fn remember(&mut self, _key: Option<&str>) {}

    /// Ask for the icon wallet `key` announces, as `size`×`size` RGBA. `None`
    /// when this bridge has no icons to give — a native run, a test — which
    /// leaves the wallet on its monogram.
    fn icon(&mut self, _key: &str, _size: u16) -> Option<ReqId> {
        None
    }

    /// The pixels of an [`Bridge::icon`] request that polled `Ok`.
    fn take_icon(&mut self, _id: ReqId, _size: u16) -> Option<Vec<u8>> {
        None
    }

    /// Ask the connected wallet for its balance (CIP-30 `getBalance`, hex
    /// CBOR), which is where its ADA Handle is read from. `None` when this
    /// bridge has no balance to give, which leaves the account unnamed.
    fn balance(&mut self) -> Option<ReqId> {
        None
    }
}

/// The production bridge: the miniquad plugin over `window.cardano`.
#[derive(Default)]
pub struct MiniquadBridge;

/// Where the remembered provider key lives. Namespaced by crate so two
/// macroquad apps on one origin cannot overwrite each other's choice.
const PROVIDER_KEY: &str = "macroquad-wallet-flow.provider";

impl Bridge for MiniquadBridge {
    fn list_providers(&self) -> Vec<Provider> {
        wallet::list_providers()
    }
    fn connect(&mut self, key: &str) -> ReqId {
        wallet::connect(key)
    }
    fn network_id(&mut self) -> ReqId {
        wallet::network_id()
    }
    fn change_address(&mut self) -> ReqId {
        wallet::change_address()
    }
    fn reward_address(&mut self) -> ReqId {
        wallet::reward_address()
    }
    fn poll(&mut self, id: ReqId) -> PollResultDetailed {
        wallet::poll_detailed(id)
    }
    fn cancel(&mut self, id: ReqId) {
        wallet::cancel(id)
    }
    fn disconnect(&mut self) {
        wallet::disconnect()
    }
    fn remembered(&self) -> Option<String> {
        miniquad_platform::storage_get(PROVIDER_KEY)
    }
    fn remember(&mut self, key: Option<&str>) {
        match key {
            Some(key) => miniquad_platform::storage_set(PROVIDER_KEY, key),
            None => miniquad_platform::storage_remove(PROVIDER_KEY),
        }
    }
    fn icon(&mut self, key: &str, size: u16) -> Option<ReqId> {
        wallet::AVAILABLE.then(|| wallet::icon(key, size))
    }
    fn take_icon(&mut self, id: ReqId, size: u16) -> Option<Vec<u8>> {
        wallet::take_icon(id, size)
    }
    fn balance(&mut self) -> Option<ReqId> {
        wallet::AVAILABLE.then(wallet::balance)
    }
}

/// Where the connect flow is, as the host sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// No browser bridge: native, or a page without the wallet plugin.
    Unavailable,
    /// Nothing connected; the picker is showing.
    Disconnected,
    Connecting,
    FetchingNetwork,
    FetchingAddress,
    FetchingStake,
    /// Connected and ready to build. `address_hex` is the change address in
    /// the form CIP-30 and the route builder speak; `address_bech32` is what
    /// the UI shows.
    Ready {
        name: String,
        address_hex: String,
        address_bech32: String,
        stake_address: Option<String>,
        network_id: u8,
    },
    /// The wallet is on the wrong network. Disconnected before landing here.
    WrongNetwork {
        network_id: u8,
        expected: u8,
    },
    /// A wallet or bridge fault. A reader's refusal never lands here.
    Failed {
        message: String,
    },
}

/// A request that is in flight, with what the next step needs to carry.
enum InFlight {
    Connecting {
        req: ReqId,
        name: String,
    },
    Network {
        req: ReqId,
        name: String,
    },
    Address {
        req: ReqId,
        name: String,
        network_id: u8,
    },
    Stake {
        req: ReqId,
        name: String,
        address_hex: String,
        network_id: u8,
    },
    /// A change-address re-read before a build. The stage stays `Ready` while
    /// it is in flight; a different answer replaces the connection and leaves a
    /// notice, a failure blocks the build.
    RefreshingAddress {
        req: ReqId,
        name: String,
        address_hex: String,
        network_id: u8,
    },
    /// [`WalletFlow::recheck`]'s first read: the reward address, which is the
    /// account's identity. The stage stays `Ready` throughout, and a failure
    /// leaves it as it was.
    RecheckStake { req: ReqId, held: Held },
    /// The recheck's second read, made only when the identity looks different
    /// or the wallet gave no reward address: the change address. `stake` is
    /// the reward address the first read found, if any.
    RecheckAddress {
        req: ReqId,
        held: Held,
        stake: Option<String>,
    },
}

/// The connection a recheck compares against, as it was when it started.
struct Held {
    name: String,
    address_hex: String,
    stake_address: Option<String>,
    network_id: u8,
}

impl InFlight {
    fn req(&self) -> ReqId {
        match self {
            Self::Connecting { req, .. }
            | Self::Network { req, .. }
            | Self::Address { req, .. }
            | Self::Stake { req, .. }
            | Self::RefreshingAddress { req, .. }
            | Self::RecheckStake { req, .. }
            | Self::RecheckAddress { req, .. } => *req,
        }
    }

    fn is_recheck(&self) -> bool {
        matches!(self, Self::RecheckStake { .. } | Self::RecheckAddress { .. })
    }
}

/// CIP-30 connect, bridged across frames (request → poll).
pub struct WalletFlow<B: Bridge = MiniquadBridge> {
    bridge: B,
    providers: Vec<Provider>,
    pending: Option<InFlight>,
    stage: Stage,
    /// The widget's view of `stage`/`providers`. Kept in step by
    /// [`Self::set_stage`] so [`Self::state`] can hand out a reference — which
    /// is what keeps a host from having to clone the list every frame.
    vm: WalletConnectVm,
    /// The network this surface needs, if it has one. `None` accepts any.
    expected_network: Option<u8>,
    /// A short line about the last outcome — currently a cancelled connect.
    notice: Option<String>,
    /// The key of the connect attempt in flight, so a `Ready` knows what to
    /// remember and a failed `enable()` knows what to forget.
    attempt: Option<String>,
    /// Bumped whenever the stage is written, so a host can tell "nothing
    /// changed" without comparing the stage — which carries the addresses.
    revision: u64,
}

impl WalletFlow<MiniquadBridge> {
    pub fn new() -> Self {
        Self::with_bridge(MiniquadBridge)
    }
}

impl Default for WalletFlow<MiniquadBridge> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bridge> WalletFlow<B> {
    /// Build a flow over an arbitrary bridge. The production path is [`Self::new`];
    /// this is the seam a test or a soak uses.
    pub fn with_bridge(bridge: B) -> Self {
        let providers = bridge.list_providers();
        let stage = if bridge.available() {
            Stage::Disconnected
        } else {
            Stage::Unavailable
        };
        let vm = WalletConnectVm {
            state: state_for(&providers, &stage),
        };
        let mut flow = Self {
            bridge,
            providers,
            pending: None,
            stage,
            vm,
            expected_network: None,
            notice: None,
            attempt: None,
            revision: 0,
        };
        flow.auto_connect();
        flow
    }

    /// Changes whenever the stage is written. Equal across two reads means
    /// the stage has not moved between them.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Connect the wallet this host last used, if it is still installed.
    ///
    /// CIP-30's `enable()` only prompts when it has to — a browser that still
    /// holds the permission grant answers silently — so a returning reader is
    /// connected before the first frame. ONE attempt per page load, never a
    /// loop: if the grant is gone the reader is back at the picker, and the
    /// key is forgotten rather than retried every visit.
    fn auto_connect(&mut self) {
        if !self.bridge.available() {
            return;
        }
        let Some(key) = self.bridge.remembered() else {
            return;
        };
        if !self.providers.iter().any(|p| p.key == key) {
            // Installed last time, not now. Keep the memory: the extension may
            // be disabled for this session, or the reader may be on another
            // machine's profile in the same browser.
            return;
        }
        self.connect(&key);
    }

    /// Require a CIP-30 network id (1 = mainnet). A mismatch disconnects and
    /// lands in [`Stage::WrongNetwork`] rather than letting a build fail later.
    pub fn expect_network(mut self, network_id: u8) -> Self {
        self.expected_network = Some(network_id);
        self
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// The change address a builder funds from, once connected.
    pub fn address_hex(&self) -> Option<&str> {
        match &self.stage {
            Stage::Ready { address_hex, .. } => Some(address_hex),
            _ => None,
        }
    }

    /// The connected CIP-30 network id, once ready.
    pub fn network_id(&self) -> Option<u8> {
        match &self.stage {
            Stage::Ready { network_id, .. } => Some(*network_id),
            _ => None,
        }
    }

    /// The stake address (bech32), when the wallet has one.
    pub fn stake_address(&self) -> Option<&str> {
        match &self.stage {
            Stage::Ready { stake_address, .. } => stake_address.as_deref(),
            _ => None,
        }
    }

    /// Whether a pre-build change-address check is in flight. A host must not
    /// build while this is true: the address may be about to change.
    pub fn is_refreshing(&self) -> bool {
        matches!(self.pending, Some(InFlight::RefreshingAddress { .. }))
    }

    /// Re-read the change address before a build.
    ///
    /// A wallet can switch accounts without telling the page. The cached
    /// address would then fund a transaction from the wrong account, or fail
    /// in the wallet dialog. This re-read is the cheap check: the stage stays
    /// `Ready`, the answer is compared, and only a changed address replaces the
    /// connection. A failed read blocks the build instead of guessing.
    pub fn refresh_address(&mut self) {
        if self.is_refreshing() {
            return;
        }
        // A build's check outranks a recheck: it reads the same thing, and a
        // build is waiting on it.
        if let Some(pending) = self.pending.take_if(|p| p.is_recheck()) {
            self.bridge.cancel(pending.req());
        }
        let Stage::Ready {
            name,
            address_hex,
            network_id,
            ..
        } = &self.stage
        else {
            return;
        };
        let (name, address_hex, network_id) = (name.clone(), address_hex.clone(), *network_id);
        let req = self.bridge.change_address();
        self.pending = Some(InFlight::RefreshingAddress {
            req,
            name,
            address_hex,
            network_id,
        });
    }

    /// Ask the connected wallet which account it is on now, quietly.
    ///
    /// For the page coming back into view: a CIP-30 wallet can switch account
    /// while the page is hidden and never say so. The reward address is read
    /// first, because it is what a sign-in proves; only when it looks
    /// different (or the wallet has none) is the change address read too.
    /// The stage stays `Ready` throughout. A different account replaces the
    /// connection and leaves the "Wallet account changed" notice, as
    /// [`Self::refresh_address`] does; a failed read changes nothing, because
    /// a wallet that is merely locked is not a reason to drop the page's
    /// connection. A no-op unless the flow is `Ready` and idle.
    pub fn recheck(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let Stage::Ready {
            name,
            address_hex,
            stake_address,
            network_id,
            ..
        } = &self.stage
        else {
            return;
        };
        let held = Held {
            name: name.clone(),
            address_hex: address_hex.clone(),
            stake_address: stake_address.clone(),
            network_id: *network_id,
        };
        let req = self.bridge.reward_address();
        self.pending = Some(InFlight::RecheckStake { req, held });
    }

    /// Whether a [`Self::recheck`] is in flight.
    pub fn is_rechecking(&self) -> bool {
        self.pending.as_ref().is_some_and(InFlight::is_recheck)
    }

    /// A short line about the last outcome, e.g. a cancelled connect. Taken
    /// once so a host can show it and forget it.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Why a transaction cannot be built right now, in the words a user should
    /// see. Handed to a cart as its `blocker` rather than checked at the call
    /// site, so every surface words a missing wallet the same way.
    pub fn blocker(&self) -> Option<String> {
        if self.is_refreshing() {
            return Some("Checking the wallet…".to_string());
        }
        match &self.stage {
            Stage::Ready { .. } => None,
            Stage::Unavailable => Some("A wallet needs a browser".to_string()),
            Stage::Disconnected => Some("Connect a wallet first".to_string()),
            Stage::Connecting
            | Stage::FetchingNetwork
            | Stage::FetchingAddress
            | Stage::FetchingStake => Some("Connecting a wallet…".to_string()),
            Stage::WrongNetwork {
                network_id,
                expected,
            } => Some(network_message(*network_id, *expected)),
            Stage::Failed { message } => Some(message.clone()),
        }
    }

    /// What the widget draws.
    pub fn vm(&self) -> &WalletConnectVm {
        &self.vm
    }

    /// The connect state as the stateless widget's vocabulary.
    pub fn state(&self) -> &WalletState {
        &self.vm.state
    }

    /// Start connecting the wallet with this CIP-30 key (`window.cardano.<key>`).
    pub fn connect(&mut self, key: &str) {
        if let Some(pending) = self.pending.take() {
            self.bridge.cancel(pending.req());
        }
        let name = self
            .providers
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| key.to_string());
        let req = self.bridge.connect(key);
        self.pending = Some(InFlight::Connecting { req, name });
        self.attempt = Some(key.to_string());
        self.set_stage(Stage::Connecting);
        self.notice = None;
    }

    /// Forget the connection and list the wallets again.
    ///
    /// Also forgets the remembered key: a reader who disconnects is saying
    /// "do not bring me back here", and silently reconnecting them next visit
    /// is exactly the behaviour that makes a wallet page feel haunted.
    pub fn disconnect(&mut self) {
        self.bridge.disconnect();
        self.attempt = None;
        self.bridge.remember(None);
        self.reset();
    }

    /// After an error or a refusal: list the wallets again.
    pub fn retry(&mut self) {
        self.reset();
    }

    /// Advance in-flight requests. Call once per frame.
    pub fn poll(&mut self) {
        let Some(req) = self.pending.as_ref().map(InFlight::req) else {
            return;
        };
        match self.bridge.poll(req) {
            PollResultDetailed::Pending => {}
            PollResultDetailed::Ok { data } => self.advance(data),
            PollResultDetailed::Err { data, kind } => self.fail(data, kind),
        }
    }

    /// Alias kept for the existing `fight-macroquad` host.
    pub fn update(&mut self) {
        self.poll();
    }

    /// Draw the connect panel through the host's painter and report the action
    /// it produced. Applying it is [`Self::apply`]'s job, so drawing stays
    /// side-effect free — which is what makes a `--shot` reproducible.
    pub fn draw(&self, p: &Painter, x: f32, y: f32, w: f32) -> Option<WalletAction> {
        wallet_connect(p, &self.vm, x, y, w).action
    }

    /// Act on what [`Self::draw`] reported.
    pub fn apply(&mut self, action: WalletAction) {
        match action {
            WalletAction::Connect(key) => self.connect(&key),
            WalletAction::Disconnect => self.disconnect(),
            WalletAction::Retry => self.retry(),
        }
    }

    fn reset(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.bridge.cancel(pending.req());
        }
        self.attempt = None;
        self.providers = self.bridge.list_providers();
        let stage = if self.bridge.available() {
            Stage::Disconnected
        } else {
            Stage::Unavailable
        };
        self.set_stage(stage);
        self.notice = None;
    }

    /// Set the stage and the widget view together — the one place they can
    /// drift, so they are written in one move.
    fn set_stage(&mut self, stage: Stage) {
        self.stage = stage;
        self.revision += 1;
        self.vm = WalletConnectVm {
            state: state_for(&self.providers, &self.stage),
        };
    }

    /// A resolved request, one step further along.
    fn advance(&mut self, data: String) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending {
            InFlight::Connecting { name, .. } => {
                let req = self.bridge.network_id();
                self.pending = Some(InFlight::Network { req, name });
                self.set_stage(Stage::FetchingNetwork);
            }
            InFlight::Network { name, .. } => {
                let Ok(network_id) = data.trim().parse::<u8>() else {
                    // The attempt ended without a usable wallet: nothing to
                    // remember, whether or not this was an auto-connect.
                    self.attempt = None;
                    self.bridge.disconnect();
                    self.providers = self.bridge.list_providers();
                    self.set_stage(Stage::Failed {
                        message: format!("wallet returned network id `{}`", short(&data)),
                    });
                    return;
                };
                if let Some(expected) = self.expected_network
                    && network_id != expected
                {
                    self.attempt = None;
                    self.bridge.disconnect();
                    self.providers = self.bridge.list_providers();
                    self.set_stage(Stage::WrongNetwork {
                        network_id,
                        expected,
                    });
                    return;
                }
                let req = self.bridge.change_address();
                self.pending = Some(InFlight::Address {
                    req,
                    name,
                    network_id,
                });
                self.set_stage(Stage::FetchingAddress);
            }
            InFlight::Address {
                name, network_id, ..
            } => {
                let req = self.bridge.reward_address();
                self.pending = Some(InFlight::Stake {
                    req,
                    name,
                    address_hex: data,
                    network_id,
                });
                self.set_stage(Stage::FetchingStake);
            }
            InFlight::Stake {
                name,
                address_hex,
                network_id,
                ..
            } => {
                // A reward address is optional. An empty answer or a wallet
                // that cannot produce one falls back to the change address's
                // own stake part, which is where `wallet-pallas` reads it too.
                let stake_address =
                    reward_bech32(&data).or_else(|| wallet::stake_address_bech32(&address_hex));
                self.ready(name, address_hex, stake_address, network_id);
            }
            InFlight::RefreshingAddress {
                name,
                address_hex,
                network_id,
                ..
            } => {
                if data == address_hex {
                    self.pending = None;
                    return;
                }
                let stake_address = wallet::stake_address_bech32(&data);
                self.ready(name, data, stake_address, network_id);
                self.notice = Some("Wallet account changed".to_string());
            }
            InFlight::RecheckStake { held, .. } => {
                let stake = reward_bech32(&data);
                if stake.is_some() && stake == held.stake_address {
                    return;
                }
                let req = self.bridge.change_address();
                self.pending = Some(InFlight::RecheckAddress { req, held, stake });
            }
            InFlight::RecheckAddress { held, stake, .. } => {
                let stake = stake.or_else(|| wallet::stake_address_bech32(&data));
                if data == held.address_hex && stake == held.stake_address {
                    return;
                }
                self.ready(held.name, data, stake, held.network_id);
                self.notice = Some("Wallet account changed".to_string());
            }
        }
    }

    /// A rejected request. A refusal at `enable()` returns to the picker; a
    /// refused reward address is not fatal; everything else is a fault.
    fn fail(&mut self, data: String, kind: FailureKind) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        // The reward address is the one optional step: a wallet with no stake
        // credential (or one that refuses the read) still has a usable change
        // address.
        match pending {
            InFlight::Stake {
                name,
                address_hex,
                network_id,
                ..
            } => {
                let stake_address = wallet::stake_address_bech32(&address_hex);
                self.ready(name, address_hex, stake_address, network_id);
            }
            // A recheck is a question, not a step: a locked wallet or a
            // refused read leaves the connection as it was.
            InFlight::RecheckStake { .. } | InFlight::RecheckAddress { .. } => {}
            InFlight::RefreshingAddress { .. } => {
                self.bridge.disconnect();
                self.providers = self.bridge.list_providers();
                self.set_stage(Stage::Failed {
                    message: short(&data),
                });
            }
            other => {
                self.bridge.disconnect();
                self.providers = self.bridge.list_providers();
                if matches!(other, InFlight::Connecting { .. }) {
                    // The grant is gone, or the wallet would not open. Forget
                    // it: auto-connecting again next visit would fail again
                    // without saying so. The picker is the one place that can
                    // ask.
                    self.attempt = None;
                    self.bridge.remember(None);
                    if kind == FailureKind::Declined {
                        self.set_stage(Stage::Disconnected);
                        self.notice = Some("Connection cancelled".to_string());
                        return;
                    }
                }
                self.set_stage(Stage::Failed {
                    message: short(&data),
                });
            }
        }
    }

    fn ready(
        &mut self,
        name: String,
        address_hex: String,
        stake_address: Option<String>,
        network_id: u8,
    ) {
        // The one place a wallet becomes "the wallet this browser uses": the
        // connect completed, so the next page load may bring it back.
        if let Some(key) = self.attempt.take() {
            self.bridge.remember(Some(&key));
        }
        let address_bech32 = wallet::hex_to_bech32(&address_hex);
        self.set_stage(Stage::Ready {
            name,
            address_hex,
            address_bech32,
            stake_address,
            network_id,
        });
        self.pending = None;
    }
}

fn state_for(providers: &[Provider], stage: &Stage) -> WalletState {
    match stage {
        Stage::Unavailable | Stage::Disconnected => WalletState::Disconnected(
            providers
                .iter()
                .map(|p| WalletItem::new(p.key.clone(), p.name.clone()))
                .collect(),
        ),
        Stage::Connecting
        | Stage::FetchingNetwork
        | Stage::FetchingAddress
        | Stage::FetchingStake => WalletState::Connecting,
        Stage::Ready {
            name,
            address_bech32,
            ..
        } => WalletState::Connected {
            name: name.clone(),
            address: address_bech32.clone(),
        },
        Stage::WrongNetwork {
            network_id,
            expected,
        } => WalletState::Error(network_message(*network_id, *expected)),
        Stage::Failed { message } => WalletState::Error(message.clone()),
    }
}

/// A `getRewardAddresses` answer as bech32, or `None` for a wallet that gave
/// none.
fn reward_bech32(data: &str) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    wallet::stake_address_bech32(data).or_else(|| Some(wallet::hex_to_bech32(data)))
}

fn network_message(network_id: u8, expected: u8) -> String {
    format!("Wrong network: the wallet reports {network_id}, this app needs {expected}")
}

/// One line, short enough for the widget, so a wallet's stack trace does not
/// become the screen.
fn short(s: &str) -> String {
    let line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 120 {
        line
    } else {
        let mut out: String = line.chars().take(117).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A mainnet base address: header 0x01, a key payment hash, a key stake
    /// hash. Constructed rather than pasted so the headers are visible.
    fn base_address() -> String {
        format!("01{}{}", "11".repeat(28), "22".repeat(28))
    }

    /// A bare mainnet reward address (header 0xe1).
    fn reward_address() -> String {
        format!("e1{}", "33".repeat(28))
    }

    struct FakeBridge {
        providers: Vec<Provider>,
        results: VecDeque<PollResultDetailed>,
        calls: Vec<&'static str>,
        disconnected: bool,
        available: bool,
        /// What the host had stored when the flow was built.
        remembered: Option<String>,
        /// Every `remember` the flow asked for, in order.
        stored: Vec<Option<String>>,
    }

    impl FakeBridge {
        fn new(results: Vec<PollResultDetailed>) -> Self {
            Self {
                providers: vec![Provider {
                    key: "eternl".to_string(),
                    name: "Eternl".to_string(),
                    version: "2".to_string(),
                }],
                results: results.into(),
                calls: Vec::new(),
                disconnected: false,
                available: true,
                remembered: None,
                stored: Vec::new(),
            }
        }

        /// The same bridge, as if this browser had connected `key` before.
        fn remembering(mut self, key: &str) -> Self {
            self.remembered = Some(key.to_string());
            self
        }
    }

    impl Bridge for FakeBridge {
        fn available(&self) -> bool {
            self.available
        }
        fn list_providers(&self) -> Vec<Provider> {
            self.providers.clone()
        }
        fn connect(&mut self, _key: &str) -> ReqId {
            self.calls.push("connect");
            ReqId(1)
        }
        fn network_id(&mut self) -> ReqId {
            self.calls.push("network");
            ReqId(2)
        }
        fn change_address(&mut self) -> ReqId {
            self.calls.push("change");
            ReqId(3)
        }
        fn reward_address(&mut self) -> ReqId {
            self.calls.push("reward");
            ReqId(4)
        }
        fn poll(&mut self, _id: ReqId) -> PollResultDetailed {
            self.results
                .pop_front()
                .unwrap_or(PollResultDetailed::Pending)
        }
        fn cancel(&mut self, _id: ReqId) {
            self.calls.push("cancel");
        }
        fn disconnect(&mut self) {
            self.disconnected = true;
        }
        fn remembered(&self) -> Option<String> {
            self.remembered.clone()
        }
        fn remember(&mut self, key: Option<&str>) {
            self.stored.push(key.map(str::to_string));
        }
    }

    fn ok(data: &str) -> PollResultDetailed {
        PollResultDetailed::Ok {
            data: data.to_string(),
        }
    }

    fn err(data: &str, kind: FailureKind) -> PollResultDetailed {
        PollResultDetailed::Err {
            data: data.to_string(),
            kind,
        }
    }

    fn flow(results: Vec<PollResultDetailed>) -> WalletFlow<FakeBridge> {
        WalletFlow::with_bridge(FakeBridge::new(results)).expect_network(1)
    }

    fn ready_flow() -> WalletFlow<FakeBridge> {
        let mut flow = flow(vec![
            ok("eternl"),
            ok("1"),
            ok(&base_address()),
            ok(&reward_address()),
        ]);
        flow.connect("eternl");
        for _ in 0..4 {
            flow.poll();
        }
        assert!(matches!(flow.stage(), Stage::Ready { .. }));
        flow
    }

    #[test]
    fn a_changed_account_is_noticed_and_replaced() {
        let mut flow = ready_flow();
        let changed = format!("01{}{}", "44".repeat(28), "55".repeat(28));
        flow.bridge.results.push_back(ok(&changed));

        flow.refresh_address();
        assert!(flow.is_refreshing());
        assert_eq!(
            flow.blocker().as_deref(),
            Some("Checking the wallet…"),
            "a build must not start during the check"
        );
        flow.poll();

        assert!(!flow.is_refreshing());
        let Stage::Ready { address_hex, .. } = flow.stage() else {
            panic!("expected Ready, got {:?}", flow.stage());
        };
        assert_eq!(address_hex, &changed);
        assert_eq!(
            flow.take_notice().as_deref(),
            Some("Wallet account changed")
        );
    }

    #[test]
    fn an_unchanged_account_keeps_ready_without_a_notice() {
        let mut flow = ready_flow();
        flow.bridge.results.push_back(ok(&base_address()));

        flow.refresh_address();
        flow.poll();

        assert!(matches!(flow.stage(), Stage::Ready { .. }));
        assert!(flow.notice().is_none());
        let expected = base_address();
        assert_eq!(flow.address_hex(), Some(expected.as_str()));
    }

    #[test]
    fn a_failed_refresh_blocks_the_build() {
        let mut flow = ready_flow();
        flow.bridge
            .results
            .push_back(err("changed in another tab", FailureKind::Failed));

        flow.refresh_address();
        flow.poll();

        assert!(matches!(flow.stage(), Stage::Failed { .. }));
        assert!(flow.blocker().is_some());
        assert!(flow.address_hex().is_none());
    }

    /// The common case on every return to the tab: one cheap read, nothing
    /// written, no notice.
    #[test]
    fn a_recheck_of_the_same_account_reads_only_the_stake() {
        let mut flow = ready_flow();
        let revision = flow.revision();
        flow.bridge.calls.clear();
        flow.bridge.results.push_back(ok(&reward_address()));

        flow.recheck();
        assert!(flow.is_rechecking());
        assert!(flow.blocker().is_none(), "a recheck never blocks a build");
        flow.poll();

        assert!(!flow.is_rechecking());
        assert_eq!(flow.bridge.calls, ["reward"]);
        assert_eq!(flow.revision(), revision, "nothing was rewritten");
        assert!(flow.notice().is_none());
    }

    /// Switched account in the wallet while the tab was hidden: the new
    /// account replaces the old one, stake and address both.
    #[test]
    fn a_recheck_finds_a_switched_account() {
        let mut flow = ready_flow();
        let was = flow.stake_address().map(str::to_string);
        let address = format!("01{}{}", "44".repeat(28), "55".repeat(28));
        let reward = format!("e1{}", "55".repeat(28));
        flow.bridge.results.push_back(ok(&reward));
        flow.bridge.results.push_back(ok(&address));

        flow.recheck();
        flow.poll();
        assert!(flow.is_rechecking(), "the address is read too");
        flow.poll();

        assert_eq!(flow.address_hex(), Some(address.as_str()));
        let now = flow.stake_address().expect("a stake address");
        assert!(now.starts_with("stake1"), "{now}");
        assert_ne!(Some(now.to_string()), was);
        assert_eq!(
            flow.take_notice().as_deref(),
            Some("Wallet account changed")
        );
    }

    /// A locked wallet refuses the read. That is not a disconnect.
    #[test]
    fn a_failed_recheck_keeps_the_connection() {
        let mut flow = ready_flow();
        flow.bridge
            .results
            .push_back(err("wallet locked", FailureKind::Failed));

        flow.recheck();
        flow.poll();

        assert!(matches!(flow.stage(), Stage::Ready { .. }));
        assert!(!flow.bridge.disconnected);
        assert!(flow.notice().is_none());
    }

    /// A wallet with no reward address: the change address decides.
    #[test]
    fn a_recheck_without_a_reward_address_compares_the_address() {
        let mut flow = flow(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        flow.connect("eternl");
        for _ in 0..4 {
            flow.poll();
        }
        let revision = flow.revision();
        flow.bridge.calls.clear();
        flow.bridge.results.push_back(ok(""));
        flow.bridge.results.push_back(ok(&base_address()));

        flow.recheck();
        flow.poll();
        flow.poll();

        assert_eq!(flow.bridge.calls, ["reward", "change"]);
        assert_eq!(flow.revision(), revision, "the same account");
        assert!(flow.notice().is_none());
    }

    /// A build's check takes over from a recheck rather than queueing behind
    /// it.
    #[test]
    fn a_refresh_replaces_a_recheck() {
        let mut flow = ready_flow();
        flow.recheck();
        flow.refresh_address();
        assert!(flow.is_refreshing());
        assert!(!flow.is_rechecking());
        assert!(flow.bridge.calls.contains(&"cancel"));
    }

    #[test]
    fn a_recheck_before_a_wallet_is_ready_is_a_no_op() {
        let mut flow = flow(Vec::new());
        flow.recheck();
        assert!(!flow.is_rechecking());
        assert!(flow.bridge.calls.is_empty());
    }

    #[test]
    fn a_refresh_before_a_wallet_is_ready_is_a_no_op() {
        let mut flow = flow(Vec::new());
        flow.refresh_address();
        assert!(!flow.is_refreshing());
    }

    #[test]
    fn a_connect_reaches_ready_with_network_and_address() {
        let mut flow = flow(vec![
            ok("eternl"),
            ok("1"),
            ok(&base_address()),
            ok(&reward_address()),
        ]);
        flow.connect("eternl");
        for _ in 0..4 {
            flow.poll();
        }

        let Stage::Ready {
            network_id,
            address_hex,
            address_bech32,
            stake_address,
            ..
        } = flow.stage()
        else {
            panic!("expected Ready, got {:?}", flow.stage());
        };
        assert_eq!(*network_id, 1);
        assert_eq!(address_hex, &base_address());
        assert!(address_bech32.starts_with("addr1"), "{address_bech32}");
        let stake = stake_address.as_deref().expect("the reward address");
        assert!(stake.starts_with("stake1"), "{stake}");
        assert!(flow.blocker().is_none());
        assert!(matches!(
            flow.state(),
            WalletState::Connected { name, .. } if name.as_str() == "Eternl"
        ));
    }

    #[test]
    fn a_declined_connect_returns_to_the_picker_with_a_notice() {
        let mut flow = flow(vec![err("User declined", FailureKind::Declined)]);
        flow.connect("eternl");
        flow.poll();

        assert!(matches!(flow.stage(), Stage::Disconnected));
        assert_eq!(flow.take_notice().as_deref(), Some("Connection cancelled"));
        assert!(flow.take_notice().is_none(), "a notice is shown once");
        assert!(matches!(flow.state(), WalletState::Disconnected(items) if items.len() == 1));
    }

    #[test]
    fn a_wrong_network_is_refused_before_a_build() {
        let mut flow = flow(vec![ok("eternl"), ok("0")]);
        flow.connect("eternl");
        flow.poll();
        flow.poll();

        assert!(matches!(
            flow.stage(),
            Stage::WrongNetwork {
                network_id: 0,
                expected: 1
            }
        ));
        assert!(flow.blocker().expect("a blocker").contains("Wrong network"));
        assert!(flow.bridge.disconnected, "the wrong wallet is let go");
        assert!(flow.address_hex().is_none());
    }

    /// A wallet that cannot produce a reward address still has a usable
    /// change address: the stake part comes from that instead.
    #[test]
    fn a_refused_reward_address_still_connects() {
        let mut flow = flow(vec![
            ok("eternl"),
            ok("1"),
            ok(&base_address()),
            err("no reward address", FailureKind::Failed),
        ]);
        flow.connect("eternl");
        for _ in 0..4 {
            flow.poll();
        }

        let Stage::Ready { stake_address, .. } = flow.stage() else {
            panic!("expected Ready, got {:?}", flow.stage());
        };
        assert!(
            stake_address
                .as_deref()
                .is_some_and(|s| s.starts_with("stake1")),
            "{stake_address:?}"
        );
    }

    #[test]
    fn a_pending_request_does_not_advance() {
        let mut flow = flow(vec![PollResultDetailed::Pending]);
        flow.connect("eternl");
        assert!(matches!(flow.stage(), Stage::Connecting));
        flow.poll();
        assert!(matches!(flow.stage(), Stage::Connecting));
    }

    #[test]
    fn a_retry_clears_the_failure_and_re_lists() {
        let mut flow = flow(vec![err("boom", FailureKind::Failed)]);
        flow.connect("eternl");
        flow.poll();
        assert!(matches!(flow.stage(), Stage::Failed { .. }));

        flow.retry();
        assert!(matches!(flow.stage(), Stage::Disconnected));
        assert!(flow.notice().is_none());
        assert!(flow.blocker().is_some());
    }

    #[test]
    fn every_unready_stage_names_a_blocker() {
        let mut flow = flow(Vec::new());
        assert_eq!(flow.blocker().as_deref(), Some("Connect a wallet first"));
        flow.connect("eternl");
        assert_eq!(flow.blocker().as_deref(), Some("Connecting a wallet…"));
    }

    #[test]
    fn a_native_bridge_is_unavailable_not_an_empty_picker() {
        let mut bridge = FakeBridge::new(Vec::new());
        bridge.available = false;
        let flow = WalletFlow::with_bridge(bridge);
        assert!(matches!(flow.stage(), Stage::Unavailable));
        assert_eq!(flow.blocker().as_deref(), Some("A wallet needs a browser"));
    }

    /// A reader who connected once should not have to pick again. The connect
    /// starts in `with_bridge`, before the first frame, and the key is written
    /// back once it is READY — not on the attempt.
    #[test]
    fn a_remembered_wallet_connects_on_its_own() {
        let mut flow = WalletFlow::with_bridge(
            FakeBridge::new(vec![
                ok("eternl"),
                ok("1"),
                ok(&base_address()),
                ok(&reward_address()),
            ])
            .remembering("eternl"),
        )
        .expect_network(1);

        assert!(
            matches!(flow.stage(), Stage::Connecting),
            "the connect starts before the first frame: {:?}",
            flow.stage()
        );
        for _ in 0..4 {
            flow.poll();
        }
        assert!(matches!(flow.stage(), Stage::Ready { .. }));
        assert_eq!(flow.bridge.stored, vec![Some("eternl".to_string())]);
    }

    /// Installed last time, not now: nothing to connect, and the memory is
    /// kept — the extension may only be disabled for this session.
    #[test]
    fn a_remembered_wallet_that_is_not_installed_is_left_alone() {
        let flow = WalletFlow::with_bridge(FakeBridge::new(Vec::new()).remembering("nami"))
            .expect_network(1);

        assert!(matches!(flow.stage(), Stage::Disconnected));
        assert!(flow.bridge.calls.is_empty());
        assert!(flow.bridge.stored.is_empty(), "nothing to rewrite");
    }

    /// The grant is gone, or the wallet would not open. One attempt, then back
    /// to the picker with the key forgotten — never a retry loop.
    #[test]
    fn a_refused_auto_connect_forgets_the_wallet() {
        let mut flow = WalletFlow::with_bridge(
            FakeBridge::new(vec![err("User declined", FailureKind::Declined)])
                .remembering("eternl"),
        )
        .expect_network(1);

        flow.poll();
        assert!(matches!(flow.stage(), Stage::Disconnected));
        assert_eq!(flow.take_notice().as_deref(), Some("Connection cancelled"));
        assert_eq!(flow.bridge.stored, vec![None]);
    }

    /// A connect that never reached READY is not remembered — "this worked
    /// here" is the only thing worth bringing back.
    #[test]
    fn a_wrong_network_wallet_is_not_remembered() {
        let mut flow = flow(vec![ok("eternl"), ok("0")]);
        flow.connect("eternl");
        for _ in 0..2 {
            flow.poll();
        }
        assert!(matches!(flow.stage(), Stage::WrongNetwork { .. }));
        assert!(flow.bridge.stored.is_empty());
    }

    /// Disconnecting means "do not bring me back here".
    #[test]
    fn disconnecting_forgets_the_wallet() {
        let mut flow = ready_flow();
        assert_eq!(flow.bridge.stored, vec![Some("eternl".to_string())]);

        flow.disconnect();
        assert_eq!(
            flow.bridge.stored.last(),
            Some(&None),
            "the remembered key is cleared"
        );
        assert!(matches!(flow.stage(), Stage::Disconnected));
    }
}
