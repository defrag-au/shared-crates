//! [`CardanoHost`] — the browser's Cardano wallets, as a [`ChainHost`] for
//! the [`crate::WalletConnector`].
//!
//! Built on [`WalletFlow`] — the same connect sequence, network check and
//! remembered wallet every macroquad surface already uses — plus the wallets'
//! own icons, which the flow never needed.
//!
//! The flow is CIP-30's mechanics; what they *mean* is a
//! `macroquad_gamekit::wallet_connection::Connection`, the lifecycle machine
//! every chain's host feeds. When the flow's stage moves, the move is reported
//! to it as an event, and the modal's phase is read back from it — so every
//! chain's host gets the same rules: a late answer to a replaced request is
//! dropped, a refusal is not a fault, an account can change without a
//! reconnect.
//!
//! ```ignore
//! let wallets = WalletConnector::new()
//!     .host(CardanoHost::new(WalletFlow::new().expect_network(1)));
//! ```
//!
//! ## Only what changed
//!
//! A frame where nothing moved costs the bridge's polls and nothing else: the
//! account is written out and reported only when the flow's stage, its notice
//! or the handle changes, and the listing is rebuilt only when the connection,
//! the wallet list or an icon did.
//!
//! ## Icons
//!
//! Each detected wallet's CIP-30 `icon` is asked for once, through
//! [`Bridge::icon`], and arrives as RGBA the browser rasterised — SVG or PNG
//! alike. A wallet whose icon is missing or will not load keeps its monogram.
//! Pixels become a texture the first time the listing is read after they
//! arrive, which must happen where a graphics context exists.
//!
//! ## The account
//!
//! A connected wallet is written out as the modal's `Account`: its ADA Handle
//! as the label, then the handle, the receiving (change) address and the stake
//! address to copy, and the network. The handle is read from the wallet's own
//! balance (CIP-30 `getBalance`, walked by `wallet_miniquad::balance`), once
//! per address — no network call, and nothing to trust but the wallet. Until
//! it arrives, or for a wallet without one, the account is its address.
//!
use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, ChainPhase, WalletModalAction, WalletOption,
};
use wallet_miniquad::{PollResultDetailed, ReqId};

pub use crate::icons::ICON_PX;
use crate::icons::IconCache;
use crate::{Bridge, ChainHost, MiniquadBridge, Stage, WalletFlow, network_message};

/// Polls between re-reads of the extension list while nothing is connected:
/// half a second at 60 frames a second.
pub(crate) const RELIST_EVERY: u32 = 30;

/// The connected account's ADA Handle, read once per change address.
#[derive(Default)]
enum Handle {
    #[default]
    Unasked,
    Asked {
        req: ReqId,
        address: String,
    },
    /// `handle` is `None` for a wallet that holds none, or that would not say.
    Known {
        address: String,
        handle: Option<String>,
    },
}

/// What the connection was last told about, so an unchanged frame reports
/// nothing: the flow's stage revision, whether it held a notice, and the
/// handle's revision.
type Reported = (u64, bool, u64);

/// See the [module docs](self).
pub struct CardanoHost<B: Bridge = MiniquadBridge> {
    flow: WalletFlow<B>,
    /// The connection's lifecycle, the same machine every chain's host
    /// feeds: the flow is CIP-30's mechanics, this is what they mean.
    connection: Connection,
    /// Per provider key.
    icons: IconCache,
    /// Polls since the extension list was last read.
    since_list: u32,
    handle: Handle,
    /// Bumped when the handle read settles.
    handle_rev: u64,
    reported: Option<Reported>,
    /// The listing needs writing again.
    dirty: bool,
}

impl<B: Bridge> CardanoHost<B> {
    /// Wrap a flow. A flow that is already connecting its remembered wallet
    /// hands that wallet over, so the button says who it is waiting on.
    pub fn new(flow: WalletFlow<B>) -> Self {
        let mut connection = Connection::new();
        if let Some(remembered) = &flow.attempt {
            connection.handle(&ConnectionEvent::Ask(remembered.clone()));
        }
        let mut host = Self {
            flow,
            connection,
            icons: IconCache::default(),
            since_list: 0,
            handle: Handle::Unasked,
            handle_rev: 0,
            reported: None,
            dirty: true,
        };
        host.ask_icons();
        host
    }

    /// The flow underneath, for the reads a builder needs: the change address,
    /// the network, the blocker.
    pub fn flow(&self) -> &WalletFlow<B> {
        &self.flow
    }

    pub fn flow_mut(&mut self) -> &mut WalletFlow<B> {
        &mut self.flow
    }

    /// Advance the connect, the wallet list, the icons and the handle, and
    /// report a moved stage to the connection.
    fn advance(&mut self) {
        self.flow.poll();
        self.since_list += 1;
        if self.since_list >= RELIST_EVERY {
            self.since_list = 0;
            self.relist();
        }
        self.ask_icons();
        let bridge = &mut self.flow.bridge;
        self.dirty |= self.icons.poll_all(|req| {
            let result = bridge.poll(req);
            let pixels = match result {
                PollResultDetailed::Ok { .. } => bridge.take_icon(req, ICON_PX),
                _ => None,
            };
            (result, pixels)
        });
        self.poll_handle();
        let now = (
            self.flow.revision(),
            self.flow.notice().is_some(),
            self.handle_rev,
        );
        if self.reported != Some(now) {
            self.reported = Some(now);
            if let Some(event) = self.outcome() {
                self.feed(&event);
            }
        }
    }

    /// Tell the connection, and mark the listing stale when that changed it.
    fn feed(&mut self, event: &ConnectionEvent) {
        self.dirty |= self.connection.handle(event);
    }

    /// What the flow's stage means for the wallet being asked, as the
    /// connection's event. Read only when the stage, its notice or the handle
    /// has moved; the machine drops an answer for a wallet it is no longer
    /// asking.
    fn outcome(&self) -> Option<ConnectionEvent> {
        let wallet = self.connection.wallet()?.to_string();
        Some(match &self.flow.stage {
            Stage::Connecting
            | Stage::FetchingNetwork
            | Stage::FetchingAddress
            | Stage::FetchingStake => return None,
            Stage::Ready {
                address_bech32,
                stake_address,
                network_id,
                ..
            } => ConnectionEvent::Answered {
                wallet,
                account: cardano_account(
                    address_bech32,
                    stake_address.as_deref(),
                    *network_id,
                    self.handle(),
                ),
            },
            // A refused `enable()` leaves the flow on the picker with a
            // notice: the reader's no, not a fault.
            Stage::Disconnected if self.flow.notice().is_some() => ConnectionEvent::Refused(wallet),
            Stage::Disconnected | Stage::Unavailable => ConnectionEvent::Release,
            Stage::WrongNetwork {
                network_id,
                expected,
            } => ConnectionEvent::Faulted {
                wallet,
                message: network_message(*network_id, *expected),
            },
            Stage::Failed { message } => ConnectionEvent::Faulted {
                wallet,
                message: message.clone(),
            },
        })
    }

    /// Ask for the connected wallet's handle once its address is known, and
    /// again only if the address changes; read it when the balance arrives.
    fn poll_handle(&mut self) {
        let Stage::Ready { address_hex, .. } = &self.flow.stage else {
            if !matches!(self.handle, Handle::Unasked) {
                self.handle = Handle::Unasked;
                self.handle_rev += 1;
            }
            return;
        };
        let current = match &self.handle {
            Handle::Asked { address, .. } | Handle::Known { address, .. } => Some(address.as_str()),
            Handle::Unasked => None,
        };
        if current != Some(address_hex.as_str()) {
            let address = address_hex.clone();
            self.handle = match self.flow.bridge.balance() {
                Some(req) => Handle::Asked { req, address },
                None => Handle::Known {
                    address,
                    handle: None,
                },
            };
        }
        if let Handle::Asked { req, address } = &self.handle {
            let handle = match self.flow.bridge.poll(*req) {
                PollResultDetailed::Pending => return,
                // Already `$`-prefixed: the walker writes the handle as a
                // reader sees it.
                PollResultDetailed::Ok { data } => wallet_miniquad::balance::extract_handle(&data),
                PollResultDetailed::Err { .. } => None,
            };
            self.handle = Handle::Known {
                address: address.clone(),
                handle,
            };
            self.handle_rev += 1;
        }
    }

    /// The connected wallet's ADA Handle, with its `$`, once read.
    pub fn handle(&self) -> Option<&str> {
        match &self.handle {
            Handle::Known { handle, .. } => handle.as_deref(),
            _ => None,
        }
    }

    /// What the modal asked of Cardano. Another chain's action is not this
    /// host's; the connector never passes one, and it is ignored if it does.
    fn act(&mut self, action: &WalletModalAction) {
        match action {
            WalletModalAction::Connect {
                family: ChainFamily::Cardano,
                wallet,
            } => {
                self.feed(&ConnectionEvent::Ask(wallet.clone()));
                self.flow.connect(wallet);
            }
            // Back to the picker, with the wallet's dialog let go of. Not a
            // disconnect: nothing was connected, and the remembered wallet is
            // not this request's to forget.
            WalletModalAction::Cancel(ChainFamily::Cardano) => {
                self.feed(&ConnectionEvent::Release);
                self.flow.retry();
            }
            WalletModalAction::Disconnect(ChainFamily::Cardano) => {
                self.feed(&ConnectionEvent::Release);
                self.handle = Handle::Unasked;
                self.handle_rev += 1;
                self.flow.disconnect();
            }
            _ => {}
        }
    }

    /// Cardano as the modal lists it: the detected wallets, with their icons
    /// once loaded, and the connection's phase.
    fn listing(&mut self) -> ChainOption {
        self.dirty = false;
        let mut wallets = Vec::with_capacity(self.flow.providers.len());
        for provider in &self.flow.providers {
            let mut option = WalletOption::new(provider.key.clone(), provider.name.clone());
            if let Some(texture) = self.icons.texture(&provider.key) {
                option = option.with_icon(texture);
            }
            wallets.push(option);
        }
        ChainOption::ready(ChainFamily::Cardano, wallets).with_phase(self.phase())
    }

    /// The connection, in the modal's words.
    pub fn phase(&self) -> ChainPhase {
        self.connection.phase()
    }

    /// Re-read the extension list while the picker is up. A wallet injects
    /// `window.cardano.<key>` when it pleases — some after the page has loaded
    /// — and the flow reads the list only when it starts or resets, so without
    /// this a late wallet would be missing until a reload.
    fn relist(&mut self) {
        if self.flow.pending.is_some() || !matches!(self.flow.stage, Stage::Disconnected) {
            return;
        }
        let providers = self.flow.bridge.list_providers();
        let same = providers.len() == self.flow.providers.len()
            && providers
                .iter()
                .zip(&self.flow.providers)
                .all(|(a, b)| a.key == b.key);
        if !same {
            self.flow.providers = providers;
            let stage = self.flow.stage.clone();
            self.flow.set_stage(stage);
            self.dirty = true;
        }
    }

    /// Ask for the icon of every listed wallet not yet asked about.
    fn ask_icons(&mut self) {
        let bridge = &mut self.flow.bridge;
        for provider in &self.flow.providers {
            self.icons
                .ask(&provider.key, || bridge.icon(&provider.key, ICON_PX));
        }
    }
}

impl<B: Bridge + 'static> ChainHost for CardanoHost<B> {
    fn family(&self) -> ChainFamily {
        ChainFamily::Cardano
    }

    fn poll(&mut self) -> bool {
        self.advance();
        self.dirty
    }

    fn option(&mut self) -> ChainOption {
        self.listing()
    }

    fn apply(&mut self, action: &WalletModalAction) {
        self.act(action);
    }

    fn connection(&self) -> &Connection {
        &self.connection
    }

    /// [`WalletFlow::recheck`]: the reward address, then the change address
    /// only if that differs. A switched account reaches the connection as an
    /// answer with the new account, which updates it in place.
    fn recheck(&mut self) {
        self.flow.recheck();
    }
}

/// A Cardano account as the modal shows it: named by its handle when it has
/// one, with every value a reader might paste elsewhere. Public so a story or
/// a test draws exactly what a real connect produces.
pub fn cardano_account(
    address: &str,
    stake: Option<&str>,
    network_id: u8,
    handle: Option<&str>,
) -> Account {
    let mut account = Account::new(address);
    if let Some(handle) = handle {
        account = account.with_label(handle).copyable("ADA Handle", handle);
    }
    account = account.copyable("Receiving address", address);
    if let Some(stake) = stake {
        account = account.copyable("Stake address", stake);
    }
    // CIP-30's network id is 1 for mainnet and 0 for every testnet: it cannot
    // tell preprod from preview, so neither is claimed.
    let network = match network_id {
        1 => "Mainnet",
        _ => "Testnet",
    };
    account.fact("Network", network)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use wallet_miniquad::{FailureKind, Provider};

    /// A mainnet base address: header 0x01, a key payment hash, a key stake
    /// hash. Constructed so the headers are visible.
    fn base_address() -> String {
        format!("01{}{}", "11".repeat(28), "22".repeat(28))
    }

    /// Answers polls in order; icons are asked but never arrive, which is what
    /// a test without a browser can honestly say.
    struct Fake {
        results: VecDeque<PollResultDetailed>,
        icons_asked: Vec<String>,
        providers: Vec<Provider>,
        /// The balance CBOR the wallet answers `getBalance` with, if asked.
        balance: Option<String>,
        balances_asked: u32,
    }

    /// The balance id the fake hands out, answered from `Fake::balance`.
    const BALANCE: ReqId = ReqId(200);

    fn eternl() -> Provider {
        Provider {
            key: "eternl".into(),
            name: "Eternl".into(),
            version: "1".into(),
        }
    }

    impl Bridge for Fake {
        fn available(&self) -> bool {
            true
        }
        fn list_providers(&self) -> Vec<Provider> {
            self.providers.clone()
        }
        fn connect(&mut self, _key: &str) -> ReqId {
            ReqId(1)
        }
        fn network_id(&mut self) -> ReqId {
            ReqId(2)
        }
        fn change_address(&mut self) -> ReqId {
            ReqId(3)
        }
        fn reward_address(&mut self) -> ReqId {
            ReqId(4)
        }
        fn poll(&mut self, id: ReqId) -> PollResultDetailed {
            if id.0 == BALANCE.0 {
                return match &self.balance {
                    Some(data) => ok(data),
                    None => PollResultDetailed::Pending,
                };
            }
            if id.0 >= 100 {
                return PollResultDetailed::Pending;
            }
            self.results
                .pop_front()
                .unwrap_or(PollResultDetailed::Pending)
        }
        fn cancel(&mut self, _id: ReqId) {}
        fn disconnect(&mut self) {}
        fn icon(&mut self, key: &str, _size: u16) -> Option<ReqId> {
            self.icons_asked.push(key.to_string());
            Some(ReqId(100))
        }
        fn balance(&mut self) -> Option<ReqId> {
            self.balances_asked += 1;
            self.balance.is_some().then_some(BALANCE)
        }
    }

    fn ok(data: &str) -> PollResultDetailed {
        PollResultDetailed::Ok {
            data: data.to_string(),
        }
    }

    fn host(results: Vec<PollResultDetailed>) -> CardanoHost<Fake> {
        host_with_balance(results, None)
    }

    fn host_with_balance(
        results: Vec<PollResultDetailed>,
        balance: Option<String>,
    ) -> CardanoHost<Fake> {
        CardanoHost::new(
            WalletFlow::with_bridge(Fake {
                results: results.into(),
                icons_asked: Vec::new(),
                providers: vec![eternl()],
                balance,
                balances_asked: 0,
            })
            .expect_network(1),
        )
    }

    /// A CIP-30 balance holding one legacy (unprefixed) ADA Handle, encoded
    /// by hand so each byte's meaning is visible: `[coin, {policy: {name: 1}}]`.
    fn balance_with_handle(name: &str) -> String {
        let policy = "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a";
        let name_hex: String = name.bytes().map(|b| format!("{b:02x}")).collect();
        // 0x82 array(2) · 0x1a u32 coin · 0xa1 map(1) · 0x58 0x1c bytes(28)
        // policy · 0xa1 map(1) · 0x40|len bytes(len) name · 0x01 quantity.
        format!(
            "821a000f4240a1581c{policy}a1{:02x}{name_hex}01",
            0x40 + name.len()
        )
    }

    fn connect(host: &mut CardanoHost<Fake>) {
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Cardano,
            wallet: "eternl".into(),
        });
    }

    #[test]
    fn every_listed_wallet_has_its_icon_asked_for_once() {
        let mut host = host(Vec::new());
        host.poll();
        host.poll();
        assert_eq!(host.flow.bridge.icons_asked, vec!["eternl".to_string()]);
        let chain = host.option();
        assert_eq!(chain.wallets.len(), 1);
        assert!(chain.wallets[0].icon.is_none(), "no pixels arrived");
    }

    /// Once settled, a frame where nothing moved reports no change — so the
    /// connector rebuilds nothing.
    #[test]
    fn an_idle_host_reports_no_change() {
        let mut host = host(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        connect(&mut host);
        for _ in 0..4 {
            host.poll();
        }
        let _ = host.option();
        for frame in 0..(RELIST_EVERY as usize * 3) {
            assert!(!host.poll(), "frame {frame}: nothing moved");
        }
        assert!(host.connection().is_connected());
    }

    #[test]
    fn a_connect_reads_as_connecting_then_connected() {
        let mut host = host(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        connect(&mut host);
        assert_eq!(
            host.phase(),
            ChainPhase::Connecting {
                wallet: "eternl".into()
            }
        );
        for _ in 0..4 {
            host.poll();
        }
        let ChainPhase::Connected { wallet, account } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        let address = &account.address;
        assert_eq!(wallet, "eternl");
        assert!(address.starts_with("addr1"), "{address}");
    }

    /// The account names itself by the handle in the wallet, offers every
    /// value to copy, and says which network — and the handle is read once.
    #[test]
    fn a_connected_account_carries_its_handle_and_every_address() {
        let mut host = host_with_balance(
            vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")],
            Some(balance_with_handle("boef")),
        );
        connect(&mut host);
        for _ in 0..6 {
            host.poll();
        }
        assert_eq!(host.handle(), Some("$boef"));
        assert_eq!(host.flow.bridge.balances_asked, 1, "one read per address");
        let ChainPhase::Connected { account, .. } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        assert_eq!(account.display(), "$boef");
        let labels: Vec<&str> = account.details.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "ADA Handle",
                "Receiving address",
                "Stake address",
                "Network"
            ]
        );
        let stake = &account.details[2].value;
        assert!(stake.starts_with("stake1"), "{stake}");
        assert_eq!(account.details[3].value, "Mainnet");
    }

    /// A wallet without a handle is named by its address until it has one.
    #[test]
    fn an_account_without_a_handle_is_its_address() {
        let mut host = host(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        connect(&mut host);
        for _ in 0..4 {
            host.poll();
        }
        let ChainPhase::Connected { account, .. } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        assert!(account.label.is_none());
        assert_eq!(account.display(), account.address);
    }

    #[test]
    fn a_refusal_reads_as_declined_and_a_cancel_as_idle() {
        let mut host = host(vec![PollResultDetailed::Err {
            data: "User declined".into(),
            kind: FailureKind::Declined,
        }]);
        connect(&mut host);
        host.poll();
        assert_eq!(
            host.phase(),
            ChainPhase::Declined {
                wallet: "eternl".into()
            }
        );

        connect(&mut host);
        host.apply(&WalletModalAction::Cancel(ChainFamily::Cardano));
        assert_eq!(host.phase(), ChainPhase::Idle);
    }

    #[test]
    fn a_wrong_network_reads_as_a_failure_that_says_so() {
        let mut host = host(vec![ok("eternl"), ok("0")]);
        connect(&mut host);
        host.poll();
        host.poll();
        let ChainPhase::Failed { message, .. } = host.phase() else {
            panic!("expected failed, got {:?}", host.phase());
        };
        assert!(message.contains("Wrong network"), "{message}");
    }

    /// An extension that injects itself after the page loaded is listed — and
    /// has its icon asked for — without a reload.
    #[test]
    fn a_wallet_that_arrives_late_is_picked_up() {
        let mut host = CardanoHost::new(WalletFlow::with_bridge(Fake {
            results: VecDeque::new(),
            icons_asked: Vec::new(),
            providers: Vec::new(),
            balance: None,
            balances_asked: 0,
        }));
        assert!(host.option().wallets.is_empty());
        host.flow.bridge.providers.push(eternl());
        for _ in 0..RELIST_EVERY {
            host.poll();
        }
        let chain = host.option();
        assert_eq!(chain.wallets.len(), 1);
        assert_eq!(chain.wallets[0].name, "Eternl");
        assert_eq!(host.flow.bridge.icons_asked, vec!["eternl".to_string()]);
    }

    /// Switched account in the wallet while away: after a recheck the modal's
    /// account is the new one, still connected, never through a reconnect.
    #[test]
    fn a_recheck_carries_a_switched_account_to_the_modal() {
        let mut host = host(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        connect(&mut host);
        for _ in 0..4 {
            host.poll();
        }
        let ChainPhase::Connected { account: was, .. } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        let switched = format!("01{}{}", "44".repeat(28), "55".repeat(28));
        host.flow.bridge.results.push_back(ok(""));
        host.flow.bridge.results.push_back(ok(&switched));

        host.recheck();
        host.poll();
        assert!(host.poll(), "the listing changed");

        let ChainPhase::Connected { wallet, account } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        assert_eq!(wallet, "eternl");
        assert_ne!(account.address, was.address);
        let stake = |a: &Account| {
            a.details
                .iter()
                .find(|d| d.label == "Stake address")
                .map(|d| d.value.clone())
        };
        assert_ne!(stake(&account), stake(&was));
    }

    #[test]
    fn another_chains_action_is_left_alone() {
        let mut host = host(Vec::new());
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "io.rabby".into(),
        });
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert!(matches!(host.flow.stage(), Stage::Disconnected));
    }
}
