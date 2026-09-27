//! Parsing tests over captured OpenSea responses.
//!
//! Each fixture under `tests/fixtures/` is the **verbatim body** of one live
//! request, captured from `api.opensea.io` on 2026-09-27 and promoted to a file
//! without editing. These tests therefore assert the API's shape as it really
//! answered, offline and deterministically — no key, no network, nothing to
//! flake.
//!
//! What they cannot tell you is whether OpenSea still answers that way. That is
//! `tests/live.rs`, behind the `live` feature.

use opensea_client::{
    ChainInfo, ChainSlug, CollectionDetails, CollectionPage, CollectionStats, Contract, NftPage,
    OpenseaClient,
};

const CHAINS: &str = include_str!("fixtures/chains.json");
const COLLECTIONS_PAGE: &str = include_str!("fixtures/collections_robinhood_page.json");
const COLLECTION: &str = include_str!("fixtures/collection_heritage_hood.json");
const COLLECTION_STATS: &str = include_str!("fixtures/collection_heritage_hood_stats.json");
const CONTRACT: &str = include_str!("fixtures/contract_stonkbrokers.json");
const NFTS_PAGE: &str = include_str!("fixtures/contract_stonkbrokers_nfts_page.json");

/// `/collections?chain=…` returns the bare array inside a named envelope, and
/// `chains()` is the only method that unwraps one; the rest are one-to-one with
/// their body. This is that envelope.
#[derive(serde::Deserialize)]
struct ChainEnvelope {
    chains: Vec<ChainInfo>,
}

#[test]
fn chains_parse_and_robinhood_is_named() {
    let envelope: ChainEnvelope = serde_json::from_str(CHAINS).expect("chains.json should parse");

    // The capture carries thirty chains; the count pins the fixture against
    // accidental truncation rather than asserting anything about OpenSea today.
    assert_eq!(envelope.chains.len(), 30);

    let robinhood = envelope
        .chains
        .iter()
        .find(|chain| chain.chain.as_str() == "robinhood")
        .expect("robinhood should be one of the captured chains");
    assert_eq!(robinhood.name, "Robinhood Chain");
    assert_eq!(robinhood.symbol, "ETH");
    assert!(robinhood.supports_swaps);
    assert_eq!(robinhood.block_explorer_url, "https://robin.etherscan.io");

    // A `false` in the capture, so the bool is parsed rather than defaulted.
    let sei = envelope
        .chains
        .iter()
        .find(|chain| chain.chain.as_str() == "sei")
        .expect("sei should be one of the captured chains");
    assert!(!sei.supports_swaps);
}

#[test]
fn chain_slug_is_the_bare_string_on_the_wire() {
    let slug: ChainSlug = serde_json::from_str("\"robinhood\"").expect("a slug should parse");
    assert_eq!(slug, ChainSlug::new("robinhood"));
    assert_eq!(slug.to_string(), "robinhood");

    // The shape a derived nested enum would have written. If this ever parses,
    // the wire form has regressed to something that no longer matches
    // `"chain": "robinhood"`.
    assert!(serde_json::from_str::<ChainSlug>("{\"Robinhood\":null}").is_err());
}

#[test]
fn a_collections_page_carries_its_next_cursor() {
    let page: CollectionPage =
        serde_json::from_str(COLLECTIONS_PAGE).expect("the page should parse");

    assert_eq!(page.collections.len(), 3);
    assert_eq!(page.collections[0].collection, "the-duck-hoood");

    // The third entry is the one also captured in detail, and it has no twitter
    // handle while its neighbours do — so `Option` is doing real work here.
    let heritage = &page.collections[2];
    assert_eq!(heritage.name, "Heritage Hood");
    assert_eq!(heritage.twitter_username, None);
    assert_eq!(
        page.collections[0].twitter_username.as_deref(),
        Some("KenzoA283019")
    );

    // `banner_image_url` is null on a list entry and a URL in the detail
    // response; `image_url` is populated on both.
    assert_eq!(heritage.banner_image_url, None);
    assert!(heritage.image_url.is_some());

    let cursor = page
        .next
        .expect("a three-item page should offer a next cursor");
    assert!(!cursor.is_empty());
}

#[test]
fn collection_detail_carries_what_the_list_does_not() {
    let detail: CollectionDetails =
        serde_json::from_str(COLLECTION).expect("the collection should parse");

    assert_eq!(detail.summary.collection, "heritage-hood");
    assert_eq!(detail.total_supply, 3306);
    assert_eq!(detail.unique_item_count, 3306);
    assert_eq!(detail.created_date, "2026-09-26");

    assert_eq!(detail.editors.len(), 1);
    assert_eq!(detail.fees.len(), 1);
    assert_eq!(detail.fees[0].fee, 1.0);
    assert!(detail.fees[0].required);

    let listing = &detail.pricing_currencies.listing_currency;
    assert_eq!(listing.symbol, "USDG");
    assert_eq!(listing.decimals, 6);
    // Prices stay text, exactly as sent — including the trailing zeros a float
    // would have dropped.
    assert_eq!(listing.usd_price, "0.999948");
    assert_eq!(detail.pricing_currencies.offer_currency, *listing);

    assert_eq!(
        detail.summary.contracts[0].address,
        "0xbe269fd5a147ca1601bb91ecf86967bee4a02c36"
    );
}

#[test]
fn the_shared_summary_agrees_with_the_detail_response_except_the_banner() {
    // `CollectionDetails` flattens `CollectionSummary`, so the overlap is the
    // part worth checking: the same collection captured by two endpoints must
    // produce equal values for every field they share.
    let page: CollectionPage =
        serde_json::from_str(COLLECTIONS_PAGE).expect("the page should parse");
    let detail: CollectionDetails =
        serde_json::from_str(COLLECTION).expect("the collection should parse");

    let from_list = page
        .collections
        .iter()
        .find(|collection| collection.collection == detail.summary.collection)
        .expect("heritage-hood is in both captures");

    // Exactly one field differs, and it is a property of the API rather than of
    // this parse: the list leaves the banner null and the detail populates it.
    // Asserting the difference rather than ignoring the field keeps the equality
    // below exhaustive — a second divergence would fail it.
    assert_eq!(from_list.banner_image_url, None);
    assert!(detail.summary.banner_image_url.is_some());

    let mut expected = detail.summary.clone();
    expected.banner_image_url = None;
    assert_eq!(*from_list, expected);
}

#[test]
fn stats_parse_into_a_total_and_three_intervals() {
    let stats: CollectionStats =
        serde_json::from_str(COLLECTION_STATS).expect("the stats should parse");

    assert_eq!(stats.total.sales, 3306);
    assert_eq!(stats.total.num_owners, 669);
    assert_eq!(stats.total.volume, 0.0);
    assert_eq!(stats.total.volume_symbol, "ETH");

    // Note the floor is priced in a different currency from the volume: the
    // USDG floor is what a buyer pays today, the ETH volume is what has traded.
    assert_eq!(stats.total.floor_price, Some(0.15));
    assert_eq!(stats.total.floor_price_symbol, "USDG");

    let intervals: Vec<&str> = stats
        .intervals
        .iter()
        .map(|interval| interval.interval.as_str())
        .collect();
    assert_eq!(intervals, vec!["one_day", "seven_day", "thirty_day"]);
}

#[test]
fn a_contract_names_its_collection_and_standard() {
    let contract: Contract = serde_json::from_str(CONTRACT).expect("the contract should parse");

    assert_eq!(
        contract.address,
        "0x539cdd042c2f3d93ebc5be7dfff0c79f3b4fabf0"
    );
    assert_eq!(contract.chain, ChainSlug::new(ChainSlug::ROBINHOOD));
    assert_eq!(contract.contract_standard, "erc721");
    assert_eq!(contract.name.as_deref(), Some("StonkBrokers"));

    // The collection slug here is *not* the slug in the OpenSea URL, which is
    // exactly why the two are separate fields rather than one.
    assert_eq!(contract.collection, "stonkbrokers-434284142");
}

#[test]
fn an_nfts_page_parses_tokens_and_their_traits() {
    let page: NftPage = serde_json::from_str(NFTS_PAGE).expect("the nfts page should parse");

    assert_eq!(page.nfts.len(), 2);

    let broker = &page.nfts[0];
    assert_eq!(broker.identifier, "1672");
    assert_eq!(broker.token_standard, "erc721");
    assert_eq!(broker.name.as_deref(), Some("Stonk Broker #1672"));
    assert_eq!(broker.traits.len(), 12);

    let wallet = broker
        .traits
        .iter()
        .find(|trait_| trait_.trait_type == "Wallet Address")
        .expect("a broker carries its token-bound wallet as a trait");
    assert_eq!(wallet.value, "0xf9facb7bbbe1232dd8013d3e67fbc4e993f8ae9d");

    // Both are null for every token in the capture, and modelled as `Option`
    // because of it.
    assert_eq!(broker.estimated_value_usd, None);
    assert_eq!(broker.decimals, None);

    assert!(page.next.is_some());
}

#[test]
fn a_public_client_can_be_built_without_a_key() {
    // Compile-and-construct only: no request is made, so no network is touched.
    let _ = OpenseaClient::without_key();
    let _ = OpenseaClient::new("not-a-real-key");
}
