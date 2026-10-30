//! Stories for the one-button wallet connector — `macroquad_wallet_flow`'s
//! `WalletConnector` over `macroquad_gamekit::wallet_modal` — and every state
//! the flow can be in.
//!
//! Every story but the button gallery holds a real `WalletConnector`, exactly
//! as a game would. What changes is the hosts behind it. [`FakeChain`] is a
//! `ChainHost` with no bridge: it is asked, and answers a moment later through
//! the same `Connection` a real host feeds. Which answer comes back is decided
//! by the wallet, so every outcome is one click away in the live stories:
//!
//! | wallet | answers |
//! |---|---|
//! | Vespr | declines |
//! | Typhon | fails, with a sentence long enough to wrap |
//! | any other | connects |
//!
//! The accounts a connect answers with are fixtures, not live wallets: a
//! synthetic Cardano base address turned into its `addr1…` and `stake1…` by
//! the same pallas conversion a real connect uses, written out by the real
//! host's own `cardano_account`, with the estate's fixture handle; and the
//! synthetic `0x1f…` EVM address the egui stories use.
//!
//! The "real wallets" stories are the exception: their connector holds the
//! real `CardanoHost` over the CIP-30 bridge and the real `EvmHost` over the
//! EIP-6963 one, so in the web storybook they list this browser's extensions
//! on both chains with their own icons and connect them for real, both at once
//! if you like. Natively there is no browser, and they say so.
//!
//! The "stress" story lists every chain family as connectable, each with a
//! dozen made-up wallets: the most a connector will be asked to draw.
//!
//! The "sign in" stories are a site that signs in after connecting, as the
//! staking site does: Cardano only, the modal's sign-in stage set by the
//! story playing the site. In the live one a connect starts a sign-in, the
//! wallet signs after a moment — Vespr refuses, Typhon's signature is
//! rejected — and Disconnect signs out.
//!
//! The step stories open the modal settled at one state, so a capture at the
//! default moment is that state. The walkthrough scripts a whole connect on a
//! loop, so `--at` can capture any moment of any transition:
//!
//! | `--at` | what is on screen |
//! |---|---|
//! | 0.75 | the card rising in |
//! | 1.5 | the chain step, settled |
//! | 2.08 | the slide from chains to Cardano's wallets |
//! | 3.0 | Cardano's wallets, settled |
//! | 3.28 | the slide to the answer, the card shrinking |
//! | 3.7 | waiting on Eternl |
//! | 5.0 | connected, the ring drawing |
//! | 6.0 | connected, settled |
//! | 6.48 | the card leaving |
//! | 7.5 | the button, connected |

use std::cell::Cell;
use std::rc::Rc;

use macroquad::prelude::*;
use macroquad_gamekit::paint::{Gradient, fill_gradient};
use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, SignInPhase, WalletModalAction, WalletOption, wallet_button,
};
use macroquad_gamekit::{fill_rounded, stroke_rounded};
use macroquad_wallet_flow::{
    CardanoHost, ChainHost, EvmHost, Stage, WalletConnector, WalletFlow, cardano_account,
    robinhood_chain,
};
use macroquad_widgets::{Painter, TextSize};

/// Seconds a fake wallet takes to answer: long enough to see the wait.
const ANSWER_SECS: f32 = 1.6;
/// The walkthrough's loop.
const LOOP_SECS: f32 = 8.0;
/// A phone's width, in points.
const PHONE_W: f32 = 390.0;
/// Wallets per chain in the stress story.
const STRESS_WALLETS: usize = 12;

const INTRO: &str = "Connecting shares your address. Nothing is signed or sent.";
const TYPHON_FAILS: &str =
    "Typhon did not answer. Check the extension is unlocked and on mainnet, then try again.";

/// Which story this is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalView {
    /// The button in each of its states.
    Buttons,
    /// The button and the whole flow, live, against fake chains.
    Live,
    /// The flow opened at one state.
    Step(Preset),
    /// A connect scripted on a loop.
    Walkthrough,
    /// The extensions this browser has, through the real bridges.
    Real,
    /// Every chain family connectable, a dozen wallets each, opened on the
    /// chain step — or, given one, on that chain's wallets.
    Stress(Option<ChainFamily>),
    /// A site that signs in, live: Cardano only, as the staking site is.
    /// Connect, then sign in; Vespr refuses to sign and Typhon's signature is
    /// not accepted, as they do for connecting.
    SignIn,
}

/// The state a step story opens at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Chains,
    Wallets,
    NoWallets,
    Connecting,
    /// A Cardano account with its handle: the account view.
    Connected,
    /// The same account before, or without, a handle: named by its address.
    ConnectedNoHandle,
    /// An EVM account named by a placeholder where an ENS name will go.
    EvmNamed,
    Declined,
    Failed,
    /// An action only EVM can satisfy: no chain step.
    OnlyEvm,
    /// Cardano and EVM connected at once: the account view with chain tabs.
    TwoConnected,
    /// Connected on a site that signs in, and not yet signed in.
    SignInOffered,
    /// The wallet is showing its signing dialog.
    SigningIn,
    /// The reader refused to sign.
    SignInDeclined,
    /// The signature was not accepted.
    SignInFailed,
    /// Signed in.
    SignedIn,
    /// Signed in, then the wallet switched account while the page was away.
    SignInChanged,
}

/// What the walkthrough does, and when.
#[derive(Clone, Copy)]
enum Cue {
    Open,
    Chain(ChainFamily),
    Wallet(&'static str),
    Done,
}

const SCRIPT: [(f32, Cue); 4] = [
    (0.6, Cue::Open),
    (2.0, Cue::Chain(ChainFamily::Cardano)),
    (3.2, Cue::Wallet("eternl")),
    (6.4, Cue::Done),
];

/// Story seconds, shared with the fake chains so they answer on the story's
/// clock — `ChainHost::poll` has no `dt`, and a capture at `--at` must see the
/// same frame every time.
type Clock = Rc<Cell<f32>>;

/// The estate's fixture handle (`egui-widgets`' wallet fixtures use it too).
const HANDLE: &str = "$boef";

/// A synthetic EVM address, as the egui wallet fixtures use: a real one would
/// be a real wallet's.
fn evm_address() -> String {
    format!("0x{}", "1f".repeat(20))
}

/// A synthetic mainnet base address (header 0x01 over two made-up key
/// hashes), the one the flow's tests use — not a real wallet's.
fn base_address_hex() -> String {
    format!("01{}{}", "11".repeat(28), "22".repeat(28))
}

/// The Cardano account a connect produces, built by the real host's own
/// builder from the synthetic address, with or without a handle.
fn cardano_fixture(handle: Option<&str>) -> Account {
    let hex = base_address_hex();
    let address = wallet_miniquad::hex_to_bech32(&hex);
    let stake = wallet_miniquad::stake_address_bech32(&hex);
    cardano_account(&address, stake.as_deref(), 1, handle)
}

/// An EVM account. `name` stands in for an ENS name: there is no ENS resolver
/// yet, so a story passing one is showing where a resolved name will sit, and
/// the name it passes is a placeholder.
fn evm_fixture(name: Option<&str>) -> Account {
    let address = evm_address();
    let mut account = Account::new(&address);
    if let Some(name) = name {
        account = account.with_label(name).copyable("ENS name", name);
    }
    account.copyable("Address", &address)
}

/// The account a fake connect on `family` answers with. The chains with no
/// bridge yet have no address format here either, so theirs says what it is.
fn fixture(family: ChainFamily) -> Account {
    match family {
        ChainFamily::Cardano => cardano_fixture(Some(HANDLE)),
        ChainFamily::Evm => evm_fixture(None),
        other => {
            let address = format!("synthetic {other:?} address");
            Account::new(&address).copyable("Address", &address)
        }
    }
}

/// A chain with no bridge: asked, it answers [`ANSWER_SECS`] later on the
/// story's clock, through the same `Connection` a real host feeds.
struct FakeChain {
    family: ChainFamily,
    wallets: Vec<WalletOption>,
    connection: Connection,
    clock: Clock,
    /// The wallet asked, and when it answers.
    pending: Option<(String, f32)>,
    dirty: bool,
}

impl FakeChain {
    fn new(family: ChainFamily, wallets: Vec<WalletOption>, clock: &Clock) -> Self {
        Self {
            family,
            wallets,
            connection: Connection::new(),
            clock: clock.clone(),
            pending: None,
            dirty: false,
        }
    }

    /// Cardano's wallets, Eternl with `icon` when there is one.
    fn cardano(clock: &Clock, icon: Option<Texture2D>) -> Self {
        let mut eternl = WalletOption::new("eternl", "Eternl");
        if let Some(icon) = icon {
            eternl = eternl.with_icon(icon);
        }
        let wallets = vec![
            eternl,
            WalletOption::new("lace", "Lace"),
            WalletOption::new("vespr", "Vespr"),
            WalletOption::new("typhoncip30", "Typhon"),
        ];
        Self::new(ChainFamily::Cardano, wallets, clock)
    }

    fn evm(clock: &Clock) -> Self {
        let wallets = vec![
            WalletOption::new("io.rabby", "Rabby"),
            WalletOption::new("io.metamask", "MetaMask"),
        ];
        Self::new(ChainFamily::Evm, wallets, clock)
    }

    /// Start from what `events` leave behind: a preset's state.
    fn after(mut self, events: &[ConnectionEvent]) -> Self {
        for event in events {
            self.connection.handle(event);
        }
        self
    }

    /// Start connected to `wallet` as `account`.
    fn connected(self, wallet: &str, account: Account) -> Self {
        self.after(&[
            ConnectionEvent::Ask(wallet.into()),
            ConnectionEvent::Answered {
                wallet: wallet.into(),
                account,
            },
        ])
    }

    /// What `wallet` answers with: see the table in the module docs.
    fn answer(&self, wallet: String) -> ConnectionEvent {
        match wallet.as_str() {
            "vespr" => ConnectionEvent::Refused(wallet),
            "typhoncip30" => ConnectionEvent::Faulted {
                wallet,
                message: TYPHON_FAILS.into(),
            },
            _ => ConnectionEvent::Answered {
                wallet,
                account: fixture(self.family),
            },
        }
    }
}

impl ChainHost for FakeChain {
    fn family(&self) -> ChainFamily {
        self.family
    }

    fn poll(&mut self) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, due)| self.clock.get() >= *due)
            && let Some((wallet, _)) = self.pending.take()
        {
            let answer = self.answer(wallet);
            self.dirty |= self.connection.handle(&answer);
        }
        std::mem::take(&mut self.dirty)
    }

    fn option(&mut self) -> ChainOption {
        ChainOption::ready(self.family, self.wallets.clone()).with_phase(self.connection.phase())
    }

    fn apply(&mut self, action: &WalletModalAction) {
        let event = match action {
            WalletModalAction::Connect { wallet, .. } => {
                self.pending = Some((wallet.clone(), self.clock.get() + ANSWER_SECS));
                ConnectionEvent::Ask(wallet.clone())
            }
            WalletModalAction::Cancel(_) | WalletModalAction::Disconnect(_) => {
                self.pending = None;
                ConnectionEvent::Release
            }
            // The connector sends copies to the clipboard, never to a host.
            WalletModalAction::Copy(_) => return,
            // Signing in is the site's, not the chain's: the sign-in story
            // plays the site.
            WalletModalAction::SignIn(_) => return,
        };
        self.dirty |= self.connection.handle(&event);
    }

    fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// The roadmap as a site would register it today: two chains with bridges,
/// four on the way.
fn roadmap(cardano: FakeChain, evm: FakeChain) -> WalletConnector {
    WalletConnector::new()
        .host(cardano)
        .host(evm)
        .coming(ChainFamily::Solana)
        .coming(ChainFamily::Midnight)
        .coming(ChainFamily::Tezos)
        .coming(ChainFamily::Xrpl)
        .intro(INTRO)
}

/// The roadmap with the chains this browser can actually connect, through
/// the real bridges.
fn real_roadmap() -> WalletConnector {
    WalletConnector::new()
        .host(CardanoHost::new(WalletFlow::new()))
        // Robinhood Chain, the EVM chain cnft.dev-workers onboards
        // collections on: a wallet on any other is asked to switch, and
        // offered the chain to add.
        .host(EvmHost::new().expect_chain(robinhood_chain()))
        .coming(ChainFamily::Solana)
        .coming(ChainFamily::Midnight)
        .coming(ChainFamily::Tezos)
        .coming(ChainFamily::Xrpl)
        .intro("Your installed wallets. Connecting shares your address only.")
}

/// Every family connectable, [`STRESS_WALLETS`] made-up wallets each.
fn stress(clock: &Clock) -> WalletConnector {
    ChainFamily::ALL
        .into_iter()
        .fold(WalletConnector::new().intro(INTRO), |connector, family| {
            let wallets = (1..=STRESS_WALLETS)
                .map(|n| WalletOption::new(format!("test-{n}"), format!("Test wallet {n}")))
                .collect();
            connector.host(FakeChain::new(family, wallets, clock))
        })
}

/// The connector a step story opens at, settled.
fn preset(preset: Preset, clock: &Clock, icon: Option<Texture2D>) -> WalletConnector {
    use ChainFamily::{Cardano, Evm};
    let cardano = FakeChain::cardano(clock, icon);
    let evm = FakeChain::evm(clock);
    let mut connector = match preset {
        Preset::NoWallets => {
            let mut cardano = cardano;
            cardano.wallets.clear();
            roadmap(cardano, evm)
        }
        // Asked, and never answering: the wait, held.
        Preset::Connecting => roadmap(cardano.after(&[ConnectionEvent::Ask("eternl".into())]), evm),
        Preset::Connected => roadmap(
            cardano.connected("eternl", cardano_fixture(Some(HANDLE))),
            evm,
        ),
        Preset::ConnectedNoHandle => {
            roadmap(cardano.connected("eternl", cardano_fixture(None)), evm)
        }
        Preset::EvmNamed => roadmap(
            cardano,
            evm.connected("io.rabby", evm_fixture(Some("example.eth"))),
        ),
        Preset::Declined => roadmap(
            cardano.after(&[
                ConnectionEvent::Ask("vespr".into()),
                ConnectionEvent::Refused("vespr".into()),
            ]),
            evm,
        ),
        Preset::Failed => roadmap(
            cardano.after(&[
                ConnectionEvent::Ask("typhoncip30".into()),
                ConnectionEvent::Faulted {
                    wallet: "typhoncip30".into(),
                    message: TYPHON_FAILS.into(),
                },
            ]),
            evm,
        ),
        Preset::TwoConnected => roadmap(
            cardano.connected("eternl", cardano_fixture(Some(HANDLE))),
            evm.connected("io.rabby", evm_fixture(None)),
        ),
        Preset::SignInOffered
        | Preset::SigningIn
        | Preset::SignInDeclined
        | Preset::SignInFailed
        | Preset::SignedIn
        | Preset::SignInChanged => {
            let mut connector =
                signs_in(cardano.connected("eternl", cardano_fixture(Some(HANDLE))));
            let sign_in = match preset {
                Preset::SigningIn => signing_step("Eternl"),
                Preset::SignInDeclined => SignInPhase::Declined,
                Preset::SignInFailed => SignInPhase::Failed {
                    message: NOT_ACCEPTED.into(),
                },
                Preset::SignedIn => SignInPhase::SignedIn {
                    detail: Some("for 8 hours".into()),
                },
                // Synthetic stake addresses: the shape of the real thing,
                // nobody's account.
                Preset::SignInChanged => SignInPhase::Changed {
                    was: format!("stake1u9{}k4", "q".repeat(48)),
                    now: format!("stake1u8{}m7", "z".repeat(48)),
                },
                _ => SignInPhase::Offered,
            };
            connector.set_sign_in(Cardano, sign_in);
            connector
        }
        Preset::Chains | Preset::Wallets | Preset::OnlyEvm => roadmap(cardano, evm),
    };
    match preset {
        Preset::Wallets | Preset::NoWallets | Preset::Connecting => connector.open_on(Cardano),
        Preset::OnlyEvm => connector.open_on(Evm),
        Preset::Declined | Preset::Failed => {
            connector.open_on(Cardano);
            // A refusal is an answer to a wallet just asked, so the flow is
            // on that answer rather than on the list. Through the modal, not
            // the connector: the chain already holds the answer, and asking
            // again would start a new connect.
            let key = if preset == Preset::Declined {
                "vespr"
            } else {
                "typhoncip30"
            };
            let _ = connector.modal_mut().choose_wallet(key);
        }
        Preset::Chains
        | Preset::Connected
        | Preset::ConnectedNoHandle
        | Preset::EvmNamed
        | Preset::TwoConnected
        | Preset::SignInOffered
        | Preset::SigningIn
        | Preset::SignInDeclined
        | Preset::SignInFailed
        | Preset::SignedIn
        | Preset::SignInChanged => connector.open(),
    }
    connector.settle();
    connector
}

/// A Cardano-only site that signs in, as the staking site is.
fn signs_in(cardano: FakeChain) -> WalletConnector {
    WalletConnector::new()
        .host(cardano)
        .title("Sign in to staking")
        .intro(SIGN_IN_INTRO)
}

/// What the staking site says before a wallet is picked.
const SIGN_IN_INTRO: &str = "Your wallet signs a one-time message to prove it is yours. \
     No transaction is made and nothing leaves your wallet.";

/// The words while `wallet` shows its signing dialog.
fn signing_step(wallet: &str) -> SignInPhase {
    SignInPhase::Working {
        step: format!(
            "Approve the message in {wallet}. It proves the wallet is yours; nothing is sent."
        ),
        in_wallet: true,
    }
}

/// A server refusing a signature, in a site's words.
const NOT_ACCEPTED: &str = "The site could not check the signature. Try again in a moment.";

/// Seconds the sign-in story's wallet takes to sign.
const SIGN_SECS: f32 = 1.6;

/// The button gallery's rows: each a connector left in one state.
fn gallery(clock: &Clock) -> Vec<(&'static str, WalletConnector)> {
    let cardano = || FakeChain::cardano(clock, None);
    let evm = || FakeChain::evm(clock);
    let eternl = || cardano().connected("eternl", cardano_fixture(Some(HANDLE)));
    let rabby = || evm().connected("io.rabby", evm_fixture(None));
    vec![
        ("nothing connected", roadmap(cardano(), evm())),
        (
            "connecting",
            roadmap(
                cardano().after(&[ConnectionEvent::Ask("eternl".into())]),
                evm(),
            ),
        ),
        ("Cardano connected", roadmap(eternl(), evm())),
        ("EVM connected", roadmap(cardano(), rabby())),
        ("two chains connected", roadmap(eternl(), rabby())),
    ]
}

pub struct WalletModalStory {
    view: ModalView,
    /// Laid out at a phone's width, where the card is a bottom sheet.
    phone: bool,
    icon: Option<Texture2D>,
    /// Story seconds, never reset: what the fake chains answer by.
    clock: Clock,
    /// The connector, as a game holds one.
    connector: WalletConnector,
    /// The button gallery's connectors, one per state.
    gallery: Vec<(&'static str, WalletConnector)>,
    /// Seconds into the walkthrough's loop.
    t: f32,
    /// The next cue the walkthrough plays.
    cue: usize,
    /// The last thing the modal or a button reported.
    last: String,
    /// Whether the real connector has been made. It is made the first time a
    /// real story draws: making it lists the extensions and reconnects the
    /// remembered wallets, which no other story should set off at startup.
    real: bool,
    /// The sign-in story's site: a sign-in asked for and not yet started
    /// (a connect just picked), and when the wallet's signature lands.
    sign_after_connect: bool,
    signed_at: Option<f32>,
}

impl WalletModalStory {
    pub fn new(view: ModalView, phone: bool, icon: Option<Texture2D>) -> Self {
        let clock = Clock::default();
        let connector = match view {
            ModalView::Step(at) => preset(at, &clock, icon.clone()),
            ModalView::Stress(on) => {
                let mut connector = stress(&clock);
                match on {
                    Some(family) => connector.open_on(family),
                    None => connector.open(),
                }
                connector.settle();
                connector
            }
            // Made on first draw, not here.
            ModalView::Real | ModalView::Buttons => WalletConnector::new(),
            ModalView::SignIn => signs_in(FakeChain::cardano(&clock, icon.clone())),
            ModalView::Live | ModalView::Walkthrough => roadmap(
                FakeChain::cardano(&clock, icon.clone()),
                FakeChain::evm(&clock),
            ),
        };
        let gallery = if view == ModalView::Buttons {
            gallery(&clock)
        } else {
            Vec::new()
        };
        Self {
            view,
            phone,
            icon,
            clock,
            connector,
            gallery,
            t: 0.0,
            cue: 0,
            last: String::new(),
            real: false,
            sign_after_connect: false,
            signed_at: None,
        }
    }

    /// The sign-in story's site, after the modal reported `action`: a connect
    /// starts a sign-in once the wallet is connected, "Sign in" and "Try
    /// again" start one at once, and the wallet's answer lands after
    /// [`SIGN_SECS`] — refused by Vespr, rejected for Typhon, else signed in.
    fn play_site(&mut self, action: Option<&WalletModalAction>) {
        use ChainFamily::Cardano;
        let now = self.clock.get();
        let connected = self
            .connector
            .vm()
            .connected()
            .next()
            .map(|(_, wallet, _)| wallet.to_string());
        match action {
            Some(WalletModalAction::Connect { .. }) => self.sign_after_connect = true,
            Some(WalletModalAction::SignIn(_)) => self.signed_at = Some(now + SIGN_SECS),
            Some(WalletModalAction::Disconnect(_) | WalletModalAction::Cancel(_)) => {
                self.sign_after_connect = false;
                self.signed_at = None;
            }
            _ => {}
        }
        let Some(wallet) = connected else {
            self.connector.set_sign_in(Cardano, SignInPhase::NotNeeded);
            return;
        };
        if std::mem::take(&mut self.sign_after_connect) {
            self.signed_at = Some(now + SIGN_SECS);
        }
        let name = self
            .connector
            .vm()
            .chain(Cardano)
            .map_or(wallet.clone(), |c| c.wallet_name(&wallet).to_string());
        let sign_in = match self.signed_at {
            Some(at) if now < at => signing_step(&name),
            Some(_) => {
                self.signed_at = None;
                match wallet.as_str() {
                    "vespr" => SignInPhase::Declined,
                    "typhoncip30" => SignInPhase::Failed {
                        message: NOT_ACCEPTED.into(),
                    },
                    _ => SignInPhase::SignedIn {
                        detail: Some("for 8 hours".into()),
                    },
                }
            }
            None => match &self
                .connector
                .vm()
                .chain(Cardano)
                .map(|c| c.sign_in.clone())
            {
                Some(SignInPhase::NotNeeded) | None => SignInPhase::Offered,
                Some(kept) => kept.clone(),
            },
        };
        self.connector.set_sign_in(Cardano, sign_in);
    }

    pub fn draw(&mut self, p: &Painter, area: Rect, dt: f32) {
        let stage = if self.phone {
            Rect::new(area.x, area.y, PHONE_W.min(area.w), area.h.min(760.0))
        } else {
            area
        };
        backdrop(p, stage);
        match self.view {
            ModalView::Buttons => self.buttons(p, stage),
            _ => self.flow(p, stage, dt),
        }
        if !self.last.is_empty() {
            p.mono(
                &self.last,
                stage.x,
                stage.bottom() + 16.0,
                p.size(TextSize::Md),
                p.theme.color.accent,
            );
        }
    }

    fn flow(&mut self, p: &Painter, stage: Rect, dt: f32) {
        self.clock.set(self.clock.get() + dt);
        if self.view == ModalView::Walkthrough {
            self.play(dt);
        }
        if self.view == ModalView::Real {
            if !self.real {
                self.connector = real_roadmap();
                self.real = true;
            }
            let unavailable = self
                .connector
                .host_ref::<CardanoHost>()
                .is_some_and(|host| matches!(host.flow().stage(), Stage::Unavailable));
            if unavailable {
                p.text(
                    "Real wallets need a browser: build the web storybook (see its Cargo.toml).",
                    stage.x + 16.0,
                    stage.bottom() - 16.0,
                    p.size(TextSize::Md),
                    p.theme.color.warning,
                );
            }
        }

        let w = if self.phone { 180.0 } else { 220.0 };
        let button = Rect::new(stage.right() - w - 16.0, stage.y + 16.0, w, 44.0);
        if self.connector.button(p, button) {
            self.last = "button: open".into();
        }
        let action = self.connector.show(p, stage, dt);
        if let Some(action) = &action {
            self.last = format!("action: {action:?}");
        }
        if self.view == ModalView::SignIn {
            self.play_site(action.as_ref());
        }
    }

    /// The walkthrough's script, on a loop.
    fn play(&mut self, dt: f32) {
        self.t += dt;
        if self.t >= LOOP_SECS {
            self.t -= LOOP_SECS;
            self.cue = 0;
            self.connector = roadmap(
                FakeChain::cardano(&self.clock, self.icon.clone()),
                FakeChain::evm(&self.clock),
            );
        }
        while let Some(&(at, cue)) = SCRIPT.get(self.cue) {
            if self.t < at {
                break;
            }
            self.cue += 1;
            match cue {
                Cue::Open => self.connector.open(),
                Cue::Chain(family) => self.connector.choose_chain(family),
                Cue::Wallet(key) => self.connector.choose_wallet(key),
                Cue::Done => self.connector.close(),
            }
        }
    }

    /// The button in each state it can be in, side by side, each live to hover.
    fn buttons(&mut self, p: &Painter, stage: Rect) {
        let c = p.theme.color;
        // Below the backdrop's "your game" plate.
        let x = stage.x + 24.0;
        let mut y = stage.y + 84.0;
        for (label, connector) in &self.gallery {
            p.text_mid(label, x, y + 22.0, p.size(TextSize::Md), c.text_muted);
            for (i, w) in [220.0, 170.0].into_iter().enumerate() {
                let bx = x + 170.0 + i as f32 * 240.0;
                if bx + w > stage.right() - 8.0 {
                    break;
                }
                if wallet_button(p, connector.vm(), Rect::new(bx, y, w, 44.0)) {
                    self.last = format!("tapped: {label}");
                }
            }
            y += 64.0;
        }
        p.text(
            "the right column is the same button at a phone's width",
            x,
            y + 8.0,
            p.size(TextSize::Md),
            c.text_muted,
        );
    }
}

/// A stand-in game screen, so the modal is judged over something.
fn backdrop(p: &Painter, stage: Rect) {
    let c = p.theme.color;
    fill_gradient(
        stage,
        12.0,
        Gradient::Vertical {
            top: c.bg_secondary,
            bottom: c.bg_primary,
        },
    );
    let step = 32.0;
    let ink = Color::new(c.border.r, c.border.g, c.border.b, 0.35);
    let mut gx = stage.x + step;
    while gx < stage.right() {
        draw_line(gx, stage.y, gx, stage.bottom(), 1.0, ink);
        gx += step;
    }
    let mut gy = stage.y + step;
    while gy < stage.bottom() {
        draw_line(stage.x, gy, stage.right(), gy, 1.0, ink);
        gy += step;
    }
    let plate = Rect::new(stage.x + 16.0, stage.y + 16.0, 150.0, 44.0);
    fill_rounded(plate, 10.0, c.bg_highlight);
    p.text_mid(
        "YOUR GAME",
        plate.x + 14.0,
        plate.center().y,
        p.size(TextSize::Lg),
        c.text_secondary,
    );
    stroke_rounded(stage, 12.0, 1.0, c.border);
}
