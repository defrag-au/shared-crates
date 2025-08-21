use std::collections::HashMap;

use cardano_assets::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionAssetsRequest {
    #[serde(rename = "policyId")]
    pub policy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(rename = "saleType", skip_serializing_if = "Option::is_none")]
    pub sale_type: Option<SaleType>,
    #[serde(rename = "orderBy", skip_serializing_if = "Option::is_none")]
    pub order_by: Option<OrderBy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaleType {
    All,
    ListedOnly,
    Bundles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderBy {
    PriceAsc,
    PriceDesc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionAssetsResponse {
    pub page_state: Option<PageState>,
    pub count: u32,
    pub results: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageState {
    pub page_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub unit: AssetId,
    pub policy_id: String,
    pub owner_stake_keyhash: Option<String>,
    #[serde(default)]
    pub is_script: bool,
    pub quantity: u32,
    pub name: String,
    pub name_idx: Option<u32>,
    pub image: Option<String>,
    pub media: Option<AssetMedia>,
    pub label: Option<String>,
    #[serde(default)]
    pub version: AssetVersion,
    pub last_update_tx_hash: String,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    pub listing: Option<Listing>,
    pub collection: Option<Collection>,
    pub rarity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AssetVersion {
    #[default]
    Cip25,
    Cip68,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMedia {
    src: String,
    blur: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub price: u64,
    pub tx_hash_index: String,
    pub script_hash: String,
    pub bundle_size: Option<u32>,
    pub is_processing: bool,
    #[serde(alias = "type")]
    pub marketplace: Marketplace,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub name: String,
    pub logo: Option<String>,
    pub policy_id: String,
    pub description: Option<String>,
    pub supply: Option<u32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum Marketplace {
    JpgStore,
    Wayup,
    Unknown(String),
}

impl Serialize for Marketplace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Marketplace::JpgStore => "jpg.store",
            Marketplace::Wayup => "wayup",
            Marketplace::Unknown(name) => name,
        };
        serializer.serialize_str(s)
    }
}

impl<'de> serde::Deserialize<'de> for Marketplace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.to_lowercase().as_str() {
            "jpg.store" | "jpgstore" => Marketplace::JpgStore,
            "wayup" => Marketplace::Wayup,
            _ => Marketplace::Unknown(s),
        })
    }
}

impl std::fmt::Display for Marketplace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Marketplace::JpgStore => write!(f, "jpg.store"),
            Marketplace::Wayup => write!(f, "wayup"),
            Marketplace::Unknown(name) => write!(f, "{}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_deserialization() {
        // Known marketplaces
        let jpg_store: Marketplace = serde_json::from_str("\"jpg.store\"").unwrap();
        assert!(matches!(jpg_store, Marketplace::JpgStore));

        let jpg_store_alt: Marketplace = serde_json::from_str("\"jpgstore\"").unwrap();
        assert!(matches!(jpg_store_alt, Marketplace::JpgStore));

        let wayup: Marketplace = serde_json::from_str("\"wayup\"").unwrap();
        assert!(matches!(wayup, Marketplace::Wayup));

        // Unknown marketplace
        let unknown: Marketplace = serde_json::from_str("\"foo\"").unwrap();
        match unknown {
            Marketplace::Unknown(name) => assert_eq!(name, "foo"),
            _ => panic!("Expected Unknown variant"),
        }

        // Case insensitive for known marketplaces
        let jpg_upper: Marketplace = serde_json::from_str("\"JPG.STORE\"").unwrap();
        assert!(matches!(jpg_upper, Marketplace::JpgStore));
    }

    #[test]
    fn test_marketplace_serialization() {
        let jpg_store = Marketplace::JpgStore;
        let serialized = serde_json::to_string(&jpg_store).unwrap();
        assert_eq!(serialized, "\"jpg.store\"");

        let wayup = Marketplace::Wayup;
        let serialized = serde_json::to_string(&wayup).unwrap();
        assert_eq!(serialized, "\"wayup\"");

        let unknown = Marketplace::Unknown("foo".to_string());
        let serialized = serde_json::to_string(&unknown).unwrap();
        assert_eq!(serialized, "\"foo\"");
    }

    #[test]
    fn test_marketplace_roundtrip() {
        // Test that unknown marketplaces roundtrip correctly
        let original = Marketplace::Unknown("someNewMarketplace".to_string());
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Marketplace = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            Marketplace::Unknown(name) => assert_eq!(name, "someNewMarketplace"),
            _ => panic!("Expected Unknown variant"),
        }
    }

    #[test]
    fn test_marketplace_display() {
        assert_eq!(Marketplace::JpgStore.to_string(), "jpg.store");
        assert_eq!(Marketplace::Wayup.to_string(), "wayup");
        assert_eq!(Marketplace::Unknown("foo".to_string()).to_string(), "foo");
    }
}
