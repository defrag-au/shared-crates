//! Watching a policy: the transactions of a block that touch a token, whole.
//!
//! A host that already follows block bodies (the gateway) picks out the
//! transactions carrying any policy a client is watching and hands them over
//! as they are: the body and its auxiliary data, untouched. What a transaction
//! MEANS (a curve buy, a pool swap, a transfer) is the client's to decode, with
//! the same crates it would use anywhere, so the host stays a filter.
//!
//! # The smell
//!
//! A transaction moves a token only if the token's policy appears in its body:
//! in an output's value, or in the mint. Both encode the policy as a 28-byte
//! CBOR byte string, `0x58 0x1c` then the id, so a byte search for those 30
//! bytes finds every such transaction with no decoding at all. That catches
//! curve trades, pool swaps, batched fills, order placements and cancels, and
//! plain transfers alike.
//!
//! It can over-match, never under-match: anything else in the body that is the
//! same 28 bytes (a required signer's key hash equal to the policy id) passes
//! too. That is a client decoding one transaction it did not need to, which is
//! the cheap direction. An address carrying a script credential holds its 28
//! bytes inside the address's own byte string, without the prefix, so a
//! transaction that merely pays a policy's minting script is not a match.
//!
//! Transactions that failed phase-2 validation are left out: their outputs and
//! mint never happened.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::block::{BlockError, block_transactions, split_block};
use crate::header::{BlockHeader, HeaderError};
use crate::network::Network;

/// The ui-flow domain a watched transaction rides on: its own socket, whose
/// clients decode `Notify` as [`WatchEvent`].
pub const WATCH_DOMAIN: &str = "watch";

/// Policies one socket may watch. A token page watches one; a handful covers
/// a page comparing tokens, and the bound keeps a socket's filter cheap.
pub const MAX_WATCHED: usize = 8;

/// A minting policy's id: the 28-byte hash of its script.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyId(pub [u8; 28]);

impl PolicyId {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// How the policy appears in a transaction body: a CBOR byte string of
    /// length 28 (`0x58 0x1c`), then the id.
    fn needle(&self) -> [u8; 30] {
        let mut needle = [0u8; 30];
        needle[0] = 0x58;
        needle[1] = 0x1c;
        needle[2..].copy_from_slice(&self.0);
        needle
    }
}

impl fmt::Debug for PolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PolicyId({})", self.to_hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a policy id is 56 hex characters: {0:?}")]
pub struct BadPolicyId(pub String);

impl FromStr for PolicyId {
    type Err = BadPolicyId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s.trim()).map_err(|_| BadPolicyId(s.to_string()))?;
        let id: [u8; 28] = bytes.try_into().map_err(|_| BadPolicyId(s.to_string()))?;
        Ok(Self(id))
    }
}

/// Every policy in `list` (comma-separated hex), sorted and without repeats:
/// a socket's `?policy=` parameter. Empty, malformed or more than
/// [`MAX_WATCHED`] is an error, so a client cannot open a socket that watches
/// nothing, or everything.
pub fn parse_watch_list(list: &str) -> Result<Vec<PolicyId>, WatchListError> {
    let mut policies = Vec::new();
    for part in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        policies.push(part.parse::<PolicyId>()?);
    }
    policies.sort();
    policies.dedup();
    match policies.len() {
        0 => Err(WatchListError::Empty),
        n if n > MAX_WATCHED => Err(WatchListError::TooMany(n)),
        _ => Ok(policies),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WatchListError {
    #[error("watch at least one policy")]
    Empty,
    #[error("watch at most {MAX_WATCHED} policies, not {0}")]
    TooMany(usize),
    #[error(transparent)]
    Bad(#[from] BadPolicyId),
}

/// One transaction that touched a watched policy, as the block carried it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedTx {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub height: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot: u64,
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub block_time_unix: Option<u64>,
    /// Hex. The blake2b-256 of `body`, so a client can check it.
    pub tx_hash: String,
    /// Which of the watched policies it touched, hex, sorted.
    pub policies: Vec<String>,
    /// The transaction body's exact bytes.
    #[serde(with = "bytes")]
    pub body: Vec<u8>,
    /// Its auxiliary data's exact bytes (where a venue writes its memo), if
    /// it has any.
    #[serde(default, with = "opt_bytes")]
    pub aux: Option<Vec<u8>>,
}

/// What a watch socket carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WatchEvent {
    /// A transaction that touched a watched policy, from a block on the chain.
    Tx(WatchedTx),
    /// The chain rolled back: every transaction above `to_slot` sent so far
    /// is gone. `None` when the host could not say how far (a reconnect that
    /// did not resume): treat everything sent as unconfirmed.
    RolledBack {
        #[serde(default, with = "wasm_safe_serde::u64_option")]
        to_slot: Option<u64>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error(transparent)]
    Block(#[from] BlockError),
    #[error(transparent)]
    Header(#[from] HeaderError),
}

/// The transactions of an era-wrapped `block` that touch any of `policies`,
/// in block order. Decodes the header (for the height and slot) and finds each
/// transaction's bytes; does not decode a transaction.
pub fn watch_block(
    network: Network,
    block: &[u8],
    policies: &[PolicyId],
) -> Result<Vec<WatchedTx>, WatchError> {
    if policies.is_empty() {
        return Ok(Vec::new());
    }
    let parts = split_block(block)?;
    let header = BlockHeader::decode(parts.header_variant, parts.header_cbor)?;
    let transactions = block_transactions(block)?;
    let needles: Vec<(PolicyId, [u8; 30])> = policies.iter().map(|p| (*p, p.needle())).collect();

    let mut found = Vec::new();
    for (index, body) in transactions.bodies.iter().enumerate() {
        if transactions.invalid.contains(&(index as u32)) {
            continue;
        }
        let mut touched: Vec<String> = needles
            .iter()
            .filter(|(_, needle)| contains(body, needle))
            .map(|(policy, _)| policy.to_hex())
            .collect();
        if touched.is_empty() {
            continue;
        }
        touched.sort();
        found.push(WatchedTx {
            height: header.height,
            slot: header.slot,
            block_time_unix: network.slot_to_unix_secs(header.slot),
            tx_hash: hex::encode(crate::blake2b::<32>(body)),
            policies: touched,
            body: body.to_vec(),
            aux: transactions.aux.get(&(index as u32)).map(|a| a.to_vec()),
        });
    }
    Ok(found)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Bytes as a MessagePack (or CBOR) byte string rather than an array of
/// numbers, which is what serde makes of a plain `Vec<u8>`: half the size on
/// the wire for a transaction body. Reads either, so a JSON consumer's array
/// still decodes.
mod bytes {
    use serde::de::{SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        d.deserialize_byte_buf(BytesVisitor)
    }

    pub(super) struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bytes")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }

        fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(v)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(b) = seq.next_element::<u8>()? {
                out.push(b);
            }
            Ok(out)
        }
    }
}

/// [`bytes`], optional.
mod opt_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_some(&Wrapped(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Ok(Option::<Unwrapped>::deserialize(d)?.map(|u| u.0))
    }

    struct Wrapped<'a>(&'a [u8]);

    impl serde::Serialize for Wrapped<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_bytes(self.0)
        }
    }

    struct Unwrapped(Vec<u8>);

    impl<'de> Deserialize<'de> for Unwrapped {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            d.deserialize_byte_buf(super::bytes::BytesVisitor)
                .map(Unwrapped)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    /// Every policy minted or held in any output of the fixture block, found by
    /// walking it, so the test does not depend on a hand-copied id.
    fn policies_in(block: &[u8]) -> Vec<PolicyId> {
        let txs = block_transactions(block).expect("the fixture splits");
        let mut found = Vec::new();
        for body in &txs.bodies {
            for w in body.windows(30) {
                if w[0] == 0x58 && w[1] == 0x1c {
                    found.push(PolicyId(w[2..].try_into().expect("28 bytes")));
                }
            }
        }
        found.sort();
        found.dedup();
        found
    }

    #[test]
    fn a_policy_list_parses_sorted_and_bounded() {
        let a = "a1b085f414fb7799d378e90e39d24c9e54c841a311d72b2f57cba293";
        let b = "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7";
        let list = parse_watch_list(&format!("{a}, {b},{a}")).expect("parses");
        assert_eq!(list.len(), 2, "repeats are dropped");
        assert_eq!(list[0].to_hex(), b, "sorted");
        assert_eq!(parse_watch_list(""), Err(WatchListError::Empty));
        assert!(matches!(
            parse_watch_list("abc"),
            Err(WatchListError::Bad(_))
        ));
        let many: Vec<String> = (0..=MAX_WATCHED as u8)
            .map(|i| hex::encode([i; 28]))
            .collect();
        assert_eq!(
            parse_watch_list(&many.join(",")),
            Err(WatchListError::TooMany(MAX_WATCHED + 1))
        );
    }

    /// A transaction carrying a watched policy is found, with its exact bytes
    /// and a hash that is the body's; a policy no transaction carries finds
    /// nothing.
    #[test]
    fn the_block_yields_the_transactions_that_carry_a_policy() {
        let candidates = policies_in(BLOCK);
        let txs = block_transactions(BLOCK).expect("the fixture splits");
        let policy = *candidates.first().expect("the fixture carries a token");

        let found = watch_block(Network::Mainnet, BLOCK, &[policy]).expect("watches");
        assert!(!found.is_empty());
        for tx in &found {
            assert_eq!(tx.slot, 186_000_000);
            assert_eq!(tx.policies, vec![policy.to_hex()]);
            assert_eq!(tx.tx_hash, hex::encode(crate::blake2b::<32>(&tx.body)));
            assert!(txs.bodies.contains(&tx.body.as_slice()));
            assert!(contains(&tx.body, &policy.needle()));
        }
        // Exactly the valid bodies that carry it.
        let expected = txs
            .bodies
            .iter()
            .enumerate()
            .filter(|(i, b)| !txs.invalid.contains(&(*i as u32)) && contains(b, &policy.needle()))
            .count();
        assert_eq!(found.len(), expected);

        let absent = PolicyId([0xee; 28]);
        assert!(!candidates.contains(&absent));
        assert!(
            watch_block(Network::Mainnet, BLOCK, &[absent])
                .expect("watches")
                .is_empty()
        );
    }

    /// Bytes ride as a byte string, half the size of an array of numbers, and
    /// come back the same.
    #[test]
    fn a_watched_tx_round_trips_with_its_bytes_as_bytes() {
        let tx = WatchedTx {
            height: 1,
            slot: 2,
            block_time_unix: Some(3),
            tx_hash: "00".into(),
            policies: vec!["aa".into()],
            body: vec![0x84, 0xa4, 0xff, 0x00],
            aux: Some(vec![0xd9, 0x01]),
        };
        let event = WatchEvent::Tx(tx.clone());
        let bytes = rmp_serde::to_vec_named(&event).expect("encodes");
        // The body is a msgpack bin (0xc4), not an array of four numbers.
        assert!(
            bytes
                .windows(6)
                .any(|w| w == [0xc4, 4, 0x84, 0xa4, 0xff, 0x00])
        );
        let back: WatchEvent = rmp_serde::from_slice(&bytes).expect("decodes");
        assert_eq!(back, event);
        let rolled = WatchEvent::RolledBack { to_slot: None };
        let back: WatchEvent =
            rmp_serde::from_slice(&rmp_serde::to_vec_named(&rolled).expect("encodes"))
                .expect("decodes");
        assert_eq!(back, rolled);
    }
}
