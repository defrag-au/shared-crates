//! The four market-intent types and the revalidate, each against a captured frame.
//!
//! These exist because the guide is not the wire. A `collection_offer` and an
//! `order_revalidate` both carry `"item": {}`; a bid carries `created_date` where a
//! listing carries `listing_date`. A type written from the documentation would have
//! failed on the first real frame.

use opensea_stream::{StreamEvent, TraitCriterion};

const ITEM_LISTED: &str = include_str!("fixtures/item_listed.json");
const ITEM_RECEIVED_BID: &str = include_str!("fixtures/item_received_bid.json");
const COLLECTION_OFFER: &str = include_str!("fixtures/collection_offer.json");
const TRAIT_OFFER: &str = include_str!("fixtures/trait_offer.json");
const ORDER_REVALIDATE: &str = include_str!("fixtures/order_revalidate.json");

#[test]
fn a_listing_carries_its_ask_and_the_order_it_placed() {
    let event = StreamEvent::from_wire(ITEM_LISTED).unwrap().unwrap();
    let StreamEvent::ItemListed(listed) = event else {
        panic!("expected a listing");
    };

    assert_eq!(listed.chain, "bera_chain");
    assert_eq!(listed.collection.slug, "steady-teddys-bera");
    assert_eq!(listed.base_price, 88_956_480_000_000_000_000);
    assert_eq!(listed.payment_token.symbol, "BERA");
    assert_eq!(listed.payment_token.decimals, 18);
    assert_eq!(
        listed.order_hash,
        "0x9348ccf516128754670f0ba9dea07518bdd82638814e34db36af264fe0bee2c0"
    );
    assert_eq!(
        listed.item.nft_id,
        "bera_chain/0x88888888a9361f15aadbaca355a6b2938c6a674e/8502"
    );
}

#[test]
fn a_listing_reports_a_placement_date_where_a_bid_reports_a_creation_date() {
    let listed = StreamEvent::from_wire(ITEM_LISTED).unwrap().unwrap();
    let StreamEvent::ItemListed(listed) = listed else {
        panic!("expected a listing");
    };
    assert_eq!(
        listed.listing_date.as_deref(),
        Some("2026-10-29T07:24:33.000000Z")
    );

    let bid = StreamEvent::from_wire(ITEM_RECEIVED_BID).unwrap().unwrap();
    let StreamEvent::ItemReceivedBid(bid) = bid else {
        panic!("expected a bid");
    };
    assert_eq!(
        bid.created_date.as_deref(),
        Some("2026-10-29T07:24:33.000000Z")
    );
    assert_eq!(bid.base_price, 196_000_000_000_000_000);
    assert_eq!(bid.payment_token.symbol, "WETH");
    assert_eq!(bid.collection.slug, "good-vibes-club");
}

#[test]
fn a_collection_offer_is_scoped_by_contract_and_collection_rather_than_by_item() {
    let event = StreamEvent::from_wire(COLLECTION_OFFER).unwrap().unwrap();
    let StreamEvent::CollectionOffer(offer) = event else {
        panic!("expected a collection offer");
    };

    assert_eq!(
        offer
            .asset_contract_criteria
            .as_ref()
            .map(|criteria| criteria.address.as_str()),
        Some("0x19875070bb70abbb31e248e48c9b617d9d14029e")
    );
    assert_eq!(
        offer
            .collection_criteria
            .as_ref()
            .map(|criteria| criteria.slug.as_str()),
        Some("we-are-the-skin-tags-on-the-sagging-ass-of-humanit")
    );
    assert_eq!(offer.base_price, 758_000_000_000_000_000);
    assert_eq!(offer.quantity, 1);
}

#[test]
fn a_trait_offer_names_the_constraint_it_bids_against() {
    let event = StreamEvent::from_wire(TRAIT_OFFER).unwrap().unwrap();
    let StreamEvent::TraitOffer(offer) = event else {
        panic!("expected a trait offer");
    };

    assert_eq!(
        offer.trait_criteria,
        Some(TraitCriterion {
            trait_type: "Color Band".to_owned(),
            trait_name: "Twenty".to_owned(),
        })
    );
    // The frame carries the same constraint twice, so both fields are read and both
    // agree; a type that kept only one would have to pick without evidence.
    assert_eq!(offer.trait_criteria_list.len(), 1);
    assert_eq!(
        offer.trait_criteria_list[0],
        offer.trait_criteria.clone().unwrap()
    );
    assert_eq!(offer.base_price, 115_000_000_000_000_000);
}

#[test]
fn a_revalidation_carries_no_item_even_though_it_names_an_order() {
    let event = StreamEvent::from_wire(ORDER_REVALIDATE).unwrap().unwrap();
    let StreamEvent::OrderRevalidate(order) = event else {
        panic!("expected a revalidation");
    };

    assert_eq!(order.collection.slug, "streethoodminis");
    assert_eq!(
        order.order_hash,
        "0x56e64fad8c81401b8d7bca874d2c53394ab034ad68cd90f7e108592b47da0357"
    );
    // `"item": {}` on the wire. `None` rather than an invented item, and the same
    // absence the invalidate can have.
    assert!(order.item.is_none());
}
