//! PFP City API integration for fetching NFT assets.
//!
//! This module provides functions to fetch random assets from the PFP City API
//! for use in the memory game.

use crate::types::{Card, CardId};
pub use cardano_assets::AssetId;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::Deserialize;
use worker::*;

const API_BASE_URL: &str = "https://a2.pfp.city";

/// Asset details from PFP City API (V3)
#[derive(Deserialize, Debug, Clone)]
pub struct AssetDetails {
    /// Asset ID object
    pub id: AssetId,
    /// Human-readable name
    pub name: String,
    /// Image URL (IPFS or HTTP)
    #[serde(default)]
    pub image: Option<String>,
}

/// Pagination info from API
#[derive(Deserialize, Debug, Clone)]
pub struct PaginationInfo {
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
    pub total_available: Option<u32>,
}

/// Inner data from API response
#[derive(Deserialize, Debug, Clone)]
pub struct CollectionAssetsData {
    pub assets: Vec<AssetDetails>,
    pub pagination: PaginationInfo,
}

/// Response from /v3/api/collections/{policy_id}/assets endpoint
#[derive(Deserialize, Debug, Clone)]
pub struct CollectionAssetsResponse {
    pub success: bool,
    pub data: CollectionAssetsData,
}

/// Fetch a batch of assets from a collection at a specific offset
pub async fn fetch_assets_batch(
    policy_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<AssetDetails>> {
    let url = format!(
        "{API_BASE_URL}/v3/api/collections/{policy_id}/assets?limit={limit}&offset={offset}"
    );

    tracing::debug!("Fetching assets from: {}", url);

    let mut response = Fetch::Url(Url::parse(&url)?).send().await?;

    let status = response.status_code();
    tracing::debug!("Response status: {}", status);

    if !(200..300).contains(&status) {
        return Err(Error::from(format!("PFP City API error: {status}")));
    }

    let text = response.text().await?;
    tracing::debug!(
        "Response body (first 500 chars): {}",
        &text[..text.len().min(500)]
    );

    let resp: CollectionAssetsResponse =
        serde_json::from_str(&text).map_err(|e| Error::from(format!("Serde Error: {e}")))?;

    tracing::debug!("Parsed {} assets", resp.data.assets.len());
    Ok(resp.data.assets)
}

/// Generate image URL for an asset
pub fn image_url(policy_id: &str, asset_id: &str) -> String {
    // Use thumbnail size (400px) for card display
    format!("https://img.pfp.city/{policy_id}/img/thumb/{asset_id}.png")
}

/// Fetch assets and create card pairs for the memory game.
///
/// # Arguments
/// * `policy_id` - The Cardano policy ID for the NFT collection
/// * `pair_count` - Number of pairs needed (e.g., 32 for an 8x8 grid)
/// * `seed` - Random seed for deterministic shuffling
///
/// # Returns
/// A shuffled vector of cards (2 * pair_count cards total)
pub async fn fetch_game_cards(policy_id: &str, pair_count: u8, seed: u64) -> Result<Vec<Card>> {
    // Create RNG from seed for shuffling
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);

    // Fetch enough assets from the start of the collection
    // We request more than needed in case of duplicates
    let limit = (pair_count as u32).max(50);
    let mut all_assets = fetch_assets_batch(policy_id, limit, 0).await?;

    if all_assets.len() < pair_count as usize {
        return Err(Error::from(format!(
            "Could only fetch {} unique assets, need {}",
            all_assets.len(),
            pair_count
        )));
    }

    // Truncate to exact pair count
    all_assets.truncate(pair_count as usize);

    // Create card pairs - each asset appears twice
    let mut cards: Vec<Card> = Vec::with_capacity(pair_count as usize * 2);

    for (pair_id, asset) in all_assets.into_iter().enumerate() {
        let concatenated_id = asset.id.concatenated();

        // Use image from API if available, otherwise generate from asset ID
        let img_url = asset
            .image
            .clone()
            .unwrap_or_else(|| image_url(policy_id, &concatenated_id));

        // Create two cards with unique IDs for each pair
        for _ in 0..2 {
            cards.push(Card {
                card_id: CardId::new(),
                asset_id: concatenated_id.clone(),
                name: asset.name.clone(),
                image_url: img_url.clone(),
                matched: false,
                matched_by: None,
                pair_id: pair_id as u8,
            });
        }
    }

    // Shuffle the cards deterministically using the seed
    cards.shuffle(&mut rng);

    Ok(cards)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_url() {
        let url = image_url(
            "5ac825392b7608d6e92a4e5c528fe9b8fadd6eaa3e36a685e37175d1",
            "abc123",
        );
        assert_eq!(
            url,
            "https://img.pfp.city/5ac825392b7608d6e92a4e5c528fe9b8fadd6eaa3e36a685e37175d1/img/thumb/abc123.png"
        );
    }
}
