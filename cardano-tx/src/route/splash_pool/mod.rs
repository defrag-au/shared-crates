//! Splash royalty pools — the AMM pools that can be spent DIRECTLY.
//!
//! Distinct from [`crate::dex::splash`], which builds spot ORDERS for a
//! batcher to fill. A royalty pool is an ordinary script UTxO: name it, spend
//! it, put back a continuing output the validator accepts, and the swap
//! settles in your own transaction at your own quoted numbers.
//!
//! Everything resolves from [`address_registry::dex::SPLASH_ROYALTY_POOL`].
//!
//! Spec: `LUMPPAD_INTEGRATION.md` §7.5.

pub mod datum;
#[cfg(test)]
mod golden;
pub mod leg;
pub mod quote;

use address_registry::dex::SPLASH_ROYALTY_POOL;
use cardano_assets::AssetId;
use pallas_addresses::Address;

pub use datum::{CounterSide, RoyaltyPoolDatum};
pub use leg::SplashSwapLeg;
pub use quote::{PoolReserves, SplashDirection, SwapQuote, quote_swap};

/// The address every royalty pool UTxO sits at — shared by ~400 pools, which
/// is why a pool is identified by its NFT and never by its address.
pub fn pool_address() -> Result<Address, crate::route::leg::RouteError> {
    crate::route::parse_address(SPLASH_ROYALTY_POOL.pool_address)
}

/// The NFT identifying a named pool, for the `asset_utxos` lookup that finds
/// it.
pub fn pool_nft(pool: &address_registry::dex::SplashPool) -> AssetId {
    AssetId::new_unchecked(
        pool.nft.policy_id.to_string(),
        pool.nft.asset_name_hex.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_address_resolves_from_the_registry() {
        assert_eq!(
            pool_address().unwrap().to_bech32().unwrap(),
            SPLASH_ROYALTY_POOL.pool_address
        );
    }

    /// The datum on chain names the same NFT the registry records for the
    /// LUMP/ADA pool — the check that a registry entry and a live pool have
    /// not drifted apart.
    #[test]
    fn the_registrys_lump_ada_nft_matches_the_pools_datum() {
        let datum = RoyaltyPoolDatum::from_cbor(
            &hex::decode(datum::tests::LUMP_ADA_POOL_DATUM_CBOR).unwrap(),
        )
        .unwrap();
        let nft = datum.pool_nft().unwrap();
        let registry = address_registry::dex::SPLASH_LUMP_ADA.nft;
        assert_eq!(nft.policy_hex(), registry.policy_id);
        assert_eq!(nft.name_hex(), registry.asset_name_hex);
    }
}
