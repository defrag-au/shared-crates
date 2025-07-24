mod error;
mod test;

pub use error::*;

use http_client::HttpClient;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;

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
    #[serde(
        default,
        alias = "Trait Count",
        alias = "traitCount",
        deserialize_with = "deserialize_u32_string"
    )]
    pub trait_count: u32,
    #[serde(alias = "encodedName")]
    pub encoded_name: String,
    #[serde(alias = "buildType")]
    pub build_type: Option<String>,
    #[serde(alias = "rarityRank", deserialize_with = "deserialize_u32_string")]
    pub rarity_rank: u32,
    #[serde(alias = "ownerStakeKey")]
    pub owner_stake_key: String,

    #[serde(flatten, deserialize_with = "deserialize_traits")]
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
    client: HttpClient,
}

impl Default for CnftApi {
    fn default() -> Self {
        Self {
            client: HttpClient::new()
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json"),
        }
    }
}

impl CnftApi {
    pub fn extract_rarity(asset: &CnftAsset) -> AssetRarity {
        AssetRarity(asset.encoded_name.clone(), asset.rarity_rank)
    }

    pub async fn get_for_policy(&self, policy_id: &str) -> Result<Vec<CnftAsset>, CnftError> {
        let url = format!("https://{}/{}", BASE_URL, policy_id);
        tracing::info!("[cnft-tools] requesting {}", url);
        self.client.get(&url).await.map_err(CnftError::Request)
    }
}

pub(crate) fn deserialize_u32_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<u32>().map_err(serde::de::Error::custom)
}

fn deserialize_traits<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TraitsVisitor;

    impl<'de> Visitor<'de> for TraitsVisitor {
        type Value = HashMap<String, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map of trait key-value pairs")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map = HashMap::new();

            while let Some((key, value)) = access.next_entry::<String, String>()? {
                if value != "None" {
                    map.insert(key, value);
                }
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(TraitsVisitor)
}
