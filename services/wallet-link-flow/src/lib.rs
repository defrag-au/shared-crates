//! `wallet-link-flow` — linking wallets to a Discord account through
//! auth-service, as a pure state machine.
//!
//! The machine decides; it does nothing. Each [`Event`] it is handed (a
//! reply arrived, the wallet signed, the reader tapped Link) returns the
//! [`Effect`]s owed: a call to make, a message for the wallet to sign, the
//! connector to open, a link to celebrate. The host executes them with
//! whatever its runtime has (miniquad's HTTP and wallet plugins, a browser's
//! `fetch`, a test's script) and hands the outcomes back as events. So one
//! flow, tested here without a window or a network, serves a macroquad
//! client, a wasm-bindgen frontend and a soak.
//!
//! ```ignore
//! let mut link = Linker::new();
//! run(link.handle(Event::Start));            // GET /auth/me, GET /auth/wallets
//! // a reply arrives:
//! run(link.handle(Event::Answer(answer)));
//! // the reader taps "Link a wallet":
//! run(link.handle(Event::Link(None)));       // Effect::OpenConnector
//! // a wallet connected in the connector:
//! run(link.handle(Event::Claim(claim)));     // POST challenge → Effect::Sign → POST verify
//! ```
//!
//! ## The flow
//!
//! Loading asks who the bearer is and what they have linked, both at once.
//! A link runs connect → challenge → sign → verify, and each step's failure
//! stops it with a sentence ([`words`]) and a retry that asks for a fresh
//! challenge, because auth-service burns a challenge before checking the
//! signature. A wallet that is already linked is recognised before anything
//! is signed. An unlink is confirmed first. A refused token (401) ends the
//! session wherever it is noticed.
//!
//! No wallet is primary and the order is the server's: the list is shown as
//! it arrives (see `wallet_link::LinkedWallet`).

mod claim;
mod machine;
mod token;
pub mod words;

use chains::ChainFamily;
use statig::prelude::{InitializedStateMachine, IntoStateMachineExt};
use wallet_link::LinkedWallet;
use wallet_link::link::{
    CARDANO_CHALLENGE_PATH, CARDANO_VERIFY_PATH, CardanoLinkRequest, EVM_CHALLENGE_PATH,
    EVM_VERIFY_PATH, EvmLinkRequest, Identity, ME_PATH, UNLINK_PATH, UnlinkRequest, WALLETS_PATH,
};

pub use claim::{Claim, LINKABLE, SignRequest, Signature, bech32_to_hex, hex};
pub use machine::Stop;
pub use token::TokenClaims;
pub use wallet_link;

use machine::{Desk, State};

/// Something the host tells the flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Begin: load the identity and the wallets.
    Start,
    /// Load again after the service was unreachable, or retry a stopped link.
    Retry,
    /// A call the flow asked for has answered.
    Answer(Answer),
    /// The reader asked to link a wallet, on one chain or any.
    Link(Option<ChainFamily>),
    /// A wallet is connected in the connector, holding this account.
    Claim(Claim),
    /// The wallet signed.
    Signed(Signature),
    /// The reader refused to sign. Not an error.
    SignRefused,
    /// The wallet failed to sign, in its words.
    SignFailed(String),
    /// The connector was closed.
    ConnectorClosed,
    /// The host has shown a finished link; back to idle.
    Done,
    /// The reader asked to unlink the wallet at this address.
    AskUnlink(String),
    ConfirmUnlink,
    /// The reader decided to keep it.
    KeepWallet,
}

/// A call's reply, tagged with which call it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub call: CallKind,
    pub reply: Reply,
}

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// A 2xx, with its body.
    Ok(String),
    /// Any other status, with its body (auth-service's errors are plain text).
    Status { status: u16, body: String },
    /// No reply at all: offline, DNS, CORS.
    Offline(String),
}

impl Reply {
    /// From a status and body, as an HTTP client reports them.
    pub fn from_status(status: u16, body: String) -> Self {
        if (200..300).contains(&status) {
            Self::Ok(body)
        } else {
            Self::Status { status, body }
        }
    }
}

/// Which call a reply answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallKind {
    Me,
    Wallets,
    Challenge,
    Verify,
    Unlink,
}

/// A call to auth-service, bearer-authenticated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiCall {
    Me,
    Wallets,
    Challenge(ChainFamily),
    VerifyCardano(CardanoLinkRequest),
    VerifyEvm(EvmLinkRequest),
    Unlink(UnlinkRequest),
}

impl ApiCall {
    pub fn kind(&self) -> CallKind {
        match self {
            Self::Me => CallKind::Me,
            Self::Wallets => CallKind::Wallets,
            Self::Challenge(_) => CallKind::Challenge,
            Self::VerifyCardano(_) | Self::VerifyEvm(_) => CallKind::Verify,
            Self::Unlink(_) => CallKind::Unlink,
        }
    }

    /// `"GET"` or `"POST"`.
    pub fn method(&self) -> &'static str {
        match self {
            Self::Me | Self::Wallets => "GET",
            _ => "POST",
        }
    }

    /// The path, appended to the service's base URL.
    pub fn path(&self) -> &'static str {
        match self {
            Self::Me => ME_PATH,
            Self::Wallets => WALLETS_PATH,
            Self::Challenge(ChainFamily::Evm) => EVM_CHALLENGE_PATH,
            // Only linkable chains reach a challenge (see [`LINKABLE`]).
            Self::Challenge(_) => CARDANO_CHALLENGE_PATH,
            Self::VerifyCardano(_) => CARDANO_VERIFY_PATH,
            Self::VerifyEvm(_) => EVM_VERIFY_PATH,
            Self::Unlink(_) => UNLINK_PATH,
        }
    }

    /// The JSON body, for a call that has one.
    pub fn body(&self) -> Option<String> {
        match self {
            Self::VerifyCardano(r) => serde_json::to_string(r).ok(),
            Self::VerifyEvm(r) => serde_json::to_string(r).ok(),
            Self::Unlink(r) => serde_json::to_string(r).ok(),
            _ => None,
        }
    }
}

/// Something the host owes the flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Make this call; answer with [`Event::Answer`].
    Call(ApiCall),
    /// Ask the connected wallet to sign; answer with [`Event::Signed`],
    /// [`Event::SignRefused`] or [`Event::SignFailed`].
    Sign(SignRequest),
    /// Open the wallet connector, on one chain or on the chain step. Answer
    /// with [`Event::Claim`] once a wallet is connected, or
    /// [`Event::ConnectorClosed`].
    OpenConnector(Option<ChainFamily>),
    /// A wallet was linked: show it arriving.
    Linked(LinkedWallet),
    /// A wallet was unlinked: show it leaving.
    Unlinked(LinkedWallet),
    /// A sentence to pass on (a toast): something failed that the screen
    /// does not otherwise show.
    Notice(String),
}

/// Where a link is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkStep {
    /// The connector is open; no wallet connected yet.
    Connect,
    /// Asking for the message to sign.
    Prepare,
    /// The wallet's signing dialog is up: the reader's move.
    Sign,
    /// Checking the signature.
    Check,
    /// Linked.
    Linked {
        address: String,
    },
    /// The connected wallet was already linked.
    Already {
        address: String,
    },
    Declined,
    Failed(String),
}

/// What the screen shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Not started, or loading.
    Loading,
    /// The service could not be reached; [`Event::Retry`] loads again.
    Unreachable(String),
    /// The token was refused.
    Expired,
    /// Loaded, nothing under way.
    Idle,
    /// A link under way, on `family` once a wallet is connected.
    Link {
        family: Option<ChainFamily>,
        step: LinkStep,
    },
    /// Asking whether to unlink this wallet.
    Confirming(LinkedWallet),
    /// Unlinking it.
    Unlinking(LinkedWallet),
}

/// The flow. See the [crate docs](crate).
pub struct Linker {
    machine: InitializedStateMachine<Desk>,
}

impl Default for Linker {
    fn default() -> Self {
        Self::new()
    }
}

impl Linker {
    pub fn new() -> Self {
        Self {
            machine: Desk::default()
                .uninitialized_state_machine()
                .init_with_context(&mut Vec::new()),
        }
    }

    /// Hand the flow `event`; the effects it owes, in order.
    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        let mut out = Vec::new();
        self.machine.handle_with_context(&event, &mut out);
        out
    }

    /// The linked wallets, in the server's order. `None` until loaded.
    pub fn wallets(&self) -> Option<&[LinkedWallet]> {
        self.machine.wallets.as_deref()
    }

    /// Who the bearer is, once `/auth/me` has answered.
    pub fn identity(&self) -> Option<&Identity> {
        self.machine.identity.as_ref()
    }

    /// The chain the link under way is on, once a wallet is connected.
    pub fn link_family(&self) -> Option<ChainFamily> {
        machine::family(self.machine.state())
    }

    /// Where the flow is, for the screen.
    pub fn phase(&self) -> Phase {
        let link = |family, step| Phase::Link { family, step };
        match self.machine.state() {
            State::Starting {} | State::Loading { .. } => Phase::Loading,
            State::Unreachable { reason } => Phase::Unreachable(reason.clone()),
            State::Expired {} => Phase::Expired,
            State::Idle {} => Phase::Idle,
            State::Connecting {} => link(None, LinkStep::Connect),
            State::Challenging { claim } => link(Some(claim.family()), LinkStep::Prepare),
            State::Signing { claim } => link(Some(claim.family()), LinkStep::Sign),
            State::Verifying { claim } => link(Some(claim.family()), LinkStep::Check),
            State::Celebrating { address } => link(
                self.family_of(address),
                LinkStep::Linked {
                    address: address.clone(),
                },
            ),
            State::Already { address } => link(
                self.family_of(address),
                LinkStep::Already {
                    address: address.clone(),
                },
            ),
            State::Stopped { claim, stop } => link(
                Some(claim.family()),
                match stop {
                    Stop::Declined => LinkStep::Declined,
                    Stop::Failed(m) => LinkStep::Failed(m.clone()),
                },
            ),
            State::Confirming { wallet } => Phase::Confirming(wallet.clone()),
            State::Unlinking { wallet } => Phase::Unlinking(wallet.clone()),
        }
    }

    /// The chain a linked address is on.
    fn family_of(&self, address: &str) -> Option<ChainFamily> {
        let wallet = self.wallets()?.iter().find(|w| w.address == address)?;
        family_of(wallet)
    }
}

/// The chain family a link is on, from its CAIP namespace.
pub fn family_of(wallet: &LinkedWallet) -> Option<ChainFamily> {
    let namespace = wallet
        .chain
        .split_once(':')
        .map_or(wallet.chain.as_str(), |(ns, _)| ns);
    ChainFamily::from_namespace(namespace)
}

#[cfg(test)]
mod tests;
