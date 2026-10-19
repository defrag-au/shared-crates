//! Holdings built from real collection-ownership responses.
//!
//! Both fixtures are live responses captured on 2026-10-19 with `curl`:
//!
//! - `toolheads-rarest.json`: `/api/bundle/<Toolheads>?limit=3&traits=decoded&sort=rarity`,
//!   a CIP-25 collection, last page (no cursor).
//! - `baby-sneklets-page.json`: `/api/bundle/<Baby SNEKlets>?limit=2&traits=decoded`,
//!   a CIP-68 collection, with a `next_cursor`.
//!
//! Every response also carries fields the wire types do not name (`matched`).
//! Parsing them is part of what is tested: the service grows fields, and a
//! game must not stop loading when it does.

use cnft_holdings::{
    BundleResponse, Holdings, HoldingsLoad, LoadState, MAX_PAGES, OWNERSHIP_BASE, PolicyId, Source,
};

const TOOLHEADS: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";
const BABY_SNEKLETS: &str = "0f0dc956aa0bf7fdf513a658f305755cda09ca27579ad6459466a25d";
const TOOLHEADS_PAGE: &str = include_str!("fixtures/toolheads-rarest.json");
const SNEKLETS_PAGE: &str = include_str!("fixtures/baby-sneklets-page.json");

fn policy(p: &str) -> PolicyId {
    PolicyId::new(p).unwrap()
}

fn page(json: &str) -> BundleResponse {
    serde_json::from_str(json).expect("a live response parses")
}

#[test]
fn a_cip25_page_becomes_named_ranked_assets_rarest_first() {
    let mut h = Holdings::new(policy(TOOLHEADS));
    assert_eq!(h.absorb(page(TOOLHEADS_PAGE)), None, "the last page");

    let names: Vec<&str> = h.assets().iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        ["Toolhead #4957", "Toolhead #0794", "Toolhead #6442"]
    );

    let rarest = h.rarest().unwrap();
    assert_eq!(rarest.rarity_rank, Some(1));
    assert_eq!(
        rarest.owner_account,
        "stake1u90usm20k3svksccyrrry4gxxq2arsnnkguaee4z5tn3s9cgmhth6"
    );
    assert_eq!(rarest.trait_value("role"), Some("Star Legion"));
    assert_eq!(
        rarest.trait_values("head").unwrap(),
        ["Codec Headset", "Necromancer Processor", "Cranium Cover"]
    );
    // CIP-25: the art is the asset itself.
    assert_eq!(rarest.image_name_hex, rarest.asset_name_hex);
    assert_eq!(
        rarest.art().build(),
        format!(
            "https://iiif.hodlcroft.com/iiif/3/{TOOLHEADS}:546f6f6c6865616434393537/full/400,/0/default.auto"
        )
    );
}

#[test]
fn a_cip68_page_keeps_the_held_token_and_draws_the_reference_token() {
    let mut h = Holdings::new(policy(BABY_SNEKLETS));
    let cursor = h.absorb(page(SNEKLETS_PAGE));
    assert_eq!(
        cursor.as_deref(),
        Some("000de14042616279536e656b6c657430303033")
    );

    // Rank 919 before rank 3386, whatever order the service sent.
    let a = &h.assets()[0];
    assert_eq!(a.name, "BabySneklet #3");
    assert_eq!(a.asset_name_hex, "000de14042616279536e656b6c657430303033");
    assert_eq!(a.image_name_hex, "000643b042616279536e656b6c657430303033");
    assert!(a.has_trait("clothes", "raider's gear"));
}

#[test]
fn reference_tokens_in_a_page_are_not_holdings_and_repeats_are_not_doubled() {
    // A page as the live service would send it if a reference token were
    // seated with a holder: the Baby SNEKlets #2 entry, relabelled 100.
    let with_reference = SNEKLETS_PAGE.replacen(
        "000de14042616279536e656b6c657430303032",
        "000643b042616279536e656b6c657430303032",
        1,
    );
    let mut h = Holdings::new(policy(BABY_SNEKLETS));
    h.absorb(page(&with_reference));
    assert_eq!(h.len(), 1, "the metadata twin is nobody's holding");

    // The same page twice is the same holdings.
    h.absorb(page(SNEKLETS_PAGE));
    h.absorb(page(SNEKLETS_PAGE));
    assert_eq!(h.len(), 2);
    assert!(h.find("000DE14042616279536E656B6C657430303032").is_some());
}

#[test]
fn a_load_follows_the_cursor_then_finishes() {
    let source = Source::stakes(
        OWNERSHIP_BASE,
        ["stake1uxakmmlk2rcmxv5d3aywpe4rfrlgrq0e0s9hdvnd3ntvzjs8fa4c5"],
    )
    .unwrap();
    let mut load = HoldingsLoad::new(source, policy(BABY_SNEKLETS));

    let first = load.next_request().expect("the first page");
    assert!(!first.url.contains("cursor="));
    assert_eq!(load.next_request(), None, "one request at a time");

    load.on_response(Ok(SNEKLETS_PAGE));
    assert_eq!(load.state(), &LoadState::Loading);
    let second = load.next_request().expect("the cursor's page");
    assert!(
        second
            .url
            .ends_with("&cursor=000de14042616279536e656b6c657430303033")
    );

    load.on_response(Ok(TOOLHEADS_PAGE));
    assert_eq!(load.state(), &LoadState::Ready);
    assert_eq!(load.next_request(), None);
    assert_eq!(load.pages(), 2);
}

#[test]
fn a_repeated_cursor_or_endless_paging_ends_the_load() {
    let source = Source::worker("api/holdings", "tok");

    // The server hands back the cursor it was given: stop, do not loop.
    let mut load = HoldingsLoad::new(source.clone(), policy(BABY_SNEKLETS));
    load.next_request();
    load.on_response(Ok(SNEKLETS_PAGE));
    load.next_request();
    load.on_response(Ok(SNEKLETS_PAGE));
    assert_eq!(load.state(), &LoadState::Ready);

    // A server that always has another page is cut off at the cap.
    let mut load = HoldingsLoad::new(source, policy(BABY_SNEKLETS));
    let mut n = 0;
    while load.next_request().is_some() {
        n += 1;
        let next = format!("\"next_cursor\":\"c{n}\"");
        let body = SNEKLETS_PAGE.replace(
            "\"next_cursor\":\"000de14042616279536e656b6c657430303033\"",
            &next,
        );
        load.on_response(Ok(&body));
    }
    assert_eq!(n, MAX_PAGES);
    assert_eq!(load.state(), &LoadState::Ready);
}

#[test]
fn a_failed_or_unreadable_page_fails_the_load_and_keeps_what_arrived() {
    let source = Source::worker("api/holdings", "tok");
    let mut load = HoldingsLoad::new(source.clone(), policy(BABY_SNEKLETS));
    load.next_request();
    load.on_response(Ok(SNEKLETS_PAGE));
    load.next_request();
    load.on_response(Err("503: upstream".to_string()));
    assert_eq!(
        load.state(),
        &LoadState::Failed("503: upstream".to_string())
    );
    assert_eq!(load.holdings().len(), 2, "the first page is kept");

    let mut load = HoldingsLoad::new(source, policy(BABY_SNEKLETS));
    load.next_request();
    load.on_response(Ok("<html>not json</html>"));
    assert!(matches!(load.state(), LoadState::Failed(m) if m.starts_with("unreadable")));

    // A response nobody asked for is ignored.
    load.on_response(Ok(SNEKLETS_PAGE));
    assert!(load.holdings().is_empty());
}

#[test]
fn an_empty_page_is_a_ready_answer_not_a_failure() {
    // What the service sends for a holder of nothing, captured from the live
    // service for an untracked policy.
    let empty = r#"{"cache_generation":0,"schema_version":0,"bitmap_size":0,"asset_count":0,"matched":0,"entries":[]}"#;
    let mut load = HoldingsLoad::new(Source::worker("api/holdings", "t"), policy(TOOLHEADS));
    load.next_request();
    load.on_response(Ok(empty));
    assert_eq!(load.state(), &LoadState::Ready);
    assert!(load.holdings().is_empty());
}
