//! UTxORPC integration for Cardano asset extraction
//!
//! This module provides utilities for extracting AssetV2 instances from UTxORPC transaction data,
//! including CIP-25 metadata parsing using JSON conversion for robust compatibility with various
//! NFT metadata formats.

use crate::{Asset, AssetId, AssetV2, Traits, AssetMetadata as CardanoAssetMetadata};
use serde_json::Value;
use tracing::debug;
use utxorpc_spec::utxorpc::v1alpha::cardano as u5c;

/// Extracted metadata from UTxORPC transaction
#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub image: String,
    pub traits: Traits,
}

impl Default for AssetMetadata {
    fn default() -> Self {
        Self {
            image: String::new(),
            traits: Traits::new(),
        }
    }
}

/// Extract AssetV2 instances from UTxORPC mint data with real CIP-25 metadata
///
/// This function processes all minted assets in a UTxORPC transaction and returns
/// a vector of AssetV2 instances with:
/// - Proper AssetId (policy_id + asset_name_hex)
/// - Decoded asset name for human readability
/// - Real CIP-25 metadata extracted from transaction auxiliary data (image URLs and traits)
///
/// The metadata extraction uses JSON conversion to leverage the battle-tested
/// cardano-assets AssetMetadata deserializer for robust parsing of various CIP-25 formats.
pub fn extract_mint_assets_from_utxorpc_tx(tx: &u5c::Tx) -> Vec<AssetV2> {
    let mut assets = Vec::new();
    
    for multiasset in &tx.mint {
        let policy_id = hex::encode(&multiasset.policy_id);
        
        for asset in &multiasset.assets {
            let asset_name_hex = hex::encode(&asset.name);
            
            // Handle empty asset names (fungible tokens) with a placeholder
            let asset_name_for_id = if asset_name_hex.is_empty() {
                "00".to_string()
            } else {
                asset_name_hex.clone()
            };
            
            let asset_id = AssetId::new_unchecked(policy_id.clone(), asset_name_for_id);
            
            // Decode asset name for display (fallback to hex if not valid UTF-8)
            let display_name = if asset_name_hex.is_empty() {
                format!("{}[fungible]", &policy_id[..8])
            } else {
                match hex::decode(&asset_name_hex) {
                    Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| asset_name_hex.clone()),
                    Err(_) => asset_name_hex.clone(),
                }
            };
            
            // Extract real metadata from UTxORPC transaction auxiliary data
            let metadata = extract_asset_metadata(tx, &policy_id, &asset_name_hex);
            
            if !metadata.image.is_empty() || !metadata.traits.inner().is_empty() {
                debug!("Found CIP-25 metadata for {}: image={}, traits={}", 
                       display_name, metadata.image, metadata.traits.inner().len());
            } else {
                debug!("No CIP-25 metadata found for {}", display_name);
            }
            
            // Create AssetV2 with extracted metadata (or empty if none found)
            let asset_v2 = AssetV2::new(
                asset_id,
                display_name,
                metadata.image, // Real metadata or empty string
                metadata.traits, // Real traits or empty
                None, // No rarity rank - would need marketplace data
                vec![], // Empty tags - would need rarity/marketplace data
            );
            
            debug!(
                "Extracted mint asset: policy={}, name_hex={}, display_name={}",
                policy_id, asset_name_hex, asset_v2.name
            );
            
            assets.push(asset_v2);
        }
    }
    
    debug!("Extracted {} mint assets from UTxORPC transaction", assets.len());
    assets
}

/// Extract metadata for a specific asset from UTxORPC transaction auxiliary data
/// 
/// This function extracts CIP-25 metadata from the transaction's auxiliary data
/// which contains label 721 metadata with policy_id -> asset_name -> metadata mapping.
pub fn extract_asset_metadata(
    tx: &u5c::Tx,
    policy_id: &str,
    asset_name_hex: &str,
) -> AssetMetadata {
    debug!("Extracting metadata for policy {} asset {}", policy_id, asset_name_hex);
    
    // Start with default empty metadata
    let mut metadata = AssetMetadata::default();
    
    // Check if transaction has auxiliary data with CIP-25 metadata
    if let Some(auxiliary) = &tx.auxiliary {
        debug!("Found auxiliary data, searching for CIP-25 metadata...");
        
        for aux_metadata in &auxiliary.metadata {
            // CIP-25 metadata uses label 721
            if aux_metadata.label == 721 {
                debug!("Found label 721 metadata, extracting asset metadata...");
                
                if let Some(value) = &aux_metadata.value {
                    metadata = extract_from_cip25_metadata(value, policy_id, asset_name_hex);
                    break;
                }
            }
        }
    } else {
        debug!("No auxiliary data found in transaction");
    }
    
    metadata
}

/// Extract metadata from CIP-25 metadata structure using JSON conversion
/// 
/// This function converts the UTxORPC Metadatum to JSON and then leverages
/// the existing cardano-assets AssetMetadata deserializer for robust parsing.
fn extract_from_cip25_metadata(
    metadatum: &u5c::Metadatum,
    policy_id: &str,
    asset_name_hex: &str,
) -> AssetMetadata {
    debug!("Extracting CIP-25 metadata for policy {} asset {}", policy_id, asset_name_hex);
    
    // Try the JSON conversion approach using cardano-assets deserializer
    if let Ok(json_value) = metadatum_to_json_value(metadatum) {
        if let Some(metadata) = extract_metadata_via_json(&json_value, policy_id, asset_name_hex) {
            debug!("Successfully extracted metadata via JSON conversion");
            return metadata;
        }
    }
    
    debug!("JSON conversion failed, returning empty metadata");
    AssetMetadata::default()
}

/// Convert UTxORPC Metadatum to string if possible
fn metadatum_to_string(metadatum: &u5c::Metadatum) -> Option<String> {
    if let Some(metadatum_kind) = &metadatum.metadatum {
        match metadatum_kind {
            u5c::metadatum::Metadatum::Text(text) => Some(text.clone()),
            u5c::metadatum::Metadatum::Bytes(bytes) => {
                // Try to decode bytes as UTF-8 string
                String::from_utf8(bytes.to_vec()).ok()
            }
            u5c::metadatum::Metadatum::Int(int) => Some(int.to_string()),
            _ => None,
        }
    } else {
        None
    }
}

/// Convert UTxORPC Metadatum to JSON Value
fn metadatum_to_json_value(metadatum: &u5c::Metadatum) -> Result<Value, serde_json::Error> {
    if let Some(metadatum_kind) = &metadatum.metadatum {
        match metadatum_kind {
            u5c::metadatum::Metadatum::Map(map) => {
                let mut json_map = serde_json::Map::new();
                
                for pair in &map.pairs {
                    if let (Some(key_metadatum), Some(value_metadatum)) = (&pair.key, &pair.value) {
                        if let (Some(key_str), Ok(value_json)) = (metadatum_to_string(key_metadatum), metadatum_to_json_value(value_metadatum)) {
                            json_map.insert(key_str, value_json);
                        }
                    }
                }
                
                Ok(Value::Object(json_map))
            }
            u5c::metadatum::Metadatum::Array(array) => {
                let mut json_array = Vec::new();
                
                for item_metadatum in &array.items {
                    if let Ok(json_value) = metadatum_to_json_value(item_metadatum) {
                        json_array.push(json_value);
                    }
                }
                
                Ok(Value::Array(json_array))
            }
            u5c::metadatum::Metadatum::Text(text) => {
                Ok(Value::String(text.clone()))
            }
            u5c::metadatum::Metadatum::Bytes(bytes) => {
                // Try to decode as UTF-8 string first, fallback to hex
                if let Ok(string) = String::from_utf8(bytes.to_vec()) {
                    Ok(Value::String(string))
                } else {
                    Ok(Value::String(hex::encode(bytes)))
                }
            }
            u5c::metadatum::Metadatum::Int(int) => {
                Ok(Value::Number(serde_json::Number::from(*int)))
            }
        }
    } else {
        Ok(Value::Null)
    }
}

/// Extract metadata using cardano-assets JSON deserializer
fn extract_metadata_via_json(
    json_metadata: &Value,
    policy_id: &str,
    asset_name_hex: &str,
) -> Option<AssetMetadata> {
    // Navigate the CIP-25 structure: { policy_id: { asset_name: metadata } }
    if let Some(policy_map) = json_metadata.get(policy_id).and_then(|v| v.as_object()) {
        // Try to find asset by decoded name first
        let asset_name_decoded = match hex::decode(asset_name_hex) {
            Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| asset_name_hex.to_string()),
            Err(_) => asset_name_hex.to_string(),
        };
        
        // Look for the asset metadata 
        let asset_json = policy_map.get(&asset_name_decoded)
            .or_else(|| policy_map.get(asset_name_hex))?;
        
        // Try to deserialize using cardano-assets AssetMetadata
        if let Ok(cardano_metadata) = serde_json::from_value::<CardanoAssetMetadata>(asset_json.clone()) {
            debug!("Successfully deserialized metadata using cardano-assets");
            
            // Convert to our AssetMetadata format
            let asset = Asset::from(cardano_metadata);
            return Some(AssetMetadata {
                image: asset.image,
                traits: asset.traits,
            });
        } else {
            debug!("Failed to deserialize with cardano-assets, metadata may be in unsupported format");
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_metadatum_to_json_value() {
        // Test text conversion
        let text_metadatum = u5c::Metadatum {
            metadatum: Some(u5c::metadatum::Metadatum::Text("hello".to_string())),
        };
        
        let result = metadatum_to_json_value(&text_metadatum).unwrap();
        assert_eq!(result, json!("hello"));
    }
    
    #[test]
    fn test_metadatum_to_string() {
        // Test text conversion
        let text_metadatum = u5c::Metadatum {
            metadatum: Some(u5c::metadatum::Metadatum::Text("test".to_string())),
        };
        
        assert_eq!(metadatum_to_string(&text_metadatum), Some("test".to_string()));
        
        // Test integer conversion
        let int_metadatum = u5c::Metadatum {
            metadatum: Some(u5c::metadatum::Metadatum::Int(42)),
        };
        
        assert_eq!(metadatum_to_string(&int_metadatum), Some("42".to_string()));
    }
}