use serde::{Deserialize, Serialize};

mod constants;
mod insights;

pub use cardano_assets::AssetId;
pub use constants::*;

/// Asset with pricing information for marketplace operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PricedAsset {
    pub asset: AssetId,
    /// The listing/offer price for this asset in lovelace
    pub price_lovelace: Option<u64>,
    /// Price change for updates (positive = increase, negative = decrease)
    pub delta_lovelace: Option<i64>,
}

impl PricedAsset {
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            price_lovelace: None,
            delta_lovelace: None,
        }
    }

    pub fn with_price(asset: AssetId, price_lovelace: u64) -> Self {
        Self {
            asset,
            price_lovelace: Some(price_lovelace),
            delta_lovelace: None,
        }
    }

    pub fn with_price_delta(asset: AssetId, price_lovelace: u64, delta_lovelace: i64) -> Self {
        Self {
            asset,
            price_lovelace: Some(price_lovelace),
            delta_lovelace: Some(delta_lovelace),
        }
    }
}

impl std::fmt::Display for PricedAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.asset.asset_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationPayload {
    Lovelace {
        amount: u64,
    },
    NativeToken {
        policy_id: String,
        encoded_name: String,
        amount: u64,
    },
}

impl OperationPayload {
    pub fn get_asset(&self) -> Option<AssetId> {
        match self {
            Self::Lovelace { .. } => None,
            Self::NativeToken {
                policy_id,
                encoded_name,
                ..
            } => AssetId::new(policy_id.clone(), encoded_name.clone()).ok(),
        }
    }
}

/// Parse asset ID into policy_id and asset_name
pub fn parse_asset_id(asset_id: &str) -> (String, String) {
    use crate::constants::cardano::POLICY_ID_LENGTH;

    if asset_id.len() >= POLICY_ID_LENGTH {
        let policy_id = asset_id[..POLICY_ID_LENGTH].to_string();
        let asset_name = asset_id[POLICY_ID_LENGTH..].to_string();
        (policy_id, asset_name)
    } else {
        // If less than expected length, treat entire string as policy_id (no asset name)
        (asset_id.to_string(), String::new())
    }
}
