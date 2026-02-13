//! PFP City API client for WASM
//!
//! Provides unauthenticated access to PFP City collection and asset APIs.
//!
//! ## Endpoints Used
//!
//! - `GET /v3/api/collections/{policy_id}/assets` - List collection assets (paginated)
//! - `GET /v3/api/assets/{policy_id}:{asset_name_hex}` - Get single asset details with traits
//!
//! ## Usage
//!
//! ```ignore
//! use crate::api::pfp_city::{fetch_collection_assets, fetch_asset_details};
//!
//! // Fetch collection listing
//! let assets = fetch_collection_assets(policy_id, 12, 0).await?;
//!
//! // Fetch single asset with traits
//! let details = fetch_asset_details(policy_id, asset_name_hex).await?;
//! ```

pub use cardano_assets::AssetId;
use gloo_net::http::Request;
use serde::Deserialize;
use std::collections::HashMap;

const API_BASE_URL: &str = "https://a2.pfp.city";

// ============================================================================
// Collection Assets API
// ============================================================================

/// Asset from collection listing (minimal info)
#[derive(Deserialize, Debug, Clone)]
pub struct CollectionAsset {
    pub id: AssetId,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
}

/// Pagination info from collection API
#[derive(Deserialize, Debug, Clone)]
pub struct PaginationInfo {
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
    #[serde(default)]
    pub total_available: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
struct CollectionAssetsData {
    pub assets: Vec<CollectionAsset>,
    pub pagination: PaginationInfo,
}

#[derive(Deserialize, Debug, Clone)]
struct CollectionAssetsResponse {
    pub success: bool,
    pub data: CollectionAssetsData,
}

/// Fetch assets from a collection with pagination
///
/// # Arguments
/// * `policy_id` - Cardano policy ID
/// * `limit` - Number of assets to fetch (max ~100)
/// * `offset` - Pagination offset
///
/// # Returns
/// Tuple of (assets, pagination_info)
pub async fn fetch_collection_assets(
    policy_id: &str,
    limit: u32,
    offset: u32,
) -> Result<(Vec<CollectionAsset>, PaginationInfo), String> {
    let url = format!(
        "{API_BASE_URL}/v3/api/collections/{policy_id}/assets?limit={limit}&offset={offset}"
    );

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: CollectionAssetsResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    if !data.success {
        return Err("API returned success: false".to_string());
    }

    Ok((data.data.assets, data.data.pagination))
}

// ============================================================================
// Asset Details API
// ============================================================================

/// Full asset details including traits
#[derive(Deserialize, Debug, Clone)]
pub struct AssetDetails {
    pub id: AssetId,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub traits: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub rarity_rank: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct AssetDetailsResponse {
    pub success: bool,
    pub data: AssetDetails,
}

/// Fetch single asset details with traits
///
/// # Arguments
/// * `policy_id` - Cardano policy ID
/// * `asset_name_hex` - Asset name in hex format
///
/// # Returns
/// Full asset details including traits and rarity
pub async fn fetch_asset_details(
    policy_id: &str,
    asset_name_hex: &str,
) -> Result<AssetDetails, String> {
    let url = format!("{API_BASE_URL}/v3/api/assets/{policy_id}:{asset_name_hex}");

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !response.ok() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: AssetDetailsResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    if !data.success {
        return Err("API returned success: false".to_string());
    }

    Ok(data.data)
}

// ============================================================================
// Known Collections
// ============================================================================

/// Known NFT collections for demos
pub const KNOWN_COLLECTIONS: &[(&str, &str)] = &[
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "Black Flag Pirates",
    ),
    (
        "8972aab912aed2cf44b65916e206324c6bdcb6fbd3dc4eb634fdbd28",
        "Ug",
    ),
    (
        "e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0",
        "Perps",
    ),
];
