//! Splash DEX integration
//!
//! Provides datum construction, beacon calculation, and configuration
//! for submitting spot swap orders to the Splash DEX on Cardano.
//!
//! A Splash spot order is structurally a standard ADA send to a script address
//! with an inline datum — no Plutus execution is needed at order creation time.
//! The Splash batcher picks up and executes the order against the pool.

pub mod beacon;
pub mod config;
pub mod datum;
pub mod fetch;

use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_crypto::hash::Hash;
use pallas_primitives::Fragment;

use beacon::{calculate_beacon, EMPTY_BEACON};
use config::SpotOrderConfig;
use datum::{DatumAsset, RationalPrice, SpotOrderParams};

/// Errors from Splash DEX operations
#[derive(Debug, thiserror::Error)]
pub enum SplashError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("CBOR encoding failed: {0}")]
    CborEncoding(String),
    #[error("config fetch failed: {0}")]
    ConfigFetch(String),
    #[error("address error: {0}")]
    Address(String),
}

/// Parameters for building a complete spot order
pub struct SpotOrderRequest {
    /// Input asset to sell
    pub input_asset: DatumAsset,
    /// Amount of input asset to sell (in native units — lovelace for ADA)
    pub input_amount: u64,
    /// Output asset to buy
    pub output_asset: DatumAsset,
    /// Limit price as numerator/denominator (e.g. 1000/1 = 1000 output per input)
    pub price: RationalPrice,
    /// User's payment public key hash (28 bytes)
    pub payment_pkh: [u8; 28],
    /// User's staking key hash (28 bytes, for script address delegation)
    pub stake_key_hash: Option<[u8; 28]>,
    /// Input UTxO transaction hash (32 bytes) — needed for beacon
    pub input_tx_hash: [u8; 32],
    /// Input UTxO index — needed for beacon
    pub input_utxo_index: u64,
    /// Resolved config from `config::fetch_config()`
    pub config: SpotOrderConfig,
    /// Executor fee (from `config::fetch_executor_fee()` or default)
    pub executor_fee: u64,
    /// Whether this is a market order (affects step count and deposit)
    pub is_market: bool,
    /// Network (mainnet or testnet)
    pub network: Network,
}

/// Result of building a spot order — everything needed to construct the transaction
pub struct SpotOrder {
    /// CBOR-encoded datum bytes (to be set as inline datum on the script output)
    pub datum_bytes: Vec<u8>,
    /// Script address to send to (includes user's stake key for delegation)
    pub script_address: Address,
    /// Total lovelace required for the script output
    pub total_lovelace: u64,
    /// Beacon hex for logging/tracking
    pub beacon_hex: String,
}

/// Build a complete Splash spot order.
///
/// This calculates the beacon, constructs the datum, derives the script address,
/// and computes the total lovelace deposit. The caller then uses these values
/// to build and submit the transaction.
pub fn build_spot_order(req: &SpotOrderRequest) -> Result<SpotOrder, SplashError> {
    // 1. Build a datum with an empty beacon to calculate the real beacon
    let empty_beacon_params = SpotOrderParams {
        beacon: EMPTY_BEACON.to_vec(),
        input_asset: DatumAsset {
            policy_id: req.input_asset.policy_id.clone(),
            asset_name: req.input_asset.asset_name.clone(),
        },
        input_amount: req.input_amount,
        cost_per_ex_step: req.config.step_cost,
        min_marginal_output: calculate_min_marginal_output(req),
        output_asset: DatumAsset {
            policy_id: req.output_asset.policy_id.clone(),
            asset_name: req.output_asset.asset_name.clone(),
        },
        price: RationalPrice {
            numerator: req.price.numerator,
            denominator: req.price.denominator,
        },
        executor_fee: req.executor_fee,
        payment_pkh: req.payment_pkh.to_vec(),
        stake_pkh: req.stake_key_hash.map(|h| h.to_vec()),
        cancel_pkh: req.payment_pkh.to_vec(),
        permitted_executors: vec![
            hex::decode(config::DEFAULT_BATCHER_KEY).expect("DEFAULT_BATCHER_KEY is valid hex")
        ],
    };

    let empty_beacon_datum = datum::build_spot_order_datum(&empty_beacon_params);
    let empty_beacon_cbor = empty_beacon_datum
        .encode_fragment()
        .map_err(|e| SplashError::CborEncoding(format!("{e}")))?;

    // 2. Calculate the real beacon
    let beacon = calculate_beacon(
        &req.input_tx_hash,
        req.input_utxo_index,
        0, // order_index: 0 for single-order transactions
        &empty_beacon_cbor,
    );

    // 3. Build the final datum with the real beacon
    let final_params = SpotOrderParams {
        beacon: beacon.to_vec(),
        ..empty_beacon_params
    };

    let datum_bytes = datum::encode_datum(&final_params)?;

    // 4. Construct the script address (script hash + user's stake key)
    let script_address = build_script_address(
        &req.config.script_hash,
        req.stake_key_hash.as_ref(),
        req.network,
    )?;

    // 5. Calculate total lovelace for the output
    let input_lovelace = if req.input_asset.policy_id.is_empty() {
        req.input_amount
    } else {
        0
    };
    let total_lovelace = config::calculate_order_deposit(
        &req.config,
        req.executor_fee,
        input_lovelace,
        req.is_market,
    );

    Ok(SpotOrder {
        datum_bytes,
        script_address,
        total_lovelace,
        beacon_hex: hex::encode(beacon),
    })
}

/// Build the script address from the validator hash and optional stake key.
///
/// The resulting address has a script payment credential and (optionally)
/// a key-based stake credential, allowing the user to receive staking rewards
/// on ADA locked in their order.
fn build_script_address(
    script_hash_hex: &str,
    stake_key_hash: Option<&[u8; 28]>,
    network: Network,
) -> Result<Address, SplashError> {
    let script_hash_bytes: [u8; 28] = hex::decode(script_hash_hex)
        .map_err(|e| SplashError::Address(format!("invalid script hash hex: {e}")))?
        .try_into()
        .map_err(|_| SplashError::Address("script hash must be 28 bytes".to_string()))?;

    let payment = ShelleyPaymentPart::Script(Hash::from(script_hash_bytes));

    let delegation = match stake_key_hash {
        Some(hash) => ShelleyDelegationPart::Key(Hash::from(*hash)),
        None => ShelleyDelegationPart::Null,
    };

    let shelley = ShelleyAddress::new(network, payment, delegation);
    Ok(Address::Shelley(shelley))
}

/// Calculate minimum marginal output for a spot order.
///
/// This represents the minimum output amount per execution step.
/// For ADA→Token swaps: based on the price and step cost.
/// For Token→ADA swaps: the min UTxO threshold.
fn calculate_min_marginal_output(req: &SpotOrderRequest) -> u64 {
    // SDK: minMarginalOutput = Math.ceil(inputAmount * price.numerator / price.denominator)
    // This is the expected output for the full input amount
    if req.price.denominator == 0 {
        return 0;
    }

    let numerator = req.input_amount as u128 * req.price.numerator as u128;
    let result = numerator.div_ceil(req.price.denominator as u128);

    result.min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_script_address_mainnet() {
        let script_hash = "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02";
        let stake_hash: [u8; 28] =
            hex::decode("de7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f")
                .unwrap()
                .try_into()
                .unwrap();

        let addr = build_script_address(script_hash, Some(&stake_hash), Network::Mainnet).unwrap();

        let bech32 = addr.to_string();
        assert!(bech32.starts_with("addr1"));
        // Script addresses start with "addr1z" (type 0x3x in Shelley)
        assert!(
            bech32.starts_with("addr1z"),
            "expected addr1z prefix for script+key address, got: {bech32}"
        );
    }

    #[test]
    fn test_build_script_address_no_stake() {
        let script_hash = "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02";

        let addr = build_script_address(script_hash, None, Network::Mainnet).unwrap();

        let bech32 = addr.to_string();
        assert!(bech32.starts_with("addr1"));
    }

    #[test]
    fn test_calculate_min_marginal_output() {
        let req = SpotOrderRequest {
            input_asset: DatumAsset::ada(),
            input_amount: 1_000_000,
            output_asset: DatumAsset::from_hex("abcd", "1234").unwrap(),
            price: RationalPrice {
                numerator: 5,
                denominator: 2,
            },
            payment_pkh: [0u8; 28],
            stake_key_hash: None,
            input_tx_hash: [0u8; 32],
            input_utxo_index: 0,
            config: SpotOrderConfig {
                script_hash: "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02".to_string(),
                step_cost: 1_000_000,
                worst_step_cost: 1_500_000,
                default_executor_fee: 1_000_000,
                max_steps: 5,
                max_steps_market: 1,
            },
            executor_fee: 1_000_000,
            is_market: false,
            network: Network::Mainnet,
        };

        // 1_000_000 * 5 / 2 = 2_500_000
        assert_eq!(calculate_min_marginal_output(&req), 2_500_000);
    }

    #[test]
    fn test_min_marginal_output_rounds_up() {
        let req = SpotOrderRequest {
            input_asset: DatumAsset::ada(),
            input_amount: 7,
            output_asset: DatumAsset::from_hex("abcd", "1234").unwrap(),
            price: RationalPrice {
                numerator: 1,
                denominator: 3,
            },
            payment_pkh: [0u8; 28],
            stake_key_hash: None,
            input_tx_hash: [0u8; 32],
            input_utxo_index: 0,
            config: SpotOrderConfig {
                script_hash: "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02".to_string(),
                step_cost: 1_000_000,
                worst_step_cost: 1_500_000,
                default_executor_fee: 1_000_000,
                max_steps: 5,
                max_steps_market: 1,
            },
            executor_fee: 1_000_000,
            is_market: false,
            network: Network::Mainnet,
        };

        // ceil(7 * 1 / 3) = ceil(2.33) = 3
        assert_eq!(calculate_min_marginal_output(&req), 3);
    }

    #[test]
    fn test_build_spot_order_produces_valid_output() {
        let config = SpotOrderConfig {
            script_hash: "464eeee89f05aff787d40045af2a40a83fd96c513197d32fbc54ff02".to_string(),
            step_cost: 1_000_000,
            worst_step_cost: 1_500_000,
            default_executor_fee: 1_000_000,
            max_steps: 5,
            max_steps_market: 1,
        };

        let req = SpotOrderRequest {
            input_asset: DatumAsset::ada(),
            input_amount: 100_000_000, // 100 ADA
            output_asset: DatumAsset::from_hex(
                "fb4f75d1ad4eb5c21efd5a32a90c076e63a79daccf25afe4ccd4f714",
                "24504f50534e454b",
            )
            .unwrap(),
            price: RationalPrice {
                numerator: 1000,
                denominator: 1,
            },
            payment_pkh: hex::decode("74104cd5ca6288c1dd2e22ee5c874fdcfc1b81897462d91153496430")
                .unwrap()
                .try_into()
                .unwrap(),
            stake_key_hash: Some(
                hex::decode("de7866fe5068ebf3c87dcdb568da528da5dcb5f659d9b60010e7450f")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            input_tx_hash: [0xAA; 32],
            input_utxo_index: 0,
            config,
            executor_fee: 1_000_000,
            is_market: false,
            network: Network::Mainnet,
        };

        let order = build_spot_order(&req).unwrap();

        // Datum should be non-empty CBOR starting with d879 (Constr 0 tag)
        assert!(!order.datum_bytes.is_empty());
        assert_eq!(order.datum_bytes[0], 0xd8);
        assert_eq!(order.datum_bytes[1], 0x79);

        // Beacon should be 28 bytes (56 hex chars)
        assert_eq!(order.beacon_hex.len(), 56);

        // Script address should be valid
        let addr_str = order.script_address.to_string();
        assert!(addr_str.starts_with("addr1z"));

        // Total lovelace should include deposit + input (limit order, max_steps=5)
        // 100_000_000 (input) + 1_500_000 (worst_step) + 1_000_000 * 4 (steps) + 1_000_000 (executor) + 1_500_000 (receive deposit)
        assert_eq!(
            order.total_lovelace,
            100_000_000 + 1_500_000 + 4_000_000 + 1_000_000 + 1_500_000
        );
    }
}
