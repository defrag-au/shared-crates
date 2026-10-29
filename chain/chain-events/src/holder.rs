//! Ownership moved.
//!
//! The payload is a **party list**, not `from`/`to`/`quantity`. That is the shape
//! `mitos/crates/policy-archive` already carries, and its doc comment states the
//! reason: the deltas are the primitive, direction is derived from them, and the
//! derivation is allowed to decline. Perps does decline — roughly twenty of its 30,766
//! movement rows are a side with several parties (fee and royalty splits, one batch
//! payout) — and a `from`/`to` payload would have had to drop them or guess a sender.
//!
//! EVM's `0x0` sentinel and Cardano's absent party both normalise here: a mint has one
//! positive party and no negative, a burn the reverse.

use serde::{Deserialize, Serialize};

use crate::event::AccountRef;

/// One asset's movement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderEvent {
    /// Which movement.
    pub kind: HolderKind,
    /// Every account whose balance changed, with its signed delta.
    pub parties: Vec<PartyDelta>,
}

impl HolderEvent {
    /// An event with no parties yet.
    pub fn new(kind: HolderKind) -> Self {
        Self {
            kind,
            parties: Vec::new(),
        }
    }

    /// A movement between two accounts, from a source that states it that way.
    ///
    /// This is the one place the `0x0` sentinel becomes a kind: a zero sender is a
    /// mint and a zero recipient a burn, and the zero party is not kept as an account
    /// — it is not one. A source that already knows the kind can build the parties
    /// directly instead.
    pub fn transfer(from: AccountRef, to: AccountRef, quantity: u64) -> Self {
        // A quantity that does not fit `i64` is not a collection. Saturating keeps the
        // event rather than panicking in an ingest loop; the alternative is a
        // `Result` every adapter has to unwrap.
        let delta = i64::try_from(quantity).unwrap_or(i64::MAX);

        if from.is_zero() {
            return Self {
                kind: HolderKind::Mint,
                parties: vec![PartyDelta { account: to, delta }],
            };
        }
        if to.is_zero() {
            return Self {
                kind: HolderKind::Burn,
                parties: vec![PartyDelta {
                    account: from,
                    delta: -delta,
                }],
            };
        }
        Self {
            kind: HolderKind::Transfer,
            parties: vec![
                PartyDelta {
                    account: from,
                    delta: -delta,
                },
                PartyDelta { account: to, delta },
            ],
        }
    }

    /// How many units moved — the sum of the positive deltas.
    pub fn quantity(&self) -> u64 {
        self.parties
            .iter()
            .filter(|party| party.delta > 0)
            .map(|party| party.delta as u64)
            .sum()
    }

    /// A stable string over the party set, for [`EventId::derive`](crate::EventId::derive).
    ///
    /// Sorted, so two adapters that enumerate the same parties in a different order
    /// produce one id. This is what separates a batch's two moves of one asset in one
    /// transaction, which is the case no source gives a log index for.
    pub fn discriminator(&self) -> String {
        let mut pairs: Vec<(&str, i64)> = self
            .parties
            .iter()
            .map(|party| (party.account.address.as_str(), party.delta))
            .collect();
        pairs.sort_unstable();
        pairs
            .iter()
            .map(|(address, delta)| format!("{address}:{delta}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// One account's signed change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyDelta {
    /// Who changed.
    pub account: AccountRef,
    /// Positive is received, negative is sent. Zero is not an event and is dropped by
    /// the fold that produced it.
    pub delta: i64,
}

/// Which movement this was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HolderKind {
    /// Supply was created.
    Mint,
    /// Ownership moved.
    Transfer,
    /// Supply was destroyed.
    Burn,
}

impl HolderKind {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mint => "mint",
            Self::Transfer => "transfer",
            Self::Burn => "burn",
        }
    }
}

/// What an event's deltas say happened, or a statement that they do not say.
///
/// Copied from `policy-archive`'s `Direction` rather than derived again: two answers to
/// the same question would eventually disagree, and the archive's is the one the
/// ledger's correctness argument is built on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    /// Exactly one loser and one gainer.
    Transfer {
        /// Where it came from.
        from: AccountRef,
        /// Where it went.
        to: AccountRef,
    },
    /// Created here; one recipient.
    Mint {
        /// Who received it.
        to: AccountRef,
    },
    /// Destroyed here; one source.
    Burn {
        /// Who sent it.
        from: AccountRef,
    },
    /// Several parties on a side — a batched fill, a bulk move. Stated, never guessed.
    Ambiguous,
    /// One arrival and no source, because the walk began after the move did. Resolves
    /// by walking deeper, where `Ambiguous` does not resolve at all.
    SourceBelowFloor {
        /// Who received it.
        to: AccountRef,
    },
}

/// Turns an event's deltas into a direction, or says it cannot.
pub fn direction(event: &HolderEvent) -> Direction {
    let losers: Vec<&PartyDelta> = event.parties.iter().filter(|p| p.delta < 0).collect();
    let gainers: Vec<&PartyDelta> = event.parties.iter().filter(|p| p.delta > 0).collect();

    match (event.kind, losers.as_slice(), gainers.as_slice()) {
        (HolderKind::Mint, [], [to]) => Direction::Mint {
            to: to.account.clone(),
        },
        (HolderKind::Burn, [from], []) => Direction::Burn {
            from: from.account.clone(),
        },
        (HolderKind::Transfer, [from], [to]) => Direction::Transfer {
            from: from.account.clone(),
            to: to.account.clone(),
        },
        (HolderKind::Transfer, [], [to]) => Direction::SourceBelowFloor {
            to: to.account.clone(),
        },
        _ => Direction::Ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chains::ChainRef;
    use std::str::FromStr;

    fn chain() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    fn evm() -> ChainRef {
        ChainRef::from_str("eip155:4663").unwrap()
    }

    fn account(name: &str) -> AccountRef {
        AccountRef::new(chain(), name)
    }

    #[test]
    fn a_transfer_is_one_negative_and_one_positive_delta() {
        let event = HolderEvent::transfer(account("addr1va"), account("addr1vb"), 3);
        assert_eq!(event.kind, HolderKind::Transfer);
        assert_eq!(event.quantity(), 3);
        assert_eq!(
            direction(&event),
            Direction::Transfer {
                from: account("addr1va"),
                to: account("addr1vb"),
            }
        );
    }

    #[test]
    fn a_zero_sender_is_a_mint_and_the_sentinel_is_not_an_account() {
        let zero = AccountRef::new(evm(), "0x0000000000000000000000000000000000000000");
        let to = AccountRef::new(evm(), "0x00000000000000000000000000000000000000aa");
        let event = HolderEvent::transfer(zero, to.clone(), 1);
        assert_eq!(event.kind, HolderKind::Mint);
        assert_eq!(event.parties.len(), 1);
        assert_eq!(event.parties[0].account, to);
        assert_eq!(event.parties[0].delta, 1);
    }

    #[test]
    fn a_zero_recipient_is_a_burn() {
        let zero = AccountRef::new(evm(), "0x0000000000000000000000000000000000000000");
        let from = AccountRef::new(evm(), "0x00000000000000000000000000000000000000aa");
        let event = HolderEvent::transfer(from.clone(), zero, 1);
        assert_eq!(event.kind, HolderKind::Burn);
        assert_eq!(event.parties.len(), 1);
        assert_eq!(event.parties[0].account, from);
        assert_eq!(event.parties[0].delta, -1);
    }

    #[test]
    fn a_split_is_ambiguous_rather_than_a_guessed_sender() {
        // One seller, two recipients: a marketplace fee and a royalty. The archive
        // declines this shape rather than picking the largest delta as the sender.
        let event = HolderEvent {
            kind: HolderKind::Transfer,
            parties: vec![
                PartyDelta {
                    account: account("addr1vseller"),
                    delta: -10,
                },
                PartyDelta {
                    account: account("addr1vroyalty"),
                    delta: 7,
                },
                PartyDelta {
                    account: account("addr1vmarket"),
                    delta: 3,
                },
            ],
        };
        assert_eq!(direction(&event), Direction::Ambiguous);
        assert_eq!(event.quantity(), 10);
    }

    #[test]
    fn a_lone_arrival_is_stated_as_below_the_floor_not_as_a_mint() {
        let event = HolderEvent {
            kind: HolderKind::Transfer,
            parties: vec![PartyDelta {
                account: account("addr1vto"),
                delta: 1,
            }],
        };
        assert_eq!(
            direction(&event),
            Direction::SourceBelowFloor {
                to: account("addr1vto"),
            }
        );
    }

    #[test]
    fn a_kind_that_does_not_match_its_parties_is_ambiguous() {
        let event = HolderEvent {
            kind: HolderKind::Mint,
            parties: vec![PartyDelta {
                account: account("addr1vfrom"),
                delta: -1,
            }],
        };
        assert_eq!(direction(&event), Direction::Ambiguous);
    }

    #[test]
    fn the_discriminator_does_not_depend_on_party_order() {
        let first = HolderEvent {
            kind: HolderKind::Transfer,
            parties: vec![
                PartyDelta {
                    account: account("addr1va"),
                    delta: -1,
                },
                PartyDelta {
                    account: account("addr1vb"),
                    delta: 1,
                },
            ],
        };
        let second = HolderEvent {
            kind: HolderKind::Transfer,
            parties: first.parties.iter().rev().cloned().collect(),
        };
        assert_eq!(first.discriminator(), second.discriminator());
    }

    #[test]
    fn two_different_counterparties_discriminate() {
        let one = HolderEvent::transfer(account("addr1va"), account("addr1vb"), 1);
        let two = HolderEvent::transfer(account("addr1va"), account("addr1vc"), 1);
        assert_ne!(one.discriminator(), two.discriminator());
    }
}
