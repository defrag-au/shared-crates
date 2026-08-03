use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Marketplace enum representing different NFT marketplaces
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum Marketplace {
    #[default]
    JpgStore,
    Wayup,
    SpaceBudz,
    Unknown(String),
}

impl Serialize for Marketplace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Self::JpgStore => "jpg.store",
            Self::Wayup => "wayup",
            Self::SpaceBudz => "spacebudz",
            Self::Unknown(name) => name,
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
            Self::JpgStore => write!(f, "jpg.store"),
            Self::Wayup => write!(f, "wayup"),
            Self::SpaceBudz => write!(f, "spacebudz"),
            Self::Unknown(name) => write!(f, "{}", name),
        }
    }
}

/// Collection social media links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSocials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
}

/// Deserialize a possibly-null-or-missing number as `f64`, defaulting to `0.0`.
/// Anvil sends `royaltyPct: null` (or omits it) for collections with no royalty
/// configured, which a bare `f64` field rejects.
fn de_f64_null_default<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or_default())
}

/// Collection information for a CNFT collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionDetails {
    #[serde(alias = "policyId")]
    pub policy_id: String,
    pub name: String,
    pub handle: Option<String>,
    pub description: Option<String>,
    #[serde(alias = "royaltyAddress")]
    pub royalty_address: Option<String>,
    #[serde(alias = "royaltyPct", default, deserialize_with = "de_f64_null_default")]
    pub royalty_percentage: f64,
    pub image: Option<String>,
    pub banner: Option<String>,
    pub socials: Option<CollectionSocials>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn royalty_percentage_tolerates_null_and_missing() {
        // Anvil returns `null` (or omits) royaltyPct for royalty-free collections.
        let null: CollectionDetails =
            serde_json::from_str(r#"{"policyId":"p","name":"n","royaltyPct":null}"#).unwrap();
        assert_eq!(null.royalty_percentage, 0.0);

        let missing: CollectionDetails =
            serde_json::from_str(r#"{"policyId":"p","name":"n"}"#).unwrap();
        assert_eq!(missing.royalty_percentage, 0.0);

        let present: CollectionDetails =
            serde_json::from_str(r#"{"policyId":"p","name":"n","royaltyPct":2.5}"#).unwrap();
        assert_eq!(present.royalty_percentage, 2.5);
    }

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
