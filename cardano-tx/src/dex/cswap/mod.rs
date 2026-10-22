//! CSWAP constant product AMM integration
//!
//! Provides datum construction, pool state decoding, and configuration
//! for submitting swap orders to CSWAP's constant product AMM pools on Cardano.
//!
//! A CSWAP order is a UTxO sent to the order script address with an
//! inline datum. The CSWAP batcher picks up and executes the order
//! against the pool.

pub mod config;
pub mod datum;
pub mod pool;

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
        // Buying tokens: min_receive = [min_utxo_ada, min_tokens] — ADA first,
        // as the CSWAP UI writes it (the real order `759d8fb1…` is pinned in
        // the tests byte for byte).
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

    // Build script address (always uses canonical CSWAP stake key)
    let script_address = build_order_script_address(req.network)?;

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
/// Always uses the order script's payment credential + the script's own
/// stake key. Unlike Splash, CSWAP does NOT embed the user's stake key
/// in the order address — the batcher monitors the canonical script address.
/// The user's destination is encoded in the datum instead.
fn build_order_script_address(network: Network) -> Result<Address, CswapError> {
    let script_hash_bytes: [u8; 28] = hex::decode(config::ORDER_SCRIPT_HASH)
        .map_err(|e| CswapError::Address(format!("invalid script hash hex: {e}")))?
        .try_into()
        .map_err(|_| CswapError::Address("script hash must be 28 bytes".to_string()))?;

    let stake_bytes: [u8; 28] = hex::decode(config::ORDER_STAKE_KEY_HASH)
        .map_err(|e| CswapError::Address(format!("invalid stake key hex: {e}")))?
        .try_into()
        .map_err(|_| CswapError::Address("stake key must be 28 bytes".to_string()))?;

    let payment = ShelleyPaymentPart::Script(Hash::from(script_hash_bytes));
    let delegation = ShelleyDelegationPart::Key(Hash::from(stake_bytes));

    let shelley = ShelleyAddress::new(network, payment, delegation);
    Ok(Address::Shelley(shelley))
}

/// The order as a transaction output: the deposit at the order address, with
/// the datum inline — the shape every order on the $ALIENS pool carried
/// (2026-10-22).
pub fn order_output(order: &CswapOrder) -> pallas_txbuilder::Output {
    crate::helpers::output::create_ada_output(order.script_address.clone(), order.total_lovelace)
        .set_inline_datum(order.datum_bytes.clone())
}

/// Calculate total ADA deposit for a CSWAP order.
///
/// For buy orders (ADA -> Token):
///   swap_amount + min_utxo_return + batcher_fee + batcher_network_fee
///
/// For sell orders (Token -> ADA):
///   min_utxo + batcher_fee + batcher_network_fee
pub fn calculate_order_deposit(sell_amount: u64, is_selling_ada: bool) -> u64 {
    let overhead =
        config::MIN_UTXO_RETURN + config::BATCHER_FEE + config::BATCHER_NETWORK_FEE_ESTIMATE;
    if is_selling_ada {
        sell_amount + overhead
    } else {
        overhead
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
            execution_param: 10,
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

        // Total lovelace: 50M + 2M + 0 + 690K = 52.69M — every real order's
        // amount + 2.69 ADA.
        assert_eq!(order.total_lovelace, 52_690_000);
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
            execution_param: 10,
            fee_tier: 15,
            network: Network::Mainnet,
        };

        let order = build_cswap_order(&req).unwrap();

        // Should use the order script's own stake key
        let addr_str = order.script_address.to_string();
        assert!(addr_str.starts_with("addr1z"));

        // 10M + 2M + 0 + 690K = 12.69M
        assert_eq!(order.total_lovelace, 12_690_000);
    }

    /// A real CSWAP UI buy order, rebuilt: 35 ADA for at least 919,264
    /// $ALIENS at 10% tolerance (datum `759d8fb1…`, filled in batch
    /// `632628a3…`, 2026-10-22). Same bytes, so the same hash — the batcher
    /// reads exactly what it reads from the UI.
    #[test]
    fn a_buy_order_matches_the_cswap_ui_byte_for_byte() {
        let req = CswapOrderRequest {
            destination_pkh: hex::decode(
                "a6140750bcf6592cae9468ed16e890f258f5d5d833f1c69f35897eb6",
            )
            .unwrap()
            .try_into()
            .unwrap(),
            destination_stake_key: Some(
                hex::decode("c62a4c06adc8e9329eeea693fbb0048e08a285570d4831b3a680233b")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            sell_asset: DatumAsset::ada(),
            sell_amount: 35_000_000,
            buy_asset: DatumAsset::from_hex(
                "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7",
                "416c69656e73",
            )
            .unwrap(),
            min_receive_amount: 919_264,
            min_receive_ada: config::MIN_UTXO_RETURN,
            execution_param: 1_000,
            fee_tier: config::DEFAULT_FEE_TIER,
            network: Network::Mainnet,
        };
        let order = build_cswap_order(&req).unwrap();
        assert_eq!(
            hex::encode(&order.datum_bytes),
            "d8799fd8799fd8799f581ca6140750bcf6592cae9468ed16e890f258f5d5d833f1c69f35897eb6ffd8799fd8799fd8799f581cc62a4c06adc8e9329eeea693fbb0048e08a285570d4831b3a680233bffffffff828340401a001e848083581c16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb746416c69656e731a000e06e08183404000d879801903e80fff"
        );
        assert_eq!(
            hex::encode(order.datum_hash),
            "759d8fb1589b339256f3339b482cc986c6e70956273e6f8b4017f4e688a6e9b5"
        );
        // And the deposit the real order carried.
        assert_eq!(order.total_lovelace, 37_690_000);
        // At the order address every real order sat at.
        assert!(
            order
                .script_address
                .to_bech32()
                .unwrap()
                .starts_with("addr1z8d9k3aw6w24eyfja")
        );
    }

    #[test]
    fn test_deposit_calculation_buy() {
        // Buy: 50 ADA + 2 ADA min UTxO + 0.69 ADA network = 52.69 ADA
        assert_eq!(calculate_order_deposit(50_000_000, true), 52_690_000);
    }

    #[test]
    fn test_deposit_calculation_sell() {
        // Sell: 2 ADA min UTxO + 0.69 ADA network = 2.69 ADA, as every sell
        // order on the $ALIENS pool carried.
        assert_eq!(calculate_order_deposit(1_000_000, false), 2_690_000);
    }
}
