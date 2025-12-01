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
        alias = "Trait Count",
        alias = "traitCount",
        deserialize_with = "deserialize_optional_u32_or_string",
        default
    )]
    pub trait_count: Option<u32>,
    #[serde(alias = "encodedName")]
    pub encoded_name: String,
    #[serde(alias = "buildType")]
    pub build_type: Option<String>,
    #[serde(alias = "rarityRank", deserialize_with = "deserialize_u32_or_string")]
    pub rarity_rank: u32,
    #[serde(alias = "ownerStakeKey")]
    pub owner_stake_key: String,

    #[serde(flatten, deserialize_with = "deserialize_traits")]
    pub traits: HashMap<String, Vec<String>>,
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
        let url = format!("https://{BASE_URL}/{policy_id}");
        tracing::info!("[cnft-tools] requesting {}", url);
        self.client.get(&url).await.map_err(CnftError::Request)
    }
}

/// Deserialize u32 from either a string "123" or integer 123
/// This allows round-tripping through JSON serialization
pub(crate) fn deserialize_u32_or_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(u32),
    }

    match StringOrInt::deserialize(deserializer)? {
        StringOrInt::String(s) => s.parse::<u32>().map_err(serde::de::Error::custom),
        StringOrInt::Int(n) => Ok(n),
    }
}

/// Deserialize Option<u32> from either a string "123" or integer 123
pub(crate) fn deserialize_optional_u32_or_string<'de, D>(
    deserializer: D,
) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(u32),
    }

    let opt: Option<StringOrInt> = Option::deserialize(deserializer)?;
    match opt {
        Some(StringOrInt::String(s)) => s
            .parse::<u32>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(StringOrInt::Int(n)) => Ok(Some(n)),
        None => Ok(None),
    }
}

fn deserialize_traits<'de, D>(deserializer: D) -> Result<HashMap<String, Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TraitsVisitor;

    impl<'de> Visitor<'de> for TraitsVisitor {
        type Value = HashMap<String, Vec<String>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map of trait key-value pairs (strings or arrays)")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map = HashMap::new();

            while let Some(key) = access.next_key::<String>()? {
                #[derive(Deserialize)]
                #[serde(untagged)]
                enum TraitValue {
                    Single(String),
                    Multi(Vec<String>),
                }

                let value = access.next_value::<TraitValue>()?;

                let values = match value {
                    TraitValue::Single(s) if s != "None" => vec![s],
                    TraitValue::Multi(v) => v.into_iter().filter(|s| s != "None").collect(),
                    _ => continue,
                };

                if !values.is_empty() {
                    map.insert(key, values);
                }
            }

            Ok(map)
        }
    }

    deserializer.deserialize_map(TraitsVisitor)
}
