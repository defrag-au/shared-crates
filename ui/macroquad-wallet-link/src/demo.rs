//! A stand-in auth-service and stand-in wallets, so the screen runs, is
//! captured and is reviewed with no network, no token and no browser.
//!
//! **Nothing here is real.** The addresses are synthetic (repeated bytes
//! turned into bech32 by the same pallas conversion a real connect uses, as
//! the storybook's wallet fixtures are), the signatures are the word "demo",
//! and the server accepts whatever it is sent. The app shows a DEMO badge
//! whenever it runs on these.
//!
//! Each wallet answers in its own way, so every path is one tap away:
//!
//! | wallet | does |
//! |---|---|
//! | Eternl | connects a new Cardano account; signs |
//! | Lace | connects the Cardano account already linked |
//! | Typhon | connects; refuses to sign the first time, signs on Try again |
//! | Vespr | refuses to connect |
//! | Rabby | connects a new EVM account; signs |
//! | MetaMask | connects the EVM account already linked |
//!
//! Time is counted in polls, one a frame, so a capture at `--at` sees the same
//! moment every run.

use std::collections::VecDeque;

use chains::ChainFamily;
use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{Account, ChainOption, WalletModalAction, WalletOption};
use macroquad_wallet_flow::{ChainHost, cardano_account};
use wallet_link_flow::wallet_link::LinkedWallet;
use wallet_link_flow::wallet_link::link::{Identity, LinkChallenge, LinkVerified};
use wallet_link_flow::{Answer, ApiCall, Event, Reply, SignRequest, Signature, TokenClaims};

use crate::io::LinkIo;

/// Frames a call takes to answer.
const CALL_FRAMES: u32 = 30;
/// Frames the first load takes: long enough to see it.
const LOAD_FRAMES: u32 = 48;
/// Frames a wallet takes to answer a connect or a signature.
const WALLET_FRAMES: u32 = 70;

/// The demo's Discord user.
pub const DEMO_USER: &str = "demo";

/// The bytes a synthetic account is made of: a Cardano base address over two
/// made-up key hashes, `pay` and `stake` repeated.
fn base_hex(pay: &str, stake: &str) -> String {
    format!("01{}{}", pay.repeat(28), stake.repeat(28))
}

/// The stake address of a synthetic account whose stake hash is `byte`
/// repeated.
pub fn demo_stake(byte: &str) -> String {
    wallet_miniquad::stake_address_bech32(&base_hex("11", byte)).unwrap_or_default()
}

/// A synthetic EVM account, `byte` repeated.
pub fn demo_evm(byte: &str) -> String {
    format!("0x{}", byte.repeat(20))
}

/// What the demo user starts with linked.
pub fn demo_linked() -> Vec<LinkedWallet> {
    vec![
        LinkedWallet {
            chain: "cardano:mainnet".into(),
            address: demo_stake("33"),
            handle: Some("boef".into()),
        },
        LinkedWallet {
            chain: "eip155".into(),
            address: demo_evm("2e"),
            handle: None,
        },
    ]
}

/// `n` synthetic links, alternating Cardano and EVM, every third with a
/// handle: the crowded case.
pub fn demo_many(n: usize) -> Vec<LinkedWallet> {
    (0..n)
        .map(|i| {
            let byte = format!("{:02x}", 0x40 + i);
            if i % 2 == 0 {
                LinkedWallet {
                    chain: "cardano:mainnet".into(),
                    address: demo_stake(&byte),
                    handle: (i % 3 == 0).then(|| format!("demo{i}")),
                }
            } else {
                LinkedWallet {
                    chain: "eip155".into(),
                    address: demo_evm(&byte),
                    handle: None,
                }
            }
        })
        .collect()
}

/// The demo user's token claims: a name to greet, nothing else.
pub fn demo_claims() -> TokenClaims {
    TokenClaims {
        display_name: Some("demo".into()),
        ..TokenClaims::default()
    }
}

/// The stand-in auth-service.
pub struct DemoIo {
    wallets: Vec<LinkedWallet>,
    frame: u32,
    /// Events waiting for their frame, in order.
    due: VecDeque<(u32, Event)>,
    /// Signatures refused so far, for Typhon's refuse-once.
    refused: u32,
    loaded: bool,
}

impl Default for DemoIo {
    fn default() -> Self {
        Self::new(demo_linked())
    }
}

impl DemoIo {
    pub fn new(wallets: Vec<LinkedWallet>) -> Self {
        Self {
            wallets,
            frame: 0,
            due: VecDeque::new(),
            refused: 0,
            loaded: false,
        }
    }

    fn after(&mut self, frames: u32, event: Event) {
        let at = self.frame + frames;
        // Kept in order of when each is due.
        let i = self
            .due
            .iter()
            .position(|(t, _)| *t > at)
            .unwrap_or(self.due.len());
        self.due.insert(i, (at, event));
    }

    fn reply(&mut self, call: &ApiCall, frames: u32, body: String) {
        self.after(
            frames,
            Event::Answer(Answer {
                call: call.kind(),
                reply: Reply::Ok(body),
            }),
        );
    }

    fn list(&self) -> String {
        serde_json::to_string(&self.wallets).unwrap_or_else(|_| "[]".into())
    }

    fn link(&mut self, chain: &str, address: String) -> String {
        if !self.wallets.iter().any(|w| w.address == address) {
            self.wallets.push(LinkedWallet {
                chain: chain.into(),
                address: address.clone(),
                handle: None,
            });
        }
        serde_json::to_string(&LinkVerified {
            chain: chain.into(),
            address,
            handle: None,
            discord_user_id: DEMO_USER.into(),
        })
        .unwrap_or_default()
    }
}

impl LinkIo for DemoIo {
    fn call(&mut self, call: &ApiCall) {
        let load = if self.loaded {
            CALL_FRAMES
        } else {
            LOAD_FRAMES
        };
        match call {
            ApiCall::Me => {
                let body = serde_json::to_string(&Identity {
                    discord_user_id: DEMO_USER.into(),
                    intent: None,
                })
                .unwrap_or_default();
                self.reply(call, load, body);
            }
            ApiCall::Wallets => {
                self.loaded = true;
                let body = self.list();
                self.reply(call, load, body);
            }
            ApiCall::Challenge(_) => {
                let body = serde_json::to_string(&LinkChallenge {
                    nonce: "demo".into(),
                    message: format!(
                        "Link this wallet to your Discord account.\n\nDiscord ID: {DEMO_USER}\nNonce: demo"
                    ),
                    expires: 0,
                })
                .unwrap_or_default();
                self.reply(call, CALL_FRAMES, body);
            }
            ApiCall::VerifyCardano(r) => {
                let body = self.link("cardano:mainnet", r.stake_address.clone());
                self.reply(call, CALL_FRAMES, body);
            }
            ApiCall::VerifyEvm(r) => {
                let body = self.link("eip155", r.address.to_lowercase());
                self.reply(call, CALL_FRAMES, body);
            }
            ApiCall::Unlink(r) => {
                self.wallets.retain(|w| w.address != r.address);
                let body = self.list();
                self.reply(call, CALL_FRAMES, body);
            }
        }
    }

    fn sign(&mut self, request: &SignRequest) {
        let typhon = wallet_link_flow::bech32_to_hex(&demo_stake("44"));
        let event = match request {
            SignRequest::Cardano { address_hex, .. }
                if Some(address_hex) == typhon.as_ref() && self.refused == 0 =>
            {
                self.refused += 1;
                Event::SignRefused
            }
            SignRequest::Cardano { .. } => Event::Signed(Signature::Cardano {
                signature: "demo".into(),
                key: "demo".into(),
            }),
            SignRequest::Evm { .. } => Event::Signed(Signature::Evm {
                signature: "demo".into(),
            }),
        };
        self.after(WALLET_FRAMES, event);
    }

    fn poll(&mut self) -> Vec<Event> {
        self.frame += 1;
        let mut out = Vec::new();
        while self.due.front().is_some_and(|(t, _)| *t <= self.frame) {
            if let Some((_, e)) = self.due.pop_front() {
                out.push(e);
            }
        }
        out
    }
}

/// A stand-in wallet extension for one chain: asked, it answers
/// [`WALLET_FRAMES`] polls later, through the same `Connection` a real host
/// feeds. What it answers is the wallet's (see the module docs).
pub struct DemoChain {
    family: ChainFamily,
    wallets: Vec<WalletOption>,
    connection: Connection,
    frame: u32,
    pending: Option<(String, u32)>,
    dirty: bool,
}

impl DemoChain {
    pub fn cardano() -> Self {
        Self::new(
            ChainFamily::Cardano,
            vec![
                WalletOption::new("eternl", "Eternl"),
                WalletOption::new("lace", "Lace"),
                WalletOption::new("typhoncip30", "Typhon"),
                WalletOption::new("vespr", "Vespr"),
            ],
        )
    }

    pub fn evm() -> Self {
        Self::new(
            ChainFamily::Evm,
            vec![
                WalletOption::new("io.rabby", "Rabby"),
                WalletOption::new("io.metamask", "MetaMask"),
            ],
        )
    }

    fn new(family: ChainFamily, wallets: Vec<WalletOption>) -> Self {
        Self {
            family,
            wallets,
            connection: Connection::new(),
            frame: 0,
            pending: None,
            dirty: false,
        }
    }

    fn account(wallet: &str) -> Option<Account> {
        let cardano = |stake: &str| {
            let hex = base_hex("11", stake);
            let address = wallet_miniquad::hex_to_bech32(&hex);
            let stake = wallet_miniquad::stake_address_bech32(&hex);
            cardano_account(&address, stake.as_deref(), 1, None)
        };
        let evm = |byte: &str| {
            let address = demo_evm(byte);
            Account::new(&address).copyable("Address", &address)
        };
        Some(match wallet {
            "eternl" => cardano("22"),
            "lace" => cardano("33"),
            "typhoncip30" => cardano("44"),
            "io.rabby" => evm("1f"),
            "io.metamask" => evm("2e"),
            _ => return None,
        })
    }
}

impl ChainHost for DemoChain {
    fn family(&self) -> ChainFamily {
        self.family
    }

    fn poll(&mut self) -> bool {
        self.frame += 1;
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, due)| self.frame >= *due)
            && let Some((wallet, _)) = self.pending.take()
        {
            let answer = match Self::account(&wallet) {
                Some(account) => ConnectionEvent::Answered { wallet, account },
                None => ConnectionEvent::Refused(wallet),
            };
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
                self.pending = Some((wallet.clone(), self.frame + WALLET_FRAMES));
                ConnectionEvent::Ask(wallet.clone())
            }
            WalletModalAction::Cancel(_) | WalletModalAction::Disconnect(_) => {
                self.pending = None;
                ConnectionEvent::Release
            }
            WalletModalAction::Copy(_) | WalletModalAction::SignIn(_) => return,
        };
        self.dirty |= self.connection.handle(&event);
    }

    fn connection(&self) -> &Connection {
        &self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixtures_are_readable_addresses() {
        assert!(
            demo_stake("33").starts_with("stake1"),
            "{}",
            demo_stake("33")
        );
        assert_ne!(demo_stake("22"), demo_stake("33"));
        assert!(wallet_link_flow::bech32_to_hex(&demo_stake("44")).is_some());
    }

    /// Lace's account is the one the demo user starts with linked: the
    /// already-linked path.
    #[test]
    fn lace_connects_the_account_already_linked() {
        let lace = DemoChain::account("lace").unwrap();
        let stake = lace
            .details
            .iter()
            .find(|d| d.label == crate::STAKE_DETAIL)
            .map(|d| d.value.clone());
        assert_eq!(stake.as_deref(), Some(demo_linked()[0].address.as_str()));
    }

    #[test]
    fn calls_answer_in_order_on_the_frame_they_are_due() {
        let mut io = DemoIo::default();
        io.call(&ApiCall::Me);
        io.call(&ApiCall::Wallets);
        let mut seen = Vec::new();
        for _ in 0..LOAD_FRAMES {
            seen.extend(io.poll());
        }
        assert_eq!(seen.len(), 2);
    }
}
