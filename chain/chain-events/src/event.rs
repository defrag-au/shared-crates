//! The envelope every fact shares, and the admin family that is small enough to live
//! beside it.
//!
//! The envelope's job is to make a Cardano movement, an EVM log and an OpenSea
//! marketplace message share their *coordinates* — chain, collection, asset, time,
//! position — so a timeline is a `UNION ALL` rather than a merge of unlike things. The
//! bodies differ per family, and deliberately do not share a row shape.
//!
//! See `cnft.dev-workers/docs/design/CHAIN_EVENTS.md` for the two sources this
//! reconciles and the places they cannot be reconciled.

use chains::{AssetType, ChainFamily, ChainRef, TokenRef};
use serde::{Deserialize, Serialize};

use crate::asset::AssetEvent;
use crate::holder::HolderEvent;
use crate::id::EventId;
use crate::market::MarketEvent;

/// Which of the four families a fact belongs to.
///
/// Coverage is stated per family: movement history for a Cardano collection is
/// complete, marketplace history on an EVM collection never is (a listing is an
/// off-chain signature until it is filled or cancelled). A reader must not infer one
/// family's completeness from another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Ownership moved.
    Holder,
    /// The asset's content changed.
    Asset,
    /// Something was listed, sold, offered or withdrawn.
    Market,
    /// The contract itself changed.
    Admin,
}

impl Family {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Holder => "holder",
            Self::Asset => "asset",
            Self::Market => "market",
            Self::Admin => "admin",
        }
    }
}

/// Which system stated the fact.
///
/// A fact reported by two sources is one row, not two — the merge is an upsert by
/// [`EventId`]. This field is what lets a reader tell a complete source from a
/// best-effort one after the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// `mitos/crates/policy-archive` — the Cardano movement ledger.
    PolicyArchive,
    /// ERC-721 `Transfer` logs replayed from genesis.
    EvmLogs,
    /// OpenSea's real-time feed. Best-effort and **not replayable**.
    OpenseaStream,
    /// `mitos-marketplace-decode` — a Cardano listing, sale or offer read from the chain.
    MarketplaceDecode,
}

impl EventSource {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PolicyArchive => "policy_archive",
            Self::EvmLogs => "evm_logs",
            Self::OpenseaStream => "opensea_stream",
            Self::MarketplaceDecode => "marketplace_decode",
        }
    }
}

/// How much of this collection, in this family, the source can account for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    /// The source saw the family from genesis and reconciled it.
    Complete,
    /// The source is missing part of it, or has not proven it is not.
    ///
    /// The default, because a source that has not said is a source that has not
    /// proven anything — and an evaluator must treat an achievement over a partial
    /// family as `unavailable` rather than as a zero.
    #[default]
    Partial,
}

/// Where in its chain the fact happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChainCoord {
    /// A slot on Cardano and Solana, a block number on EVM. The chain is on the
    /// envelope, so the same number is never ambiguous.
    pub height: u64,
}

impl ChainCoord {
    /// A coordinate at `height`.
    pub const fn new(height: u64) -> Self {
        Self { height }
    }
}

/// The transaction behind a fact, and where the fact sits inside it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxRef {
    /// The chain-native transaction hash, lower-case hex without a prefix.
    pub hash: String,
    /// The transaction's position in its block.
    pub index: u32,
    /// The fact's position within the transaction: a log index on EVM, a position
    /// among the transaction's movements on Cardano.
    ///
    /// Zero when the source does not say — OpenSea's feed gives a transaction hash and
    /// no log index. Uniqueness comes from the [`EventId`] discriminator, not from
    /// this field, so a zero here is not a collision.
    pub event_index: u32,
}

impl TxRef {
    /// A transaction reference with no position within it.
    pub fn new(hash: impl Into<String>, index: u32) -> Self {
        Self {
            hash: hash.into(),
            index,
            event_index: 0,
        }
    }

    /// The same reference, at a known position within the transaction.
    pub const fn at(mut self, event_index: u32) -> Self {
        self.event_index = event_index;
        self
    }
}

/// Where the raw artefact this event was derived from is kept.
///
/// OpenSea's feed has no replay, so a decoder change is only recoverable if the frames
/// were retained before they were understood. The follower already appends every frame
/// verbatim; this is a pointer to that, not a copy of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawRef {
    /// An R2 object URI or a capture path.
    pub uri: String,
    /// The record's position within a line-oriented artefact, when it has one.
    pub line: Option<u64>,
}

impl RawRef {
    /// A pointer to a whole artefact.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            line: None,
        }
    }

    /// The same pointer, at a known line.
    pub const fn at(mut self, line: u64) -> Self {
        self.line = Some(line);
        self
    }
}

/// A chain-qualified account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountRef {
    /// Which chain the address belongs to.
    pub chain: ChainRef,
    /// The address. Lower-cased for EVM, because a checksummed form and its lower-case
    /// form are the same account and only one of them compares equal to what a feed
    /// sends.
    pub address: String,
}

impl AccountRef {
    /// An account on `chain`.
    pub fn new(chain: ChainRef, address: impl Into<String>) -> Self {
        let address = address.into();
        let address = if chain.family() == ChainFamily::Evm {
            address.to_ascii_lowercase()
        } else {
            address
        };
        Self { chain, address }
    }

    /// Whether this is EVM's zero address — the sentinel a mint or a burn is written
    /// with. Always `false` for a non-EVM chain, which has no such sentinel and
    /// expresses the same fact by an absent party.
    pub fn is_zero(&self) -> bool {
        if self.chain.family() != ChainFamily::Evm {
            return false;
        }
        let digits = self
            .address
            .strip_prefix("0x")
            .unwrap_or(self.address.as_str());
        !digits.is_empty() && digits.bytes().all(|byte| byte == b'0')
    }
}

/// A contract-level change.
///
/// Defined so the family does not have to be invented later; nothing writes one yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminEvent {
    /// Which admin fact.
    pub kind: AdminKind,
    /// The field that changed.
    pub field: String,
    /// What it was, when the source says.
    pub before: Option<String>,
    /// What it became.
    pub after: Option<String>,
}

/// The admin facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminKind {
    /// `OwnershipTransferred` on an EVM contract.
    ContractOwner,
    /// A proxy's implementation address changed.
    ProxyUpgrade,
}

impl AdminKind {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContractOwner => "contract_owner",
            Self::ProxyUpgrade => "proxy_upgrade",
        }
    }
}

/// A fact, tagged by family.
///
/// Internally tagged on `family`, so a serialised event reads
/// `{ "family": "holder", "kind": "transfer", "parties": [ … ] }` — one object, with
/// the discriminator doing the work rather than a nullable column for every family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum EventBody {
    /// Ownership moved.
    Holder(HolderEvent),
    /// The asset's content changed.
    Asset(AssetEvent),
    /// A marketplace fact.
    Market(MarketEvent),
    /// A contract-level change.
    Admin(AdminEvent),
}

impl EventBody {
    /// Which family this is.
    pub const fn family(&self) -> Family {
        match self {
            Self::Holder(_) => Family::Holder,
            Self::Asset(_) => Family::Asset,
            Self::Market(_) => Family::Market,
            Self::Admin(_) => Family::Admin,
        }
    }

    /// The kind, as the string the store keys on.
    pub fn kind(&self) -> &str {
        match self {
            Self::Holder(event) => event.kind.as_str(),
            Self::Asset(event) => event.kind.as_str(),
            Self::Market(event) => event.kind.as_str(),
            Self::Admin(event) => event.kind.as_str(),
        }
    }
}

/// One fact, with the coordinates every family shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainEvent {
    /// Deterministic identity — see [`EventId::derive`].
    pub id: EventId,
    /// Which chain.
    pub chain: ChainRef,
    /// Which collection, as a CAIP-19 reference without a token half.
    pub collection: AssetType,
    /// Which asset, or `None` for a collection-scoped fact.
    pub asset: Option<TokenRef>,
    /// What happened.
    pub body: EventBody,
    /// Chain time, unix seconds.
    pub occurred_at: u64,
    /// Where in the chain, when the source says.
    ///
    /// A marketplace message has no block: OpenSea reports a transaction hash and a
    /// wall-clock timestamp, and nothing that identifies a block. `None` is that fact
    /// rather than a zero, which would sort as genesis.
    pub coord: Option<ChainCoord>,
    /// The transaction, when there was one.
    pub tx: Option<TxRef>,
    /// Which system stated it.
    pub source: EventSource,
    /// How much of the family this source can account for.
    pub coverage: Coverage,
    /// Where the raw artefact is kept, when it was retained.
    pub raw_ref: Option<RawRef>,
    /// When the store learned it.
    pub recorded_at: u64,
    /// `None` while current; set when a correction supersedes this row.
    pub superseded_at: Option<u64>,
    /// Incremented on correction. Never rewritten in place.
    pub revision: u32,
}

impl ChainEvent {
    /// Begins an event. The optional coordinates are added with the draft's methods,
    /// and the id is derived in [`EventDraft::build`] once they are known.
    pub fn draft(
        chain: ChainRef,
        collection: AssetType,
        body: EventBody,
        occurred_at: u64,
        source: EventSource,
    ) -> EventDraft {
        EventDraft {
            chain,
            collection,
            asset: None,
            body,
            occurred_at,
            coord: None,
            tx: None,
            source,
            coverage: Coverage::default(),
            raw_ref: None,
        }
    }
}

/// An event with its id not yet derived.
///
/// The id depends on the transaction and the discriminator, both of which an adapter
/// learns after it has decided what the event is — so the two halves are assembled in
/// that order rather than passed as eleven positional arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDraft {
    chain: ChainRef,
    collection: AssetType,
    asset: Option<TokenRef>,
    body: EventBody,
    occurred_at: u64,
    coord: Option<ChainCoord>,
    tx: Option<TxRef>,
    source: EventSource,
    coverage: Coverage,
    raw_ref: Option<RawRef>,
}

impl EventDraft {
    /// Where in the chain it happened. Omit when the source does not say.
    pub fn at(mut self, coord: ChainCoord) -> Self {
        self.coord = Some(coord);
        self
    }

    /// The asset this fact is about. Omit for a collection-scoped fact.
    pub fn asset(mut self, asset: TokenRef) -> Self {
        self.asset = Some(asset);
        self
    }

    /// The transaction it happened in.
    pub fn tx(mut self, tx: TxRef) -> Self {
        self.tx = Some(tx);
        self
    }

    /// How complete the source is. Defaults to [`Coverage::Partial`].
    pub fn coverage(mut self, coverage: Coverage) -> Self {
        self.coverage = coverage;
        self
    }

    /// The retained artefact this was derived from.
    pub fn raw(mut self, raw: RawRef) -> Self {
        self.raw_ref = Some(raw);
        self
    }

    /// Derives the id and produces the event.
    ///
    /// `discriminator` separates two facts that agree on everything else — see
    /// [`EventId::derive`]. `recorded_at` is the caller's clock; this crate does not
    /// read one.
    pub fn build(self, discriminator: &str, recorded_at: u64) -> ChainEvent {
        let id = EventId::derive(
            self.chain,
            &self.body,
            self.asset.as_ref(),
            self.tx.as_ref(),
            discriminator,
        );
        ChainEvent {
            id,
            chain: self.chain,
            collection: self.collection,
            asset: self.asset,
            body: self.body,
            occurred_at: self.occurred_at,
            coord: self.coord,
            tx: self.tx,
            source: self.source,
            coverage: self.coverage,
            raw_ref: self.raw_ref,
            recorded_at,
            superseded_at: None,
            revision: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder::{HolderKind, PartyDelta};
    use std::str::FromStr;

    fn chain() -> ChainRef {
        ChainRef::from_str("cardano:mainnet").unwrap()
    }

    fn collection() -> AssetType {
        AssetType::from_str(
            "cardano:mainnet/cnt:e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0",
        )
        .unwrap()
    }

    fn draft() -> EventDraft {
        ChainEvent::draft(
            chain(),
            collection(),
            EventBody::Holder(HolderEvent {
                kind: HolderKind::Transfer,
                parties: vec![
                    PartyDelta {
                        account: AccountRef::new(chain(), "addr1vfrom"),
                        delta: -1,
                    },
                    PartyDelta {
                        account: AccountRef::new(chain(), "addr1vto"),
                        delta: 1,
                    },
                ],
            }),
            1_700_000_000,
            EventSource::PolicyArchive,
        )
        .at(ChainCoord::new(170_445_182))
    }

    #[test]
    fn a_draft_without_a_source_coverage_is_partial() {
        // The safe default: an achievement over an unproven family reports
        // `unavailable`, not zero.
        let event = draft().build("parties", 1_700_000_001);
        assert_eq!(event.coverage, Coverage::Partial);
        assert_eq!(event.source, EventSource::PolicyArchive);
        assert_eq!(event.revision, 0);
        assert!(event.superseded_at.is_none());
    }

    #[test]
    fn a_source_with_no_block_says_so_rather_than_naming_block_zero() {
        let event = ChainEvent::draft(
            chain(),
            collection(),
            EventBody::Market(crate::MarketEvent::new(crate::MarketKind::Sold)),
            1_700_000_000,
            EventSource::OpenseaStream,
        )
        .build("order", 1);

        assert!(event.coord.is_none());
    }

    #[test]
    fn the_body_is_tagged_in_one_object_rather_than_nested() {
        let event = draft().build("parties", 1);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["body"]["family"], "holder");
        assert_eq!(json["body"]["kind"], "transfer");
        assert!(json["body"]["parties"].is_array());
    }

    #[test]
    fn an_event_round_trips_through_json_unchanged() {
        let event = draft()
            .coverage(Coverage::Complete)
            .tx(TxRef::new("ab".repeat(32), 4).at(2))
            .raw(RawRef::new("r2://events/perps/0001.parquet").at(9))
            .build("parties", 1_700_000_002);

        let json = serde_json::to_string(&event).unwrap();
        let back: ChainEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn an_evm_address_is_lower_cased_at_the_identity_boundary() {
        let evm = ChainRef::from_str("eip155:4663").unwrap();
        let account = AccountRef::new(evm, "0xAbCdEf0000000000000000000000000000000001");
        assert_eq!(
            account.address,
            "0xabcdef0000000000000000000000000000000001"
        );
    }

    #[test]
    fn a_cardano_address_keeps_its_case() {
        let account = AccountRef::new(chain(), "addr1VFROM");
        assert_eq!(account.address, "addr1VFROM");
    }

    #[test]
    fn the_zero_address_is_the_mint_and_burn_sentinel() {
        let evm = ChainRef::from_str("eip155:4663").unwrap();
        assert!(AccountRef::new(evm, "0x0000000000000000000000000000000000000000").is_zero());
        assert!(!AccountRef::new(evm, "0x0000000000000000000000000000000000000001").is_zero());
        // Cardano has no sentinel and expresses the same fact by an absent party, so
        // an address that looks like zeros is still an address.
        assert!(!AccountRef::new(chain(), "0x0").is_zero());
    }

    #[test]
    fn a_family_change_is_a_different_kind_string() {
        assert_eq!(
            EventBody::Holder(HolderEvent::new(HolderKind::Mint)).kind(),
            "mint"
        );
        assert_eq!(Family::Holder.as_str(), "holder");
        assert_eq!(EventSource::EvmLogs.as_str(), "evm_logs");
    }
}
