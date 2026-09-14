//! LumpPad — LUMP-quoted constant-product launchpad pools.
//!
//! One shared PlutusV3 validator, one pool UTxO per token, three redeemers
//! (Buy, Sell, Claim). Everything here resolves its addresses, hashes and
//! curve constants from [`address_registry::dex::LUMPPAD`]; nothing is
//! restated at a call site.
//!
//! Spec: `docs/design/LUMPPAD_INTEGRATION.md`.

pub mod datum;
#[cfg(test)]
mod golden;
pub mod leg;
pub mod quote;

use address_registry::dex::LUMPPAD;
use cardano_assets::AssetId;
use pallas_addresses::Address;

pub use datum::PoolDatum;
pub use leg::{CreatorPayout, LumpPadBuyLeg, LumpPadSellLeg, stage_claim};
pub use quote::{BuyQuote, ClaimQuote, SellQuote, claim_split, quote_buy, quote_sell, spot_price};

/// The address every LumpPad pool UTxO sits at, from the registry record.
pub fn pool_address() -> Result<Address, crate::route::leg::RouteError> {
    crate::route::parse_address(LUMPPAD.pool_address)
}

/// LUMP — the asset every LumpPad pool is quoted in.
pub fn lump_asset() -> AssetId {
    AssetId::new_unchecked(
        LUMPPAD.lump.policy_id.to_string(),
        LUMPPAD.lump.asset_name_hex.to_string(),
    )
}

/// The token a pool trades, from its datum.
pub fn token_asset(datum: &PoolDatum) -> Result<AssetId, crate::route::leg::RouteError> {
    AssetId::new(datum.token_policy_hex(), datum.token_name_hex())
        .map_err(|e| crate::route::leg::RouteError::Datum(format!("pool token: {e}")))
}

/// The state NFT that identifies a pool, for the `asset_utxos` lookup that
/// finds it. One per pool, under the TOKEN's policy.
pub fn state_nft(token_policy_hex: &str) -> AssetId {
    AssetId::new_unchecked(
        token_policy_hex.to_string(),
        LUMPPAD.state_nft_name_hex.to_string(),
    )
}

/// Does this pool's fee schedule match the deployment the registry records?
///
/// The schedule lives in the DATUM, per pool, while the registry records the
/// values the validator and mint policy bake in. They have agreed on every
/// pool launched so far. A pool that disagrees is either a launch we have not
/// seen or a datum we mis-decoded; either way, quoting it against the wrong
/// constants would be wrong silently.
pub fn matches_registry_schedule(datum: &PoolDatum) -> bool {
    datum.creator_fee_bps == LUMPPAD.creator_fee_bps
        && datum.platform_fee_bps == LUMPPAD.platform_fee_bps
        && datum.flat_fee == LUMPPAD.flat_fee
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_swole_pool_uses_the_registrys_fee_schedule() {
        let datum =
            PoolDatum::from_cbor(&hex::decode(datum::tests::SWOLE_POOL_DATUM_CBOR).unwrap())
                .unwrap();
        assert!(matches_registry_schedule(&datum));
        assert_eq!(token_asset(&datum).unwrap().asset_name(), "SWOLE");
        assert_eq!(
            state_nft(&datum.token_policy_hex()).asset_name_hex(),
            LUMPPAD.state_nft_name_hex
        );
    }

    #[test]
    fn the_pool_address_resolves_from_the_registry() {
        let addr = pool_address().unwrap();
        assert_eq!(addr.to_bech32().unwrap(), LUMPPAD.pool_address);
    }
}
