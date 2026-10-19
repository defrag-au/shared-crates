//! Browsing a whole collection, against live responses.
//!
//! Fixtures captured on 2026-10-19 with `curl` from `ownership.cnft.dev`:
//!
//! - `toolheads-rarest.json`: `?limit=3&traits=decoded&sort=rarity`
//! - `toolheads-name-page.json`: `?limit=3&traits=decoded` (with a cursor)
//! - `collections-head.json`: `/api/collections`, the first three policies
//!   of the live list, every field as sent.

use cnft_holdings::{Browse, CollectionList, OWNERSHIP_BASE, Order, PolicyId};

const TOOLHEADS: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";
const RAREST: &str = include_str!("fixtures/toolheads-rarest.json");
const BY_NAME: &str = include_str!("fixtures/toolheads-name-page.json");
const COLLECTIONS: &str = include_str!("fixtures/collections-head.json");

fn browse(order: Order, page: u32) -> Browse {
    Browse::new(
        OWNERSHIP_BASE,
        PolicyId::new(TOOLHEADS).unwrap(),
        order,
        page,
    )
}

#[test]
fn rarity_order_pages_by_offset_and_only_when_asked() {
    let mut b = browse(Order::Rarity, 3);
    let first = b.next_request().expect("the first page is wanted at once");
    assert_eq!(
        first.url,
        format!(
            "https://ownership.cnft.dev/api/bundle/{TOOLHEADS}?limit=3&traits=decoded&sort=rarity"
        )
    );
    assert!(b.loading());
    assert_eq!(b.next_request(), None, "one page at a time");

    b.on_response(Ok(RAREST));
    assert_eq!(b.assets().len(), 3);
    assert_eq!(b.total(), Some(6600));
    assert!(!b.exhausted());
    assert_eq!(b.next_request(), None, "not wanted yet");

    b.want_more();
    b.want_more();
    let second = b.next_request().expect("wanted now");
    assert!(
        second.url.contains("&offset=3&sort=rarity"),
        "{}",
        second.url
    );
    assert_eq!(b.next_request(), None, "two asks are one page");
}

#[test]
fn a_short_page_ends_a_rarity_browse() {
    let mut b = browse(Order::Rarity, 10);
    b.next_request();
    b.on_response(Ok(RAREST));
    assert!(b.exhausted(), "3 entries for a page of 10 is the last page");
    b.want_more();
    assert_eq!(b.next_request(), None);
}

#[test]
fn name_order_follows_the_cursor_and_stops_when_it_stops_moving() {
    let mut b = browse(Order::Name, 3);
    let first = b.next_request().unwrap();
    assert!(!first.url.contains("sort=") && !first.url.contains("cursor="));
    b.on_response(Ok(BY_NAME));
    assert_eq!(b.assets()[0].name, "Toolhead #0001", "the service's order");

    b.want_more();
    let second = b.next_request().unwrap();
    assert!(second.url.ends_with("&cursor=546f6f6c6865616430303033"));

    // The same page again: same cursor, nothing new.
    b.on_response(Ok(BY_NAME));
    assert_eq!(b.assets().len(), 3, "repeats are not doubled");
    assert!(b.exhausted(), "a cursor that does not move ends the browse");
}

#[test]
fn a_failed_page_waits_for_a_retry() {
    let mut b = browse(Order::Rarity, 3);
    b.next_request();
    b.on_response(Err("HTTP 503: busy".into()));
    assert_eq!(b.error(), Some("HTTP 503: busy"));
    b.want_more();
    assert_eq!(b.next_request(), None, "no hammering a failing service");

    b.retry();
    let again = b.next_request().expect("the same page again");
    assert!(!again.url.contains("offset="));
}

#[test]
fn the_collection_list_parses_sorts_and_filters() {
    let list = CollectionList::parse(COLLECTIONS).unwrap();
    let names: Vec<&str> = list.policies.iter().map(|c| c.name()).collect();
    assert_eq!(names, ["@Profiles", "Abstractum", "ADA Inmates"]);
    assert_eq!(list.policies[2].asset_count, Some(5000));
    assert_eq!(list.policies[2].holder_count, Some(506));

    let hits: Vec<&str> = list.matching("ad").map(|c| c.name()).collect();
    assert_eq!(hits, ["ADA Inmates"]);
    assert_eq!(list.matching("  ").count(), 3);
    assert_eq!(list.matching("ab6d24").count(), 1, "by policy id too");
    assert_eq!(
        CollectionList::request("https://ownership.cnft.dev/").url,
        "https://ownership.cnft.dev/api/collections"
    );
}
