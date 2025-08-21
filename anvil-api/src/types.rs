use std::collections::HashMap;

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
    pub unit: String,
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
    pub marketplace: String,
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
