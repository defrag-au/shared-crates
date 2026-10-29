//! [`WalletConnector`] — every chain a site connects, behind one wallet button
//! and one modal: the thing a game holds.
//!
//! ```ignore
//! let mut wallets = WalletConnector::new()
//!     .host(CardanoHost::new(WalletFlow::new().expect_network(1)))
//!     .host(EvmHost::new().expect_chain(robinhood_chain()))
//!     .coming(ChainFamily::Solana);
//!
//! // every frame:
//! wallets.button(&p, button_rect);   // the one control on the screen
//! wallets.show(&p, screen, dt);      // the modal, when it is open
//! ```
//!
//! A Cardano-only site registers one host and nothing else: the modal then
//! has no chain step, no roadmap tiles, and no EVM code in the page.
//!
//! ## The parts
//!
//! - **A [`ChainHost`] per chain.** It owns the chain's bridge and its
//!   `Connection`, and answers four questions: which chain it is, whether
//!   anything changed this frame, what the modal should list for it, and what
//!   to do with an action for it. [`crate::CardanoHost`] and
//!   [`crate::EvmHost`] are the two today; a new chain is one more
//!   implementation and one more `.host(…)`.
//! - **The connector** owns the hosts, the modal and the view model. Each
//!   frame it polls every host, and rewrites a chain's listing only when that
//!   host says it changed — so an idle frame builds nothing. An action goes to
//!   the one host whose chain it names; a copy, which is no chain's, goes to
//!   the clipboard once.
//!
//! ## What it does not own
//!
//! The chains themselves. Each host is independent — its own connection, its
//! own lifecycle — so Cardano and EVM can be connected at once, and
//! [`WalletConnector::connected`] lists every connected account for a surface
//! that links them.

use std::any::Any;

use macroquad::prelude::Rect;
use macroquad_gamekit::wallet_connection::Connection;
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, WalletModal, WalletModalAction, WalletModalVm, wallet_button,
};
use macroquad_widgets::Painter;

/// Put `text` on the clipboard: the page's on the web, miniquad's natively.
pub fn copy_text(text: &str) {
    if !miniquad_platform::clipboard_write(text) {
        macroquad::miniquad::window::clipboard_set(text);
    }
}

/// One chain's wallets and connection, as the [`WalletConnector`] drives them.
///
/// `Any` so a site can reach a host's own reads through
/// [`WalletConnector::host`] — the Cardano flow's change address for a
/// transaction builder, say.
pub trait ChainHost: Any {
    /// The chain this host connects. One host per chain.
    fn family(&self) -> ChainFamily;

    /// Advance one frame: the bridge's answers, the wallet's own reports, the
    /// icons. `true` when what the modal lists for this chain has changed —
    /// and only then is [`ChainHost::option`] read.
    fn poll(&mut self) -> bool;

    /// The chain as the modal lists it: its wallets, their icons, and the
    /// connection's phase.
    fn option(&mut self) -> ChainOption;

    /// Act on an action for this host's chain. The connector never passes
    /// another chain's, nor a copy.
    fn apply(&mut self, action: &WalletModalAction);

    /// The connection, for a site's reads: the wallet, the account.
    fn connection(&self) -> &Connection;
}

/// See the [module docs](self).
pub struct WalletConnector {
    modal: WalletModal,
    hosts: Vec<Box<dyn ChainHost>>,
    /// What the modal and the button draw; each chain's entry rewritten only
    /// when its host says it changed.
    vm: WalletModalVm,
    /// Where a copy goes: [`copy_text`] unless the site says otherwise.
    clipboard: fn(&str),
}

impl Default for WalletConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletConnector {
    /// No chains yet: add them with [`Self::host`].
    pub fn new() -> Self {
        Self {
            modal: WalletModal::new(),
            hosts: Vec::new(),
            vm: WalletModalVm::new(Vec::new()),
            clipboard: copy_text,
        }
    }

    /// Send copies somewhere other than [`copy_text`] — the page's clipboard
    /// on the web, miniquad's natively. For a host with its own clipboard
    /// route, and for tests, which have no window to copy into.
    pub fn clipboard(mut self, copy: fn(&str)) -> Self {
        self.clipboard = copy;
        self
    }

    /// Connect a chain through `host`, listed after the chains already added.
    /// A second host for a chain already added replaces the first.
    pub fn host(mut self, mut host: impl ChainHost) -> Self {
        let family = host.family();
        let option = host.option();
        match self.vm.chains.iter().position(|c| c.family == family) {
            Some(at) => self.vm.chains[at] = option,
            None => self.vm.chains.push(option),
        }
        self.hosts.retain(|h| h.family() != family);
        self.hosts.push(Box::new(host));
        self
    }

    /// List `family` as on its way: shown, dimmed, not tappable.
    pub fn coming(mut self, family: ChainFamily) -> Self {
        if self.vm.chain(family).is_none() {
            self.vm.chains.push(ChainOption::soon(family));
        }
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.vm.title = title.into();
        self
    }

    /// Up to two lines above the chain tiles: what connecting does and does
    /// not do.
    pub fn intro(mut self, intro: impl Into<String>) -> Self {
        self.vm.intro = Some(intro.into());
        self
    }

    /// Draw the wallet button, and open the modal when it is tapped. `true`
    /// on that tap.
    ///
    /// Takes no tap while the modal is up: a tap meant for the scrim must not
    /// also press the button under it.
    pub fn button(&mut self, p: &Painter, rect: Rect) -> bool {
        let tapped = if self.modal.is_visible() {
            let behind = Painter::new(p.font, p.mono, p.theme, None).at_time(p.time());
            wallet_button(&behind, &self.vm, rect)
        } else {
            wallet_button(p, &self.vm, rect)
        };
        if tapped {
            self.modal.open();
        }
        tapped
    }

    /// Advance every chain, then draw the modal when it is open and act on
    /// what the reader asked for. Call every frame, after
    /// [`Self::button`]. Returns the action, for a site that wants to see it;
    /// it has already been applied.
    pub fn show(&mut self, p: &Painter, area: Rect, dt: f32) -> Option<WalletModalAction> {
        self.poll();
        let action = self.modal.show(p, &self.vm, area, dt)?;
        self.apply(&action);
        Some(action)
    }

    /// Advance every chain without drawing: what [`Self::show`] does first.
    /// For a frame where the modal is not drawn, and for tests.
    pub fn poll(&mut self) {
        for host in &mut self.hosts {
            if host.poll() {
                let option = host.option();
                if let Some(entry) = self
                    .vm
                    .chains
                    .iter_mut()
                    .find(|c| c.family == option.family)
                {
                    *entry = option;
                }
            }
        }
    }

    /// Act on `action`: to the host whose chain it names, or — a copy — to the
    /// clipboard. [`Self::show`] does this with what the modal reports; a site
    /// driving the flow without a pointer calls it with its own.
    pub fn apply(&mut self, action: &WalletModalAction) {
        match action.family() {
            Some(family) => {
                if let Some(host) = self.hosts.iter_mut().find(|h| h.family() == family) {
                    host.apply(action);
                }
            }
            None => {
                if let WalletModalAction::Copy(text) = action {
                    (self.clipboard)(text);
                }
            }
        }
    }

    /// Open the modal: on the account of a connected chain, on the one chain
    /// when only one is listed, else on the chain step.
    pub fn open(&mut self) {
        self.modal.open();
    }

    /// Open on `family`'s wallets, for an action only that chain can do.
    pub fn open_on(&mut self, family: ChainFamily) {
        self.modal.open_on(family);
    }

    pub fn close(&mut self) {
        self.modal.close();
    }

    /// One step back; a host's Escape key or a pad's B.
    pub fn back(&mut self) {
        self.modal.back();
    }

    /// Whether any of the modal is on screen: while it is, the screen behind
    /// should take no input.
    pub fn is_visible(&self) -> bool {
        self.modal.is_visible()
    }

    /// Pick `family` on the chain step, as a tap does: for a pad or a script.
    pub fn choose_chain(&mut self, family: ChainFamily) {
        self.modal.choose_chain(&self.vm, family);
    }

    /// Pick wallet `key` on the chain in focus, as a tap does, and apply the
    /// connect it asks for.
    pub fn choose_wallet(&mut self, key: impl Into<String>) {
        if let Some(action) = self.modal.choose_wallet(key) {
            self.apply(&action);
        }
    }

    /// Finish every transition at once, so the next frame draws the modal
    /// settled where it is: for a story or a test that opens on one state.
    pub fn settle(&mut self) {
        self.modal = std::mem::take(&mut self.modal).settled();
    }

    /// The modal, for anything the connector does not pass through.
    pub fn modal_mut(&mut self) -> &mut WalletModal {
        &mut self.modal
    }

    /// What the modal and the button draw.
    pub fn vm(&self) -> &WalletModalVm {
        &self.vm
    }

    /// `family`'s connection, when a host connects it.
    pub fn connection(&self, family: ChainFamily) -> Option<&Connection> {
        self.hosts
            .iter()
            .find(|h| h.family() == family)
            .map(|h| h.connection())
    }

    /// Every connected account, in the order the chains are listed: the set a
    /// surface linking wallets across chains starts from.
    pub fn connected(&self) -> impl Iterator<Item = (ChainFamily, &str, &Account)> {
        self.vm.connected()
    }

    /// The host of type `T`, for its own reads.
    pub fn host_ref<T: ChainHost>(&self) -> Option<&T> {
        self.hosts.iter().find_map(|h| {
            let any: &dyn Any = h.as_ref();
            any.downcast_ref::<T>()
        })
    }

    /// The host of type `T`, mutably.
    pub fn host_mut<T: ChainHost>(&mut self) -> Option<&mut T> {
        self.hosts.iter_mut().find_map(|h| {
            let any: &mut dyn Any = h.as_mut();
            any.downcast_mut::<T>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad_gamekit::wallet_connection::ConnectionEvent;
    use macroquad_gamekit::wallet_modal::{ChainPhase, WalletOption};

    /// A host with no bridge: it counts how often it is asked for its listing,
    /// and changes only when told to.
    struct Counted {
        family: ChainFamily,
        connection: Connection,
        dirty: bool,
        listed: usize,
        applied: Vec<WalletModalAction>,
    }

    impl Counted {
        fn new(family: ChainFamily) -> Self {
            Self {
                family,
                connection: Connection::new(),
                dirty: false,
                listed: 0,
                applied: Vec::new(),
            }
        }
    }

    impl ChainHost for Counted {
        fn family(&self) -> ChainFamily {
            self.family
        }
        fn poll(&mut self) -> bool {
            std::mem::take(&mut self.dirty)
        }
        fn option(&mut self) -> ChainOption {
            self.listed += 1;
            ChainOption::ready(self.family, vec![WalletOption::new("w", "Wallet")])
                .with_phase(self.connection.phase())
        }
        fn apply(&mut self, action: &WalletModalAction) {
            self.applied.push(action.clone());
            if let WalletModalAction::Connect { wallet, .. } = action {
                self.dirty |= self
                    .connection
                    .handle(&ConnectionEvent::Ask(wallet.clone()));
            }
        }
        fn connection(&self) -> &Connection {
            &self.connection
        }
    }

    fn no_clipboard(_: &str) {}

    fn two_chains() -> WalletConnector {
        WalletConnector::new()
            .clipboard(no_clipboard)
            .host(Counted::new(ChainFamily::Cardano))
            .host(Counted::new(ChainFamily::Evm))
            .coming(ChainFamily::Solana)
    }

    #[test]
    fn chains_are_listed_in_the_order_they_are_added() {
        let wallets = two_chains();
        let families: Vec<ChainFamily> = wallets.vm().chains.iter().map(|c| c.family).collect();
        assert_eq!(
            families,
            [ChainFamily::Cardano, ChainFamily::Evm, ChainFamily::Solana]
        );
    }

    /// The point of the connector: a frame where nothing changed builds
    /// nothing.
    #[test]
    fn an_idle_frame_rewrites_no_listing() {
        let mut wallets = two_chains();
        for _ in 0..100 {
            wallets.poll();
        }
        let cardano = wallets.host_ref::<Counted>().expect("a Counted host");
        assert_eq!(cardano.listed, 1, "listed once, on registration");
    }

    #[test]
    fn an_action_reaches_only_its_chain() {
        let mut wallets = two_chains();
        wallets.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "w".into(),
        });
        let applied: Vec<(ChainFamily, usize)> = wallets
            .hosts
            .iter()
            .map(|h| {
                let any: &dyn Any = h.as_ref();
                let host = any.downcast_ref::<Counted>().expect("Counted");
                (host.family, host.applied.len())
            })
            .collect();
        assert_eq!(applied, [(ChainFamily::Cardano, 0), (ChainFamily::Evm, 1)]);
    }

    /// A change reaches the view model on the next poll, and only that chain's
    /// listing is rewritten.
    #[test]
    fn a_change_rewrites_only_its_chain() {
        let mut wallets = two_chains();
        wallets.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "w".into(),
        });
        wallets.poll();
        assert_eq!(
            wallets
                .vm()
                .chain(ChainFamily::Evm)
                .map(|c| c.phase.clone()),
            Some(ChainPhase::Connecting { wallet: "w".into() })
        );
        let listed: Vec<usize> = wallets
            .hosts
            .iter()
            .map(|h| {
                let any: &dyn Any = h.as_ref();
                any.downcast_ref::<Counted>().expect("Counted").listed
            })
            .collect();
        assert_eq!(listed, [1, 2]);
    }

    /// A copy is no chain's: no host sees it.
    #[test]
    fn a_copy_reaches_no_host() {
        let mut wallets = two_chains();
        wallets.apply(&WalletModalAction::Copy(String::new()));
        assert!(wallets.hosts.iter().all(|h| {
            (h.as_ref() as &dyn Any)
                .downcast_ref::<Counted>()
                .is_some_and(|c| c.applied.is_empty())
        }));
    }

    /// A second host for a chain replaces the first rather than listing the
    /// chain twice.
    #[test]
    fn one_host_per_chain() {
        let wallets = WalletConnector::new()
            .host(Counted::new(ChainFamily::Cardano))
            .host(Counted::new(ChainFamily::Cardano));
        assert_eq!(wallets.vm().chains.len(), 1);
        assert_eq!(wallets.hosts.len(), 1);
    }
}
