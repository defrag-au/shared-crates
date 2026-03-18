//! UTxO types for Cardano transactions
//!
//! Provides common UTxO representations that can be used across different indexers
//! (Maestro, Ogmios, etc.) and converted to/from their native formats.

use crate::AssetId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Properties detected on a UTxO during decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum UtxoTag {
    /// Carries a datum (hash or inline). Typically script-locked.
    HasDatum,
    /// Carries a script reference.
    HasScriptRef,
    /// Sits at a script payment address (e.g. marketplace franken address).
    ScriptAddress,
}

/// API-friendly UTxO representation with assets as a vec
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UtxoApi {
    pub tx_hash: String,
    pub output_index: u32,
    pub lovelace: u64,
    pub assets: Vec<AssetQuantity>,
    /// Tags describing properties of this UTxO (datum, script ref, script address, etc.)
    #[serde(default)]
    pub tags: Vec<UtxoTag>,
}

impl UtxoApi {
    /// Check if this UTxO has a specific tag.
    pub fn has_tag(&self, tag: UtxoTag) -> bool {
        self.tags.contains(&tag)
    }
}

/// Asset with quantity for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssetQuantity {
    pub asset_id: AssetId,
    pub quantity: u64,
}

/// Internal UTxO representation using HashMap for efficient lookups
/// This is used by indexers internally before converting to UtxoApi
#[derive(Debug, Clone)]
pub struct Utxo {
    pub tx_hash: String,
    pub output_index: u64,
    pub lovelace: u64,
    pub assets: HashMap<AssetId, u64>,
}

impl From<Utxo> for UtxoApi {
    fn from(utxo: Utxo) -> Self {
        Self {
            tx_hash: utxo.tx_hash,
            output_index: utxo.output_index as u32,
            lovelace: utxo.lovelace,
            assets: utxo
                .assets
                .into_iter()
                .map(|(asset_id, quantity)| AssetQuantity { asset_id, quantity })
                .collect(),
            tags: vec![],
        }
    }
}
