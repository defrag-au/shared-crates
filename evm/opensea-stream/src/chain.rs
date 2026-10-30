//! OpenSea's chain slug, and which chain that is.
//!
//! The stream names a chain the way OpenSea does — `"robinhood"`, `"base"`,
//! `"ronin"` — in a movement's `chain` field and at the front of every `nft_id`.
//! It is a **slug, not an id**: `GET /chains` returns a slug, a name, a symbol and a
//! block explorer, and no chain id at all. So the slug→id pairing cannot be read
//! from the API and is not derivable; it has to be stated, once, with the evidence
//! beside it.
//!
//! # The table
//!
//! | OpenSea slug | Our identity | How the pair was established |
//! | --- | --- | --- |
//! | `ethereum` | `eip155:1` | Ethereum's EIP-155 id is `1`; the slug is the one OpenSea's own chain list uses |
//! | `optimism` | `eip155:10` | OP Mainnet's EIP-155 id is `10`; slug as above |
//! | `polygon` | `eip155:137` | Polygon PoS's EIP-155 id is `137`; slug as above |
//! | `base` | `eip155:8453` | Base's EIP-155 id is `8453`; slug as above |
//! | `robinhood` | `eip155:4663` | `chains::EvmChain::Robinhood` is 4663, verified against the live chain via `eth_chainId`; `robinhood` is the slug the stream sends for that chain's collections |
//! | `arbitrum` | `eip155:42161` | Arbitrum One's EIP-155 id is `42161`; slug as above |
//!
//! The five rows added on 2026-10-18 pair OpenSea's documented chain slugs with the
//! ids in the `chainid.network` registry, checked against that registry on the same
//! day. The Robinhood row is the older one and rests on `eth_chainId`.
//!
//! ## The registry batch (2026-10-18)
//!
//! The rest pair OpenSea's documented `ChainIdentifier` slugs with the same
//! registry's entry for that chain, read from
//! `https://chainid.network/chains_mini.json`. The single-NFT endpoint is
//! chain-uniform, so these need no per-chain code path.
//!
//! | OpenSea slug | EIP-155 id | registry name |
//! | --- | --- | --- |
//! | `unichain` | 130 | Unichain |
//! | `monad` | 143 | Monad |
//! | `shape` | 360 | Shape |
//! | `stablechain` | 988 | Stable Mainnet |
//! | `sei` | 1329 | Sei Network |
//! | `soneium` | 1868 | Soneium |
//! | `ronin` | 2020 | Ronin Mainnet |
//! | `abstract` | 2741 | Abstract |
//! | `megaeth` | 4326 | MegaETH Mainnet |
//! | `somnia` | 5031 | Somnia Mainnet |
//! | `ape_chain` | 33139 | ApeChain |
//! | `gunzilla` | 43419 | GUNZ |
//! | `ink` | 57073 | Ink |
//! | `animechain` | 69000 | Animechain Mainnet |
//! | `zora` | 7777777 | Zora |
//! | `bera_chain` | 80094 | Berachain |
//! | `blast` | 81457 | Blast |
//! | `avalanche` | 43114 | Avalanche C-Chain |
//!
//! `hyperevm` and `hyperliquid` are deliberately absent: the registry carries
//! only a Hyperliquid EVM **testnet** (998), and guessing a mainnet id is exactly
//! what this table refuses to do. `flow` is not an EVM chain.
//!
//! Adding a row means both halves: see the slug OpenSea actually uses, and see the id
//! from the chain itself (or from a second source that agrees). A row added from
//! memory is a mapping that looks right and sends movements to the wrong chain's
//! collection.
//!
//! [`chain_ref`] returns `None` for every other slug rather than guessing, which is
//! the same call [`chains::EvmChain::Id`] makes for an EIP-155 id it has not named:
//! meeting a chain we do not know yet is normal, and inventing an id for it is not.
//!
//! # Solana
//!
//! OpenSea indexes Solana under the slug `solana`, but the slug names no cluster —
//! the same string covers mainnet, devnet and testnet. So [`chain_slug`] maps
//! mainnet to it one way, and [`chain_ref`] never maps it back: recovering a
//! `SolanaNetwork` from `"solana"` would be a guess, and the wrong cluster's mint is
//! a different asset.

use chains::ChainRef;

/// The OpenSea chain slug for Robinhood Chain.
pub const ROBINHOOD_SLUG: &str = "robinhood";

/// Every OpenSea slug we can name, with the EIP-155 chain id it denotes.
///
/// One table, read by both directions, so a slug cannot be added on one side and
/// forgotten on the other.
const SLUGS: &[(&str, u64)] = &[
    ("ethereum", 1),
    ("optimism", 10),
    ("unichain", 130),
    ("polygon", 137),
    ("monad", 143),
    ("shape", 360),
    ("stablechain", 988),
    ("sei", 1329),
    ("soneium", 1868),
    ("ronin", 2020),
    ("abstract", 2741),
    ("megaeth", 4326),
    ("robinhood", 4663),
    ("somnia", 5031),
    ("base", 8453),
    ("ape_chain", 33139),
    ("arbitrum", 42161),
    ("avalanche", 43114),
    ("gunzilla", 43419),
    ("ink", 57073),
    ("animechain", 69000),
    ("zora", 7777777),
    ("bera_chain", 80094),
    ("blast", 81457),
];

/// The chain a stream slug names, where we can name it.
///
/// Case-insensitive, because the slug is OpenSea's to spell and nothing promises
/// its case — the same tolerance [`chains::CardanoNetwork::from_chain_str`] takes.
///
/// `None` is not a failure: it is a chain we have not checked yet, and the caller
/// says what it wants to do about that rather than being handed a wrong id.
pub fn chain_ref(slug: &str) -> Option<ChainRef> {
    let slug = slug.trim();
    SLUGS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(slug))
        .map(|(_, id)| ChainRef::Evm(chains::EvmChain::from_chain_id(*id)))
}

/// The OpenSea slug for a chain we can name, where there is one.
///
/// For EVM it is the inverse of [`chain_ref`], derived from the same table rather
/// than written out a second time. Solana is one-way: the slug does not name a
/// cluster, so only mainnet has an answer, and only in this direction.
pub fn chain_slug(chain: ChainRef) -> Option<&'static str> {
    match chain {
        ChainRef::Evm(evm) => SLUGS
            .iter()
            .find(|(_, id)| *id == evm.chain_id())
            .map(|(slug, _)| *slug),
        ChainRef::Solana(chains::SolanaNetwork::Mainnet) => Some("solana"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robinhood_is_chain_4663() {
        // Asserted against `chains` rather than against a literal here, so the slug
        // and the id cannot drift apart: `from_chain_id` is what names the variant.
        let chain = chain_ref(ROBINHOOD_SLUG).expect("robinhood is in the table");
        assert_eq!(chain, ChainRef::Evm(chains::EvmChain::Robinhood));
        assert_eq!(chain.as_caip2(), "eip155:4663");
    }

    #[test]
    fn the_slug_is_matched_without_regard_to_case_or_padding() {
        assert_eq!(chain_ref("Robinhood"), chain_ref("  robinhood "));
    }

    #[test]
    fn every_named_chain_round_trips() {
        for (slug, id) in SLUGS {
            let chain = chain_ref(slug).unwrap_or_else(|| panic!("{slug} is in the table"));
            assert_eq!(chain.as_caip2(), format!("eip155:{id}"));
            assert_eq!(chain_slug(chain), Some(*slug));
        }
    }

    #[test]
    fn solana_mainnet_has_a_slug_and_the_other_clusters_do_not() {
        use chains::SolanaNetwork;

        assert_eq!(
            chain_slug(ChainRef::Solana(SolanaNetwork::Mainnet)),
            Some("solana")
        );
        assert_eq!(chain_slug(ChainRef::Solana(SolanaNetwork::Devnet)), None);
        assert_eq!(chain_slug(ChainRef::Solana(SolanaNetwork::Testnet)), None);

        // The slug cannot name a cluster, so the reverse direction refuses to guess.
        assert_eq!(chain_ref("solana"), None);
    }

    #[test]
    fn the_public_chains_map_to_their_eip155_ids() {
        // The five rows added for the metadata index: the id half is what the
        // `chainid.network` registry states, and the inverse must find the slug.
        assert_eq!(
            chain_ref("base"),
            Some(ChainRef::Evm(chains::EvmChain::Id(8453)))
        );
        assert_eq!(
            chain_ref("arbitrum"),
            Some(ChainRef::Evm(chains::EvmChain::Id(42161)))
        );
        assert_eq!(
            chain_ref("polygon"),
            Some(ChainRef::Evm(chains::EvmChain::Id(137)))
        );
        assert_eq!(
            chain_ref("optimism"),
            Some(ChainRef::Evm(chains::EvmChain::Id(10)))
        );
        assert_eq!(
            chain_ref("ethereum"),
            Some(ChainRef::Evm(chains::EvmChain::Ethereum))
        );
    }

    #[test]
    fn a_chain_we_have_not_checked_is_unnamed_rather_than_guessed() {
        // `hyperevm` is in OpenSea's chain list but has no mainnet id in the
        // `chainid.network` registry, so it is deliberately absent from the
        // table. A wrong id is worse than an absent one: it would point a
        // reconcile at another chain. (`abstract` was the example here until it
        // was verified and added.)
        assert_eq!(chain_ref("hyperevm"), None);
    }

    #[test]
    fn the_slug_round_trips_from_the_chain() {
        let chain = chain_ref(ROBINHOOD_SLUG).unwrap();
        assert_eq!(chain_slug(chain), Some(ROBINHOOD_SLUG));
        assert_eq!(
            chain_slug(ChainRef::Cardano(chains::CardanoNetwork::Mainnet)),
            None
        );
        assert_eq!(
            chain_slug(ChainRef::Evm(chains::EvmChain::Id(999_999))),
            None
        );
    }
}
