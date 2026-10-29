//! The event's identity, and the hash it is built from.
//!
//! An id exists so a replay is an upsert of the same rows rather than a second copy of
//! them: the store's `id` is a primary key, and a rebuild that produced different ids
//! would double-count. That is the whole requirement — a deterministic function of the
//! fact, stable across runs and across the two sources that can report it.
//!
//! It is **not** a security boundary, and the hash is chosen accordingly: FNV-1a, 128
//! bits, no dependency, and no claim of collision resistance against an adversary. The
//! only collisions that matter are accidental, within one collection, and they are
//! stopped by the discriminator rather than by the hash's width — see [`EventId::derive`].

use std::fmt;

use chains::{ChainRef, TokenRef};
use serde::{Deserialize, Serialize};

use crate::{EventBody, TxRef};

/// The FNV-1a 128 offset basis. Also the hash of the empty input, which is the one
/// published vector this crate pins.
pub(crate) const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;

/// The FNV-1a 128 prime: 2^88 + 2^8 + 0x3b.
const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// A stable identity for one fact.
///
/// Serialised as 32 lower-case hex characters, not as a number: the value is 128 bits
/// and a JSON consumer in a browser cannot hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct EventId(u128);

impl EventId {
    /// Derives the id from the fact itself.
    ///
    /// `discriminator` is what separates two events that agree on everything the
    /// envelope names — the same transaction moving the same asset twice, which is a
    /// batch and is exactly what a marketplace feed gives no log index for. Each
    /// adapter states its own; the [`holder`](crate::holder) and
    /// [`market`](crate::market) modules document the ones their facts support.
    ///
    /// Every component is length-prefixed before it enters the hash, so `("ab", "c")`
    /// and `("a", "bc")` cannot be made to collide by moving a boundary.
    pub fn derive(
        chain: ChainRef,
        body: &EventBody,
        asset: Option<&TokenRef>,
        tx: Option<&TxRef>,
        discriminator: &str,
    ) -> Self {
        let mut state = OFFSET;
        feed(&mut state, b"chain-events/v1");
        feed(&mut state, chain.as_caip2().as_bytes());
        feed(&mut state, body.family().as_str().as_bytes());
        feed(&mut state, body.kind().as_bytes());
        let asset = asset.map(|asset| asset.to_string());
        feed_opt_str(&mut state, asset.as_deref());
        match tx {
            Some(tx) => {
                feed(&mut state, tx.hash.as_bytes());
                feed(&mut state, &tx.index.to_le_bytes());
                feed(&mut state, &tx.event_index.to_le_bytes());
            }
            None => {
                feed(&mut state, b"");
                feed(&mut state, &[0; 4]);
                feed(&mut state, &[0; 4]);
            }
        }
        feed(&mut state, discriminator.as_bytes());
        Self(state)
    }

    /// The raw value, for a store that would rather key on an integer.
    pub const fn as_u128(self) -> u128 {
        self.0
    }

    /// The 32-character hex form used on the wire and in a primary key.
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

impl From<EventId> for String {
    fn from(id: EventId) -> Self {
        id.to_hex()
    }
}

impl TryFrom<String> for EventId {
    type Error = BadEventId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() != 32 {
            return Err(BadEventId(value));
        }
        let mut raw = 0u128;
        for byte in bytes {
            let digit = (*byte as char)
                .to_digit(16)
                .ok_or_else(|| BadEventId(value.clone()))?;
            raw = (raw << 4) | u128::from(digit);
        }
        Ok(Self(raw))
    }
}

/// A string that is not a 32-character hex event id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadEventId(String);

impl fmt::Display for BadEventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} is not a 32-character hex event id", self.0)
    }
}

impl std::error::Error for BadEventId {}

/// Absorbs one length-prefixed part.
pub(crate) fn feed(state: &mut u128, part: &[u8]) {
    absorb(state, &(part.len() as u64).to_le_bytes());
    absorb(state, part);
}

/// Absorbs an optional part, with a marker so an absent value is not the empty one.
pub(crate) fn feed_opt_str(state: &mut u128, part: Option<&str>) {
    match part {
        Some(part) => {
            absorb(state, &[1]);
            feed(state, part.as_bytes());
        }
        None => absorb(state, &[0]),
    }
}

fn absorb(state: &mut u128, bytes: &[u8]) {
    for byte in bytes {
        *state ^= u128::from(*byte);
        *state = state.wrapping_mul(PRIME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountRef, HolderEvent, HolderKind, PartyDelta};
    use chains::TokenRef;
    use std::str::FromStr;

    fn chain() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    fn asset() -> TokenRef {
        TokenRef::from_str(
            "cardano:mainnet/cnt:e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0/54686520466f756e646572",
        )
        .unwrap()
    }

    fn transfer(delta: i64) -> EventBody {
        EventBody::Holder(HolderEvent {
            kind: HolderKind::Transfer,
            parties: vec![
                PartyDelta {
                    account: AccountRef::new(chain(), "addr1vone"),
                    delta: -delta,
                },
                PartyDelta {
                    account: AccountRef::new(chain(), "addr1vtwo"),
                    delta,
                },
            ],
        })
    }

    #[test]
    fn the_hash_of_nothing_is_the_offset_basis() {
        // The one published FNV-1a 128 vector. If the prime or the offset is wrong,
        // every id in the store changes, so it is pinned rather than assumed.
        assert_eq!(OFFSET, 0x6c62272e07bb014262b821756295c58d);
        let mut state = OFFSET;
        absorb(&mut state, b"");
        assert_eq!(state, OFFSET);
    }

    #[test]
    fn the_same_fact_derives_the_same_id_twice() {
        let tx = TxRef {
            hash: "ab".repeat(32),
            index: 3,
            event_index: 0,
        };
        let first = EventId::derive(chain(), &transfer(1), Some(&asset()), Some(&tx), "parties");
        let second = EventId::derive(chain(), &transfer(1), Some(&asset()), Some(&tx), "parties");
        assert_eq!(first, second);
    }

    #[test]
    fn two_moves_of_one_asset_in_one_transaction_differ_by_their_parties() {
        // The batch case: same chain, same tx, same asset, same kind, different
        // counterparty. Without a discriminator these would upsert over each other.
        let tx = TxRef {
            hash: "cd".repeat(32),
            index: 1,
            event_index: 0,
        };
        let first = EventId::derive(
            chain(),
            &transfer(1),
            Some(&asset()),
            Some(&tx),
            "addr1vtwo",
        );
        let second = EventId::derive(
            chain(),
            &transfer(1),
            Some(&asset()),
            Some(&tx),
            "addr1vthree",
        );
        assert_ne!(first, second);
    }

    #[test]
    fn a_family_change_is_a_different_id() {
        let tx = TxRef {
            hash: "ef".repeat(32),
            index: 0,
            event_index: 0,
        };
        let holder = EventId::derive(chain(), &transfer(1), Some(&asset()), Some(&tx), "");
        let market = EventId::derive(
            chain(),
            &EventBody::Market(crate::MarketEvent::new(crate::MarketKind::Sold)),
            Some(&asset()),
            Some(&tx),
            "",
        );
        assert_ne!(holder, market);
    }

    #[test]
    fn a_moved_boundary_does_not_collide() {
        // `feed` length-prefixes, so splitting one string into two parts is a
        // different input rather than the same bytes.
        let mut one = OFFSET;
        feed(&mut one, b"abc");
        let mut two = OFFSET;
        feed(&mut two, b"ab");
        feed(&mut two, b"c");
        assert_ne!(one, two);
    }

    #[test]
    fn an_id_round_trips_through_its_hex_form() {
        let id = EventId::derive(chain(), &transfer(1), Some(&asset()), None, "d");
        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);
        assert_eq!(EventId::try_from(hex).unwrap(), id);
        assert_eq!(EventId::try_from(id.to_string()).unwrap(), id);
    }

    #[test]
    fn a_malformed_id_is_refused_rather_than_defaulted() {
        assert!(EventId::try_from("not-hex".to_owned()).is_err());
        assert!(EventId::try_from("ab".repeat(15)).is_err());
        assert!(EventId::try_from("z".repeat(32)).is_err());
    }

    #[test]
    fn an_id_serialises_as_one_string() {
        let id = EventId::derive(chain(), &transfer(2), Some(&asset()), None, "d");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<EventId>(&json).unwrap(), id);
    }
}
