//! What this classification *did*, as the transaction grammar's primitives.
//!
//! The classifier already decided what a transaction was. This module is the
//! forward projection of that decision into the shape a renderer draws — the
//! same [`Sequence`] the Cardano row projections produce, so a sale seen by the
//! classifier and a sale seen on the policy feed are the same object to a
//! renderer.
//!
//! # It does not re-classify
//!
//! Every variant is mapped from what the classifier already established. A
//! `Sale` becomes an item move plus a money move, not a second guess at who
//! sold to whom; an `Offer` becomes a custody change plus a claim, not a new
//! threshold. Where a variant carries no parties or no price, the projection
//! says so with an absence rather than inventing one.
//!
//! # Strings stay strings
//!
//! `MarketplaceBundle` carries operation NAMES and a count, not the operations
//! themselves, and `SmartContract` carries a free-form operation. Neither is
//! parsed into a shape here: a string that happens to contain "list" is not a
//! listing. A bundle is drawn from its legs by whichever producer has them —
//! the flow rows' `FlowMarket::legs` is that producer.

use chains::{AssetType, ChainRef, TokenRef};
use tx_grammar::{
    Actor, Asset, Claim, Custody, Facet, Figure, Mechanism, Object, Party, PartyKey, Primitive,
    Quantity, Sequence, Step, Viewpoint,
};

use crate::{
    AssetId, MintType, OperationPayload, PricedAsset, StakeDirection, TxClassification, TxType,
    VestingDirection,
};

/// The classifier reads Cardano mainnet rows; see the row projections in
/// `shared-types` for why the chain is stated once.
pub const CARDANO: ChainRef = ChainRef::cardano_mainnet();

const ADA_TOKEN: &str = "cardano:mainnet/slip44:1815";

/// How many repetitions of one kind a projection will emit before it stops.
///
/// A `CreateOffer` with 200 offers is one fact and 200 identical primitives;
/// the cells merge into a count, and the bound is what keeps that from being
/// two hundred allocations on a row a list draws. The cap is a work limit, not
/// a claim: `Cell::more` is not used here because the count in the source is
/// already the count.
const REPEAT_LIMIT: u32 = 32;

impl TxClassification {
    /// The transaction as a phrase, read from one side.
    ///
    /// `viewpoint` decides only which named party is `own`; the shapes come
    /// from the classification either way. Use [`Viewpoint::Chain`] when no
    /// side is known.
    pub fn phrase(&self, viewpoint: &Viewpoint) -> tx_grammar::Phrase {
        let mut step = Step::new(
            CARDANO,
            self.tx_hash.clone(),
            self.context.timestamp.unwrap_or(0) as i64,
        );

        for tx_type in &self.tx_types {
            project(tx_type, viewpoint, &mut step);
        }

        Sequence::new(viewpoint.clone()).step(step).phrase()
    }

    /// The phrase with no owner: every named party is a peer.
    pub fn chain_phrase(&self) -> tx_grammar::Phrase {
        self.phrase(&Viewpoint::Chain)
    }
}

fn project(tx_type: &TxType, viewpoint: &Viewpoint, step: &mut Step) {
    match tx_type {
        TxType::Mint {
            assets,
            minter,
            mint_type,
            ..
        } => {
            if let Some(object) = object_of(assets.iter(), assets.len() as u32) {
                step.primitives
                    .push(Primitive::create(object, party(viewpoint, minter)));
            }
            match mint_type {
                MintType::Cip25 => step.facets.push(Facet::standard("cip25")),
                MintType::Cip68 => step.facets.push(Facet::standard("cip68")),
                MintType::Fungible => step.facets.push(Facet::standard("fungible")),
                MintType::Unknown => {}
            }
        }
        TxType::Burn {
            policy_id,
            asset_count,
        } => {
            // The count is real and the identities are not in the variant. A
            // collection is the honest object: it names what was burned, and a
            // lot of anonymous items would name a quantity while losing the
            // one thing a reader can act on.
            let _ = asset_count;
            if let Some(object) = collection(policy_id) {
                step.primitives
                    .push(Primitive::destroy(object, party(viewpoint, "")));
            }
        }
        TxType::Sale {
            asset,
            breakdown,
            seller,
            buyer,
            marketplace,
        } => {
            let Some(object) = priced_object(asset) else {
                return;
            };
            let from = party(viewpoint, seller);
            let to = party(viewpoint, buyer);
            step.primitives
                .push(Primitive::transfer(object, from.clone(), to.clone()));
            let price = asset.price_lovelace.unwrap_or(breakdown.total_lovelace);
            if let Some(money) = ada(price) {
                step.primitives
                    .push(Primitive::transfer(Object::asset(money), to, from));
            }
            if let Some(venue) = marketplace {
                step.facets.push(Facet::venue(venue.clone()));
            }
        }
        TxType::OfferAccept {
            asset,
            offer_lovelace,
            seller,
            buyer,
        } => {
            let Some(object) = id_object(asset, 1) else {
                return;
            };
            let from = party(viewpoint, seller);
            let to = party(viewpoint, buyer);
            step.primitives.push(Primitive::transfer(
                object.clone(),
                from.clone(),
                to.clone(),
            ));
            if let Some(money) = ada(*offer_lovelace) {
                step.primitives
                    .push(Primitive::transfer(Object::asset(money), to.clone(), from));
            }
            // The withdrawn bid is what makes the grammar read `Accept`.
            step.primitives
                .push(Primitive::retract(Claim::bid(object, to), None));
        }
        TxType::Offer {
            asset,
            offer_lovelace,
            bidder,
        } => {
            let object = match asset {
                Some(id) => id_object(id, 1),
                // A collection offer names no policy in this variant. Saying
                // "an unknown object" is the only honest object available.
                None => Some(Object::hidden(tx_grammar::Visibility::Unknown)),
            };
            let Some(object) = object else {
                return;
            };
            if let Some(figure) = bid_figure(*offer_lovelace) {
                step.primitives.push(Primitive::hold(
                    object.clone(),
                    party(viewpoint, bidder),
                    Custody::InHand,
                    Custody::Escrow,
                ));
                step.primitives.push(Primitive::declare(
                    Claim::bid(object, party(viewpoint, bidder)),
                    figure,
                ));
            }
        }
        TxType::CreateOffer {
            policy_id,
            encoded_asset_name,
            offer_count,
            offer_lovelace,
            bidder,
            marketplace,
            ..
        } => {
            let Some(object) = offer_object(policy_id, encoded_asset_name.as_deref()) else {
                return;
            };
            let Some(figure) = bid_figure(*offer_lovelace) else {
                return;
            };
            let actor = party(viewpoint, bidder);
            for _ in 0..(*offer_count).clamp(1, REPEAT_LIMIT) {
                step.primitives.push(Primitive::hold(
                    object.clone(),
                    actor.clone(),
                    Custody::InHand,
                    Custody::Escrow,
                ));
                step.primitives.push(Primitive::declare(
                    Claim::bid(object.clone(), actor.clone()).on(marketplace.clone()),
                    figure.clone(),
                ));
            }
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::OfferUpdate {
            policy_id,
            encoded_asset_name,
            original_amount,
            updated_amount,
            bidder,
            marketplace,
            ..
        } => {
            let Some(object) = offer_object(policy_id, encoded_asset_name.as_deref()) else {
                return;
            };
            let (Some(from), Some(to)) =
                (bid_figure(*original_amount), bid_figure(*updated_amount))
            else {
                return;
            };
            step.primitives.push(Primitive::revise(
                Claim::bid(object, party(viewpoint, bidder)).on(marketplace.clone()),
                Some(from),
                to,
            ));
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::OfferCancel {
            policy_id,
            encoded_asset_name,
            offer_count,
            total_cancelled_lovelace,
            bidder,
            marketplace,
        } => {
            let Some(object) = offer_object(policy_id, encoded_asset_name.as_deref()) else {
                return;
            };
            let actor = party(viewpoint, bidder);
            let figure = bid_figure(*total_cancelled_lovelace);
            for _ in 0..(*offer_count).clamp(1, REPEAT_LIMIT) {
                step.primitives.push(Primitive::hold(
                    object.clone(),
                    actor.clone(),
                    Custody::Escrow,
                    Custody::InHand,
                ));
                step.primitives.push(Primitive::retract(
                    Claim::bid(object.clone(), actor.clone()).on(marketplace.clone()),
                    figure.clone(),
                ));
            }
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::Transfer { assets } => {
            if let Some(object) = object_of(assets.iter(), assets.len() as u32) {
                step.primitives.push(Primitive::transfer(
                    object,
                    Actor::unknown(),
                    Actor::unknown(),
                ));
            }
        }
        TxType::AssetTransfer {
            assets,
            sender,
            receiver,
        } => {
            if let Some(object) = object_of(assets.iter(), assets.len() as u32) {
                step.primitives.push(Primitive::transfer(
                    object,
                    party(viewpoint, sender),
                    party(viewpoint, receiver),
                ));
            }
        }
        TxType::SmartContract {
            contract_address,
            operation,
        } => {
            // Neither field is a shape. The contract is a fact a renderer can
            // mark, and the operation string is left alone rather than parsed.
            step.facets.push(Facet::mechanism(Mechanism::Contract));
            if !contract_address.is_empty() {
                step.facets.push(Facet::venue(contract_address.clone()));
            }
            let _ = operation;
        }
        TxType::AssetStaking {
            assets,
            direction,
            staker_address,
            staking_label,
            ..
        } => {
            let Some(object) = object_of(assets.iter(), assets.len() as u32) else {
                return;
            };
            let (from, to) = match direction {
                StakeDirection::Stake => (Custody::InHand, Custody::Staked),
                StakeDirection::Unstake => (Custody::Staked, Custody::InHand),
            };
            step.primitives.push(Primitive::hold(
                object,
                party(viewpoint, staker_address),
                from,
                to,
            ));
            step.facets.push(Facet::venue(staking_label.clone()));
        }
        TxType::AssetVesting {
            assets,
            direction,
            owner_address,
            vesting_label,
            ..
        } => {
            let Some(object) = object_of(assets.iter(), assets.len() as u32) else {
                return;
            };
            let (from, to) = match direction {
                VestingDirection::Lock => (Custody::InHand, Custody::Script),
                VestingDirection::Unlock => (Custody::Script, Custody::InHand),
            };
            step.primitives.push(Primitive::hold(
                object,
                party(viewpoint, owner_address),
                from,
                to,
            ));
            step.facets.push(Facet::mechanism(Mechanism::Vesting));
            step.facets.push(Facet::venue(vesting_label.clone()));
        }
        TxType::ListingCreate {
            assets,
            seller,
            marketplace,
            ..
        } => {
            let venue = venue(marketplace);
            for priced in assets {
                let Some(object) = priced_object(priced) else {
                    continue;
                };
                step.primitives.push(Primitive::hold(
                    object.clone(),
                    party(viewpoint, seller),
                    Custody::InHand,
                    Custody::Escrow,
                ));
                if let Some(figure) = priced.price_lovelace.and_then(ask_figure) {
                    step.primitives.push(Primitive::declare(
                        Claim::ask(object, venue.clone()),
                        figure,
                    ));
                }
            }
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::ListingUpdate {
            assets,
            seller,
            marketplace,
            ..
        } => {
            for priced in assets {
                let Some(object) = priced_object(priced) else {
                    continue;
                };
                let Some(figure) = priced.price_lovelace.and_then(ask_figure) else {
                    continue;
                };
                step.primitives.push(Primitive::revise(
                    Claim::ask(object, party(viewpoint, seller)).on(marketplace.clone()),
                    None,
                    Figure::price_change(figure.amount),
                ));
            }
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::Unlisting {
            assets,
            seller,
            marketplace,
            ..
        } => {
            let venue = venue(marketplace);
            for id in assets {
                let Some(object) = id_object(id, 1) else {
                    continue;
                };
                step.primitives.push(Primitive::hold(
                    object.clone(),
                    party(viewpoint, seller),
                    Custody::Escrow,
                    Custody::InHand,
                ));
                step.primitives
                    .push(Primitive::retract(Claim::ask(object, venue.clone()), None));
            }
            step.facets.push(Facet::venue(marketplace.clone()));
        }
        TxType::DexSwap {
            dex_platform,
            asset_in,
            asset_out,
            swapper,
        } => {
            let pool = Actor::pool(Party::observed(dex_platform.clone()));
            let trader = party(viewpoint, swapper);
            if let Some(object) = payload_object(asset_out) {
                step.primitives
                    .push(Primitive::transfer(object, pool.clone(), trader.clone()));
            }
            if let Some(object) = payload_object(asset_in) {
                step.primitives
                    .push(Primitive::transfer(object, trader, pool));
            }
            step.facets.push(Facet::mechanism(Mechanism::Dex));
            step.facets.push(Facet::venue(dex_platform.clone()));
        }
        TxType::DexLiquidityAdd {
            dex_platform,
            asset_a,
            amount_a,
            asset_b,
            amount_b,
            provider,
        } => {
            let pool = Actor::pool(Party::observed(dex_platform.clone()));
            let trader = party(viewpoint, provider);
            for (id, amount) in [(asset_a, *amount_a), (asset_b, *amount_b)] {
                if let Some(object) = id_object(id, amount) {
                    step.primitives
                        .push(Primitive::transfer(object, trader.clone(), pool.clone()));
                }
            }
            step.facets.push(Facet::mechanism(Mechanism::Dex));
            step.facets.push(Facet::venue(dex_platform.clone()));
        }
        TxType::DexLiquidityRemove {
            dex_platform,
            asset_a,
            amount_a,
            asset_b,
            amount_b,
            provider,
        } => {
            let pool = Actor::pool(Party::observed(dex_platform.clone()));
            let trader = party(viewpoint, provider);
            for (id, amount) in [(asset_a, *amount_a), (asset_b, *amount_b)] {
                if let Some(object) = id_object(id, amount) {
                    step.primitives
                        .push(Primitive::transfer(object, pool.clone(), trader.clone()));
                }
            }
            step.facets.push(Facet::mechanism(Mechanism::Dex));
            step.facets.push(Facet::venue(dex_platform.clone()));
        }
        TxType::MarketplaceBundle { .. } => {
            // The operations are NAMES, not shapes. Parsing them here would be
            // a second classifier with no evidence behind it.
            step.facets.push(Facet::mechanism(Mechanism::Marketplace));
        }
        TxType::Unknown => {}
    }
}

// ============================================================================
// Objects and figures
// ============================================================================

fn named_asset(id: &AssetId, quantity: u64) -> Option<Asset> {
    let token: TokenRef = format!("cardano:mainnet/cnt:{}/{}", id.policy_id, id.asset_name_hex)
        .parse()
        .ok()?;
    let mut asset = Asset::new(token, Quantity::from(quantity));
    if let Some(label) = decoded_name(&id.asset_name_hex) {
        asset = asset.with_label(label);
    }
    Some(asset)
}

/// The CIP-67 label comes off before decoding, the same rule the verdict uses:
/// decoding `000de140…` as text yields something unprintable, and the item
/// silently loses its name.
fn decoded_name(name_hex: &str) -> Option<String> {
    let bare = cardano_assets::Asset::strip_any_cip67_prefix(name_hex);
    let bytes = hex::decode(bare).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let legible = text
        .chars()
        .all(|c| !c.is_control() && (c.is_ascii_graphic() || c == ' '));
    (legible && !text.trim().is_empty()).then(|| text.trim().to_string())
}

fn id_object(id: &AssetId, quantity: u64) -> Option<Object> {
    named_asset(id, quantity).map(Object::asset)
}

fn priced_object(priced: &PricedAsset) -> Option<Object> {
    id_object(&priced.asset, 1)
}

fn object_of<'a>(ids: impl Iterator<Item = &'a AssetId>, distinct: u32) -> Option<Object> {
    let assets: Vec<Asset> = ids.filter_map(|id| named_asset(id, 1)).collect();
    let first = assets.first()?.clone();
    let total = assets.iter().fold(Quantity::ZERO, |sum, asset| {
        sum.saturating_add(asset.quantity)
    });
    if distinct <= 1 && assets.len() <= 1 {
        return Some(Object::asset(first));
    }
    Some(Object::lot_sampled(assets, distinct, total))
}

fn collection(policy: &str) -> Option<Object> {
    if policy.len() != 56 {
        return None;
    }
    AssetType::new(CARDANO, "cnt", policy)
        .ok()
        .map(Object::collection)
}

fn offer_object(policy: &str, name_hex: Option<&str>) -> Option<Object> {
    match name_hex.filter(|n| !n.is_empty()) {
        Some(name) => named_asset(
            &AssetId::new_unchecked(policy.to_string(), name.to_string()),
            1,
        )
        .map(Object::asset),
        None => collection(policy),
    }
}

fn payload_object(payload: &OperationPayload) -> Option<Object> {
    match payload {
        OperationPayload::Lovelace { amount } => ada(*amount).map(Object::asset),
        OperationPayload::NativeToken {
            policy_id,
            encoded_name,
            amount,
        } => named_asset(
            &AssetId::new_unchecked(policy_id.clone(), encoded_name.clone()),
            *amount,
        )
        .map(Object::asset),
    }
}

fn ada(amount: u64) -> Option<Asset> {
    (amount > 0).then(|| {
        Asset::new(
            ADA_TOKEN.parse().expect("ADA is a valid CAIP-19 asset"),
            Quantity::from(amount),
        )
        .with_label("ADA")
        .with_decimals(6)
    })
}

fn ask_figure(amount: u64) -> Option<Figure> {
    ada(amount).map(Figure::ask)
}

fn bid_figure(amount: u64) -> Option<Figure> {
    ada(amount).map(Figure::bid)
}

// ============================================================================
// Parties
// ============================================================================

/// The classifier names raw addresses. A wallet viewpoint turns the one that
/// matches into `own`; every other party is a peer. No label is resolved here
/// — naming is the caller's network concern.
fn party(viewpoint: &Viewpoint, address: &str) -> Actor {
    if address.is_empty() {
        return Actor::unknown();
    }
    match viewpoint {
        Viewpoint::Wallet { who } if who.address == address => {
            Actor::own(Party::observed(address).keyed(who.clone()))
        }
        _ => Actor::peer(Party::observed(address).keyed(PartyKey::new(CARDANO, address))),
    }
}

fn venue(name: &str) -> Actor {
    Actor::venue(Party::observed(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdaFlows, Confidence, TxContext};
    use tx_grammar::Reading;

    const POLICY: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

    fn id(name: &str) -> AssetId {
        AssetId::new_unchecked(POLICY.to_string(), hex(name))
    }

    fn hex(text: &str) -> String {
        text.bytes().map(|b| format!("{b:02x}")).collect()
    }

    fn classification(tx_types: Vec<TxType>) -> TxClassification {
        TxClassification {
            tx_hash: "ab".repeat(32),
            tx_types,
            tags: Vec::new(),
            confidence: Confidence::High,
            score: 1.0,
            context: TxContext {
                block_height: None,
                timestamp: Some(1_788_408_449),
                fee: None,
                size: None,
                metadata: None,
                scripts: Vec::new(),
                notes: Vec::new(),
            },
            assets: Vec::new(),
            ada_amounts: AdaFlows {
                total_input: 0,
                total_output: 0,
                fee: 0,
                collateral_input: 0,
                collateral_output: 0,
                largest_transfer: None,
                flows: Vec::new(),
                collateral_flows: Vec::new(),
            },
            distributions: Vec::new(),
        }
    }

    #[test]
    fn a_sale_is_an_item_move_and_a_money_move() {
        let phrase = classification(vec![TxType::Sale {
            asset: PricedAsset::with_price(id("Test NFT"), 147_000_000),
            breakdown: crate::SaleBreakdown {
                total_lovelace: 147_000_000,
            },
            seller: "seller".to_string(),
            buyer: "buyer".to_string(),
            marketplace: Some("jpg".to_string()),
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Sale]);
        let cell = phrase.cells().next().expect("one sale");
        assert_eq!(
            cell.figure.as_ref().expect("a price").kind,
            tx_grammar::FigureKind::Paid
        );
        assert_eq!(cell.other, Some(party(&Viewpoint::Chain, "buyer")));
        assert!(phrase.tokens().contains("paid 147 ADA"));
    }

    #[test]
    fn an_accepted_offer_is_not_a_plain_sale() {
        let phrase = classification(vec![TxType::OfferAccept {
            asset: id("Test NFT"),
            offer_lovelace: 147_000_000,
            seller: "seller".to_string(),
            buyer: "buyer".to_string(),
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Accept]);
    }

    #[test]
    fn a_listing_is_not_a_sale_even_with_a_price() {
        let phrase = classification(vec![TxType::ListingCreate {
            assets: vec![PricedAsset::with_price(id("Test NFT"), 343_000_000)],
            total_listing_count: 1,
            seller: "seller".to_string(),
            marketplace: "wayup".to_string(),
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Listing]);
        let cell = phrase.cells().next().expect("one listing");
        assert_eq!(cell.custody, Custody::Escrow);
        assert_eq!(
            cell.figure.as_ref().expect("an ask").kind,
            tx_grammar::FigureKind::Ask
        );
    }

    #[test]
    fn a_mint_names_the_asset_and_never_the_reference_token() {
        let phrase = classification(vec![TxType::Mint {
            assets: vec![id("One")],
            reference_assets: vec![id("OneRef")],
            total_lovelace: Some(3_000_000),
            minter: "minter".to_string(),
            mint_type: MintType::Cip68,
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Mint]);
        assert!(phrase.facets.contains(&Facet::standard("cip68")));
        assert!(phrase.tokens().contains("One"));
        assert!(!phrase.tokens().contains("OneRef"));
    }

    #[test]
    fn a_transfer_between_addresses_keeps_both_ends() {
        let phrase = classification(vec![TxType::AssetTransfer {
            assets: vec![id("Test NFT")],
            sender: "alice".to_string(),
            receiver: "bob".to_string(),
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Transfer]);
        let cell = phrase.cells().next().expect("one transfer");
        assert_eq!(cell.other, Some(party(&Viewpoint::Chain, "bob")));
        assert!(phrase.tokens().contains("alice → bob"));
    }

    #[test]
    fn a_wallet_viewpoint_names_the_other_side_once() {
        let phrase = classification(vec![TxType::AssetTransfer {
            assets: vec![id("Test NFT")],
            sender: "alice".to_string(),
            receiver: "bob".to_string(),
        }])
        .phrase(&Viewpoint::Wallet {
            who: PartyKey::new(CARDANO, "alice"),
        });

        let cell = phrase.cells().next().expect("one transfer");
        assert_eq!(cell.direction, tx_grammar::Direction::Out);
        assert_eq!(cell.actor, party(&Viewpoint::Chain, "bob"));
        assert_eq!(cell.other, None, "a wallet side is never named twice");
    }

    #[test]
    fn a_dex_swap_is_not_a_transfer() {
        let phrase = classification(vec![TxType::DexSwap {
            dex_platform: "splash".to_string(),
            asset_in: OperationPayload::Lovelace { amount: 5_000_000 },
            asset_out: OperationPayload::NativeToken {
                policy_id: POLICY.to_string(),
                encoded_name: hex("PERPCOIN"),
                amount: 1_000,
            },
            swapper: "trader".to_string(),
        }])
        .chain_phrase();

        assert_eq!(phrase.readings(), vec![Reading::Sale]);
        assert!(phrase.facets.contains(&Facet::venue("splash")));
    }

    #[test]
    fn a_bundle_does_not_parse_its_operation_names() {
        let phrase = classification(vec![TxType::MarketplaceBundle {
            primary_operation: "listing".to_string(),
            operations: vec!["listing".to_string(), "offer_created".to_string()],
            asset_count: 3,
            total_lovelace: Some(500_000_000),
        }])
        .chain_phrase();

        assert!(phrase.readings().is_empty());
        assert!(
            phrase
                .facets
                .contains(&Facet::mechanism(Mechanism::Marketplace))
        );
    }
}
