#![cfg(feature = "live")]

//! Live API tests — the only part of this crate that touches the network.
//!
//! Off by default. CI runs `cargo test --workspace --all-targets` without
//! `--all-features`, so this file compiles to nothing there and no build needs a
//! credential. Run it deliberately, with a key in the environment:
//!
//! ```sh
//! OPENSEA_API_KEY=… cargo test -p opensea-client --features live
//! ```
//!
//! These assert only what is stable about the chain rather than about the
//! market: that Robinhood Chain is an indexed chain, that its collections answer
//! and belong to it, and that a known contract still resolves. Counts and floors
//! move hourly and are deliberately not asserted.

use opensea_client::{ChainSlug, OpenseaClient};

/// The StonkBrokers contract, captured on 2026-09-27. A fixed set of 4,444
/// tokens that is fully minted, so the address is stable even as the market moves.
const STONKBROKERS: &str = "0x539cdd042c2f3d93ebc5be7dfff0c79f3b4fabf0";

fn client() -> OpenseaClient {
    let key = std::env::var("OPENSEA_API_KEY")
        .expect("OPENSEA_API_KEY must be set when the `live` feature is enabled");
    OpenseaClient::new(key)
}

#[tokio::test]
async fn robinhood_chain_is_indexed() {
    let chains = client().chains().await.expect("chains should answer");

    let robinhood = chains
        .iter()
        .find(|chain| chain.chain.as_str() == ChainSlug::ROBINHOOD)
        .expect("Robinhood Chain should be an indexed chain");
    assert_eq!(robinhood.symbol, "ETH");
}

#[tokio::test]
async fn robinhood_collections_answer_and_belong_to_the_chain() {
    let client = client();
    let page = client
        .collections_page(&ChainSlug::new(ChainSlug::ROBINHOOD), Some(3), None)
        .await
        .expect("a first page should answer");

    assert!(!page.collections.is_empty());
    for collection in &page.collections {
        assert!(!collection.contracts.is_empty());
        for contract in &collection.contracts {
            assert_eq!(contract.chain.as_str(), ChainSlug::ROBINHOOD);
        }
    }
}

#[tokio::test]
async fn a_known_contract_resolves_to_its_collection() {
    let client = client();
    let contract = client
        .contract(&ChainSlug::new(ChainSlug::ROBINHOOD), STONKBROKERS)
        .await
        .expect("the StonkBrokers contract should resolve");

    assert_eq!(contract.address, STONKBROKERS);
    assert_eq!(contract.chain.as_str(), ChainSlug::ROBINHOOD);
    assert!(contract.contract_standard.starts_with("erc"));
}

#[tokio::test]
async fn pagination_advances_then_ends() {
    let client = client();
    let chain = ChainSlug::new(ChainSlug::ROBINHOOD);

    let first = client
        .collections_page(&chain, Some(2), None)
        .await
        .expect("a first page should answer");
    let cursor = first.next.expect("a two-item page should offer a cursor");

    let second = client
        .collections_page(&chain, Some(2), Some(&cursor))
        .await
        .expect("the cursor should be accepted");
    assert!(!second.collections.is_empty());

    // The cursor moved the window; the same slug must not lead both pages.
    assert_ne!(
        first.collections[0].collection,
        second.collections[0].collection
    );
}
