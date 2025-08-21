use std::collections::HashMap;

use cardano_assets::{AssetId, CollectionDetails, Marketplace};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionAssetsRequest {
    #[serde(rename = "policyId")]
    pub policy_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(rename = "minPrice", skip_serializing_if = "Option::is_none")]
    pub min_price: Option<u64>,
    #[serde(rename = "maxPrice", skip_serializing_if = "Option::is_none")]
    pub max_price: Option<u64>,
    #[serde(rename = "minRarity", skip_serializing_if = "Option::is_none")]
    pub min_rarity: Option<u32>,
    #[serde(rename = "maxRarity", skip_serializing_if = "Option::is_none")]
    pub max_rarity: Option<u32>,
    #[serde(rename = "orderBy", default)]
    pub order_by: OrderBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
    #[serde(rename = "listingType", skip_serializing_if = "Option::is_none")]
    pub listing_type: Option<ListingType>,
    #[serde(rename = "saleType", skip_serializing_if = "Option::is_none")]
    pub sale_type: Option<SaleType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<PropertyFilter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaleType {
    All,
    ListedOnly,
    Bundles,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OrderBy {
    #[default]
    PriceAsc,
    PriceDesc,
    NameAsc,
    IdxAsc,
    RecentlyListed,
    RarityAsc,
    RecentlyMinted,
}

/// Marketplace-specific listing type filter
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListingType {
    #[serde(rename = "jpgstore")]
    JpgStore,
    #[serde(rename = "wayup")]
    Wayup,
    #[serde(rename = "spacebudz")]
    SpaceBudz,
}

/// Property/attribute filter for exact matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyFilter {
    pub key: String,
    pub value: String,
}

impl From<Marketplace> for ListingType {
    fn from(marketplace: Marketplace) -> Self {
        match marketplace {
            Marketplace::JpgStore => ListingType::JpgStore,
            Marketplace::SpaceBudz => ListingType::SpaceBudz,
            _ => ListingType::Wayup,
        }
    }
}

impl CollectionAssetsRequest {
    /// Create a basic request with only policy_id
    pub fn new(policy_id: String) -> Self {
        Self {
            policy_id,
            limit: None,
            cursor: None,
            min_price: None,
            max_price: None,
            min_rarity: None,
            max_rarity: None,
            order_by: OrderBy::default(),
            term: None,
            listing_type: None,
            sale_type: None,
            properties: None,
        }
    }

    /// Create request for listed assets only with price ordering
    pub fn for_listed_assets(policy_id: String, limit: Option<u32>) -> Self {
        Self {
            policy_id,
            limit,
            cursor: None,
            min_price: None,
            max_price: None,
            min_rarity: None,
            max_rarity: None,
            order_by: OrderBy::default(),
            term: None,
            listing_type: None,
            sale_type: Some(SaleType::ListedOnly),
            properties: None,
        }
    }

    /// Builder methods for fluent API
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_cursor(mut self, cursor: String) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn with_price_range(mut self, min_price: Option<u64>, max_price: Option<u64>) -> Self {
        self.min_price = min_price;
        self.max_price = max_price;
        self
    }

    pub fn with_rarity_range(mut self, min_rarity: Option<u32>, max_rarity: Option<u32>) -> Self {
        self.min_rarity = min_rarity;
        self.max_rarity = max_rarity;
        self
    }

    pub fn with_order_by(mut self, order_by: OrderBy) -> Self {
        self.order_by = order_by;
        self
    }

    pub fn with_search_term(mut self, term: String) -> Self {
        self.term = Some(term);
        self
    }

    pub fn with_listing_type(mut self, listing_type: ListingType) -> Self {
        self.listing_type = Some(listing_type);
        self
    }

    pub fn with_marketplace(mut self, marketplace: Marketplace) -> Self {
        self.listing_type = Some(ListingType::from(marketplace));
        self
    }

    pub fn with_sale_type(mut self, sale_type: SaleType) -> Self {
        self.sale_type = Some(sale_type);
        self
    }

    pub fn with_properties(mut self, properties: Vec<PropertyFilter>) -> Self {
        self.properties = Some(properties);
        self
    }

    /// Convenience method to add a single trait filter
    pub fn with_trait(mut self, key: String, value: String) -> Self {
        let filter = PropertyFilter { key, value };
        match self.properties {
            Some(mut props) => {
                props.push(filter);
                self.properties = Some(props);
            }
            None => {
                self.properties = Some(vec![filter]);
            }
        }
        self
    }

    /// Convenience method to add multiple trait filters
    pub fn with_traits<I, K, V>(mut self, traits: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let new_filters: Vec<PropertyFilter> = traits
            .into_iter()
            .map(|(k, v)| PropertyFilter {
                key: k.into(),
                value: v.into(),
            })
            .collect();

        match self.properties {
            Some(mut props) => {
                props.extend(new_filters);
                self.properties = Some(props);
            }
            None => {
                self.properties = Some(new_filters);
            }
        }
        self
    }
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
    pub collection: Option<CollectionDetails>,
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
