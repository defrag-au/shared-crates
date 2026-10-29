//! [`EvmHost`] — the browser's EVM wallets, as a [`ChainHost`] for the
//! [`crate::WalletConnector`]: the counterpart of [`crate::CardanoHost`],
//! over `wallet_miniquad::evm`.
//!
//! The same shape as the Cardano host, which is the point of the connection
//! machine: wallets listed with their own icons, the bridge's outcomes fed to a
//! `macroquad_gamekit::wallet_connection::Connection` as events, and the modal's
//! phase read back from it. Only the mechanics are EVM's own:
//!
//! - **Discovery is EIP-6963.** Each wallet announces itself with a uuid, a
//!   name and an icon, and its own provider — so the wallet the reader picks is
//!   the one asked, even with several installed. The list is re-read while the
//!   modal runs, because a wallet may announce after the page has loaded.
//! - **A connect is two reads**: `eth_requestAccounts` (the wallet's dialog),
//!   then `eth_chainId`, so the account says which chain it is on.
//! - **The wallet can speak first.** Switching account or chain in the wallet
//!   updates the connected account in place; a wallet that drops every account
//!   — the reader disconnected the site there — lets the connection go.
//! - **The wallet is remembered**, as the Cardano flow remembers its wallet:
//!   a connect that completed is written to the page's storage, and the next
//!   load reconnects it with `eth_accounts`, which never prompts. If the site
//!   is no longer permitted, nothing is shown and the memory is forgotten.
//!
//! ## The chain it plays on
//!
//! A host for a game that plays on one chain says so with
//! [`EvmHost::expect_chain`]. A connect then ends on that chain: a wallet
//! on another is asked to switch (EIP-3326), and offered the chain to add
//! first (EIP-3085) when it does not know it. Refusing the switch is a fault
//! that says which chain the wallet is on and which the game needs, with Try
//! again. Switching away later in the wallet does the same; switching back
//! reconnects by itself. A reconnect on load never asks — a dialog nobody
//! asked for is an ambush — so a remembered wallet on the wrong chain waits
//! for the reader to open the modal and try again.
//!
//! ## The account
//!
//! The address, to copy, and the network, named when `chains` names it
//! (`Robinhood Chain`) and otherwise by its id (`Chain 1`) — never from a list
//! written from memory. The label slot is empty: an ENS name needs a reverse
//! lookup over the chain, which this bridge does not make, so the account is
//! named by its address until a resolver writes one in.

use chains::EvmChain;
use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, ChainPhase, WalletModalAction, WalletOption,
};
use wallet_miniquad::evm::{self, EvmChanges, EvmWallet};
pub use wallet_miniquad::evm::{AddChain, ChainTarget, NativeCurrency};
use wallet_miniquad::{FailureKind, PollResultDetailed, ReqId};

use crate::ChainHost;
use crate::cardano::RELIST_EVERY;
use crate::icons::{ICON_PX, IconCache};

/// What the host needs from an EVM bridge. [`MiniquadEvmBridge`] is the real
/// one; tests drive a fake through every step.
pub trait EvmBridge {
    /// Whether a browser wallet can exist here. Native builds answer `false`.
    fn available(&self) -> bool {
        wallet_miniquad::AVAILABLE
    }
    fn list_wallets(&self) -> Vec<EvmWallet>;
    /// Ask `id` for its accounts; `silent` never prompts.
    fn connect(&mut self, id: &str, silent: bool) -> ReqId;
    fn chain_id(&mut self) -> ReqId;
    /// Ask the connected wallet onto `target`; on ok, the chain it is on after.
    fn switch_chain(&mut self, target: &ChainTarget) -> ReqId;
    fn disconnect(&mut self);
    fn changes(&mut self) -> EvmChanges;
    fn poll(&mut self, req: ReqId) -> PollResultDetailed;
    fn cancel(&mut self, req: ReqId);
    fn icon(&mut self, _id: &str, _size: u16) -> Option<ReqId> {
        None
    }
    fn take_icon(&mut self, _req: ReqId, _size: u16) -> Option<Vec<u8>> {
        None
    }
    /// The wallet this host last connected, if it keeps one between runs.
    fn remembered(&self) -> Option<String> {
        None
    }
    fn remember(&mut self, _id: Option<&str>) {}
}

/// The production bridge: the miniquad `evm` plugin.
#[derive(Default)]
pub struct MiniquadEvmBridge;

/// Where the remembered wallet's uuid lives — namespaced, like the Cardano
/// flow's key, so two apps on one origin keep their own.
const WALLET_KEY: &str = "macroquad-wallet-flow.evm-wallet";

impl EvmBridge for MiniquadEvmBridge {
    fn list_wallets(&self) -> Vec<EvmWallet> {
        evm::list_wallets()
    }
    fn connect(&mut self, id: &str, silent: bool) -> ReqId {
        evm::connect(id, silent)
    }
    fn chain_id(&mut self) -> ReqId {
        evm::chain_id()
    }
    fn switch_chain(&mut self, target: &ChainTarget) -> ReqId {
        evm::switch_chain(target)
    }
    fn disconnect(&mut self) {
        evm::disconnect()
    }
    fn changes(&mut self) -> EvmChanges {
        evm::changes()
    }
    fn poll(&mut self, req: ReqId) -> PollResultDetailed {
        evm::poll(req)
    }
    fn cancel(&mut self, req: ReqId) {
        evm::cancel(req)
    }
    fn icon(&mut self, id: &str, size: u16) -> Option<ReqId> {
        wallet_miniquad::AVAILABLE.then(|| evm::icon(id, size))
    }
    fn take_icon(&mut self, req: ReqId, size: u16) -> Option<Vec<u8>> {
        evm::take_icon(req, size)
    }
    fn remembered(&self) -> Option<String> {
        miniquad_platform::storage_get(WALLET_KEY)
    }
    fn remember(&mut self, id: Option<&str>) {
        match id {
            Some(id) => miniquad_platform::storage_set(WALLET_KEY, id),
            None => miniquad_platform::storage_remove(WALLET_KEY),
        }
    }
}

/// The read in flight.
enum Step {
    Idle,
    /// The accounts request: `silent` for the reconnect on load.
    Accounts {
        req: ReqId,
        silent: bool,
    },
    /// The chain id, once an account is known.
    Chain {
        req: ReqId,
        address: String,
        silent: bool,
    },
    /// The wallet has been asked onto the expected chain. The account is
    /// already in hand; only its chain is waited on.
    Switch {
        req: ReqId,
    },
}

/// Robinhood Chain as a target: switched to, and added where the wallet does
/// not know it.
///
/// Every value is sourced, not remembered. The chain id is `chains`'
/// verified one. The name, the native currency and the explorer are the
/// community registry's (`ethereum-lists/chains`, `eip155-4663.json`), read on
/// 2026-10-29. The two RPC endpoints are the ones `evm-chain-reader` records,
/// each answering `eth_chainId` with `0x1237` on 2026-10-29; the explorer
/// answered too. Should one stop answering, the registry is where its
/// replacement is found, and `eth_chainId` is the check.
pub fn robinhood_chain() -> ChainTarget {
    ChainTarget {
        chain_id: EvmChain::Robinhood.chain_id(),
        add: Some(AddChain {
            chain_name: "Robinhood Chain".to_string(),
            rpc_urls: vec![
                "https://rpc.mainnet.chain.robinhood.com".to_string(),
                "https://robinhood-rpc.publicnode.com".to_string(),
            ],
            native_currency: NativeCurrency {
                name: "Ether".to_string(),
                symbol: "ETH".to_string(),
                decimals: 18,
            },
            block_explorer_urls: vec!["https://robinhoodchain.blockscout.com".to_string()],
        }),
    }
}

/// See the [module docs](self).
pub struct EvmHost<B: EvmBridge = MiniquadEvmBridge> {
    bridge: B,
    wallets: Vec<EvmWallet>,
    connection: Connection,
    step: Step,
    /// The connected account's address and chain, as the wallet last said.
    address: Option<String>,
    chain: Option<u64>,
    /// The remembered wallet, until a reconnect has been tried: it may
    /// announce after the first listing.
    remembered: Option<String>,
    icons: IconCache,
    since_list: u32,
    /// The chain a connect has to end on, when the game plays on one.
    target: Option<ChainTarget>,
    /// The connection failed because the wallet is on another chain: a
    /// switch back in the wallet reconnects it.
    wrong_chain: bool,
    /// The listing needs writing again.
    dirty: bool,
}

impl EvmHost<MiniquadEvmBridge> {
    pub fn new() -> Self {
        Self::with_bridge(MiniquadEvmBridge)
    }
}

impl Default for EvmHost<MiniquadEvmBridge> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: EvmBridge> EvmHost<B> {
    /// A host over `bridge`. A remembered wallet that has announced is
    /// reconnected silently, before the first frame.
    pub fn with_bridge(bridge: B) -> Self {
        let available = bridge.available();
        let wallets = if available {
            bridge.list_wallets()
        } else {
            Vec::new()
        };
        let remembered = if available { bridge.remembered() } else { None };
        let mut host = Self {
            bridge,
            wallets,
            connection: Connection::new(),
            step: Step::Idle,
            address: None,
            chain: None,
            remembered,
            icons: IconCache::default(),
            since_list: 0,
            target: None,
            wrong_chain: false,
            dirty: true,
        };
        host.try_remembered();
        host.ask_icons();
        host
    }

    /// Require a chain: a connect ends on `target`, or fails saying why. See
    /// the [module docs](self#the-chain-it-plays-on).
    pub fn expect_chain(mut self, target: ChainTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Advance the reads, the wallet's own reports and the icons.
    fn advance(&mut self) {
        self.since_list += 1;
        if self.since_list >= RELIST_EVERY {
            self.since_list = 0;
            self.relist();
            self.try_remembered();
        }
        self.ask_icons();
        let bridge = &mut self.bridge;
        self.dirty |= self.icons.poll_all(|req| {
            let result = bridge.poll(req);
            let pixels = match result {
                PollResultDetailed::Ok { .. } => bridge.take_icon(req, ICON_PX),
                _ => None,
            };
            (result, pixels)
        });
        self.poll_step();
        self.poll_changes();
    }

    /// Re-read the announced wallets; a wallet may announce after the page
    /// loaded. Marks the listing stale only when the list actually changed.
    fn relist(&mut self) {
        if !self.bridge.available() {
            return;
        }
        let wallets = self.bridge.list_wallets();
        if wallets != self.wallets {
            self.wallets = wallets;
            self.dirty = true;
        }
    }

    /// Tell the connection, and mark the listing stale when that changed it.
    fn feed(&mut self, event: &ConnectionEvent) {
        self.dirty |= self.connection.handle(event);
    }

    /// What the modal asked of EVM. Another chain's action is not this
    /// host's; the connector never passes one, and it is ignored if it does.
    fn act(&mut self, action: &WalletModalAction) {
        match action {
            WalletModalAction::Connect {
                family: ChainFamily::Evm,
                wallet,
            } => {
                self.remembered = None;
                self.ask(wallet, false);
            }
            WalletModalAction::Cancel(ChainFamily::Evm) => {
                self.drop_step();
                self.wrong_chain = false;
                self.feed(&ConnectionEvent::Release);
            }
            WalletModalAction::Disconnect(ChainFamily::Evm) => {
                self.drop_step();
                self.wrong_chain = false;
                self.bridge.disconnect();
                self.bridge.remember(None);
                self.address = None;
                self.chain = None;
                self.feed(&ConnectionEvent::Release);
            }
            _ => {}
        }
    }

    /// EVM as the modal lists it: the announced wallets, with their icons
    /// once loaded, and the connection's phase.
    fn listing(&mut self) -> ChainOption {
        self.dirty = false;
        let mut options = Vec::with_capacity(self.wallets.len());
        for wallet in &self.wallets {
            let mut option = WalletOption::new(wallet.id.clone(), wallet.name.clone());
            if let Some(texture) = self.icons.texture(&wallet.id) {
                option = option.with_icon(texture);
            }
            options.push(option);
        }
        ChainOption::ready(ChainFamily::Evm, options).with_phase(self.phase())
    }

    pub fn phase(&self) -> ChainPhase {
        self.connection.phase()
    }

    fn ask(&mut self, wallet: &str, silent: bool) {
        self.drop_step();
        self.wrong_chain = false;
        self.feed(&ConnectionEvent::Ask(wallet.to_string()));
        let req = self.bridge.connect(wallet, silent);
        self.step = Step::Accounts { req, silent };
    }

    fn drop_step(&mut self) {
        match std::mem::replace(&mut self.step, Step::Idle) {
            Step::Accounts { req, .. } | Step::Chain { req, .. } | Step::Switch { req, .. } => {
                self.bridge.cancel(req)
            }
            Step::Idle => {}
        }
    }

    /// Reconnect the remembered wallet, once it has announced.
    fn try_remembered(&mut self) {
        let Some(id) = &self.remembered else {
            return;
        };
        if !self.wallets.iter().any(|w| &w.id == id) {
            return;
        }
        let id = id.clone();
        self.remembered = None;
        self.ask(&id, true);
    }

    fn poll_step(&mut self) {
        let Some(wallet) = self.connection.wallet().map(str::to_string) else {
            return;
        };
        match &self.step {
            Step::Idle => {}
            Step::Accounts { req, silent } => {
                let (req, silent) = (*req, *silent);
                match self.bridge.poll(req) {
                    PollResultDetailed::Pending => {}
                    PollResultDetailed::Ok { data } => {
                        let chain = self.bridge.chain_id();
                        self.step = Step::Chain {
                            req: chain,
                            address: data,
                            silent,
                        };
                    }
                    // A reconnect the wallet no longer permits: nothing to
                    // say, and nothing worth remembering.
                    PollResultDetailed::Err { .. } if silent => {
                        self.step = Step::Idle;
                        self.bridge.remember(None);
                        self.feed(&ConnectionEvent::Release);
                    }
                    PollResultDetailed::Err { data, kind } => {
                        self.step = Step::Idle;
                        self.feed(&match kind {
                            FailureKind::Declined => ConnectionEvent::Refused(wallet),
                            FailureKind::Failed => ConnectionEvent::Faulted {
                                wallet,
                                message: data,
                            },
                        });
                    }
                }
            }
            Step::Chain {
                req,
                address,
                silent,
            } => {
                let (req, address, silent) = (*req, address.clone(), *silent);
                let chain = match self.bridge.poll(req) {
                    PollResultDetailed::Pending => return,
                    PollResultDetailed::Ok { data } => evm::parse_chain_id(&data),
                    // The account is real without it; the network line is
                    // simply left out.
                    PollResultDetailed::Err { .. } => None,
                };
                self.step = Step::Idle;
                self.address = Some(address);
                self.chain = chain;
                match self.target.clone() {
                    Some(target) if chain != Some(target.chain_id) => {
                        if silent {
                            // Never a dialog on load: say so when asked.
                            self.fault_wrong_chain(wallet);
                        } else {
                            let req = self.bridge.switch_chain(&target);
                            self.step = Step::Switch { req };
                        }
                    }
                    _ => self.connected(wallet),
                }
            }
            Step::Switch { req } => {
                let req = *req;
                let result = self.bridge.poll(req);
                if matches!(result, PollResultDetailed::Pending) {
                    return;
                }
                self.step = Step::Idle;
                match result {
                    PollResultDetailed::Pending => {}
                    // Read back, not assumed: a wallet can "switch" elsewhere.
                    PollResultDetailed::Ok { data } => {
                        self.chain = evm::parse_chain_id(&data);
                        if self.on_target() {
                            self.connected(wallet);
                        } else {
                            self.fault_wrong_chain(wallet);
                        }
                    }
                    PollResultDetailed::Err {
                        kind: FailureKind::Declined,
                        ..
                    } => self.fault_wrong_chain(wallet),
                    PollResultDetailed::Err { data, .. } => {
                        self.feed(&ConnectionEvent::Faulted {
                            wallet,
                            message: data,
                        });
                    }
                }
            }
        }
    }

    /// The account is in hand and on the right chain: connected, and worth
    /// remembering.
    fn connected(&mut self, wallet: String) {
        self.wrong_chain = false;
        self.bridge.remember(Some(&wallet));
        if let Some(account) = self.account() {
            self.feed(&ConnectionEvent::Answered { wallet, account });
        }
    }

    fn on_target(&self) -> bool {
        self.target
            .as_ref()
            .is_none_or(|t| self.chain == Some(t.chain_id))
    }

    /// The wallet is on another chain than the game plays on: a fault that
    /// says which, and which it needs.
    fn fault_wrong_chain(&mut self, wallet: String) {
        self.wrong_chain = true;
        let name = self
            .wallets
            .iter()
            .find(|w| w.id == wallet)
            .map_or_else(|| "The wallet".to_string(), |w| w.name.clone());
        let on = self
            .chain
            .map_or_else(|| "another network".to_string(), chain_name);
        let want = self
            .target
            .as_ref()
            .map_or_else(String::new, |t| chain_name(t.chain_id));
        let message = format!(
            "{name} is on {on}, and this game plays on {want}. Switch to {want} in {name}, then try again."
        );
        self.feed(&ConnectionEvent::Faulted { wallet, message });
    }

    /// What the connected wallet said on its own: another account, another
    /// chain, or no accounts at all.
    fn poll_changes(&mut self) {
        let changes = self.bridge.changes();
        // Failed for being on another chain: switching back in the wallet is
        // the reader fixing it, so reconnect without making them ask.
        if self.wrong_chain {
            if let Some(chain) = changes.chain {
                self.chain = evm::parse_chain_id(&chain);
                if self.on_target()
                    && let Some(wallet) = self.connection.wallet().map(str::to_string)
                {
                    // A failed connection takes no answer until it is asked
                    // again; the reader's switch is that ask.
                    self.feed(&ConnectionEvent::Ask(wallet.clone()));
                    self.connected(wallet);
                }
            }
            return;
        }
        if !self.connection.is_connected() {
            return;
        }
        let mut changed = false;
        if let Some(accounts) = changes.accounts {
            match accounts.into_iter().next() {
                Some(address) => {
                    changed |= self.address.as_deref() != Some(address.as_str());
                    self.address = Some(address);
                }
                None => {
                    // Disconnected in the wallet: there is no account left.
                    self.address = None;
                    self.chain = None;
                    self.bridge.remember(None);
                    self.feed(&ConnectionEvent::Release);
                    return;
                }
            }
        }
        if let Some(chain) = changes.chain {
            let chain = evm::parse_chain_id(&chain);
            changed |= chain != self.chain;
            self.chain = chain;
            if !self.on_target()
                && let Some(wallet) = self.connection.wallet().map(str::to_string)
            {
                self.fault_wrong_chain(wallet);
                return;
            }
        }
        if changed && let Some(account) = self.account() {
            self.feed(&ConnectionEvent::AccountChanged(account));
        }
    }

    fn account(&self) -> Option<Account> {
        Some(evm_account(self.address.as_deref()?, self.chain))
    }

    fn ask_icons(&mut self) {
        let bridge = &mut self.bridge;
        for wallet in &self.wallets {
            self.icons
                .ask(&wallet.id, || bridge.icon(&wallet.id, ICON_PX));
        }
    }
}

impl<B: EvmBridge + 'static> ChainHost for EvmHost<B> {
    fn family(&self) -> ChainFamily {
        ChainFamily::Evm
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
}

/// An EVM account as the modal shows it: the address to copy, and the chain
/// it is on, named only where `chains` names it.
pub fn evm_account(address: &str, chain: Option<u64>) -> Account {
    let account = Account::new(address).copyable("Address", address);
    match chain {
        Some(id) => account.fact("Network", chain_name(id)),
        None => account,
    }
}

fn chain_name(id: u64) -> String {
    match EvmChain::from_chain_id(id).name() {
        Some(name) => name.to_string(),
        None => format!("Chain {id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    /// Answers each request id from its own queue, so the accounts read and
    /// the chain read cannot be confused.
    #[derive(Default)]
    struct Fake {
        wallets: Vec<EvmWallet>,
        answers: HashMap<i32, VecDeque<PollResultDetailed>>,
        next: i32,
        /// What each `connect` was asked: the wallet, and whether silently.
        asked: Vec<(String, bool)>,
        changes: VecDeque<EvmChanges>,
        remembered: Option<String>,
        stored: Vec<Option<String>>,
        disconnected: bool,
        /// Queued answers for the next accounts and chain reads.
        accounts: VecDeque<PollResultDetailed>,
        chains: VecDeque<PollResultDetailed>,
        /// Queued answers for switch requests, and each target asked for.
        switches: VecDeque<PollResultDetailed>,
        switched_to: Vec<u64>,
    }

    impl Fake {
        fn issue(&mut self, from: &str) -> ReqId {
            self.next += 1;
            let queue = match from {
                "accounts" => self.accounts.pop_front(),
                "switch" => self.switches.pop_front(),
                _ => self.chains.pop_front(),
            };
            self.answers.insert(self.next, queue.into_iter().collect());
            ReqId(self.next)
        }
    }

    impl EvmBridge for Fake {
        fn available(&self) -> bool {
            true
        }
        fn list_wallets(&self) -> Vec<EvmWallet> {
            self.wallets.clone()
        }
        fn connect(&mut self, id: &str, silent: bool) -> ReqId {
            self.asked.push((id.to_string(), silent));
            self.issue("accounts")
        }
        fn chain_id(&mut self) -> ReqId {
            self.issue("chain")
        }
        fn switch_chain(&mut self, target: &ChainTarget) -> ReqId {
            self.switched_to.push(target.chain_id);
            self.issue("switch")
        }
        fn disconnect(&mut self) {
            self.disconnected = true;
        }
        fn changes(&mut self) -> EvmChanges {
            self.changes.pop_front().unwrap_or_default()
        }
        fn poll(&mut self, req: ReqId) -> PollResultDetailed {
            self.answers
                .get_mut(&req.0)
                .and_then(VecDeque::pop_front)
                .unwrap_or(PollResultDetailed::Pending)
        }
        fn cancel(&mut self, req: ReqId) {
            self.answers.remove(&req.0);
        }
        fn remembered(&self) -> Option<String> {
            self.remembered.clone()
        }
        fn remember(&mut self, id: Option<&str>) {
            self.stored.push(id.map(str::to_string));
        }
    }

    fn rabby() -> EvmWallet {
        EvmWallet {
            id: "uuid-rabby".into(),
            name: "Rabby".into(),
            rdns: "io.rabby".into(),
        }
    }

    fn ok(data: &str) -> PollResultDetailed {
        PollResultDetailed::Ok {
            data: data.to_string(),
        }
    }

    /// A synthetic address, as the egui fixtures use: not a real account's.
    fn address() -> String {
        format!("0x{}", "1f".repeat(20))
    }

    fn fake() -> Fake {
        Fake {
            wallets: vec![rabby()],
            ..Fake::default()
        }
    }

    fn connect(host: &mut EvmHost<Fake>) {
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "uuid-rabby".into(),
        });
    }

    fn run(host: &mut EvmHost<Fake>, frames: usize) {
        for _ in 0..frames {
            host.poll();
        }
    }

    /// Once settled, a frame where nothing moved reports no change.
    #[test]
    fn an_idle_host_reports_no_change() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 2);
        let _ = host.option();
        for frame in 0..(RELIST_EVERY as usize * 3) {
            assert!(!host.poll(), "frame {frame}: nothing moved");
        }
        assert!(host.connection().is_connected());
    }

    #[test]
    fn announced_wallets_are_listed() {
        let mut host = EvmHost::with_bridge(fake());
        let chain = host.option();
        assert_eq!(chain.family, ChainFamily::Evm);
        assert_eq!(chain.wallets.len(), 1);
        assert_eq!(chain.wallets[0].name, "Rabby");
        assert_eq!(chain.phase, ChainPhase::Idle);
    }

    #[test]
    fn a_connect_reads_the_account_then_its_chain() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1237"));
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        assert!(matches!(host.phase(), ChainPhase::Connecting { .. }));
        assert_eq!(host.bridge.asked, [("uuid-rabby".to_string(), false)]);
        run(&mut host, 2);
        let ChainPhase::Connected { wallet, account } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        assert_eq!(wallet, "uuid-rabby");
        assert_eq!(account.address, address());
        assert_eq!(account.details[1].value, "Robinhood Chain");
        assert_eq!(host.bridge.stored, [Some("uuid-rabby".to_string())]);
    }

    /// An id `chains` does not name is said as a number, not guessed at.
    #[test]
    fn an_unnamed_chain_is_its_id() {
        assert_eq!(chain_name(137), "Chain 137");
        assert_eq!(chain_name(1), "Ethereum Mainnet");
        assert_eq!(chain_name(4663), "Robinhood Chain");
    }

    #[test]
    fn a_refusal_reads_as_declined() {
        let mut bridge = fake();
        bridge.accounts.push_back(PollResultDetailed::Err {
            data: "User rejected the request.".into(),
            kind: FailureKind::Declined,
        });
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 1);
        assert_eq!(
            host.phase(),
            ChainPhase::Declined {
                wallet: "uuid-rabby".into()
            }
        );
        assert!(host.bridge.stored.is_empty(), "a refusal is not remembered");
    }

    #[test]
    fn a_fault_says_what_the_wallet_said() {
        let mut bridge = fake();
        bridge.accounts.push_back(PollResultDetailed::Err {
            data: "The wallet already has a request open.".into(),
            kind: FailureKind::Failed,
        });
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 1);
        let ChainPhase::Failed { message, .. } = host.phase() else {
            panic!("expected failed, got {:?}", host.phase());
        };
        assert!(message.contains("already has a request"), "{message}");
    }

    /// The reader switches account, then chain, in the wallet: the account
    /// follows without a reconnect.
    #[test]
    fn the_wallet_switching_account_or_chain_updates_in_place() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let other = format!("0x{}", "2e".repeat(20));
        bridge.changes.push_back(EvmChanges::default());
        bridge.changes.push_back(EvmChanges::default());
        bridge.changes.push_back(EvmChanges {
            accounts: Some(vec![other.clone()]),
            chain: Some("0x1237".into()),
        });
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 3);
        let account = host.connection().account().expect("still connected");
        assert_eq!(account.address, other);
        assert_eq!(account.details[1].value, "Robinhood Chain");
        assert_eq!(host.bridge.asked.len(), 1, "no reconnect");
    }

    /// The reader disconnects this site in the wallet itself.
    #[test]
    fn a_wallet_that_drops_every_account_lets_go() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        bridge.changes.push_back(EvmChanges::default());
        bridge.changes.push_back(EvmChanges::default());
        bridge.changes.push_back(EvmChanges {
            accounts: Some(Vec::new()),
            chain: None,
        });
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 3);
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert_eq!(host.bridge.stored.last(), Some(&None), "and is forgotten");
    }

    /// A returning reader is reconnected without a dialog.
    #[test]
    fn a_remembered_wallet_reconnects_silently() {
        let mut bridge = fake();
        bridge.remembered = Some("uuid-rabby".into());
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let mut host = EvmHost::with_bridge(bridge);
        assert_eq!(host.bridge.asked, [("uuid-rabby".to_string(), true)]);
        run(&mut host, 2);
        assert!(host.connection().is_connected());
    }

    /// The site is no longer permitted: nothing is shown, and the memory goes.
    #[test]
    fn a_silent_reconnect_that_is_refused_says_nothing() {
        let mut bridge = fake();
        bridge.remembered = Some("uuid-rabby".into());
        bridge.accounts.push_back(PollResultDetailed::Err {
            data: "not permitted".into(),
            kind: FailureKind::Declined,
        });
        let mut host = EvmHost::with_bridge(bridge);
        run(&mut host, 1);
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert_eq!(host.bridge.stored, [None]);
    }

    /// A remembered wallet that announces late is reconnected when it does.
    #[test]
    fn a_remembered_wallet_that_announces_late_is_reconnected_then() {
        let mut bridge = Fake {
            remembered: Some("uuid-rabby".into()),
            ..Fake::default()
        };
        bridge.accounts.push_back(ok(&address()));
        let mut host = EvmHost::with_bridge(bridge);
        assert!(host.bridge.asked.is_empty());
        host.bridge.wallets.push(rabby());
        run(&mut host, RELIST_EVERY as usize);
        assert_eq!(host.bridge.asked, [("uuid-rabby".to_string(), true)]);
    }

    #[test]
    fn disconnecting_lets_go_and_forgets() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 2);
        host.apply(&WalletModalAction::Disconnect(ChainFamily::Evm));
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert!(host.bridge.disconnected);
        assert_eq!(host.bridge.stored.last(), Some(&None));
    }

    #[test]
    fn a_cardano_action_is_left_alone() {
        let mut host = EvmHost::with_bridge(fake());
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Cardano,
            wallet: "eternl".into(),
        });
        assert!(host.bridge.asked.is_empty());
    }

    /// A Robinhood host whose wallet is on chain 1, with one switch answer.
    fn on_chain_one(switch: Option<PollResultDetailed>) -> EvmHost<Fake> {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        bridge.switches.extend(switch);
        EvmHost::with_bridge(bridge).expect_chain(robinhood_chain())
    }

    #[test]
    fn robinhood_is_the_verified_chain() {
        let target = robinhood_chain();
        assert_eq!(target.chain_id, 4663);
        assert_eq!(chain_name(target.chain_id), "Robinhood Chain");
        assert!(target.add.is_some(), "it can be added where it is unknown");
    }

    /// The wallet is on chain 1; the game plays on Robinhood. It is asked to
    /// switch, and the account is Robinhood's once it has.
    #[test]
    fn a_wallet_on_another_chain_is_asked_to_switch() {
        let mut host = on_chain_one(Some(ok("0x1237")));
        connect(&mut host);
        run(&mut host, 2);
        assert_eq!(host.bridge.switched_to, [4663]);
        assert!(matches!(host.phase(), ChainPhase::Connecting { .. }));
        run(&mut host, 1);
        let account = host.connection().account().expect("connected");
        assert_eq!(account.details[1].value, "Robinhood Chain");
        assert_eq!(host.bridge.stored, [Some("uuid-rabby".to_string())]);
    }

    /// The reader refuses the switch: a fault that names both chains, and
    /// nothing remembered.
    #[test]
    fn a_refused_switch_says_which_chain_is_needed() {
        let mut host = on_chain_one(Some(PollResultDetailed::Err {
            data: "User rejected the request.".into(),
            kind: FailureKind::Declined,
        }));
        connect(&mut host);
        run(&mut host, 3);
        let ChainPhase::Failed { message, .. } = host.phase() else {
            panic!("expected failed, got {:?}", host.phase());
        };
        assert!(
            message.contains("Rabby is on Ethereum Mainnet"),
            "{message}"
        );
        assert!(message.contains("Robinhood Chain"), "{message}");
        assert!(host.bridge.stored.is_empty());
    }

    /// A wallet that says it switched but is elsewhere is not taken at its
    /// word.
    #[test]
    fn a_switch_that_lands_elsewhere_is_a_fault() {
        let mut host = on_chain_one(Some(ok("0x89")));
        connect(&mut host);
        run(&mut host, 3);
        let ChainPhase::Failed { message, .. } = host.phase() else {
            panic!("expected failed, got {:?}", host.phase());
        };
        assert!(message.contains("Chain 137"), "{message}");
    }

    /// The reconnect on load never asks: a remembered wallet on the wrong
    /// chain waits for the reader.
    #[test]
    fn a_silent_reconnect_never_asks_to_switch() {
        let mut bridge = fake();
        bridge.remembered = Some("uuid-rabby".into());
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let mut host = EvmHost::with_bridge(bridge).expect_chain(robinhood_chain());
        run(&mut host, 2);
        assert!(host.bridge.switched_to.is_empty());
        assert!(matches!(host.phase(), ChainPhase::Failed { .. }));
    }

    /// Switching away in the wallet fails the connection; switching back
    /// reconnects it without the reader asking.
    #[test]
    fn switching_away_and_back_in_the_wallet() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1237"));
        bridge.changes.extend([
            EvmChanges::default(),
            EvmChanges::default(),
            EvmChanges {
                accounts: None,
                chain: Some("0x1".into()),
            },
            EvmChanges {
                accounts: None,
                chain: Some("0x1237".into()),
            },
        ]);
        let mut host = EvmHost::with_bridge(bridge).expect_chain(robinhood_chain());
        connect(&mut host);
        run(&mut host, 3);
        assert!(matches!(host.phase(), ChainPhase::Failed { .. }));
        run(&mut host, 1);
        assert!(host.connection().is_connected());
        assert_eq!(
            host.connection()
                .account()
                .map(|a| a.details[1].value.as_str()),
            Some("Robinhood Chain")
        );
    }

    /// Without a target, any chain is fine and nothing is asked to switch.
    #[test]
    fn with_no_target_any_chain_connects() {
        let mut bridge = fake();
        bridge.accounts.push_back(ok(&address()));
        bridge.chains.push_back(ok("0x1"));
        let mut host = EvmHost::with_bridge(bridge);
        connect(&mut host);
        run(&mut host, 2);
        assert!(host.connection().is_connected());
        assert!(host.bridge.switched_to.is_empty());
    }
}
