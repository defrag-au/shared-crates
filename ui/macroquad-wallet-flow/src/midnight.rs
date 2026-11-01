//! [`MidnightHost`] — the browser's Midnight wallets, as a [`ChainHost`] for the
//! [`crate::WalletConnector`]: the counterpart of [`crate::EvmHost`], over
//! `wallet_miniquad::midnight`.
//!
//! The same shape as the other two hosts, which is the point of the connection
//! machine: wallets listed, the bridge's outcomes fed to a
//! `macroquad_gamekit::wallet_connection::Connection` as events, and the modal's
//! phase read back from it. Only the mechanics are Midnight's own:
//!
//! - **Discovery is `window.midnight`.** Wallets install an `InitialAPI` there
//!   under a uuid they generate, with no announcement event — so the list is
//!   re-read while the modal runs, and a wallet that installed itself before
//!   the plugin loaded is still found.
//! - **A connect is three reads**, all inside the plugin: `connect("mainnet")`,
//!   the connection's own status (read back, not assumed — a wallet that
//!   connected elsewhere is refused), and the unshielded address, which is the
//!   account a link is stored against.
//! - **Nothing is remembered between loads.** The connector has no silent
//!   connect — every `connect` may put a dialog in front of the reader — so
//!   reconnecting a remembered wallet on page load would be an ambush, which is
//!   the rule the EVM host already applies to its own reconnect. The reader
//!   picks a wallet each time.
//! - **The wallet cannot speak first.** The connector defines no account or
//!   network event, so there is nothing to recheck; a Midnight wallet's account
//!   changes only through its own UI, and the next connect reads it again.
//!   [`ChainHost::recheck`] keeps its default, which does nothing.
//!
//! ## The account
//!
//! The unshielded address (`mn_addr…`), to copy, under the label the link
//! proves — see [`MIDNIGHT_DETAIL`] — and the network, which is mainnet by
//! construction because the plugin refuses a connection on any other.

use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, ChainPhase, WalletModalAction, WalletOption,
};
use wallet_miniquad::midnight::{self, MidnightWallet};
use wallet_miniquad::{PollResultDetailed, ReqId};

use crate::ChainHost;
use crate::cardano::RELIST_EVERY;

/// The label the modal gives the address a Midnight link proves, and the one
/// [`crate::`]'s consumer reads the claim from. The account is named by its
/// address too, so this is the line that says *which* address it is — a
/// Midnight wallet can hold several, and only the unshielded one is linkable.
pub const MIDNIGHT_DETAIL: &str = "Unshielded address";

/// What the host needs from a Midnight bridge. [`MiniquadMidnightBridge`] is
/// the real one; tests drive a fake through every step.
pub trait MidnightBridge {
    /// Whether a browser wallet can exist here. Native builds answer `false`.
    fn available(&self) -> bool {
        wallet_miniquad::AVAILABLE
    }
    fn list_wallets(&self) -> Vec<MidnightWallet>;
    /// Connect `id` on mainnet; on ok, its unshielded address.
    fn connect(&mut self, id: &str) -> ReqId;
    fn disconnect(&mut self);
    fn poll(&mut self, req: ReqId) -> PollResultDetailed;
    fn cancel(&mut self, req: ReqId);
}

/// The production bridge: the miniquad `midnight` plugin.
#[derive(Default)]
pub struct MiniquadMidnightBridge;

impl MidnightBridge for MiniquadMidnightBridge {
    fn list_wallets(&self) -> Vec<MidnightWallet> {
        midnight::list_wallets()
    }
    fn connect(&mut self, id: &str) -> ReqId {
        midnight::connect(id)
    }
    fn disconnect(&mut self) {
        midnight::disconnect()
    }
    fn poll(&mut self, req: ReqId) -> PollResultDetailed {
        midnight::poll(req)
    }
    fn cancel(&mut self, req: ReqId) {
        midnight::cancel(req)
    }
}

/// The read in flight.
enum Step {
    Idle,
    /// The connect, whose answer is the unshielded address.
    Connect {
        req: ReqId,
    },
}

/// See the [module docs](self).
pub struct MidnightHost<B: MidnightBridge = MiniquadMidnightBridge> {
    bridge: B,
    wallets: Vec<MidnightWallet>,
    connection: Connection,
    step: Step,
    /// The connected account, as the wallet last said.
    address: Option<String>,
    since_list: u32,
    /// The listing needs writing again.
    dirty: bool,
}

impl MidnightHost<MiniquadMidnightBridge> {
    pub fn new() -> Self {
        Self::with_bridge(MiniquadMidnightBridge)
    }
}

impl Default for MidnightHost<MiniquadMidnightBridge> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: MidnightBridge> MidnightHost<B> {
    /// A host over `bridge`, listing whatever is installed already.
    pub fn with_bridge(bridge: B) -> Self {
        let wallets = if bridge.available() {
            bridge.list_wallets()
        } else {
            Vec::new()
        };
        Self {
            bridge,
            wallets,
            connection: Connection::new(),
            step: Step::Idle,
            address: None,
            since_list: 0,
            dirty: true,
        }
    }

    /// Advance the read and the wallet list.
    fn advance(&mut self) {
        self.since_list += 1;
        if self.since_list >= RELIST_EVERY {
            self.since_list = 0;
            self.relist();
        }
        self.poll_step();
    }

    /// Re-read the installed wallets. A wallet may install itself after the
    /// page loaded, and there is no event to say so. Marks the listing stale
    /// only when the list actually changed.
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

    /// What the modal asked of Midnight. Another chain's action is not this
    /// host's; the connector never passes one, and it is ignored if it does.
    fn act(&mut self, action: &WalletModalAction) {
        match action {
            WalletModalAction::Connect {
                family: ChainFamily::Midnight,
                wallet,
            } => {
                self.drop_step();
                self.feed(&ConnectionEvent::Ask(wallet.clone()));
                let req = self.bridge.connect(wallet);
                self.step = Step::Connect { req };
            }
            WalletModalAction::Cancel(ChainFamily::Midnight) => {
                self.drop_step();
                self.feed(&ConnectionEvent::Release);
            }
            WalletModalAction::Disconnect(ChainFamily::Midnight) => {
                self.drop_step();
                self.bridge.disconnect();
                self.address = None;
                self.feed(&ConnectionEvent::Release);
            }
            _ => {}
        }
    }

    /// Midnight as the modal lists it: the installed wallets, and the
    /// connection's phase.
    fn listing(&mut self) -> ChainOption {
        self.dirty = false;
        let options = self
            .wallets
            .iter()
            .map(|wallet| WalletOption::new(wallet.id.clone(), wallet.name.clone()))
            .collect();
        ChainOption::ready(ChainFamily::Midnight, options).with_phase(self.phase())
    }

    pub fn phase(&self) -> ChainPhase {
        self.connection.phase()
    }

    fn drop_step(&mut self) {
        match std::mem::replace(&mut self.step, Step::Idle) {
            Step::Connect { req } => self.bridge.cancel(req),
            Step::Idle => {}
        }
    }

    fn poll_step(&mut self) {
        let Some(wallet) = self.connection.wallet().map(str::to_string) else {
            return;
        };
        let Step::Connect { req } = self.step else {
            return;
        };
        match self.bridge.poll(req) {
            PollResultDetailed::Pending => {}
            PollResultDetailed::Ok { data } => {
                self.step = Step::Idle;
                self.address = Some(data);
                if let Some(account) = self.account() {
                    self.feed(&ConnectionEvent::Answered { wallet, account });
                }
            }
            PollResultDetailed::Err { data, kind } => {
                self.step = Step::Idle;
                self.feed(&match kind {
                    wallet_miniquad::FailureKind::Declined => ConnectionEvent::Refused(wallet),
                    wallet_miniquad::FailureKind::Failed => ConnectionEvent::Faulted {
                        wallet,
                        message: data,
                    },
                });
            }
        }
    }

    fn account(&self) -> Option<Account> {
        Some(midnight_account(self.address.as_deref()?))
    }
}

impl<B: MidnightBridge + 'static> ChainHost for MidnightHost<B> {
    fn family(&self) -> ChainFamily {
        ChainFamily::Midnight
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

/// A Midnight account as the modal shows it: the unshielded address to copy,
/// under the label the link proves, and the network the plugin insisted on.
pub fn midnight_account(address: &str) -> Account {
    Account::new(address)
        .copyable(MIDNIGHT_DETAIL, address)
        // True by construction: `midnight.js` refuses a connection whose own
        // status reports any other network, so an account here is on mainnet.
        .fact("Network", "Mainnet")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallet_miniquad::FailureKind;

    /// The connector specification's own `makeTransfer` example address.
    const UNSHIELDED: &str = "mn_addr1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9s6e0fs3";

    struct Fake {
        wallets: Vec<MidnightWallet>,
        answers: Vec<PollResultDetailed>,
        next: usize,
        asked: Vec<String>,
        disconnected: bool,
        cancelled: Vec<ReqId>,
    }

    impl Fake {
        fn with(answers: Vec<PollResultDetailed>) -> Self {
            Self {
                wallets: vec![wallet()],
                answers,
                next: 0,
                asked: Vec::new(),
                disconnected: false,
                cancelled: Vec::new(),
            }
        }
        /// The id handed out for the next call.
        fn issue(&mut self) -> ReqId {
            self.next += 1;
            ReqId(self.next as i32)
        }
    }

    impl MidnightBridge for Fake {
        fn available(&self) -> bool {
            true
        }
        fn list_wallets(&self) -> Vec<MidnightWallet> {
            self.wallets.clone()
        }
        fn connect(&mut self, id: &str) -> ReqId {
            self.asked.push(id.to_string());
            self.issue()
        }
        fn disconnect(&mut self) {
            self.disconnected = true;
        }
        fn poll(&mut self, _req: ReqId) -> PollResultDetailed {
            if self.answers.is_empty() {
                PollResultDetailed::Pending
            } else {
                self.answers.remove(0)
            }
        }
        fn cancel(&mut self, req: ReqId) {
            self.cancelled.push(req);
        }
    }

    fn wallet() -> MidnightWallet {
        MidnightWallet {
            id: "uuid-1".into(),
            name: "1AM".into(),
            rdns: "com.example.wallet".into(),
            api_version: "4.0.1".into(),
        }
    }

    fn ok(address: &str) -> PollResultDetailed {
        PollResultDetailed::Ok {
            data: address.into(),
        }
    }

    fn fake(answers: Vec<PollResultDetailed>) -> MidnightHost<Fake> {
        MidnightHost::with_bridge(Fake::with(answers))
    }

    fn pick(host: &mut MidnightHost<Fake>, id: &str) {
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Midnight,
            wallet: id.into(),
        });
    }

    fn run(host: &mut MidnightHost<Fake>) {
        for _ in 0..3 {
            host.poll();
        }
    }

    #[test]
    fn an_idle_host_reports_no_change() {
        let mut host = fake(vec![]);
        host.poll();
        let _ = host.option();
        for frame in 0..(RELIST_EVERY as usize * 3) {
            assert!(!host.poll(), "frame {frame}: nothing moved");
        }
        assert_eq!(host.phase(), ChainPhase::Idle);
    }

    #[test]
    fn installed_wallets_are_listed() {
        let mut host = fake(vec![]);
        assert!(host.poll(), "the first listing is a change");
        let option = host.option();
        assert_eq!(option.family, ChainFamily::Midnight);
        assert_eq!(option.wallets.len(), 1);
        assert_eq!(option.wallets[0].key, "uuid-1");
        assert_eq!(option.wallets[0].name, "1AM");
    }

    #[test]
    fn a_connect_reads_the_unshielded_address_as_the_account() {
        let mut host = fake(vec![ok(UNSHIELDED)]);
        pick(&mut host, "uuid-1");
        assert!(matches!(host.phase(), ChainPhase::Connecting { .. }));
        run(&mut host);

        assert!(matches!(host.phase(), ChainPhase::Connected { .. }));
        let account = host.connection.account().expect("an account");
        assert_eq!(account.address, UNSHIELDED);
        assert_eq!(
            account
                .details
                .iter()
                .find(|d| d.label == MIDNIGHT_DETAIL)
                .map(|d| d.value.as_str()),
            Some(UNSHIELDED),
            "the detail the link's claim is read from"
        );
    }

    #[test]
    fn a_refusal_reads_as_declined() {
        let mut host = fake(vec![PollResultDetailed::Err {
            data: "User rejected".into(),
            kind: FailureKind::Declined,
        }]);
        pick(&mut host, "uuid-1");
        run(&mut host);
        assert!(matches!(host.phase(), ChainPhase::Declined { .. }));
    }

    #[test]
    fn a_fault_says_what_the_wallet_said() {
        let mut host = fake(vec![PollResultDetailed::Err {
            data: "The wallet connected to preview".into(),
            kind: FailureKind::Failed,
        }]);
        pick(&mut host, "uuid-1");
        run(&mut host);
        assert!(matches!(
            host.phase(),
            ChainPhase::Failed { ref message, .. } if message.contains("preview")
        ));
    }

    #[test]
    fn cancelling_drops_the_read_and_lets_go() {
        let mut host = fake(vec![]);
        pick(&mut host, "uuid-1");
        host.apply(&WalletModalAction::Cancel(ChainFamily::Midnight));
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert_eq!(host.bridge.cancelled.len(), 1, "the request was dropped");
        assert!(!host.bridge.asked.is_empty());
    }

    #[test]
    fn disconnecting_lets_go_and_forgets_the_account() {
        let mut host = fake(vec![ok(UNSHIELDED)]);
        pick(&mut host, "uuid-1");
        run(&mut host);
        assert!(host.connection.is_connected());

        host.apply(&WalletModalAction::Disconnect(ChainFamily::Midnight));
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert!(host.bridge.disconnected);
        assert!(host.connection.account().is_none());
    }

    #[test]
    fn another_chains_action_is_left_alone() {
        let mut host = fake(vec![]);
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "rabby".into(),
        });
        assert!(host.bridge.asked.is_empty());
    }

    /// Nothing is remembered between loads: the connector has no silent
    /// connect, so a reconnect on load would put a dialog in front of a reader
    /// who asked for nothing.
    #[test]
    fn a_load_asks_nothing() {
        let mut host = fake(vec![ok(UNSHIELDED)]);
        run(&mut host);
        assert!(host.bridge.asked.is_empty());
        assert_eq!(host.phase(), ChainPhase::Idle);
    }
}
