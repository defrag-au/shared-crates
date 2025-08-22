use cnft_tools::CnftAsset;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

pub mod asset_id;
pub mod collection;
pub mod traits;

pub use asset_id::*;
pub use collection::*;
pub use traits::*;
pub type AssetTraits = HashMap<String, String>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlatAsset(pub AssetId, pub String, pub AssetTraits, pub Option<u32>);

impl FlatAsset {
    #[must_use]
    pub fn id(&self) -> &AssetId {
        &self.0
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.1
    }

    #[must_use]
    pub fn traits(&self) -> &AssetTraits {
        &self.2
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum PrimitiveOrList<T> {
    Primitive(T),
    List(Vec<T>),
}

impl From<PrimitiveOrList<String>> for String {
    fn from(value: PrimitiveOrList<String>) -> Self {
        match value {
            PrimitiveOrList::Primitive(val) => val,
            PrimitiveOrList::List(items) => {
                let default = String::new();
                let result = items.first().unwrap_or(&default);
                result.clone()
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetFile {
    media_type: String,
    name: Option<String>,
    src: PrimitiveOrList<String>,
}

impl AssetFile {
    /// Get the source URL, handling both string and array formats
    pub fn get_src(&self) -> String {
        match &self.src {
            PrimitiveOrList::Primitive(url) => url.clone(),
            PrimitiveOrList::List(urls) => {
                // For arrays like ["ipfs://hash", "oa"], concatenate them
                urls.join("")
            }
        }
    }

    /// Get the media type
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Get the name if present
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum AssetMetadata {
    // known projects:
    // - gophers
    CodifiedTraits {
        name: String,
        image: PrimitiveOrList<String>,
        #[serde(alias = "mediaType", default = "default_media_type")]
        media_type: String,
        collection: Option<String>,
        #[serde(alias = "Discord")]
        discord: Option<String>,
        #[serde(alias = "Twitter")]
        twitter: Option<String>,
        #[serde(alias = "Website")]
        website: Option<String>,
        traits: Vec<CodifiedTrait>,
    }, // known projects
    // - snekkies
    Attributed {
        name: String,
        description: Option<PrimitiveOrList<String>>,
        image: PrimitiveOrList<String>,
        #[serde(alias = "mediaType", default = "default_media_type")]
        media_type: String,
        project: Option<String>,
        files: Option<Vec<AssetFile>>,

        #[serde(alias = "Discord")]
        discord: Option<String>,
        #[serde(alias = "Twitter")]
        twitter: Option<String>,
        #[serde(alias = "Website")]
        website: Option<String>,
        minter: Option<String>,

        #[serde(alias = "attributes")]
        traits: Traits,

        #[serde(flatten)]
        extra: HashMap<String, String>,
    },
    // known projects
    // - black flag, aquafarmers
    Flattened {
        name: String,
        image: PrimitiveOrList<String>,
        #[serde(alias = "mediaType", default = "default_media_type")]
        media_type: String,
        #[serde(alias = "Project", alias = " Project")]
        project: Option<PrimitiveOrList<String>>,
        description: Option<PrimitiveOrList<String>>,
        files: Option<Vec<AssetFile>>,

        // note - panda society use this format
        publisher: Option<Vec<String>>,
        #[serde(alias = "Discord")]
        discord: Option<PrimitiveOrList<String>>,
        #[serde(alias = "Twitter")]
        twitter: Option<PrimitiveOrList<String>>,
        #[serde(alias = "Website")]
        website: Option<PrimitiveOrList<String>>,
        #[serde(alias = "Github")]
        github: Option<PrimitiveOrList<String>>,
        #[serde(alias = "Medium")]
        medium: Option<PrimitiveOrList<String>>,

        #[serde(flatten)]
        traits: Traits,
    },
    // known projects:
    // - mallard order
    AttributeArray {
        name: String,
        image: PrimitiveOrList<String>,
        #[serde(alias = "mediaType", default = "default_media_type")]
        media_type: String,
        project: Option<String>,
        #[serde(alias = "Discord")]
        discord: Option<String>,
        #[serde(alias = "Twitter")]
        twitter: Option<String>,
        #[serde(alias = "Website")]
        website: Option<String>,

        #[serde(alias = "Attributes", alias = "traits")]
        traits: Vec<Traits>,
    },
    // known projects:
    // - jellycubes
    FlattenedMixed {
        name: String,
        image: PrimitiveOrList<String>,
        #[serde(alias = "mediaType", default = "default_media_type")]
        media_type: String,
        #[serde(alias = "Project", alias = "project")]
        project: Option<String>,
        description: Option<PrimitiveOrList<String>>,
        files: Option<Vec<AssetFile>>,

        // Social links
        #[serde(alias = "Discord")]
        discord: Option<String>,
        #[serde(alias = "Twitter")]
        twitter: Option<String>,
        #[serde(alias = "Website")]
        website: Option<String>,

        // Capture all remaining fields as Value, then convert to strings
        #[serde(flatten)]
        raw_traits: HashMap<String, serde_json::Value>,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct CodifiedTrait {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub display: CodifiedTraitDisplay,
}

impl CodifiedTrait {
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            display: CodifiedTraitDisplay::default(),
        }
    }

    pub fn new_with_display(name: &str, value: &str, display: CodifiedTraitDisplay) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            display,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CodifiedTraitDisplay {
    #[default]
    String,
    Number,
    Range,
    Multiply,
    Data,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AssetMetadata68 {
    pub purpose: NftPurpose,
    pub version: u32,
    pub metadata: AssetMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NftPurpose {
    UserNft,
    ReferenceNft,
    Unknown,
}

impl From<&str> for NftPurpose {
    fn from(value: &str) -> Self {
        if value.len() >= 8 {
            match &value[..8] {
                "000643b0" => Self::ReferenceNft,
                "000de140" => Self::UserNft,
                _ => Self::Unknown,
            }
        } else {
            // Fallback: check if the entire string matches a prefix
            match value {
                "000643b0" => Self::ReferenceNft,
                "000de140" => Self::UserNft,
                _ => Self::Unknown,
            }
        }
    }
}

impl NftPurpose {
    #[must_use]
    pub const fn as_hex(&self) -> &'static str {
        match self {
            Self::ReferenceNft => "000643b0",
            Self::UserNft => "000de140",
            Self::Unknown => "",
        }
    }
}

impl std::fmt::Display for NftPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_hex())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct YeppleEmbeddedMetadata {
    name: String,
    image: String,
    #[serde(alias = "mediaType")]
    media_type: String,
    #[serde(flatten)]
    traits: Traits,
}

fn default_media_type() -> String {
    "image/png".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiAsset(String, String, Traits, Option<u32>);

impl ApiAsset {
    #[must_use]
    pub fn get_id(&self) -> String {
        self.0.clone()
    }

    #[must_use]
    pub fn get_name(&self) -> String {
        self.1.clone()
    }

    #[must_use]
    pub fn get_traits(&self) -> Traits {
        self.2.clone()
    }

    #[must_use]
    pub fn get_rarity(&self) -> Option<u32> {
        self.3
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssetImageSource {
    Ipfs,
    Https,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataKind {
    Cip25,
    Cip68(NftPurpose),
    Unknown,
}

impl Serialize for MetadataKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match self {
            MetadataKind::Cip25 => "cip25",
            MetadataKind::Cip68(NftPurpose::UserNft) => "cip68-user",
            MetadataKind::Cip68(NftPurpose::ReferenceNft) => "cip68-reference",
            MetadataKind::Cip68(NftPurpose::Unknown) => "cip68-unknown",
            MetadataKind::Unknown => "unknown",
        };
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for MetadataKind {
    fn deserialize<D>(deserializer: D) -> Result<MetadataKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let kind = match s.as_str() {
            "cip25" => MetadataKind::Cip25,
            "cip68-user" => MetadataKind::Cip68(NftPurpose::UserNft),
            "cip68-reference" => MetadataKind::Cip68(NftPurpose::ReferenceNft),
            "cip68-unknown" => MetadataKind::Cip68(NftPurpose::Unknown),
            "unknown" => MetadataKind::Unknown,
            _ => MetadataKind::Unknown, // fallback here
        };
        Ok(kind)
    }
}

impl From<MetadataKind> for String {
    fn from(value: MetadataKind) -> Self {
        serde_json::to_string(&value)
            .expect("Serialization failed")
            .trim_matches('"') // remove the surrounding quotes from JSON string
            .to_string()
    }
}

impl MetadataKind {
    #[must_use]
    pub fn guess_id_kind(id: &str) -> Self {
        if id.len() < 8 {
            return Self::Cip25;
        }

        match NftPurpose::from(&id[..8]) {
            NftPurpose::Unknown => Self::Cip25,
            purpose => Self::Cip68(purpose),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AssetRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl AssetRarity {
    #[must_use]
    pub fn from_rank(rank: u32, max_rank: u32) -> Self {
        match rank as f32 / max_rank as f32 {
            x if x <= 0.05 => Self::Legendary,
            x if x <= 0.25 => Self::Epic,
            x if x <= 0.50 => Self::Rare,
            x if x <= 0.75 => Self::Uncommon,
            _ => Self::Common,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssetTag {
    Rarity(AssetRarity),
    OnSale,
}

impl fmt::Display for AssetTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetTag::OnSale => write!(f, "on_sale"),
            AssetTag::Rarity(rarity) => write!(f, "{}", serde_plain::to_string(rarity).unwrap()),
        }
    }
}

impl FromStr for AssetTag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "on_sale" {
            Ok(AssetTag::OnSale)
        } else {
            serde_plain::from_str::<AssetRarity>(s)
                .map(AssetTag::Rarity)
                .map_err(|_| format!("Unknown tag: {s}"))
        }
    }
}

impl Serialize for AssetTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AssetTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub image: String,
    pub traits: Traits,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity_rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<AssetTag>,
}

/// Asset with explicit ID - enhanced version for marketplace and API operations
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AssetV2 {
    /// Unique asset identifier (policy_id + asset_name_hex)
    pub id: AssetId,
    /// Asset name (human readable)
    pub name: String,
    /// Asset image URL
    pub image: String,
    /// Asset traits/attributes
    pub traits: Traits,
    /// Rarity rank if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity_rank: Option<u32>,
    /// Asset tags (rarity, on_sale, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<AssetTag>,
}

impl From<AssetV2> for Asset {
    fn from(asset_v2: AssetV2) -> Self {
        Self {
            name: asset_v2.name,
            image: asset_v2.image,
            traits: asset_v2.traits,
            rarity_rank: asset_v2.rarity_rank,
            tags: asset_v2.tags,
        }
    }
}

impl AssetV2 {
    /// Create a new AssetV2 with the given ID and asset data
    pub fn with_id(asset: Asset, id: AssetId) -> Self {
        Self {
            id,
            name: asset.name,
            image: asset.image,
            traits: asset.traits,
            rarity_rank: asset.rarity_rank,
            tags: asset.tags,
        }
    }

    /// Create a new AssetV2 with all fields
    pub fn new(
        id: AssetId,
        name: String,
        image: String,
        traits: crate::Traits,
        rarity_rank: Option<u32>,
        tags: Vec<crate::AssetTag>,
    ) -> Self {
        Self {
            id,
            name,
            image,
            traits,
            rarity_rank,
            tags,
        }
    }
}

impl From<AssetMetadata> for Asset {
    fn from(value: AssetMetadata) -> Self {
        match value {
            AssetMetadata::Attributed {
                name,
                image,
                traits,
                ..
            }
            | AssetMetadata::Flattened {
                name,
                image,
                traits,
                ..
            } => Self {
                name,
                image: image.into(),
                traits,
                rarity_rank: None,
                tags: vec![],
            },
            AssetMetadata::AttributeArray {
                name,
                image,
                traits: trait_vector,
                ..
            } => {
                let mut traits: Traits = Traits::new();
                for item in trait_vector {
                    for (key, value) in item {
                        traits.insert_vec(key, value);
                    }
                }

                Self {
                    name,
                    image: image.into(),
                    traits,
                    rarity_rank: None,
                    tags: vec![],
                }
            }
            AssetMetadata::CodifiedTraits {
                name,
                image,
                traits: trait_vector,
                ..
            } => {
                let mut traits: Traits = Traits::new();
                for item in trait_vector {
                    traits.insert_single(item.name, item.value);
                }

                Self {
                    name,
                    image: image.into(),
                    traits,
                    rarity_rank: None,
                    tags: vec![],
                }
            }
            AssetMetadata::FlattenedMixed {
                name,
                image,
                raw_traits,
                ..
            } => {
                let mut traits: Traits = Traits::new();

                // Convert all raw_traits Values to strings
                for (key, value) in raw_traits {
                    // Skip metadata fields that shouldn't be traits
                    let key_lower = key.to_lowercase();
                    if matches!(
                        key_lower.as_str(),
                        "name"
                            | "image"
                            | "description"
                            | "project"
                            | "twitter"
                            | "website"
                            | "discord"
                            | "github"
                            | "medium"
                            | "mediatype"
                    ) {
                        continue;
                    }

                    let string_value = match value {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Array(arr) => {
                            // Handle arrays by joining string elements
                            arr.into_iter()
                                .filter_map(|v| match v {
                                    serde_json::Value::String(s) => Some(s),
                                    serde_json::Value::Number(n) => Some(n.to_string()),
                                    serde_json::Value::Bool(b) => Some(b.to_string()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                        _ => continue, // Skip null or complex objects
                    };

                    if !string_value.is_empty() {
                        traits.insert_single(key, string_value);
                    }
                }

                Self {
                    name,
                    image: image.into(),
                    traits,
                    rarity_rank: None,
                    tags: vec![],
                }
            }
        }
    }
}

impl Asset {
    /**
    Single shot function for doing the following
    - stripping a leading policy id if one is present (`strip_policy_id`)
    - stripping a metadata prefix from an asset id (`strip_metadata_prefix`)
    */
    #[must_use]
    pub fn normalize_id(id: &str, policy_id: &str) -> String {
        let asset_id = Self::strip_policy_id(id, policy_id);
        Self::strip_metadata_prefix(&asset_id, &MetadataKind::guess_id_kind(&asset_id))
    }

    #[must_use]
    pub fn strip_policy_id(id: &str, policy_id: &str) -> String {
        id.replace(policy_id, "")
    }

    #[must_use]
    pub fn strip_metadata_prefix(id: &str, metadata_kind: &MetadataKind) -> String {
        if id.len() < 8 {
            return id.to_string();
        }

        match metadata_kind {
            MetadataKind::Cip68(purpose) => {
                let prefix = &id[..8];
                if prefix == format!("{purpose}") {
                    id[8..].to_string()
                } else {
                    id.to_string()
                }
            }
            _ => id.to_string(),
        }
    }

    #[must_use]
    pub fn get_full_id(id: &str, metadata_kind: &MetadataKind) -> String {
        match metadata_kind {
            MetadataKind::Cip68(nft_purpose) => {
                format!(
                    "{}{}",
                    nft_purpose,
                    Self::strip_metadata_prefix(id, metadata_kind)
                )
            }
            _ => id.to_string(),
        }
    }

    #[must_use]
    pub fn get_image_source(url: &str) -> AssetImageSource {
        let parts: Vec<_> = url.split("://").collect();
        match parts.first() {
            Some(&"https") => AssetImageSource::Https,
            Some(&"ipfs") => AssetImageSource::Ipfs,
            _ => AssetImageSource::Unknown,
        }
    }

    #[must_use]
    pub fn with_id(&self, id: &str) -> AssetWithId {
        AssetWithId {
            id: id.to_string(),
            asset: self.clone(),
        }
    }
}

impl From<Asset> for Traits {
    fn from(value: Asset) -> Self {
        let s = serde_json::to_string(&value.traits).unwrap();
        let mut map: Traits = serde_json::from_str(&s).unwrap();
        map.insert("Name".to_string(), TraitValue::Single(value.name));

        map
    }
}

#[derive(Debug, Serialize)]
pub struct AssetWithId {
    pub id: String,
    pub asset: Asset,
}

#[derive(Serialize, Default, Debug)]
pub struct TraitSummary {
    /// Map of trait name → (value → occurrence count)
    traits: HashMap<String, HashMap<String, u32>>,
    /// Total number of assets processed
    count: u32,
}

impl TraitSummary {
    /// Adds an asset's traits to the summary, handling both single and multi-valued traits.
    pub fn add_asset(&mut self, asset: &Asset) {
        for (trait_name, trait_values) in &asset.traits {
            // Ensure an entry for this trait name
            let counter = self.traits.entry(trait_name.clone()).or_default();

            // trait_values is now Vec<String>, so iterate over all values
            for val in trait_values {
                counter
                    .entry(val.clone())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
            }
        }

        // Increment the total asset count
        self.count += 1;
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TraitValueCount {
    #[serde(rename = "v")]
    pub value: String,
    #[serde(rename = "c")]
    pub count: u32,
}

impl TraitValueCount {
    #[must_use]
    pub fn new(value: String, count: u32) -> Self {
        Self { value, count }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TraitSummarySorted {
    pub traits: IndexMap<String, Vec<TraitValueCount>>,
    pub count: u32,
}

impl From<TraitSummary> for TraitSummarySorted {
    fn from(summary: TraitSummary) -> Self {
        let mut traits: IndexMap<String, Vec<TraitValueCount>> = IndexMap::new();

        for (key, values) in &summary.traits {
            let mut output: Vec<TraitValueCount> = values
                .iter()
                .map(|(val, count)| TraitValueCount::new(val.clone(), *count))
                .collect();

            output.sort_by(|a, b| a.count.cmp(&b.count));
            traits.insert(key.clone(), output);
        }

        Self {
            traits,
            count: summary.count,
        }
    }
}

impl TraitSummarySorted {
    #[must_use]
    pub fn is_filter_valid(&self, filter: &HashMap<String, Vec<String>>) -> bool {
        for (key, values) in filter {
            match self.traits.get(key) {
                None => return false, // unknown key
                Some(value_counts) => {
                    for val in values {
                        let matched = value_counts.iter().find(|t| t.value.eq(val));
                        if matched.is_none() {
                            return false; // unknown val
                        }
                    }
                }
            }
        }

        true
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AssetNotificationRequest {
    pub id: Option<String>,
    pub name: String,
    pub traits: Traits,
}

#[must_use]
pub fn get_asset_tags(asset: &CnftAsset, max_rarity: Option<u32>) -> Vec<AssetTag> {
    let mut tags: Vec<_> = if asset.on_sale.unwrap_or(false) {
        vec![AssetTag::OnSale]
    } else {
        vec![]
    };

    if let Some(rarity) = max_rarity.map(|max| AssetRarity::from_rank(asset.rarity_rank, max)) {
        tags.push(AssetTag::Rarity(rarity));
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::test_case;

    #[test]
    fn test_deserialize() {
        match serde_json::from_str::<Asset>(test_case!("5069726174653834.json")) {
            Ok(asset) => {
                assert_eq!(asset.name, "Pirate #84");
                assert_eq!(
                    asset.image,
                    "ipfs://QmbS83AUbxHHBQjMLvLFYxjARFBhwSEJKsDhJGJPtNJmSC"
                );
                assert_eq!(
                    asset.traits.get("Background"),
                    Some(&vec!["Cobalt Waves".to_string()])
                );
                assert_eq!(
                    Asset::get_image_source(&asset.image),
                    AssetImageSource::Ipfs
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_https_image() {
        match serde_json::from_str::<Asset>(test_case!("https_image.json")) {
            Ok(asset) => {
                assert_eq!(
                    Asset::get_image_source(&asset.image),
                    AssetImageSource::Https
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_traits_summary_sorted() {
        match serde_json::from_str::<TraitSummarySorted>(test_case!("blackflag-traits.json")) {
            Ok(summary) => {
                assert_eq!(summary.traits.len(), 9);
                assert!(summary.traits.contains_key("Mouth"));
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_toolhead_traits() {
        match serde_json::from_str::<AssetMetadata>(test_case!("traits-toolhead.json")) {
            Ok(metadata) => match metadata {
                AssetMetadata::Attributed {
                    name,
                    image,
                    media_type,
                    minter,
                    traits,
                    ..
                } => {
                    assert_eq!(name, "Toolhead #0001");
                    assert_eq!(
                        image,
                        PrimitiveOrList::Primitive(
                            "ipfs://QmXAybY8AvnNfsEgiZFxoKre1ujff5PxzU21tUuSVBEwkD".to_string()
                        )
                    );
                    assert_eq!(media_type, "image/png");
                    assert_eq!(minter, Some("CNFT.Tools".to_string()));
                    assert_eq!(
                        traits,
                        HashMap::from([
                            ("background", "SpectraCore Flare"),
                            ("body", "Chemical"),
                            ("accessory", "Contagion Canister"),
                            ("outfit", "Sensory Shirt"),
                            ("strap", "Bandolier"),
                            ("head", "Stealth Stream, Solo Sensor"),
                            ("role", "Quantum Chemist")
                        ])
                        .into_traits()
                    )
                }
                _ => panic!("expected attributed format"),
            },
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_mallards() {
        match serde_json::from_str::<AssetMetadata>(test_case!("traits-mallards.json")) {
            Ok(metadata) => match metadata {
                AssetMetadata::AttributeArray {
                    name,
                    image,
                    media_type,
                    traits,
                    ..
                } => {
                    assert_eq!(name, "The Mallard Order #4835");
                    assert_eq!(
                        image,
                        PrimitiveOrList::Primitive(
                            "ipfs://QmPEysw5BQGp9QaMSYQn8ruoQhwaNNPzXkbGeV5x1Lc9v4".to_string()
                        )
                    );
                    assert_eq!(media_type, "image/png");
                    assert_eq!(
                        traits,
                        vec![
                            HashMap::from([("Accessories", "None")]).into_traits(),
                            HashMap::from([("Head", "None")]).into_traits(),
                            HashMap::from([("Mask", "None")]).into_traits(),
                            HashMap::from([("Beak", "Plain")]).into_traits(),
                            HashMap::from([("Eyewear", "None")]).into_traits(),
                            HashMap::from([("Eyes", "Feline")]).into_traits(),
                            HashMap::from([("Face", "None")]).into_traits(),
                            HashMap::from([("Neckwear", "None")]).into_traits(),
                            HashMap::from([("Clothes", "Sailor Shirt")]).into_traits(),
                            HashMap::from([("Skin", "None")]).into_traits(),
                            HashMap::from([("Feathers", "Skeleton")]).into_traits(),
                            HashMap::from([("Back", "None")]).into_traits(),
                            HashMap::from([("Background", "Black")]).into_traits(),
                            HashMap::from([("School of Thought", "Magicka")]).into_traits(),
                        ]
                    )
                }
                _ => panic!("expected attributed format"),
            },
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_gopher() {
        match serde_json::from_str::<AssetMetadata>(test_case!("traits-gopher.json")) {
            Ok(metadata) => match metadata {
                AssetMetadata::CodifiedTraits {
                    name,
                    media_type,
                    traits,
                    ..
                } => {
                    assert_eq!(name, "Abacus Dawlish");
                    assert_eq!(media_type, "image/png");
                    assert_eq!(
                        traits,
                        vec![
                            CodifiedTrait::new("Profession", "Wizard"),
                            CodifiedTrait::new("Back", "Golden Sword"),
                            CodifiedTrait::new("Fur", "Yellow"),
                            CodifiedTrait::new("Clothes", "Wizard Robes"),
                            CodifiedTrait::new("Mouth", "White Viking Beard"),
                            CodifiedTrait::new("Holding", "Undead Staff"),
                            CodifiedTrait::new("Eyes", "Scar"),
                            CodifiedTrait::new("Hat", "Aqua Hat"),
                            CodifiedTrait::new("Name", "Abacus Dawlish"),
                            CodifiedTrait::new("Region", "South Goof"),
                            CodifiedTrait::new("Town", "Oldenthorpe"),
                            CodifiedTrait::new("Location", "Trading Post"),
                            CodifiedTrait::new_with_display(
                                "Kingdom Score",
                                "62",
                                CodifiedTraitDisplay::Range
                            ),
                            CodifiedTrait::new_with_display(
                                "Votes",
                                "1",
                                CodifiedTraitDisplay::Number
                            ),
                            CodifiedTrait::new_with_display(
                                "Treasury Boost",
                                "1.5",
                                CodifiedTraitDisplay::Multiply
                            ),
                            CodifiedTrait::new_with_display(
                                "Mining Boost",
                                "1",
                                CodifiedTraitDisplay::Multiply
                            ),
                            CodifiedTrait::new("Rank", "Expert"),
                        ]
                    )
                }
                _ => panic!("expected CodifiedTraits"),
            },
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_filter_valid_checks() {
        match serde_json::from_str::<TraitSummarySorted>(test_case!("blackflag-traits.json")) {
            Ok(summary) => {
                assert!(summary.is_filter_valid(&HashMap::from([(
                    "Rank".to_string(),
                    vec!["Navigator".to_string()]
                )])));

                assert!(summary.is_filter_valid(&HashMap::from([(
                    "Rank".to_string(),
                    vec!["Quartermaster".to_string(), "Navigator".to_string()]
                )])));
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_guess_metadata_kind() {
        assert_eq!(
            MetadataKind::guess_id_kind("000643b04e696b65766572736533333638"),
            MetadataKind::Cip68(NftPurpose::ReferenceNft)
        );

        assert_eq!(
            MetadataKind::guess_id_kind("000de1404e696b65766572736533333638"),
            MetadataKind::Cip68(NftPurpose::UserNft)
        );
    }

    #[test]
    fn test_cleaning_metadata_id() {
        assert_eq!(
            Asset::strip_metadata_prefix(
                "000de1404e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::UserNft)
            ),
            "4e696b65766572736533333638"
        );

        assert_eq!(
            Asset::strip_metadata_prefix(
                "000643b04e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::ReferenceNft)
            ),
            "4e696b65766572736533333638"
        );

        assert_eq!(
            Asset::strip_metadata_prefix(
                "4e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::ReferenceNft)
            ),
            "4e696b65766572736533333638"
        );

        assert_eq!(
            Asset::strip_metadata_prefix("546f6f6c6865616431383637", &MetadataKind::Cip25),
            "546f6f6c6865616431383637"
        );
    }

    #[test]
    fn test_get_full_id() {
        assert_eq!(
            Asset::get_full_id(
                "4e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::UserNft)
            ),
            "000de1404e696b65766572736533333638"
        );

        assert_eq!(
            Asset::get_full_id(
                "000de1404e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::UserNft)
            ),
            "000de1404e696b65766572736533333638"
        );

        assert_eq!(
            Asset::get_full_id(
                "000643b04e696b65766572736533333638",
                &MetadataKind::Cip68(NftPurpose::ReferenceNft)
            ),
            "000643b04e696b65766572736533333638"
        );

        assert_eq!(
            Asset::get_full_id("546f6f6c6865616431383637", &MetadataKind::Cip25),
            "546f6f6c6865616431383637"
        );
    }

    #[test]
    fn test_metadata_string_conversion() {
        assert_eq!(String::from(MetadataKind::Cip25), "cip25");
        assert_eq!(
            String::from(MetadataKind::Cip68(NftPurpose::ReferenceNft)),
            "cip68-reference"
        );
        assert_eq!(
            String::from(MetadataKind::Cip68(NftPurpose::UserNft)),
            "cip68-user"
        );
    }

    #[test]
    fn test_traits_consistent_serialization() {
        use crate::traits::Traits;
        use serde_json;

        // Test 1: Create traits with mixed single and multi values
        let mut traits = Traits::new();
        traits.insert_single("accessory".to_string(), "Photon Rifle".to_string());
        traits.insert_single("background".to_string(), "Anhedonian Eclipse".to_string());
        traits.insert_multi(
            "head".to_string(),
            vec!["Data Dome".to_string(), "Stealth Shroud".to_string()],
        );

        let serialized = serde_json::to_value(&traits).unwrap();

        // All values should be arrays
        assert_eq!(serialized["accessory"], serde_json::json!(["Photon Rifle"]));
        assert_eq!(
            serialized["background"],
            serde_json::json!(["Anhedonian Eclipse"])
        );
        assert_eq!(
            serialized["head"],
            serde_json::json!(["Data Dome", "Stealth Shroud"])
        );

        // Test 2: Deserialize mixed format and verify it normalizes
        let mixed_format_json = r#"{
            "single_trait": "value",
            "array_trait": ["val1", "val2"]
        }"#;

        let parsed_traits: Traits = serde_json::from_str(mixed_format_json).unwrap();
        let reserialized = serde_json::to_value(&parsed_traits).unwrap();

        // Both should now be arrays
        assert_eq!(reserialized["single_trait"], serde_json::json!(["value"]));
        assert_eq!(
            reserialized["array_trait"],
            serde_json::json!(["val1", "val2"])
        );
    }
}
