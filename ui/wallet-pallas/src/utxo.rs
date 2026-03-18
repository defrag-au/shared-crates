//! CIP-30 UTxO decoding
//!
//! Decode CBOR-encoded UTxOs from CIP-30 `getUtxos()` into `UtxoApi` values.
//! CIP-30 returns `TransactionUnspentOutput = (TransactionInput, TransactionOutput)` CBOR.

use crate::PallasError;
use cardano_assets::utxo::{AssetQuantity, UtxoApi, UtxoTag};
use cardano_assets::AssetId;
use pallas_addresses::Address;
use pallas_codec::minicbor;
use pallas_primitives::conway::{TransactionInput, TransactionOutput};

/// Decode a single CIP-30 CBOR-encoded UTxO hex string into a `UtxoApi`.
///
/// CIP-30 `getUtxos()` returns an array of hex-encoded CBOR values, each
/// representing a `TransactionUnspentOutput = (TransactionInput, TransactionOutput)`.
pub fn decode_utxo(hex_str: &str) -> Result<UtxoApi, PallasError> {
    let bytes = hex::decode(hex_str)?;

    // CIP-30 encodes as a two-element CBOR array: [TransactionInput, TransactionOutput]
    let (input, output): (TransactionInput, TransactionOutput) =
        minicbor::decode(&bytes).map_err(|e| PallasError::CborDecode(e.to_string()))?;

    let tx_hash = hex::encode(input.transaction_id.as_ref());
    let output_index = input.index as u32;

    let decoded = extract_output(&output);

    Ok(UtxoApi {
        tx_hash,
        output_index,
        lovelace: decoded.lovelace,
        assets: decoded.assets,
        tags: decoded.tags,
    })
}

/// Decode multiple CIP-30 CBOR-encoded UTxO hex strings into `UtxoApi` values.
pub fn decode_utxos(hex_strings: &[String]) -> Result<Vec<UtxoApi>, PallasError> {
    hex_strings.iter().map(|h| decode_utxo(h)).collect()
}

/// Decoded output fields from a TransactionOutput.
struct DecodedOutput {
    lovelace: u64,
    assets: Vec<AssetQuantity>,
    tags: Vec<UtxoTag>,
}

/// Check if raw address bytes have a script payment credential.
///
/// Shelley address header byte: top 4 bits = type, bottom 4 bits = network.
/// Types with script payment: 1, 3, 5, 7 (odd types in 0-7 range).
fn is_script_payment_address(addr_bytes: &[u8]) -> bool {
    if addr_bytes.is_empty() {
        return false;
    }
    // Try parsing with pallas-addresses for robustness
    if let Ok(Address::Shelley(shelley)) = Address::from_bytes(addr_bytes) {
        return shelley.payment().is_script();
    }
    false
}

/// Extract lovelace, multi-assets, and tags from a TransactionOutput.
fn extract_output(output: &TransactionOutput) -> DecodedOutput {
    match output {
        TransactionOutput::Legacy(legacy) => {
            use pallas_primitives::alonzo::Value;
            let (lovelace, assets) = match &legacy.amount {
                Value::Coin(lovelace) => (*lovelace, vec![]),
                Value::Multiasset(lovelace, multi_assets) => {
                    let mut assets = Vec::new();
                    for (policy_id, asset_map) in multi_assets.iter() {
                        let policy_hex = hex::encode(policy_id.as_ref());
                        for (asset_name, quantity) in asset_map.iter() {
                            let asset_name_hex = hex::encode(asset_name.as_ref() as &[u8]);
                            let asset_id =
                                AssetId::new_unchecked(policy_hex.clone(), asset_name_hex);
                            assets.push(AssetQuantity {
                                asset_id,
                                quantity: *quantity,
                            });
                        }
                    }
                    (*lovelace, assets)
                }
            };
            // Legacy outputs: check address for script credential, no datum/script_ref
            let mut tags = Vec::new();
            if is_script_payment_address(&legacy.address) {
                tags.push(UtxoTag::ScriptAddress);
            }
            DecodedOutput {
                lovelace,
                assets,
                tags,
            }
        }
        TransactionOutput::PostAlonzo(post_alonzo) => {
            use pallas_primitives::conway::Value;
            let (lovelace, assets) = match &post_alonzo.value {
                Value::Coin(lovelace) => (*lovelace, vec![]),
                Value::Multiasset(lovelace, multi_assets) => {
                    let mut assets = Vec::new();
                    for (policy_id, asset_map) in multi_assets.iter() {
                        let policy_hex = hex::encode(policy_id.as_ref());
                        for (asset_name, quantity) in asset_map.iter() {
                            let asset_name_hex = hex::encode(asset_name.as_ref() as &[u8]);
                            let asset_id =
                                AssetId::new_unchecked(policy_hex.clone(), asset_name_hex);
                            assets.push(AssetQuantity {
                                asset_id,
                                quantity: u64::from(*quantity),
                            });
                        }
                    }
                    (*lovelace, assets)
                }
            };
            let mut tags = Vec::new();
            if post_alonzo.datum_option.is_some() {
                tags.push(UtxoTag::HasDatum);
            }
            if post_alonzo.script_ref.is_some() {
                tags.push(UtxoTag::HasScriptRef);
            }
            if is_script_payment_address(&post_alonzo.address) {
                tags.push(UtxoTag::ScriptAddress);
            }
            DecodedOutput {
                lovelace,
                assets,
                tags,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_invalid_cbor() {
        let result = decode_utxo("deadbeef");
        assert!(result.is_err(), "Invalid CBOR should return error");
    }

    #[test]
    fn test_decode_utxos_empty() {
        let result = decode_utxos(&[]).unwrap();
        assert!(result.is_empty());
    }
}
