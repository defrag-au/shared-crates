//! `Connection` — one wallet connection's lifecycle as a state machine, the same on every chain: asked, answered, refused, failed, let go.
//!
//! A host owns one `Connection` per chain it can connect, feeds it what its
//! bridge reports as [`ConnectionEvent`]s, and hands the modal its
//! [`Connection::phase`]. The host keeps the chain's mechanics — CIP-30's
//! request sequence, an EIP-1193 provider — and the machine keeps the rules
//! every chain shares, written once:
//!
//! - **An answer is only for the wallet that was asked.** A late reply from a
//!   request that was cancelled, or from the wallet the reader switched away
//!   from, is dropped rather than connecting the wrong wallet.
//! - **A refusal is not a fault.** Declined is its own state, so the modal
//!   can offer to try again instead of painting the reader's choice red.
//! - **A connected account can change** — a resolved name arriving, the
//!   wallet switching accounts — without the connection being re-asked.
//! - **Asking again replaces the question.** Retrying, or picking another
//!   wallet mid-request, starts a new ask; the old one's answers then fall
//!   under the first rule.
//!
//! ## Many at once
//!
//! Each `Connection` is independent, so a reader can hold a Cardano wallet and
//! an EVM wallet at the same moment, each with its own lifecycle, and the
//! modal shows each through its chain's [`ChainPhase`]. That is also the
//! shape a wallet-linking surface needs: every connected account is a value
//! of its own ([`crate::wallet_modal::WalletModalVm::connected`] lists them),
//! not a slot one connection has to vacate for the next.
//!
//! ## Bringing a chain on
//!
//! A new chain's host is three things: its wallets as
//! [`crate::wallet_modal::WalletOption`]s, its account written out as an
//! [`Account`], and its bridge's outcomes turned into events here. Feeding the
//! current truth every frame is fine — every event is idempotent where it
//! should be (an answer for an already-connected wallet updates its account;
//! a refusal while not asking is ignored).

use statig::prelude::*;

use crate::wallet_modal::{Account, ChainPhase};

/// What a chain's bridge reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// `wallet` is being asked: the reader picked it, retried it, or the host
    /// is reconnecting the one it remembered.
    Ask(String),
    /// `wallet` connected, with `account`.
    Answered { wallet: String, account: Account },
    /// The reader refused in `wallet`'s own dialog.
    Refused(String),
    /// `wallet` or the bridge failed, in the host's words.
    Faulted { wallet: String, message: String },
    /// The connected wallet's account changed: a resolved name arrived, or
    /// the wallet switched accounts. Ignored when nothing is connected.
    AccountChanged(Account),
    /// Let go — a cancel, a disconnect, the bridge going away.
    Release,
}

/// The machine's shared storage: none. Every state carries its own data.
#[derive(Default)]
pub struct Lifecycle;

#[state_machine(initial = "State::idle()", state(derive(Debug, Clone, PartialEq, Eq)))]
impl Lifecycle {
    /// Nothing asked, nothing connected.
    #[state]
    fn idle(event: &ConnectionEvent) -> Outcome<State> {
        match event {
            ConnectionEvent::Ask(wallet) => Transition(State::asking(wallet.clone())),
            _ => Handled,
        }
    }

    /// `wallet` has been asked and has not answered.
    #[state(superstate = "engaged")]
    fn asking(wallet: &mut String, event: &ConnectionEvent) -> Outcome<State> {
        match event {
            ConnectionEvent::Answered { wallet: w, account } if w == wallet => {
                Transition(State::connected(w.clone(), account.clone()))
            }
            ConnectionEvent::Refused(w) if w == wallet => Transition(State::declined(w.clone())),
            ConnectionEvent::Faulted { wallet: w, message } if w == wallet => {
                Transition(State::failed(w.clone(), message.clone()))
            }
            // Another wallet's answer: a request this one replaced.
            ConnectionEvent::Answered { .. }
            | ConnectionEvent::Refused(_)
            | ConnectionEvent::Faulted { .. } => Handled,
            _ => Super,
        }
    }

    /// `wallet` is connected as `account`.
    #[state(superstate = "engaged")]
    fn connected(
        wallet: &mut String,
        account: &mut Account,
        event: &ConnectionEvent,
    ) -> Outcome<State> {
        match event {
            ConnectionEvent::AccountChanged(next) => {
                *account = next.clone();
                Handled
            }
            // The host reporting what it already knows: keep the account
            // current, and do not re-enter the state.
            ConnectionEvent::Answered {
                wallet: w,
                account: next,
            } if w == wallet => {
                if account != next {
                    *account = next.clone();
                }
                Handled
            }
            ConnectionEvent::Faulted { wallet: w, message } if w == wallet => {
                Transition(State::failed(w.clone(), message.clone()))
            }
            ConnectionEvent::Answered { .. }
            | ConnectionEvent::Refused(_)
            | ConnectionEvent::Faulted { .. } => Handled,
            _ => Super,
        }
    }

    /// The reader refused `wallet`. Kept until they ask again or let go.
    #[state(superstate = "engaged")]
    fn declined(wallet: &mut String, event: &ConnectionEvent) -> Outcome<State> {
        let _ = wallet;
        match event {
            ConnectionEvent::Ask(_) | ConnectionEvent::Release => Super,
            _ => Handled,
        }
    }

    /// `wallet` failed with `message`. Kept until they ask again or let go.
    #[state(superstate = "engaged")]
    fn failed(
        wallet: &mut String,
        message: &mut String,
        event: &ConnectionEvent,
    ) -> Outcome<State> {
        let _ = (wallet, message);
        match event {
            ConnectionEvent::Ask(_) | ConnectionEvent::Release => Super,
            _ => Handled,
        }
    }

    /// Every state past idle: asking again replaces the question, and letting
    /// go returns to idle.
    #[superstate]
    fn engaged(event: &ConnectionEvent) -> Outcome<State> {
        match event {
            ConnectionEvent::Ask(wallet) => Transition(State::asking(wallet.clone())),
            ConnectionEvent::Release => Transition(State::idle()),
            _ => Handled,
        }
    }
}

/// One chain's wallet connection. See the [module docs](self).
pub struct Connection {
    machine: InitializedStateMachine<Lifecycle>,
}

impl Default for Connection {
    fn default() -> Self {
        Self::new()
    }
}

impl Connection {
    pub fn new() -> Self {
        Self {
            machine: Lifecycle.uninitialized_state_machine().init(),
        }
    }

    /// Feed what the bridge reported.
    pub fn handle(&mut self, event: &ConnectionEvent) {
        self.machine.handle(event);
    }

    /// The connection in the modal's words.
    pub fn phase(&self) -> ChainPhase {
        match self.machine.state() {
            State::Idle {} => ChainPhase::Idle,
            State::Asking { wallet } => ChainPhase::Connecting {
                wallet: wallet.clone(),
            },
            State::Connected { wallet, account } => ChainPhase::Connected {
                wallet: wallet.clone(),
                account: account.clone(),
            },
            State::Declined { wallet } => ChainPhase::Declined {
                wallet: wallet.clone(),
            },
            State::Failed { wallet, message } => ChainPhase::Failed {
                wallet: wallet.clone(),
                message: message.clone(),
            },
        }
    }

    /// The wallet this connection is about — asked, connected, or refused.
    pub fn wallet(&self) -> Option<&str> {
        match self.machine.state() {
            State::Idle {} => None,
            State::Asking { wallet }
            | State::Connected { wallet, .. }
            | State::Declined { wallet }
            | State::Failed { wallet, .. } => Some(wallet),
        }
    }

    /// The connected account, once there is one.
    pub fn account(&self) -> Option<&Account> {
        match self.machine.state() {
            State::Connected { account, .. } => Some(account),
            _ => None,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.machine.state(), State::Connected { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(c: &mut Connection, wallet: &str) {
        c.handle(&ConnectionEvent::Ask(wallet.into()));
    }

    fn answer(c: &mut Connection, wallet: &str, address: &str) {
        c.handle(&ConnectionEvent::Answered {
            wallet: wallet.into(),
            account: Account::new(address),
        });
    }

    #[test]
    fn an_ask_then_its_answer_connects() {
        let mut c = Connection::new();
        assert_eq!(c.phase(), ChainPhase::Idle);
        ask(&mut c, "eternl");
        assert_eq!(
            c.phase(),
            ChainPhase::Connecting {
                wallet: "eternl".into()
            }
        );
        answer(&mut c, "eternl", "addr1");
        assert!(c.is_connected());
        assert_eq!(c.account().map(|a| a.address.as_str()), Some("addr1"));
    }

    /// The reader switched wallets mid-request: the first wallet's late reply
    /// must not connect it.
    #[test]
    fn an_answer_from_a_replaced_ask_is_dropped() {
        let mut c = Connection::new();
        ask(&mut c, "eternl");
        ask(&mut c, "lace");
        answer(&mut c, "eternl", "addr1");
        assert_eq!(
            c.phase(),
            ChainPhase::Connecting {
                wallet: "lace".into()
            }
        );
        c.handle(&ConnectionEvent::Refused("eternl".into()));
        assert_eq!(c.wallet(), Some("lace"), "nor may its refusal");
    }

    #[test]
    fn a_cancel_lets_go_and_a_late_answer_lands_nowhere() {
        let mut c = Connection::new();
        ask(&mut c, "eternl");
        c.handle(&ConnectionEvent::Release);
        answer(&mut c, "eternl", "addr1");
        assert_eq!(c.phase(), ChainPhase::Idle);
    }

    #[test]
    fn a_refusal_is_kept_until_asked_again() {
        let mut c = Connection::new();
        ask(&mut c, "lace");
        c.handle(&ConnectionEvent::Refused("lace".into()));
        assert_eq!(
            c.phase(),
            ChainPhase::Declined {
                wallet: "lace".into()
            }
        );
        answer(&mut c, "lace", "addr1");
        assert!(!c.is_connected(), "an answer to nothing asked is ignored");
        ask(&mut c, "lace");
        answer(&mut c, "lace", "addr1");
        assert!(c.is_connected());
    }

    #[test]
    fn a_connected_account_updates_in_place() {
        let mut c = Connection::new();
        ask(&mut c, "eternl");
        answer(&mut c, "eternl", "addr1");
        c.handle(&ConnectionEvent::AccountChanged(
            Account::new("addr1").with_label("$name"),
        ));
        assert_eq!(c.account().map(Account::display), Some("$name"));
        // The host re-reporting the same truth every frame changes nothing.
        c.handle(&ConnectionEvent::Answered {
            wallet: "eternl".into(),
            account: Account::new("addr1").with_label("$name"),
        });
        assert_eq!(c.account().map(Account::display), Some("$name"));
    }

    #[test]
    fn a_fault_while_connected_fails_the_connection() {
        let mut c = Connection::new();
        ask(&mut c, "eternl");
        answer(&mut c, "eternl", "addr1");
        c.handle(&ConnectionEvent::Faulted {
            wallet: "eternl".into(),
            message: "the account changed in another tab".into(),
        });
        assert!(matches!(c.phase(), ChainPhase::Failed { .. }));
        c.handle(&ConnectionEvent::Release);
        assert_eq!(c.phase(), ChainPhase::Idle);
    }

    /// Two chains, two connections: neither knows about the other.
    #[test]
    fn connections_are_independent() {
        let mut cardano = Connection::new();
        let mut evm = Connection::new();
        ask(&mut cardano, "eternl");
        answer(&mut cardano, "eternl", "addr1");
        ask(&mut evm, "io.rabby");
        evm.handle(&ConnectionEvent::Release);
        assert!(cardano.is_connected());
        assert_eq!(evm.phase(), ChainPhase::Idle);
    }
}
