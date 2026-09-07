//! What a policy's asset IS, decided from its name alone — and therefore
//! whether it is a thing anybody HOLDS.
//!
//! # Why this exists
//!
//! A policy's on-chain asset list is not its supply. Three kinds of token live
//! under a collection's policy id and only one of them is the collection:
//!
//! - the **collectible** — what a holder holds and a marketplace sells;
//! - the **CIP-68 reference token** (`000643b0`), the metadata twin of a
//!   collectible, minted one-for-one with it;
//! - the **CIP-27 royalty token**, an empty asset name, minted once and
//!   usually burned, carrying the royalty address in its metadata.
//!
//! Counting the raw list conflates all three, and the errors are not small:
//!
//! - **CIP-68 doubles the collection.** Mekka S1 is 5,000 NFTs and 10,000
//!   assets. Worse than the count being wrong, the reference tokens all sit in
//!   ONE wallet — so a holder distribution built from the raw list invents a
//!   51%-of-supply whale that is really the project's metadata vault, and the
//!   real holders are squeezed into the other half. That was on screen.
//! - **CIP-27 adds a phantom unit.** Perps is 6,000 NFTs and reads "6,001
//!   units" — the royalty token, with a total supply of zero.
//!
//! # Why here and not at each call site
//!
//! Because it was already at one call site and not the others. `tx_verdict`
//! filters `000643b0` for the wallet feed, with a comment describing exactly
//! this failure ("counting it doubles a CIP-68 mint, turning five NFTs into
//! '10 items'") — and the policy surface, written later, did not know. One
//! vocabulary, in the crate that already owns asset identity, is what stops
//! the third surface repeating it.

use serde::{Deserialize, Serialize};

/// The CIP-67 label prefixing a CIP-68 asset name — the first four bytes,
/// eight hex characters.
///
/// Only the labels that are actually minted are named. A label this does not
/// know is not an error: CIP-67 reserves the whole space and an unlabelled
/// name is the overwhelmingly common case (every CIP-25 collection), so an
/// unrecognised prefix means "no label", not "bad asset".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cip67Label {
    /// `100` — the reference token holding a collectible's metadata.
    Reference,
    /// `222` — the user token: the NFT itself.
    UserNft,
    /// `333` — a fungible token under a CIP-68 policy.
    FungibleToken,
    /// `444` — a rich fungible token: an edition with metadata, held like a
    /// collectible and issued in quantity like a currency.
    RichFungible,
}

impl Cip67Label {
    pub const ALL: [Cip67Label; 4] = [
        Cip67Label::Reference,
        Cip67Label::UserNft,
        Cip67Label::FungibleToken,
        Cip67Label::RichFungible,
    ];

    /// The eight hex characters this label is written as.
    #[must_use]
    pub const fn as_hex(self) -> &'static str {
        match self {
            Cip67Label::Reference => "000643b0",
            Cip67Label::UserNft => "000de140",
            Cip67Label::FungibleToken => "0014df10",
            Cip67Label::RichFungible => "001bc280",
        }
    }

    /// The four raw bytes this label is written as on chain.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 4] {
        match self {
            Cip67Label::Reference => [0x00, 0x06, 0x43, 0xb0],
            Cip67Label::UserNft => [0x00, 0x0d, 0xe1, 0x40],
            Cip67Label::FungibleToken => [0x00, 0x14, 0xdf, 0x10],
            Cip67Label::RichFungible => [0x00, 0x1b, 0xc2, 0x80],
        }
    }

    /// The label a hex asset name carries, or `None` for an unlabelled name.
    ///
    /// Case-insensitive: asset names travel as hex and nothing guarantees the
    /// case, so a `000643B0` from an upstream that upper-cases must not read
    /// as an unlabelled collectible and slip back into the count.
    #[must_use]
    pub fn of(name_hex: &str) -> Option<Self> {
        let head = name_hex.get(..8)?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.as_hex() == head)
    }

    /// The same question of a RAW asset name.
    ///
    /// For callers that hold the ledger's own bytes — a chain walker counting
    /// units per pass reads millions of names, and hex-encoding each one just
    /// to look at its first four bytes is work with no answer in it.
    #[must_use]
    pub fn of_bytes(name: &[u8]) -> Option<Self> {
        let head: [u8; 4] = name.get(..4)?.try_into().ok()?;
        Self::ALL.into_iter().find(|l| l.as_bytes() == head)
    }
}

/// What an asset under a collection's policy actually is.
///
/// Decided from the NAME alone — no chain lookup, no datum, no quantity — so
/// it is available anywhere an asset name is, including inside a wasm frontend
/// holding nothing but a decoded artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    /// The thing a holder holds: a CIP-25 NFT, a CIP-68 user token (`222`), or
    /// a rich fungible edition (`444`).
    Collectible,
    /// A CIP-68 reference token (`100`) — the metadata twin of a collectible.
    /// Real on chain, never a holding.
    ReferenceMetadata,
    /// A CIP-27 royalty token: the empty asset name, minted under the policy
    /// to declare a royalty address.
    Royalty,
    /// A CIP-68 fungible token (`333`) — a currency issued under the same
    /// policy as a collection, not a member of it.
    FungibleToken,
}

impl AssetRole {
    /// Classify an asset by its hex name, within its policy.
    ///
    /// The empty name is the CIP-27 royalty token. That is not a guess: an
    /// asset name is optional in the ledger and a collection never mints one
    /// without a name, so the empty name under a collection policy is the
    /// royalty declaration and nothing else.
    #[must_use]
    pub fn of(name_hex: &str) -> Self {
        if name_hex.is_empty() {
            return AssetRole::Royalty;
        }
        match Cip67Label::of(name_hex) {
            Some(Cip67Label::Reference) => AssetRole::ReferenceMetadata,
            Some(Cip67Label::FungibleToken) => AssetRole::FungibleToken,
            // A `444` edition is held, traded and displayed like an NFT; that
            // it has a quantity does not make it plumbing.
            Some(Cip67Label::UserNft | Cip67Label::RichFungible) => AssetRole::Collectible,
            // Unlabelled: CIP-25, which is most of the chain.
            None => AssetRole::Collectible,
        }
    }

    /// Classify a RAW asset name — the ledger's own bytes. See
    /// [`Cip67Label::of_bytes`] for why this exists beside [`Self::of`].
    #[must_use]
    pub fn of_bytes(name: &[u8]) -> Self {
        if name.is_empty() {
            return AssetRole::Royalty;
        }
        match Cip67Label::of_bytes(name) {
            Some(Cip67Label::Reference) => AssetRole::ReferenceMetadata,
            Some(Cip67Label::FungibleToken) => AssetRole::FungibleToken,
            Some(Cip67Label::UserNft | Cip67Label::RichFungible) => AssetRole::Collectible,
            None => AssetRole::Collectible,
        }
    }

    /// Does this asset take a place in the collection — counted in supply,
    /// seated in a holder distribution, drawn as a dot?
    ///
    /// A named decision rather than an `is_*` predicate, so a caller `match`es
    /// it and a new role forces every counting site to say what it means
    /// rather than silently falling into whichever branch `_` covered.
    #[must_use]
    pub fn standing(self) -> UnitStanding {
        match self {
            AssetRole::Collectible => UnitStanding::Unit,
            AssetRole::ReferenceMetadata | AssetRole::Royalty | AssetRole::FungibleToken => {
                UnitStanding::Plumbing
            }
        }
    }
}

/// Whether an asset is a unit of the collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitStanding {
    /// Counted in supply and seated in a distribution.
    Unit,
    /// Exists on chain and belongs to nobody's holdings — metadata twins,
    /// royalty declarations, fungible tranches.
    Plumbing,
}

/// The units among a policy's asset names, in order — the supply-counting
/// filter as one call, so a caller cannot apply half of it.
pub fn units<'a, I>(names: I) -> impl Iterator<Item = &'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    names
        .into_iter()
        .filter(|n| AssetRole::of(n).standing() == UnitStanding::Unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hex of `MD0001`, the Mekka S1 asset name under both labels.
    const MEKKA: &str = "4d4430303031";

    #[test]
    fn a_cip68_pair_is_one_unit_not_two() {
        let reference = format!("{}{MEKKA}", Cip67Label::Reference.as_hex());
        let user = format!("{}{MEKKA}", Cip67Label::UserNft.as_hex());
        assert_eq!(AssetRole::of(&reference), AssetRole::ReferenceMetadata);
        assert_eq!(AssetRole::of(&user), AssetRole::Collectible);
        assert_eq!(
            units([reference.as_str(), user.as_str()]).count(),
            1,
            "5,000 NFTs is 10,000 assets and 5,000 units — counting both is \
             what invented a 51% whale out of the metadata vault"
        );
    }

    /// The `+1` on Perps: 6,000 NFTs reading as "6,001 units".
    #[test]
    fn the_royalty_token_is_not_a_unit() {
        assert_eq!(AssetRole::of(""), AssetRole::Royalty);
        assert_eq!(AssetRole::of("").standing(), UnitStanding::Plumbing);
    }

    /// CIP-25 is unlabelled and is most of the chain. An unlabelled name must
    /// never be mistaken for plumbing — that would empty whole collections.
    #[test]
    fn an_unlabelled_name_is_a_collectible() {
        // "Perp2214", and a name too short to carry a label at all.
        for name in ["5065727032323134", "ab"] {
            assert_eq!(AssetRole::of(name), AssetRole::Collectible, "{name}");
        }
    }

    /// Asset names travel as hex from several upstreams and nothing pins the
    /// case. An upper-cased reference token that read as unlabelled would slip
    /// straight back into the supply count.
    #[test]
    fn a_label_is_recognised_in_either_case() {
        let upper = format!("000643B0{MEKKA}");
        assert_eq!(AssetRole::of(&upper), AssetRole::ReferenceMetadata);
    }

    /// A `333` tranche is a currency; a `444` edition is held like an NFT.
    #[test]
    fn fungible_labels_are_told_apart() {
        let ft = format!("{}{MEKKA}", Cip67Label::FungibleToken.as_hex());
        let rft = format!("{}{MEKKA}", Cip67Label::RichFungible.as_hex());
        assert_eq!(AssetRole::of(&ft).standing(), UnitStanding::Plumbing);
        assert_eq!(AssetRole::of(&rft).standing(), UnitStanding::Unit);
    }

    /// Every label round-trips through its own hex, so `as_hex` and `of`
    /// cannot drift apart as labels are added.
    #[test]
    fn every_label_round_trips() {
        for label in Cip67Label::ALL {
            assert_eq!(Cip67Label::of(label.as_hex()), Some(label));
            assert_eq!(
                Cip67Label::of(&format!("{}{MEKKA}", label.as_hex())),
                Some(label)
            );
        }
    }

    /// THE TWO SPELLINGS MUST AGREE. A walker classifying raw bytes and a
    /// frontend classifying hex are answering one question about one asset;
    /// if they ever disagreed, the header and the field would disagree with
    /// each other and only one of them could be right.
    #[test]
    fn bytes_and_hex_classify_alike() {
        let cases: [&[u8]; 6] = [
            b"",                                   // royalty
            b"Perp2214",                           // CIP-25
            &[0x00, 0x06, 0x43, 0xb0, b'M', b'D'], // 100
            &[0x00, 0x0d, 0xe1, 0x40, b'M', b'D'], // 222
            &[0x00, 0x14, 0xdf, 0x10, b'M', b'D'], // 333
            &[0x00, 0x1b, 0xc2, 0x80, b'M', b'D'], // 444
        ];
        for raw in cases {
            let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                AssetRole::of_bytes(raw),
                AssetRole::of(&hex),
                "raw {raw:?} and hex {hex:?} disagree"
            );
        }
    }
}
