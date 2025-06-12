mod error;
mod test;

pub use error::*;

use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::{Client, Response};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

const BASE_URL: &str = "api.cnft.tools/api/external";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CnftAsset {
    #[serde(alias = "onSale")]
    pub on_sale: Option<bool>,
    #[serde(alias = "assetName")]
    pub asset_name: Option<String>,
    #[serde(alias = "assetID")]
    pub asset_id: String,
    pub name: String,
    #[serde(alias = "iconurl")]
    pub icon_url: Option<String>,
    #[serde(alias = "Trait Count", deserialize_with = "deserialize_u32_string")]
    pub trait_count: u32,
    #[serde(alias = "encodedName")]
    pub encoded_name: String,
    #[serde(alias = "buildType")]
    pub build_type: String,
    #[serde(alias = "rarityRank", deserialize_with = "deserialize_u32_string")]
    pub rarity_rank: u32,
    #[serde(alias = "ownerStakeKey")]
    pub owner_stake_key: String,

    #[serde(flatten)]
    pub traits: HashMap<String, String>,
}

impl PartialEq for CnftAsset {
    fn eq(&self, other: &Self) -> bool {
        self.encoded_name == other.encoded_name
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AssetRarity(pub String, pub u32);

pub struct CnftApi {
    client: Client,
}

impl Default for CnftApi {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl CnftApi {
    pub fn for_env(_: &worker::Env) -> worker::Result<Self> {
        Ok(Self {
            client: Client::new(),
        })
    }

    pub fn extract_rarity(asset: &CnftAsset) -> AssetRarity {
        AssetRarity(asset.encoded_name.clone(), asset.rarity_rank)
    }

    pub async fn get_for_policy(&self, policy_id: &str) -> Result<Vec<CnftAsset>, CnftError> {
        self.get_url(&format!("/{}", policy_id))
            .await?
            .json::<Vec<CnftAsset>>()
            .await
            .map_err(CnftError::Request)
    }

    async fn get_url(&self, path: &str) -> reqwest::Result<Response> {
        self.client
            .get(format!("https://{}{}", BASE_URL, path))
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?
            .error_for_status()
    }
}

pub(crate) fn deserialize_u32_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<u32>().map_err(serde::de::Error::custom)
}
