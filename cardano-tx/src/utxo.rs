use cardano_assets::AssetId;
use maestro::{AddressUtxo, ProtocolParameters};

/// Find a UTxO containing a specific asset unit.
///
/// Returns `None` if no UTxO contains the target asset.
pub fn find_asset<'a>(utxos: &'a [AddressUtxo], target: &str) -> Option<&'a AddressUtxo> {
    utxos
        .iter()
        .find(|utxo| utxo.assets.iter().any(|asset| asset.unit == target))
}

/// Parameters for calculating minimum ADA for an output
#[derive(Debug, Clone, Default)]
pub struct OutputParams {
    /// Optional inline datum size in bytes (for CIP-68 reference tokens)
    pub datum_size: Option<usize>,
}

impl OutputParams {
    /// Create params for an output with an inline datum
    pub fn with_datum(datum_bytes: &[u8]) -> Self {
        Self {
            datum_size: Some(datum_bytes.len()),
        }
    }
}

/// Calculate the minimum ADA (lovelace) required for a UTxO output based on Cardano protocol.
///
/// The minimum ADA is calculated using the formula:
/// `minUTxOValue = (160 + |serialized_output|) * coinsPerUTxOByte`
///
/// # Arguments
///
/// * `protocol_params` - Protocol parameters from the network
/// * `assets` - Slice of asset IDs that will be in the output
///
/// # Returns
///
/// Minimum lovelace required for the output
pub fn calculate_min_ada(protocol_params: &ProtocolParameters, assets: &[AssetId]) -> u64 {
    calculate_min_ada_with_params(protocol_params, assets, &OutputParams::default())
}

/// Calculate the minimum ADA for an output with additional parameters (e.g., inline datum).
///
/// This extended version supports:
/// - Native assets (same as `calculate_min_ada`)
/// - Inline datums (for CIP-68 reference tokens)
///
/// # Arguments
///
/// * `protocol_params` - Protocol parameters from the network
/// * `assets` - Slice of asset IDs that will be in the output
/// * `params` - Additional output parameters (datum size, etc.)
///
/// # Returns
///
/// Minimum lovelace required for the output
///
/// # CBOR Structure Reference
///
/// From actual transaction analysis, a Babbage/Conway output with datum looks like:
/// ```text
/// a3                              -- map(3) - 1 byte
///   00                            -- key 0 - 1 byte
///   5839 [57 bytes]               -- address (59 bytes: 2 tag + 57 data)
///   01                            -- key 1 - 1 byte
///   82                            -- array(2) - 1 byte
///     1a [4 bytes]                -- lovelace (5 bytes)
///     a1                          -- map(1) policies - 1 byte
///       581c [28 bytes]           -- policy id (30 bytes: 2 tag + 28 data)
///       a1                        -- map(1) assets - 1 byte
///         54 [20 bytes]           -- asset name (21 bytes: 1 tag + 20 data)
///         01                      -- quantity - 1 byte
///   02                            -- key 2 - 1 byte
///   82 01                         -- array [1, ...] inline datum - 2 bytes
///     d8 18                       -- tag(24) embedded CBOR - 2 bytes
///     58 f1 [241 bytes]           -- datum bytes (244 bytes: 2 len + 241 data)
/// ```
/// Total: 370 bytes for this example
pub fn calculate_min_ada_with_params(
    protocol_params: &ProtocolParameters,
    assets: &[AssetId],
    params: &OutputParams,
) -> u64 {
    // Conway/Babbage formula: (160 + |serialized_output|) * coinsPerUTxOByte
    const UTXO_OVERHEAD: u64 = 160;

    // Output map header: a2 (no datum) or a3 (with datum) = 1 byte
    let map_header: u64 = 1;

    // Key 0 (1 byte) + Address (59 bytes for typical address with stake key)
    // Address breakdown: 5839 (2 bytes) + 57 bytes raw = 59 bytes
    let address_with_key: u64 = 1 + 59;

    // Key 1 (1 byte) + Value
    let value_size: u64 = if assets.is_empty() {
        // Pure lovelace: 1a + 4 bytes = 5 bytes typical
        5
    } else {
        // Array [lovelace, asset_map]: 82 (1 byte)
        let array_tag: u64 = 1;
        // Lovelace: 1a + 4 bytes = 5 bytes
        let lovelace_bytes: u64 = 5;

        // Group assets by policy
        let mut policy_assets: std::collections::HashMap<&str, Vec<&AssetId>> =
            std::collections::HashMap::new();
        for asset in assets {
            policy_assets
                .entry(&asset.policy_id)
                .or_default()
                .push(asset);
        }

        let num_policies = policy_assets.len() as u64;

        // Policies map tag: a1-b7 (1 byte) or b9 + 2 bytes
        let policies_map_tag: u64 = if num_policies < 24 { 1 } else { 3 };

        let mut policy_size: u64 = 0;
        for assets_in_policy in policy_assets.values() {
            // Policy ID: 581c (2 bytes tag) + 28 bytes = 30 bytes
            policy_size += 30;

            // Inner assets map tag
            let num_assets = assets_in_policy.len() as u64;
            let inner_map_tag: u64 = if num_assets < 24 { 1 } else { 3 };
            policy_size += inner_map_tag;

            for asset in assets_in_policy {
                // Asset name: tag + bytes
                let name_len = (asset.asset_name_hex.len() / 2) as u64;
                let name_tag: u64 = if name_len < 24 {
                    1 // 40-57
                } else if name_len < 256 {
                    2 // 58 xx
                } else {
                    3 // 59 xx xx
                };
                policy_size += name_tag + name_len;

                // Quantity: 1 byte for small values (01)
                policy_size += 1;
            }
        }

        array_tag + lovelace_bytes + policies_map_tag + policy_size
    };
    let value_with_key: u64 = 1 + value_size;

    // Key 2 (1 byte) + Datum (if present)
    // Inline datum structure: 82 01 d8 18 58/59 xx [datum bytes]
    let datum_with_key: u64 = match params.datum_size {
        Some(size) => {
            // 82 01 = array [1, ...] indicating inline datum = 2 bytes
            // d8 18 = tag(24) embedded CBOR = 2 bytes
            let datum_wrapper: u64 = 4;
            // Length prefix: 58 xx (2 bytes) or 59 xx xx (3 bytes)
            let length_prefix: u64 = if size < 256 { 2 } else { 3 };
            // Key 02 = 1 byte
            1 + datum_wrapper + length_prefix + size as u64
        }
        None => 0,
    };

    let serialized_output_size = map_header + address_with_key + value_with_key + datum_with_key;

    // Apply Babbage/Conway formula (no safety margin needed with correct calculation)
    (UTXO_OVERHEAD + serialized_output_size) * protocol_params.min_utxo_deposit_coefficient
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test against actual transaction data from logs
    #[test]
    fn test_min_ada_calculation_matches_real_tx() {
        // From failed transaction:
        // - Asset name hex: 000643b0686f646c63726f66745f303030303034 (20 bytes)
        // - Policy ID: 88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9
        // - Datum size: 241 bytes
        // - Required min UTxO: 2284300 lovelace

        let protocol_params = ProtocolParameters {
            min_utxo_deposit_coefficient: 4310,
            min_fee_coefficient: 44,
            min_fee_constant: maestro::AdaLovelace {
                ada: maestro::AdaAmount { lovelace: 155381 },
            },
            script_execution_prices: None,
        };

        // Create asset ID matching the transaction
        let asset_id: AssetId =
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9.000643b0686f646c63726f66745f303030303034"
                .parse()
                .unwrap();

        let params = OutputParams {
            datum_size: Some(241),
        };

        let calculated = calculate_min_ada_with_params(&protocol_params, &[asset_id], &params);

        // Expected: (160 + 370) * 4310 = 2284300
        assert_eq!(
            calculated, 2284300,
            "Calculated {calculated} but expected 2284300"
        );
    }
}
