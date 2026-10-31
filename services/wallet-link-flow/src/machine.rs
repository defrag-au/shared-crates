//! The flow itself, as a statig machine over a desk that holds what was
//! loaded. The effects an event produces go to the handler's context, an
//! outbox [`crate::Linker::handle`] returns.

use chains::ChainFamily;
use statig::prelude::*;
use wallet_link::LinkedWallet;
use wallet_link::link::{Identity, LinkChallenge, LinkVerified, UnlinkRequest};

use crate::claim::{Claim, Signature, verify_call};
use crate::words;
use crate::{Answer, ApiCall, CallKind, Effect, Event, Reply};

/// The effects one event produced, in order.
pub(crate) type Outbox = Vec<Effect>;

/// The machine's shared storage: what the screen shows whatever step it is
/// on.
#[derive(Default)]
pub struct Desk {
    pub(crate) identity: Option<Identity>,
    /// `None` until the first list arrives.
    pub(crate) wallets: Option<Vec<LinkedWallet>>,
}

fn load(out: &mut Outbox) {
    out.push(Effect::Call(ApiCall::Me));
    out.push(Effect::Call(ApiCall::Wallets));
}

/// Ask for a challenge for `claim`.
fn challenge(claim: &Claim, out: &mut Outbox) -> State {
    out.push(Effect::Call(ApiCall::Challenge(claim.family())));
    State::challenging(claim.clone())
}

/// A link stopped by `reply`, in the reader's words.
fn failed(claim: &Claim, reply: &Reply) -> State {
    State::stopped(claim.clone(), Stop::Failed(words::reply_failed(reply)))
}

impl Desk {
    /// The linked row `claim` is, if it is one.
    fn linked(&self, claim: &Claim) -> Option<&LinkedWallet> {
        self.wallets.as_ref()?.iter().find(|w| claim.is(w))
    }

    fn take_wallets(&mut self, body: &str) -> bool {
        match serde_json::from_str::<Vec<LinkedWallet>>(body) {
            Ok(list) => {
                self.wallets = Some(list);
                true
            }
            Err(_) => false,
        }
    }

    /// Where a newly connected `claim` leads: recognised (named by the linked
    /// row's own address, which is how the screen finds it), or a challenge.
    fn claimed(&self, claim: &Claim, out: &mut Outbox) -> State {
        match self.linked(claim) {
            Some(w) => State::already(w.address.clone()),
            None => challenge(claim, out),
        }
    }
}

/// How a link stopped short, kept until the reader retries or moves on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The reader refused in the wallet. Not an error.
    Declined,
    /// The wallet, the network or the server said no, in the reader's words.
    Failed(String),
}

/// Whether `answer` is the reply to a `kind` call.
fn is(answer: &Answer, kind: CallKind) -> bool {
    answer.call == kind
}

#[state_machine(
    initial = "State::starting()",
    state(derive(Debug, Clone, PartialEq, Eq))
)]
impl Desk {
    /// Built, not yet asked anything.
    #[state]
    fn starting(context: &mut Outbox, event: &Event) -> Outcome<State> {
        match event {
            Event::Start => {
                load(context);
                Transition(State::loading(false, false))
            }
            _ => Handled,
        }
    }

    /// Asking who the bearer is and what they have linked.
    #[state(superstate = "session")]
    fn loading(&mut self, me: &mut bool, listed: &mut bool, event: &Event) -> Outcome<State> {
        let Event::Answer(answer) = event else {
            return Super;
        };
        let body = match &answer.reply {
            Reply::Ok(body) => body,
            Reply::Status { status: 401, .. } => return Super,
            reply => return Transition(State::unreachable(words::reply_failed(reply))),
        };
        match answer.call {
            CallKind::Me => match serde_json::from_str::<Identity>(body) {
                Ok(id) => {
                    self.identity = Some(id);
                    *me = true;
                }
                Err(_) => return Transition(State::unreachable(words::UNREADABLE.into())),
            },
            CallKind::Wallets => {
                if !self.take_wallets(body) {
                    return Transition(State::unreachable(words::UNREADABLE.into()));
                }
                *listed = true;
            }
            _ => return Handled,
        }
        if *me && *listed {
            Transition(State::idle())
        } else {
            Handled
        }
    }

    /// The service could not be reached or answered nonsense. `Retry` loads
    /// again.
    #[state(superstate = "session")]
    fn unreachable(reason: &mut String, context: &mut Outbox, event: &Event) -> Outcome<State> {
        let _ = reason;
        match event {
            Event::Retry => {
                load(context);
                Transition(State::loading(false, false))
            }
            _ => Super,
        }
    }

    /// The token was refused. Only a new token helps, and that is the host's
    /// to get: this state is the end of this desk.
    #[state]
    fn expired(event: &Event) -> Outcome<State> {
        let _ = event;
        Handled
    }

    /// Loaded, nothing under way.
    #[state(superstate = "ready")]
    fn idle(&mut self, context: &mut Outbox, event: &Event) -> Outcome<State> {
        match event {
            Event::Link(family) => {
                context.push(Effect::OpenConnector(*family));
                Transition(State::connecting())
            }
            Event::AskUnlink(address) => {
                match self
                    .wallets
                    .as_ref()
                    .and_then(|ws| ws.iter().find(|w| &w.address == address))
                {
                    Some(w) => Transition(State::confirming(w.clone())),
                    None => Handled,
                }
            }
            _ => Super,
        }
    }

    /// The connector is open; waiting for a wallet to connect.
    #[state(superstate = "linking")]
    fn connecting(&mut self, context: &mut Outbox, event: &Event) -> Outcome<State> {
        match event {
            Event::Claim(claim) => Transition(self.claimed(claim, context)),
            _ => Super,
        }
    }

    /// Asked auth-service for a message to sign.
    #[state(superstate = "linking")]
    fn challenging(claim: &mut Claim, context: &mut Outbox, event: &Event) -> Outcome<State> {
        match event {
            Event::Answer(a) if is(a, CallKind::Challenge) => match &a.reply {
                Reply::Ok(body) => {
                    let Ok(c) = serde_json::from_str::<LinkChallenge>(body) else {
                        return Transition(State::stopped(
                            claim.clone(),
                            Stop::Failed(words::UNREADABLE.into()),
                        ));
                    };
                    match claim.sign_request(&c.message) {
                        Some(request) => {
                            context.push(Effect::Sign(request));
                            Transition(State::signing(claim.clone()))
                        }
                        None => Transition(State::stopped(
                            claim.clone(),
                            Stop::Failed(words::UNREADABLE_ADDRESS.into()),
                        )),
                    }
                }
                Reply::Status { status: 401, .. } => Super,
                reply => Transition(failed(claim, reply)),
            },
            _ => Super,
        }
    }

    /// The wallet is showing its signing dialog.
    #[state(superstate = "linking")]
    fn signing(claim: &mut Claim, context: &mut Outbox, event: &Event) -> Outcome<State> {
        match event {
            Event::Signed(signature) => match verify_call(claim, signature) {
                Some(call) => {
                    context.push(Effect::Call(call));
                    Transition(State::verifying(claim.clone()))
                }
                None => Transition(State::stopped(
                    claim.clone(),
                    Stop::Failed(words::WRONG_SIGNATURE.into()),
                )),
            },
            Event::SignRefused => Transition(State::stopped(claim.clone(), Stop::Declined)),
            Event::SignFailed(message) => {
                Transition(State::stopped(claim.clone(), Stop::Failed(message.clone())))
            }
            _ => Super,
        }
    }

    /// Sent the signature; waiting for auth-service to check it.
    #[state(superstate = "linking")]
    fn verifying(
        &mut self,
        claim: &mut Claim,
        context: &mut Outbox,
        event: &Event,
    ) -> Outcome<State> {
        match event {
            Event::Answer(a) if is(a, CallKind::Verify) => match &a.reply {
                Reply::Ok(body) => {
                    let Ok(verified) = serde_json::from_str::<LinkVerified>(body) else {
                        return Transition(State::stopped(
                            claim.clone(),
                            Stop::Failed(words::UNREADABLE.into()),
                        ));
                    };
                    let wallet = verified.wallet();
                    let list = self.wallets.get_or_insert_with(Vec::new);
                    if !list.iter().any(|w| w.address == wallet.address) {
                        list.push(wallet.clone());
                    }
                    context.push(Effect::Linked(wallet.clone()));
                    // The server's list is the truth: re-read it quietly, so
                    // a handle resolved since lands too.
                    context.push(Effect::Call(ApiCall::Wallets));
                    Transition(State::celebrating(wallet.address))
                }
                Reply::Status { status: 401, .. } => Super,
                reply => Transition(failed(claim, reply)),
            },
            _ => Super,
        }
    }

    /// Linked. The host shows it, then sends `Done`.
    #[state(superstate = "linking")]
    fn celebrating(address: &mut String, event: &Event) -> Outcome<State> {
        let _ = address;
        match event {
            Event::Done => Transition(State::idle()),
            _ => Super,
        }
    }

    /// The wallet connected is one already linked: nothing to sign.
    #[state(superstate = "linking")]
    fn already(address: &mut String, event: &Event) -> Outcome<State> {
        let _ = address;
        match event {
            Event::Done => Transition(State::idle()),
            _ => Super,
        }
    }

    /// Stopped short: refused or failed. `Retry` asks for a new challenge,
    /// since the old one was burned; a different account starts over with it.
    #[state(superstate = "linking")]
    fn stopped(
        &mut self,
        claim: &mut Claim,
        stop: &mut Stop,
        context: &mut Outbox,
        event: &Event,
    ) -> Outcome<State> {
        let _ = stop;
        match event {
            Event::Retry => Transition(challenge(claim, context)),
            Event::Claim(next) if next != claim => Transition(self.claimed(next, context)),
            _ => Super,
        }
    }

    /// Every step of a link. A failed call stops it; the connector closing
    /// abandons it (a late reply then finds the desk idle and is dropped).
    #[superstate(superstate = "ready")]
    fn linking(event: &Event) -> Outcome<State> {
        match event {
            Event::ConnectorClosed => Transition(State::idle()),
            // Another account mid-step is ignored: the step finishes for the
            // account it started with.
            Event::Claim(_) | Event::Link(_) => Handled,
            _ => Super,
        }
    }

    /// Asking whether to unlink `wallet`.
    #[state(superstate = "ready")]
    fn confirming(
        wallet: &mut LinkedWallet,
        context: &mut Outbox,
        event: &Event,
    ) -> Outcome<State> {
        match event {
            Event::ConfirmUnlink => {
                context.push(Effect::Call(ApiCall::Unlink(UnlinkRequest {
                    address: wallet.address.clone(),
                })));
                Transition(State::unlinking(wallet.clone()))
            }
            Event::KeepWallet => Transition(State::idle()),
            _ => Super,
        }
    }

    /// Asked auth-service to unlink `wallet`.
    #[state(superstate = "ready")]
    fn unlinking(
        &mut self,
        wallet: &mut LinkedWallet,
        context: &mut Outbox,
        event: &Event,
    ) -> Outcome<State> {
        match event {
            Event::Answer(a) if is(a, CallKind::Unlink) => match &a.reply {
                Reply::Ok(body) => {
                    if !self.take_wallets(body) {
                        // Unlinked, but the list that came back is unreadable:
                        // drop the one row, and read the list again.
                        if let Some(list) = &mut self.wallets {
                            list.retain(|w| w.address != wallet.address);
                        }
                        context.push(Effect::Call(ApiCall::Wallets));
                    }
                    context.push(Effect::Unlinked(wallet.clone()));
                    Transition(State::idle())
                }
                Reply::Status { status: 401, .. } => Super,
                reply => {
                    context.push(Effect::Notice(words::unlink_failed(reply)));
                    Transition(State::idle())
                }
            },
            _ => Super,
        }
    }

    /// Loaded: the wallet list may be re-read at any step.
    #[superstate(superstate = "session")]
    fn ready(&mut self, event: &Event) -> Outcome<State> {
        match event {
            Event::Answer(a) if is(a, CallKind::Wallets) => match &a.reply {
                Reply::Ok(body) => {
                    self.take_wallets(body);
                    Handled
                }
                Reply::Status { status: 401, .. } => Super,
                // A refresh that failed leaves the list as it was.
                _ => Handled,
            },
            _ => Super,
        }
    }

    /// Every state with a token: a refused token ends the session wherever it
    /// is noticed; any other failed reply is the step's to describe.
    #[superstate]
    fn session(event: &Event) -> Outcome<State> {
        match event {
            Event::Answer(Answer {
                reply: Reply::Status { status: 401, .. },
                ..
            }) => Transition(State::expired()),
            _ => Handled,
        }
    }
}

/// The chain a state is about, if it is about one.
pub(crate) fn family(state: &State) -> Option<ChainFamily> {
    match state {
        State::Challenging { claim }
        | State::Signing { claim }
        | State::Verifying { claim }
        | State::Stopped { claim, .. } => Some(claim.family()),
        _ => None,
    }
}

impl Signature {
    /// The chain this signature is for.
    pub fn family(&self) -> ChainFamily {
        match self {
            Self::Cardano { .. } => ChainFamily::Cardano,
            Self::Evm { .. } => ChainFamily::Evm,
        }
    }
}
