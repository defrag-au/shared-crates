use serde::{Deserialize, Serialize};

/// Marketplace enum representing different NFT marketplaces
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

/// Collection social media links
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSocials {
    pub discord: String,
    pub twitter: String,
    pub website: String,
}

/// Collection information for a CNFT collection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDetails {
    pub policy_id: String,
    pub name: String,
    pub handle: String,
    pub description: Option<String>,
    pub royalty_address: Option<String>,
    #[serde(alias = "royaltyPct")]
    pub royalty_percentage: f64,
    pub image: Option<String>,
    pub banner: Option<String>,
    pub socials: Option<CollectionSocials>,
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
