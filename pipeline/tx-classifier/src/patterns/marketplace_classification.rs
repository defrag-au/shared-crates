//! Marketplace-specific classification logic
//!
//! This module contains classification logic that was previously in address-registry
//! but belongs in the tx-classifier domain since it analyzes transaction content.

use address_registry::Marketplace;
use tracing::debug;
use transactions::{RawTxData, TxDatum};

/// Helper function to check if a string is a valid policy ID (56 hex characters)
pub fn is_valid_policy_id(s: &str) -> bool {
    s.len() == 56 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Helper function to find the first policy ID in a string
/// Returns the first 56-character hex string found, or None if no valid policy ID exists
pub fn find_policy_id_in_string(text: &str) -> Option<&str> {
    // Early return if text is too short to contain a policy ID
    if text.len() < 56 {
        return None;
    }

    let text_lower = text.to_lowercase();
    let chars: Vec<char> = text_lower.chars().collect();

    // Ensure we don't go out of bounds - need at least 56 characters from position i
    for i in 0..=(chars.len() - 56) {
        let candidate: String = chars[i..i + 56].iter().collect();
        if is_valid_policy_id(&candidate) {
            // Return the slice from the original text to preserve case
            return Some(&text[i..i + 56]);
        }
    }

    None
}

/// Extract policy ID from offer contract datums - marketplace specific implementation
pub fn extract_offer_policy_id(
    marketplace: Marketplace,
    raw_tx_data: &RawTxData,
    script_address: &str,
    output_indices: &[u32],
) -> String {
    let (policy_id, _) =
        extract_offer_policy_and_asset(marketplace, raw_tx_data, script_address, output_indices);
    policy_id
}

/// Extract both policy ID and encoded asset name from offer contract datums
/// Returns (policy_id, encoded_asset_name) where asset_name is None for collection offers
pub fn extract_offer_policy_and_asset(
    marketplace: Marketplace,
    raw_tx_data: &RawTxData,
    script_address: &str,
    output_indices: &[u32],
) -> (String, Option<String>) {
    match marketplace {
        Marketplace::JpgStore => {
            debug!(
                "JPG.store: Extracting policy ID and asset from address {}",
                script_address
            );

            // For offer updates, the original offer data is in INPUT datums (where policy+asset info is stored)
            // For other operations, the new offer data is in OUTPUT datums
            // We'll check INPUTs first, then fall back to OUTPUTs

            // First, try to extract from INPUT datums (for offer updates)
            debug!("JPG.store: Checking INPUT datums for offer data");
            for (input_idx, input) in raw_tx_data.inputs.iter().enumerate() {
                if input.address == script_address {
                    debug!(
                        "JPG.store: Found input {} at script address, checking datum",
                        input_idx
                    );
                    if let Some(datum_info) = &input.datum {
                        debug!("JPG.store: Found input datum, extracting policy and asset...");

                        // Try CBOR decoding if we have bytes
                        if let Some(bytes) = datum_info.bytes() {
                            debug!(
                                "JPG.store: Attempting CBOR decoding from input bytes (length: {})",
                                bytes.len()
                            );
                            debug!(
                                "JPG.store: Input CBOR bytes prefix: {}",
                                &bytes[..100.min(bytes.len())]
                            );
                            let tx_datum = match datum_info.json() {
                                Some(json) => TxDatum::Json {
                                    hash: datum_info.hash().to_string(),
                                    json: json.clone(),
                                    bytes: Some(bytes.to_string()),
                                },
                                None => TxDatum::Bytes {
                                    hash: datum_info.hash().to_string(),
                                    bytes: bytes.to_string(),
                                },
                            };

                            let policy_assets = tx_datum.extract_policy_assets();
                            debug!(
                                "JPG.store: Input CBOR policy-asset extraction returned {} pairs",
                                policy_assets.len()
                            );
                            if !policy_assets.is_empty() {
                                debug!(
                                    "JPG.store: Found {} policy-asset pairs via input CBOR: {:?}",
                                    policy_assets.len(),
                                    policy_assets
                                );
                                // Return the first policy-asset pair found
                                return policy_assets[0].clone();
                            }

                            // Fallback to old method if new one doesn't find anything
                            let policy_ids = tx_datum.extract_policy_ids();
                            if !policy_ids.is_empty() {
                                debug!(
                                    "JPG.store: Found {} policy IDs via input CBOR fallback: {:?}",
                                    policy_ids.len(),
                                    policy_ids
                                );
                                return (policy_ids[0].clone(), None);
                            }
                        }

                        // Fallback to JSON if CBOR didn't work and JSON is available
                        if let Some(datum) = datum_info.json() {
                            debug!("JPG.store: Fallback to input JSON parsing");
                            if let Some(datum_str) = datum.to_string().as_str().get(..) {
                                debug!(
                                    "JPG.store: Input datum string: {}",
                                    &datum_str[..100.min(datum_str.len())]
                                );
                                // Try to find any 56-character hex string (policy ID length)
                                if let Some(policy_id) = find_policy_id_in_string(datum_str) {
                                    debug!(
                                        "JPG.store: Found policy via input JSON search: {}",
                                        policy_id
                                    );
                                    return (policy_id.to_string(), None);
                                }
                            }
                        }

                        debug!("JPG.store: No policy ID found in input datum {}", input_idx);
                    } else {
                        debug!("JPG.store: No datum found for input {}", input_idx);
                    }
                }
            }

            // Second, try CBOR and JSON extraction from OUTPUT datums (for new offers)
            debug!(
                "JPG.store: No policy found in inputs, checking {} outputs",
                output_indices.len()
            );
            for &index in output_indices {
                debug!("JPG.store: Checking output index {}", index);
                if let Some(output) = raw_tx_data.outputs.get(index as usize) {
                    debug!(
                        "JPG.store: Output {} goes to address {}",
                        index, output.address
                    );
                    if output.address == script_address {
                        debug!("JPG.store: Address matches! Checking for datum...");
                        if let Some(datum_info) = &output.datum {
                            debug!("JPG.store: Found output datum, extracting policy and asset...");

                            // First, try CBOR decoding if we have bytes
                            if let Some(bytes) = datum_info.bytes() {
                                debug!("JPG.store: Attempting CBOR decoding from output bytes (length: {})", bytes.len());
                                debug!(
                                    "JPG.store: Output CBOR bytes prefix: {}",
                                    &bytes[..100.min(bytes.len())]
                                );
                                let tx_datum = match datum_info.json() {
                                    Some(json) => TxDatum::Json {
                                        hash: datum_info.hash().to_string(),
                                        json: json.clone(),
                                        bytes: Some(bytes.to_string()),
                                    },
                                    None => TxDatum::Bytes {
                                        hash: datum_info.hash().to_string(),
                                        bytes: bytes.to_string(),
                                    },
                                };

                                let policy_assets = tx_datum.extract_policy_assets();
                                debug!("JPG.store: Output CBOR policy-asset extraction returned {} pairs", policy_assets.len());
                                if !policy_assets.is_empty() {
                                    debug!(
                                        "JPG.store: Found {} policy-asset pairs via output CBOR: {:?}",
                                        policy_assets.len(),
                                        policy_assets
                                    );
                                    // Return the first policy-asset pair found
                                    return policy_assets[0].clone();
                                }

                                // Fallback to old method if new one doesn't find anything
                                let policy_ids = tx_datum.extract_policy_ids();
                                if !policy_ids.is_empty() {
                                    debug!(
                                        "JPG.store: Found {} policy IDs via output CBOR fallback: {:?}",
                                        policy_ids.len(),
                                        policy_ids
                                    );
                                    return (policy_ids[0].clone(), None);
                                }
                            }

                            // Fallback to JSON if CBOR didn't work and JSON is available
                            if let Some(datum) = datum_info.json() {
                                debug!("JPG.store: Fallback to output JSON parsing");
                                if let Some(datum_str) = datum.to_string().as_str().get(..) {
                                    debug!(
                                        "JPG.store: Output datum string: {}",
                                        &datum_str[..100.min(datum_str.len())]
                                    );
                                    // Try to find any 56-character hex string (policy ID length)
                                    if let Some(policy_id) = find_policy_id_in_string(datum_str) {
                                        debug!(
                                            "JPG.store: Found policy via output JSON search: {}",
                                            policy_id
                                        );
                                        return (policy_id.to_string(), None);
                                    }
                                }
                            }

                            debug!("JPG.store: No policy ID found in output datum {}", index);
                        } else {
                            debug!("JPG.store: No datum found for output {}", index);
                        }
                    } else {
                        debug!(
                            "JPG.store: Address mismatch - expected {}, got {}",
                            script_address, output.address
                        );
                    }
                } else {
                    debug!("JPG.store: Output index {} not found", index);
                }
            }

            // JPG.store specific fallback: search transaction metadata for policy IDs
            debug!("JPG.store: No policy found in datums, checking transaction metadata");
            if let Some(policy_id) = extract_policy_from_metadata(raw_tx_data) {
                debug!("JPG.store: Found policy in metadata: {}", policy_id);
                return (policy_id, None);
            }

            debug!("JPG.store: No policy found, returning unknown");
            ("unknown".to_string(), None)
        }
        _ => {
            debug!(
                "Marketplace {:?}: No specific policy extraction logic implemented",
                marketplace
            );
            ("unknown".to_string(), None)
        }
    }
}

/// Extract policy ID from JPG.store transaction metadata
fn extract_policy_from_metadata(raw_tx_data: &RawTxData) -> Option<String> {
    let metadata = raw_tx_data.metadata.as_ref()?;

    // Search through all metadata values for policy IDs
    if let Some(metadata_obj) = metadata.as_object() {
        for (key, value) in metadata_obj {
            debug!("JPG.store: Checking metadata key {}: {}", key, value);

            // Convert metadata value to string for searching
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };

            // First, look for JPG.store specific format: policy_id::asset_name
            if let Some(policy_id) = extract_jpg_store_policy_from_string(&value_str) {
                debug!(
                    "JPG.store: Found JPG.store policy ID in metadata key {}: {}",
                    key, policy_id
                );
                return Some(policy_id);
            }

            // Fallback: look for any 56-character hex string (standard policy ID)
            if let Some(policy_id) = find_policy_id_in_string(&value_str) {
                debug!(
                    "JPG.store: Found standard policy ID in metadata key {}: {}",
                    key, policy_id
                );
                return Some(policy_id.to_string());
            }
        }
    }

    debug!("JPG.store: No policy ID found in transaction metadata");
    None
}

/// Extract policy ID from JPG.store specific format: policy_id::asset_name
fn extract_jpg_store_policy_from_string(text: &str) -> Option<String> {
    // Look for pattern: 56_char_hex::something
    if let Some(double_colon_pos) = text.find("::") {
        let potential_policy = &text[..double_colon_pos];

        // Verify it's exactly 56 characters and all hex
        if potential_policy.len() == 56 && is_valid_policy_id(potential_policy) {
            debug!(
                "JPG.store: Extracted JPG.store policy from '{}': {}",
                text, potential_policy
            );
            return Some(potential_policy.to_string());
        }
    }
    None
}
