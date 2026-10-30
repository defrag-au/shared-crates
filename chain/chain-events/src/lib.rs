//! One envelope for chain and marketplace facts.
//!
//! A Cardano policy ledger, an EVM `Transfer` log and an OpenSea marketplace message
//! are not the same kind of thing: the first two are fact ledgers, complete from
//! genesis and replayable; the third is a best-effort message feed with no replay. They
//! converge on **ownership** and **content**, and only partly on **market** — a listing
//! is a UTxO on Cardano and an off-chain signature on EVM.
//!
//! This crate is the vocabulary both are translated into, so a consumer writes
//!
//! ```text
//! match event.body {
//!     EventBody::Holder(movement) => /* ownership */,
//!     EventBody::Asset(change)     => /* traits and image */,
//!     EventBody::Market(fact)      => /* listings, sales, offers */,
//!     EventBody::Admin(change)     => /* the contract itself */,
//! }
//! ```
//!
//! and never branches on [`EventSource`]. The source and the coverage stay on the
//! envelope, because a reader must be able to tell a complete answer from a partial one
//! without asking the adapter.
//!
//! # The three shapes worth knowing
//!
//! - **A movement is a party list**, not `from`/`to`. That is `policy-archive`'s
//!   primitive, it is the only shape that survives a fee split, and [`direction`]
//!   declines to guess when the deltas do not determine one.
//! - **A content change is a hash and its algorithm.** A datum hash and a JSON
//!   fingerprint are not comparable, so [`HashAlgo`] travels with every one.
//! - **A market fact is an order transition**, not a message. Six transitions carry it:
//!   placed, updated, reopened, filled, cancelled, invalidated. An order's cancellation
//!   and a fill are different facts, so they are different variants rather than one kind
//!   with a nullable price; and `Invalidated` exists because OpenSea's `order_invalidate`
//!   does not say whether the order was withdrawn, filled or expired, which is a gap
//!   worth stating rather than guessing at.
//!
//! # Two feeds, one vocabulary
//!
//! A Cardano ledger builds [`ChainEvent`] directly from its own ingest tool. OpenSea's
//! messages have their translation in the crate, behind the `opensea` feature:
//! [`opensea::from_opensea`] maps every type the feed delivers, refuses one it does not
//! know by name, and keeps each message to what it actually states — an invalidation is
//! not promoted to a delisting, and a sale is also its movement.
//!
//! # Determinism
//!
//! [`EventId`] is derived from the fact, so replaying a source upserts the same rows.
//! Ids are stable across runs and across the two sources that can report the same
//! transfer — the merge is an upsert, never an append.
//!
//! The design is `cnft.dev-workers/docs/design/CHAIN_EVENTS.md`, beside the store it
//! feeds in `HOLDER_EVENTS.md`.

pub mod asset;
pub mod event;
pub mod holder;
pub mod id;
pub mod market;
/// The translation from OpenSea's messages into this vocabulary.
#[cfg(feature = "opensea")]
pub mod opensea;

pub use asset::{
    AssetEvent, AssetKind, CIP68_DATUM_V1, HashAlgo, IMAGE_METADATA_V1, ImageMetadataV1,
};
pub use event::{
    AccountRef, AdminEvent, AdminKind, ChainCoord, ChainEvent, Coverage, EventBody, EventDraft,
    EventSource, Family, RawRef, TxRef,
};
pub use holder::{Direction, HolderEvent, HolderKind, PartyDelta, direction};
pub use id::{BadEventId, EventId};
pub use market::{
    Currency, MarketEvent, MarketParty, OfferScope, Order, OrderRef, PartyRole, Price, Side,
};
#[cfg(feature = "opensea")]
pub use opensea::{TranslateError, from_opensea};

// The identity types, re-exported so a consumer names an asset or a chain without
// also taking `chains` as a dependency of its own. `AssetRefError` comes with them because
// it is the error `AssetType::new` returns, and a caller that cannot name it cannot
// propagate the failure.
pub use chains::{AssetRefError, AssetType, ChainRef, TokenRef};

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn cardano() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    fn evm() -> ChainRef {
        ChainRef::from_str("eip155:4663").unwrap()
    }

    fn collection(chain: ChainRef, namespace: &str, reference: &str) -> AssetType {
        AssetType::new(chain, namespace, reference).unwrap()
    }

    #[test]
    fn a_cardano_movement_and_an_evm_transfer_produce_the_same_body_shape() {
        let policy = "e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0";
        let cardano_event = ChainEvent::draft(
            cardano(),
            collection(cardano(), "cnt", policy),
            EventBody::Holder(HolderEvent::transfer(
                AccountRef::new(cardano(), "addr1vfrom"),
                AccountRef::new(cardano(), "addr1vto"),
                1,
            )),
            1_700_000_000,
            EventSource::PolicyArchive,
        )
        .at(ChainCoord::new(170_445_182))
        .build("addr1vto:1", 1_700_000_001);

        let contract = "027aca2794e44f24950d81227dcd516ffbb49d6e";
        let evm_event = ChainEvent::draft(
            evm(),
            collection(evm(), "erc721", contract),
            EventBody::Holder(HolderEvent::transfer(
                AccountRef::new(evm(), "0x6399ed6725cc163d019aa64ff55b22149d7179a8"),
                AccountRef::new(evm(), "0x61040e143a77f165ba44543af4a079f2c809d14b"),
                1,
            )),
            1_790_472_889,
            EventSource::EvmLogs,
        )
        .at(ChainCoord::new(33_000_000))
        .coverage(Coverage::Complete)
        .build("0x61040e:1", 1_790_472_890);

        // One shape, one direction, two chains — which is the point of the crate.
        assert!(matches!(cardano_event.body, EventBody::Holder(_)));
        assert_eq!(cardano_event.body.kind(), "transfer");
        assert_eq!(evm_event.body.kind(), "transfer");
        assert!(matches!(
            direction(match &evm_event.body {
                EventBody::Holder(event) => event,
                _ => unreachable!(),
            }),
            Direction::Transfer { .. }
        ));
    }

    #[test]
    fn an_event_id_is_the_same_from_either_source_for_the_same_fact() {
        // The merge is an upsert: a transfer seen by the chain and by the marketplace
        // feed must land on one row.
        let tx = TxRef::new("ab".repeat(32), 2);
        let movement = HolderEvent::transfer(
            AccountRef::new(evm(), "0x00000000000000000000000000000000000000aa"),
            AccountRef::new(evm(), "0x00000000000000000000000000000000000000bb"),
            1,
        );
        let one = EventId::derive(
            evm(),
            &EventBody::Holder(movement.clone()),
            None,
            Some(&tx),
            &movement.discriminator(),
        );
        let two = EventId::derive(
            evm(),
            &EventBody::Holder(movement.clone()),
            None,
            Some(&tx),
            &movement.discriminator(),
        );
        assert_eq!(one, two);
    }
}
