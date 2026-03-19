//! CSWAP concentrated liquidity pool integration
//!
//! Provides datum construction and configuration for submitting swap
//! orders to CSWAP's concentrated liquidity pools on Cardano.
//!
//! A CSWAP order is a UTxO sent to the order script address with a
//! datum hash. The CSWAP batcher picks up and executes the order
//! against the concentrated pool.

pub mod config;
pub mod datum;

use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_crypto::hash::{Hash, Hasher};
use pallas_primitives::Fragment;

use crate::dex::splash::datum::DatumAsset;
use datum::CswapOrderParams;

/// Errors from CSWAP operations
#[derive(Debug, thiserror::Error)]
pub enum CswapError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("CBOR encoding failed: {0}")]
    CborEncoding(String),
    #[error("address error: {0}")]
    Address(String),
}

/// Parameters for building a CSWAP swap order
pub struct CswapOrderRequest {
    /// User's payment public key hash (28 bytes)
    pub destination_pkh: [u8; 28],
    /// User's staking key hash (28 bytes, optional)
    pub destination_stake_key: Option<[u8; 28]>,
    /// Asset being sold
    pub sell_asset: DatumAsset,
    /// Amount of asset being sold (native units)
    pub sell_amount: u64,
    /// Asset being bought
    pub buy_asset: DatumAsset,
    /// Minimum amount of buy_asset the user must receive
    pub min_receive_amount: u64,
    /// Minimum ADA to include in min_receive (2 ADA for buy orders)
    pub min_receive_ada: u64,
    /// Execution parameter (default: 1000)
    pub execution_param: u64,
    /// Fee tier (default: 15)
    pub fee_tier: u64,
    /// Network (mainnet or testnet)
    pub network: Network,
}

/// Result of building a CSWAP order
pub struct CswapOrder {
    /// CBOR-encoded datum bytes
    pub datum_bytes: Vec<u8>,
    /// PlutusData for inclusion in TX witness set
    pub datum_plutus_data: pallas_primitives::alonzo::PlutusData,
    /// Blake2b-256 hash of the datum CBOR
    pub datum_hash: Hash<32>,
    /// Script address to send the order UTxO to
    pub script_address: Address,
    /// Total lovelace for the order UTxO
    pub total_lovelace: u64,
}

/// Build a CSWAP swap order.
///
/// Constructs the datum, computes its hash, derives the script address,
/// and calculates the total deposit. The caller uses these values to
/// build the transaction with a datum-hash output.
pub fn build_cswap_order(req: &CswapOrderRequest) -> Result<CswapOrder, CswapError> {
    let is_selling_ada = req.sell_asset.policy_id.is_empty();

    // Build min_receive entries
    let mut min_receive = Vec::new();
    if is_selling_ada {
        // Buying tokens: min_receive = [min_utxo_ada, min_tokens]
        if req.min_receive_ada > 0 {
            min_receive.push((vec![], vec![], req.min_receive_ada));
        }
        min_receive.push((
            req.buy_asset.policy_id.clone(),
            req.buy_asset.asset_name.clone(),
            req.min_receive_amount,
        ));
    } else {
        // Selling tokens: min_receive = [min_ada]
        min_receive.push((vec![], vec![], req.min_receive_amount));
    }

    // Build sell_asset entries (0 = sell all)
    let sell_asset = vec![(
        req.sell_asset.policy_id.clone(),
        req.sell_asset.asset_name.clone(),
        0u64,
    )];

    let params = CswapOrderParams {
        payment_pkh: req.destination_pkh.to_vec(),
        stake_pkh: req.destination_stake_key.map(|h| h.to_vec()),
        min_receive,
        sell_asset,
        execution_param: req.execution_param,
        fee_tier: req.fee_tier,
    };

    // Build datum and encode
    let datum_plutus_data = datum::build_cswap_order_datum(&params);
    let datum_bytes = datum_plutus_data
        .encode_fragment()
        .map_err(|e| CswapError::CborEncoding(format!("{e}")))?;

    // Hash the datum CBOR with blake2b-256
    let datum_hash = Hasher::<256>::hash(&datum_bytes);

    // Build script address
    let script_address =
        build_order_script_address(req.destination_stake_key.as_ref(), req.network)?;

    // Calculate total lovelace
    let total_lovelace = calculate_order_deposit(req.sell_amount, is_selling_ada);

    Ok(CswapOrder {
        datum_bytes,
        datum_plutus_data,
        datum_hash,
        script_address,
        total_lovelace,
    })
}

/// Build the CSWAP order script address.
///
/// Uses the order script's payment credential and the user's stake key
/// for staking delegation (same pattern as Splash).
fn build_order_script_address(
    stake_key_hash: Option<&[u8; 28]>,
    network: Network,
) -> Result<Address, CswapError> {
    let script_hash_bytes: [u8; 28] = hex::decode(config::ORDER_SCRIPT_HASH)
        .map_err(|e| CswapError::Address(format!("invalid script hash hex: {e}")))?
        .try_into()
        .map_err(|_| CswapError::Address("script hash must be 28 bytes".to_string()))?;

    let payment = ShelleyPaymentPart::Script(Hash::from(script_hash_bytes));

    let delegation = match stake_key_hash {
        Some(hash) => ShelleyDelegationPart::Key(Hash::from(*hash)),
        None => {
            // Fall back to the order script's own stake key
            let stake_bytes: [u8; 28] = hex::decode(config::ORDER_STAKE_KEY_HASH)
                .map_err(|e| CswapError::Address(format!("invalid stake key hex: {e}")))?
                .try_into()
                .map_err(|_| CswapError::Address("stake key must be 28 bytes".to_string()))?;
            ShelleyDelegationPart::Key(Hash::from(stake_bytes))
        }
    };

    let shelley = ShelleyAddress::new(network, payment, delegation);
    Ok(Address::Shelley(shelley))
}

/// Calculate total ADA deposit for a CSWAP order.
///
/// For buy orders (ADA -> Token):
///   swap_amount + min_utxo_return + batcher_fee
///
/// For sell orders (Token -> ADA):
///   min_utxo (to hold the token in the order UTxO) + batcher_fee
fn calculate_order_deposit(sell_amount: u64, is_selling_ada: bool) -> u64 {
    if is_selling_ada {
        sell_amount + config::MIN_UTXO_RETURN + config::BATCHER_FEE_ESTIMATE
    } else {
        // Selling tokens: need enough ADA for the script UTxO + batcher fee
        // The user's tokens are in the UTxO, plus ADA for min UTxO + batcher
        config::MIN_UTXO_RETURN + config::BATCHER_FEE_ESTIMATE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_buy_order() {
        let req = CswapOrderRequest {
            destination_pkh: hex::decode(
                "8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2",
            )
            .unwrap()
            .try_into()
            .unwrap(),
            destination_stake_key: Some(
                hex::decode("38220e3d6473be31a145f81eac6c32fd71231da373ff9ea07de72b2f")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            sell_asset: DatumAsset::ada(),
            sell_amount: 50_000_000, // 50 ADA
            buy_asset: DatumAsset::from_hex(
                "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7",
                "416c69656e73",
            )
            .unwrap(),
            min_receive_amount: 405_535,
            min_receive_ada: 2_000_000,
            execution_param: 1000,
            fee_tier: 15,
            network: Network::Mainnet,
        };

        let order = build_cswap_order(&req).unwrap();

        // Datum should be non-empty CBOR starting with d879
        assert!(!order.datum_bytes.is_empty());
        assert_eq!(order.datum_bytes[0], 0xd8);
        assert_eq!(order.datum_bytes[1], 0x79);

        // Datum hash should be 32 bytes
        assert_eq!(order.datum_hash.len(), 32);

        // Script address should be valid
        let addr_str = order.script_address.to_string();
        assert!(
            addr_str.starts_with("addr1z"),
            "expected addr1z prefix, got: {addr_str}"
        );

        // Total lovelace: 50M + 2M + 700K = 52.7M
        assert_eq!(order.total_lovelace, 52_700_000);
    }

    #[test]
    fn test_build_buy_order_no_stake_key() {
        let req = CswapOrderRequest {
            destination_pkh: hex::decode(
                "8fc751e6882866e2a113d7e31a55e8eb2721f70d953eecbfc5b48aa2",
            )
            .unwrap()
            .try_into()
            .unwrap(),
            destination_stake_key: None,
            sell_asset: DatumAsset::ada(),
            sell_amount: 10_000_000,
            buy_asset: DatumAsset::from_hex(
                "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7",
                "416c69656e73",
            )
            .unwrap(),
            min_receive_amount: 80_000,
            min_receive_ada: 2_000_000,
            execution_param: 1000,
            fee_tier: 15,
            network: Network::Mainnet,
        };

        let order = build_cswap_order(&req).unwrap();

        // Should use the order script's own stake key
        let addr_str = order.script_address.to_string();
        assert!(addr_str.starts_with("addr1z"));

        // 10M + 2M + 700K = 12.7M
        assert_eq!(order.total_lovelace, 12_700_000);
    }

    #[test]
    fn test_deposit_calculation_buy() {
        // Buy: 50 ADA + 2 ADA min UTxO + 0.7 ADA batcher = 52.7 ADA
        assert_eq!(calculate_order_deposit(50_000_000, true), 52_700_000);
    }

    #[test]
    fn test_deposit_calculation_sell() {
        // Sell: 2 ADA min UTxO + 0.7 ADA batcher = 2.7 ADA
        assert_eq!(calculate_order_deposit(1_000_000, false), 2_700_000);
    }
}
