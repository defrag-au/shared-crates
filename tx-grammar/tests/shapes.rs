//! The shape sheet. Every fixture is a hand-built sequence representing a real
//! shape the producers emit; the source captures live with the producers
//! (`tx-classifier/resources/test/txs`, `opensea-stream/tests`, and the Midnight
//! wallet bridge). This crate stays free of pallas, alloy and I/O, so the
//! grammar can be tested without a chain.

use chains::{AssetType, ChainRef, TokenRef};
use tx_grammar::*;

const POLICY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NAME: &str = "000de14054657374204e4654";
const EVM_CONTRACT: &str = "0x1111111111111111111111111111111111111111";
const EVM_WETH: &str = "0x2222222222222222222222222222222222222222";

fn chain(id: &str) -> ChainRef {
    id.parse().expect("a CAIP-2 chain")
}

fn token(reference: &str) -> TokenRef {
    reference.parse().expect("a CAIP-19 token")
}

fn cardano() -> ChainRef {
    chain("cardano:mainnet")
}

fn evm() -> ChainRef {
    chain("eip155:4663")
}

fn midnight() -> ChainRef {
    chain("midnight:mainnet")
}

fn ada(quantity: u64) -> Asset {
    Asset::new(
        token("cardano:mainnet/slip44:1815"),
        Quantity::from(quantity),
    )
    .with_label("ADA")
    .with_decimals(6)
}

fn weth(quantity: u128) -> Asset {
    Asset::new(
        token(&format!("eip155:4663/erc20:{EVM_WETH}")),
        Quantity::from(quantity),
    )
    .with_label("WETH")
    .with_decimals(18)
}

fn cardano_nft() -> Asset {
    Asset::one(token(&format!("cardano:mainnet/cnt:{POLICY}/{NAME}"))).with_label("Test NFT")
}

fn cardano_nft_named(name_hex: &str, label: &str) -> Asset {
    Asset::one(token(&format!("cardano:mainnet/cnt:{POLICY}/{name_hex}"))).with_label(label)
}

fn evm_nft(token_id: &str, label: &str) -> Asset {
    Asset::one(token(&format!(
        "eip155:4663/erc721:{EVM_CONTRACT}/{token_id}"
    )))
    .with_label(label)
}

fn erc1155(token_id: &str, quantity: u64, label: &str) -> Asset {
    Asset::new(
        token(&format!("eip155:4663/erc1155:{EVM_CONTRACT}/{token_id}")),
        Quantity::from(quantity),
    )
    .with_label(label)
}

fn own() -> Actor {
    Actor::own(Party::observed("you"))
}

fn peer(label: &str) -> Actor {
    Actor::peer(Party::observed(label))
}

fn venue(label: &str) -> Actor {
    Actor::venue(Party::observed(label))
}

fn contract(label: &str) -> Actor {
    Actor::contract(Party::observed(label))
}

fn wallet(chain: ChainRef) -> Viewpoint {
    Viewpoint::Wallet {
        who: PartyKey::new(chain, "stake1test"),
    }
}

fn cardano_collection() -> Object {
    Object::collection(AssetType::new(cardano(), "cnt", POLICY).expect("a Cardano collection"))
}

fn evm_collection() -> Object {
    Object::collection(AssetType::new(evm(), "erc721", EVM_CONTRACT).expect("an EVM collection"))
}

fn one(step: Step) -> Phrase {
    Sequence::new(wallet(step.chain)).step(step).phrase()
}

#[test]
fn a_cardano_sale_is_two_opposite_moves() {
    let item = cardano_nft();
    let price = ada(147_000_000);
    let phrase = one(Step::new(cardano(), "tx-sale", 0)
        .primitive(Primitive::transfer(
            Object::asset(item.clone()),
            peer("seller"),
            own(),
        ))
        .primitive(Primitive::transfer(
            Object::asset(price),
            own(),
            peer("seller"),
        )));

    assert_eq!(phrase.readings(), vec![Reading::Sale]);
    let cell = phrase.cells().next().expect("one sale");
    assert_eq!(cell.direction, Direction::In);
    assert_eq!(cell.custody, Custody::InHand);
    assert_eq!(
        cell.figure.as_ref().expect("a price").kind,
        FigureKind::Paid
    );
    assert_eq!(cell.actor, peer("seller"));
    assert_eq!(cell.label(), "sale");
    assert_eq!(phrase.tokens(), "sale · Test NFT · paid 147 ADA · seller");
    assert!(
        phrase
            .sentence()
            .contains("Bought Test NFT from seller for 147 ADA.")
    );
}

#[test]
fn a_listing_is_custody_plus_an_ask_not_a_transfer() {
    let item = cardano_nft();
    let phrase = one(Step::new(cardano(), "tx-list", 0)
        .primitive(Primitive::hold(
            Object::asset(item.clone()),
            own(),
            Custody::InHand,
            Custody::Escrow,
        ))
        .primitive(Primitive::declare(
            Claim::ask(Object::asset(item), venue("wayup")).on("wayup"),
            Figure::ask(ada(343_000_000)),
        )));

    assert_eq!(phrase.readings(), vec![Reading::Listing]);
    let cell = phrase.cells().next().expect("one listing");
    assert_eq!(cell.custody, Custody::Escrow);
    assert_eq!(cell.direction, Direction::InPlace);
    assert_eq!(cell.figure.as_ref().expect("an ask").kind, FigureKind::Ask);
    assert_eq!(cell.actor, venue("wayup"));
    assert!(phrase.tokens().contains("ask 343 ADA"));
    assert!(phrase.tokens().contains("escrow"));
    assert!(
        phrase
            .sentence()
            .contains("Listed Test NFT on wayup at 343 ADA.")
    );
}

#[test]
fn a_batched_submission_keeps_both_readings() {
    let lot = Object::lot(
        vec![
            cardano_nft_named("01", "One"),
            cardano_nft_named("02", "Two"),
            cardano_nft_named("03", "Three"),
        ],
        Quantity::from(3u64),
    );
    let collection = cardano_collection();
    let phrase = one(Step::new(cardano(), "tx-batch", 0)
        .primitive(Primitive::hold(
            lot.clone(),
            own(),
            Custody::InHand,
            Custody::Escrow,
        ))
        .primitive(Primitive::declare(
            Claim::ask(lot, venue("wayup")).on("wayup"),
            Figure::ask(ada(343_000_000)),
        ))
        .primitive(Primitive::hold(
            collection.clone(),
            own(),
            Custody::InHand,
            Custody::Escrow,
        ))
        .primitive(Primitive::declare(
            Claim::bid(collection, peer("buyer")).on("wayup"),
            Figure::bid(ada(147_000_000)),
        )));

    assert_eq!(phrase.readings(), vec![Reading::Listing, Reading::Offer]);
    assert_eq!(phrase.cells().count(), 2);
    assert_eq!(
        phrase.tokens(),
        "list · 3 items · ask 343 ADA · wayup · escrow ∥ offer · collection aaaaaa…aaaa · bid 147 ADA · buyer · escrow"
    );
    assert_eq!(
        phrase.sentence(),
        "On cardano:mainnet: Listed 3 items on wayup at 343 ADA. Offered 147 ADA for a collection."
    );
    let listing = phrase.cells().next().expect("a listing");
    assert_eq!(listing.object.distinct(), 3);
    assert!(phrase.tokens().contains("list"));
    assert!(phrase.tokens().contains("offer"));
    assert!(phrase.tokens().contains("3 items"));
}

#[test]
fn a_sale_with_a_withdrawn_bid_reads_as_an_accept() {
    let item = cardano_nft();
    let phrase = one(Step::new(cardano(), "tx-accept", 0)
        .primitive(Primitive::transfer(
            Object::asset(item.clone()),
            peer("seller"),
            own(),
        ))
        .primitive(Primitive::transfer(
            Object::asset(ada(147_000_000)),
            own(),
            peer("seller"),
        ))
        .primitive(Primitive::retract(
            Claim::bid(Object::asset(item), own()),
            None,
        )));

    assert_eq!(phrase.readings(), vec![Reading::Accept]);
    assert!(phrase.sentence().contains("Accepted an offer"));
}

#[test]
fn a_mint_after_a_payment_is_one_event_across_transactions() {
    let phrase = Sequence::new(wallet(cardano()))
        .step(
            Step::new(cardano(), "tx-pay", 0).primitive(Primitive::transfer(
                Object::asset(ada(160_000_000)),
                own(),
                peer("mint provider"),
            )),
        )
        .step(
            Step::new(cardano(), "tx-mint", 90).primitive(Primitive::create(
                Object::lot(
                    vec![
                        cardano_nft_named("01", "One"),
                        cardano_nft_named("02", "Two"),
                    ],
                    Quantity::from(2u64),
                ),
                own(),
            )),
        )
        .link(Link::economic_event(
            0,
            1,
            90,
            Basis::Inferred,
            vec![Evidence::TimeGap { secs: 90 }],
        ))
        .phrase();

    assert_eq!(phrase.composition, Composition::AcrossTransactions);
    assert_eq!(phrase.readings(), vec![Reading::Transfer, Reading::Mint]);
    assert!(phrase.tokens().contains(" → "));
    assert!(
        phrase
            .sentence()
            .contains("One event across 2 transactions.")
    );
    assert!(phrase.sentence().contains("Minted 2 items."));
}

#[test]
fn an_evm_sale_keeps_the_payment_asset() {
    let item = evm_nft("781", "Madjacket #781");
    let phrase = one(Step::new(evm(), "0xevm", 0)
        .primitive(Primitive::transfer(
            Object::asset(item.clone()),
            peer("seller"),
            own(),
        ))
        .primitive(Primitive::transfer(
            Object::asset(weth(227_000_000_000_000)),
            own(),
            peer("seller"),
        )));

    assert_eq!(phrase.readings(), vec![Reading::Sale]);
    assert!(phrase.tokens().contains("paid 0.000227 WETH"));
    assert!(phrase.tokens().contains("Madjacket #781"));
    assert!(phrase.sentence().contains("for 0.000227 WETH"));
    assert!(!phrase.tokens().contains("ADA"));
}

#[test]
fn an_erc1155_quantity_is_units_not_items() {
    let edition = erc1155("3", 5, "Edition");
    let phrase = one(
        Step::new(evm(), "0xedition", 0).primitive(Primitive::transfer(
            Object::asset(edition),
            peer("minter"),
            own(),
        )),
    );

    let cell = phrase.cells().next().expect("a transfer");
    assert_eq!(cell.object.distinct(), 1);
    assert!(phrase.tokens().contains("5 × Edition"));
    assert!(phrase.sentence().contains("5 × Edition"));
    assert!(!phrase.tokens().contains("5 items"));
}

#[test]
fn an_ambiguous_cancellation_is_not_a_delisting() {
    let phrase = one(
        Step::new(evm(), "0xcancel", 0).primitive(Primitive::retract(
            Claim::unknown(Object::asset(evm_nft("9", "Item")), peer("seller")).on("opensea"),
            None,
        )),
    );

    assert_eq!(phrase.readings(), vec![Reading::Cancel]);
    let cell = phrase.cells().next().expect("a cancellation");
    assert_eq!(cell.absence, Some(Absence::Unknown));
    assert!(phrase.sentence().contains("The side is not known."));
    assert!(!phrase.sentence().contains("Delisted"));
}

#[test]
fn an_approval_is_a_permission_not_a_transfer() {
    let phrase = one(Step::new(evm(), "0xapprove", 0).primitive(Primitive::grant(
        evm_collection(),
        own(),
        contract("seaport"),
    )));

    assert_eq!(phrase.readings(), vec![Reading::Approve]);
    let cell = phrase.cells().next().expect("an approval");
    assert_eq!(cell.direction, Direction::Unchanged);
    assert_eq!(cell.actor, contract("seaport"));
    assert!(
        phrase
            .sentence()
            .contains("Approved seaport to spend a collection.")
    );
}

#[test]
fn a_reverted_step_keeps_its_actions_as_attempted() {
    let phrase = one(Step::new(evm(), "0xreverted", 0)
        .outcome(Outcome::Reverted)
        .primitive(Primitive::transfer(
            Object::asset(evm_nft("1", "Item")),
            peer("seller"),
            own(),
        )));

    assert!(phrase.tokens().starts_with("reverted: "));
    let cell = phrase.cells().next().expect("an attempted transfer");
    assert_eq!(cell.outcome, Outcome::Reverted);
    assert!(phrase.sentence().contains("Attempted"));
    assert!(phrase.sentence().contains("The transaction reverted."));
}

#[test]
fn an_untraced_party_is_a_mark_not_a_guess() {
    let phrase = one(
        Step::new(evm(), "0xinternal", 0).primitive(Primitive::transfer(
            Object::asset(evm_nft("2", "Item")),
            Actor {
                role: Role::Peer,
                party: Party::Untraced,
            },
            own(),
        )),
    );

    let cell = phrase.cells().next().expect("a transfer");
    assert!(cell.actor.party.is_absent());
    assert!(phrase.sentence().contains("untraced"));
}

#[test]
fn a_shielded_transfer_never_invents_a_party_or_an_amount() {
    let phrase = one(Step::new(midnight(), "mn-tx", 0).primitive(
        Primitive::transfer(
            Object::hidden(Visibility::Shielded),
            Actor {
                role: Role::Peer,
                party: Party::Shielded { commitment: None },
            },
            own(),
        )
        .with_visibility(Visibility::Shielded),
    ));

    let cell = phrase.cells().next().expect("a shielded transfer");
    assert_eq!(cell.visibility, Visibility::Shielded);
    assert!(cell.object.is_hidden());
    assert_eq!(cell.figure, None);
    assert!(phrase.tokens().contains("shielded asset"));
    assert!(phrase.sentence().contains("shielded"));
    assert!(!phrase.sentence().contains('0'));
}

#[test]
fn a_cross_chain_phrase_names_each_chain() {
    let phrase = Sequence::new(Viewpoint::Chain)
        .step(
            Step::new(chain("eip155:1"), "0xbridge-in", 0).primitive(Primitive::transfer(
                Object::asset(evm_nft("1", "Item")),
                own(),
                contract("bridge"),
            )),
        )
        .step(
            Step::new(cardano(), "tx-bridge-out", 30).primitive(Primitive::transfer(
                Object::asset(cardano_nft()),
                contract("bridge"),
                own(),
            )),
        )
        .link(Link::cross_chain(0, 1, "bridge", Basis::Observed))
        .phrase();

    assert_eq!(phrase.composition, Composition::AcrossChains);
    assert!(phrase.is_multi_chain());
    assert!(phrase.tokens().contains("[eip155:1]"));
    assert!(phrase.tokens().contains("[cardano:mainnet]"));
    assert!(phrase.sentence().contains("One event across 2 chains."));
}

#[test]
fn reconciliation_is_per_asset() {
    let phrase = Sequence::new(wallet(evm()))
        .reconcile(BalanceDelta::debit(ada(1_000_000)))
        .reconcile(BalanceDelta::credit(weth(1_000_000_000_000_000_000)))
        .phrase();

    assert_eq!(phrase.reconciliation.len(), 2);
    assert_eq!(phrase.reconciliation[0].direction, Direction::Out);
    assert_eq!(
        phrase.reconciliation[1].asset.label.as_deref(),
        Some("WETH")
    );
}

#[test]
fn a_hold_without_a_claim_reads_as_a_lock() {
    let phrase = one(
        Step::new(cardano(), "tx-lock", 0).primitive(Primitive::hold(
            Object::asset(cardano_nft()),
            own(),
            Custody::InHand,
            Custody::Script,
        )),
    );

    assert_eq!(phrase.readings(), vec![Reading::Lock]);
    let cell = phrase.cells().next().expect("a lock");
    assert_eq!(cell.custody, Custody::Script);
    assert!(phrase.sentence().contains("Locked"));
}

#[test]
fn a_vesting_lock_reads_as_a_vest() {
    let phrase = one(Step::new(cardano(), "tx-vest", 0)
        .facet(Facet::mechanism(Mechanism::Vesting))
        .primitive(Primitive::hold(
            Object::asset(cardano_nft()),
            own(),
            Custody::InHand,
            Custody::Script,
        )));

    assert_eq!(phrase.readings(), vec![Reading::Vest]);
    assert!(phrase.sentence().contains("vesting"));
}

#[test]
fn an_escrowed_item_without_an_ask_still_reads_as_a_listing() {
    let phrase = one(
        Step::new(cardano(), "tx-escrow", 0).primitive(Primitive::hold(
            Object::asset(cardano_nft()),
            own(),
            Custody::InHand,
            Custody::Escrow,
        )),
    );

    assert_eq!(phrase.readings(), vec![Reading::Listing]);
    assert!(phrase.sentence().contains("Listed"));
}

#[test]
fn a_bundle_price_stays_a_lot_price() {
    let lot = Object::lot(
        vec![
            evm_nft("1", "One"),
            evm_nft("2", "Two"),
            evm_nft("3", "Three"),
            evm_nft("4", "Four"),
            evm_nft("5", "Five"),
        ],
        Quantity::from(5u64),
    );
    let phrase = one(Step::new(evm(), "0xbundle", 0)
        .primitive(Primitive::hold(
            lot.clone(),
            own(),
            Custody::InHand,
            Custody::Escrow,
        ))
        .primitive(Primitive::declare(
            Claim::ask(lot, venue("opensea")),
            Figure::ask(weth(500_000_000_000_000_000)).per(Per::Lot),
        )));

    let cell = phrase.cells().next().expect("a bundle listing");
    let figure = cell.figure.as_ref().expect("an ask");
    assert_eq!(figure.per, Per::Lot);
    assert!(phrase.tokens().contains("ask 0.5 WETH"));
    assert!(!phrase.tokens().contains("2.5 WETH"));
}

#[test]
fn quantity_round_trips_as_a_decimal_string() {
    let quantity = Quantity::from_decimal("227000000000000").expect("a quantity");
    assert_eq!(quantity.to_decimal(), "227000000000000");
    let json = serde_json::to_string(&quantity).expect("serialize");
    assert_eq!(json, "\"227000000000000\"");
    let back: Quantity = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, quantity);
    assert_eq!(
        Quantity::from_decimal(
            "99999999999999999999999999999999999999999999999999999999999999999999999999999"
        )
        .expect("the 256-bit ceiling")
        .to_decimal(),
        "99999999999999999999999999999999999999999999999999999999999999999999999999999"
    );
    assert_eq!(
        Quantity::from_decimal(
            "999999999999999999999999999999999999999999999999999999999999999999999999999999"
        ),
        Err(QuantityError::Overflow)
    );
}

#[test]
fn a_phrase_round_trips_through_json() {
    let phrase = one(Step::new(cardano(), "tx-json", 0)
        .primitive(Primitive::transfer(
            Object::asset(cardano_nft()),
            peer("seller"),
            own(),
        ))
        .primitive(Primitive::transfer(
            Object::asset(ada(147_000_000)),
            own(),
            peer("seller"),
        )));
    let json = serde_json::to_string(&phrase).expect("serialize");
    let back: Phrase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, phrase);
    assert_eq!(back.tokens(), phrase.tokens());
}

#[test]
fn a_pair_viewpoint_keeps_both_ends_of_a_movement() {
    let phrase = Sequence::new(Viewpoint::Pair)
        .step(
            Step::new(cardano(), "tx-policy-transfer", 0).primitive(Primitive::transfer(
                Object::asset(cardano_nft()),
                peer("alice"),
                peer("bob"),
            )),
        )
        .phrase();

    assert_eq!(phrase.readings(), vec![Reading::Transfer]);
    let cell = phrase.cells().next().expect("one transfer");
    assert_eq!(cell.actor, peer("alice"));
    assert_eq!(cell.other, Some(peer("bob")));
    assert_eq!(phrase.tokens(), "transfer · Test NFT · alice → bob");
    assert!(
        phrase
            .sentence()
            .contains("Test NFT moved from alice to bob.")
    );
}

#[test]
fn a_pair_sale_names_the_seller_the_buyer_and_the_price() {
    let phrase = Sequence::new(Viewpoint::Pair)
        .step(
            Step::new(cardano(), "tx-policy-sale", 0)
                .primitive(Primitive::transfer(
                    Object::asset(cardano_nft()),
                    peer("seller"),
                    peer("buyer"),
                ))
                .primitive(Primitive::transfer(
                    Object::asset(ada(147_000_000)),
                    peer("buyer"),
                    peer("seller"),
                )),
        )
        .phrase();

    assert_eq!(phrase.readings(), vec![Reading::Sale]);
    let cell = phrase.cells().next().expect("one sale");
    assert_eq!(cell.actor, peer("seller"));
    assert_eq!(cell.other, Some(peer("buyer")));
    assert!(
        phrase
            .sentence()
            .contains("Test NFT moved from seller to buyer for 147 ADA.")
    );
}

#[test]
fn a_wallet_viewpoint_never_names_its_own_side_twice() {
    let phrase = one(
        Step::new(cardano(), "tx-wallet-transfer", 0).primitive(Primitive::transfer(
            Object::asset(cardano_nft()),
            peer("alice"),
            own(),
        )),
    );

    let cell = phrase.cells().next().expect("one transfer");
    assert_eq!(cell.actor, peer("alice"));
    assert_eq!(cell.other, None);
}
