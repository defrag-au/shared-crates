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
//! | `robinhood` | `eip155:4663` | `chains::EvmChain::Robinhood` is 4663, verified against the live chain via `eth_chainId`; `robinhood` is the slug the stream sends for that chain's collections |
//!
//! One row, and that is the honest size of what has been checked. Adding a row means
//! both halves: see the slug on a captured frame's `chain` field, and see the id
//! from the chain itself (or from a second source that agrees). A row added from
//! memory is a mapping that looks right and sends movements to the wrong chain's
//! collection.
//!
//! [`chain_ref`] returns `None` for every other slug rather than guessing, which is
//! the same call [`chains::EvmChain::Id`] makes for an EIP-155 id it has not named:
//! meeting a chain we do not know yet is normal, and inventing an id for it is not.

use chains::ChainRef;

/// The OpenSea chain slug for Robinhood Chain.
pub const ROBINHOOD_SLUG: &str = "robinhood";

/// The chain a stream slug names, where we can name it.
///
/// Case-insensitive, because the slug is OpenSea's to spell and nothing promises
/// its case — the same tolerance [`chains::CardanoNetwork::from_chain_str`] takes.
///
/// `None` is not a failure: it is a chain we have not checked yet, and the caller
/// says what it wants to do about that rather than being handed a wrong id.
pub fn chain_ref(slug: &str) -> Option<ChainRef> {
    match slug.trim().to_ascii_lowercase().as_str() {
        ROBINHOOD_SLUG => Some(ChainRef::Evm(chains::EvmChain::Robinhood)),
        _ => None,
    }
}

/// The OpenSea slug for a chain we can name, where there is one.
///
/// The inverse of [`chain_ref`], and derived from the same table rather than written
/// out a second time.
pub fn chain_slug(chain: ChainRef) -> Option<&'static str> {
    match chain {
        ChainRef::Evm(chains::EvmChain::Robinhood) => Some(ROBINHOOD_SLUG),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robinhood_is_chain_4663() {
        // The one row in the table, asserted against `chains` rather than against a
        // literal here, so the slug and the id cannot drift apart.
        let chain = chain_ref(ROBINHOOD_SLUG).expect("robinhood is in the table");
        assert_eq!(chain, ChainRef::Evm(chains::EvmChain::Robinhood));
        assert_eq!(chain.as_caip2(), "eip155:4663");
    }

    #[test]
    fn the_slug_is_matched_without_regard_to_case_or_padding() {
        assert_eq!(chain_ref("Robinhood"), chain_ref("  robinhood "));
    }

    #[test]
    fn a_chain_we_have_not_checked_is_unnamed_rather_than_guessed() {
        // Both appear on captured frames — `base` on a transfer, `abstract` on a
        // metadata update — and neither has a verified chain id here. A wrong id is
        // worse than an absent one: it would point a reconcile at another chain.
        assert_eq!(chain_ref("base"), None);
        assert_eq!(chain_ref("abstract"), None);
    }

    #[test]
    fn the_slug_round_trips_from_the_chain() {
        let chain = chain_ref(ROBINHOOD_SLUG).unwrap();
        assert_eq!(chain_slug(chain), Some(ROBINHOOD_SLUG));
        assert_eq!(
            chain_slug(ChainRef::Cardano(chains::CardanoNetwork::Mainnet)),
            None
        );
    }
}
