use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use crate::{Asset, MetadataKind};

/// Cardano policy ID length in hex characters (28 bytes = 56 hex chars)
const POLICY_ID_LENGTH: usize = 56;

/// A compound asset identifier representing an on-chain Cardano native token
///
/// Combines policy_id and asset_name_hex into a unified type that can
/// represent itself in multiple formats: concatenated, dot-delimited, and JSON.
///
/// Note: All Cardano native tokens (both fungible and non-fungible) have asset names.
/// ADA is a special case with no policy ID or asset name and is not represented by this type.
///
/// # Examples
///
/// ```
/// use cardano_assets::AssetId;
///
/// // Create from components
/// let asset_id = AssetId::new(
///     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6".to_string(),
///     "50697261746531303836".to_string() // "Pirate1086" in hex
/// ).expect("Valid asset ID");
///
/// // Different format representations
/// assert_eq!(
///     asset_id.concatenated(),
///     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f650697261746531303836"
/// );
/// assert_eq!(
///     asset_id.dot_delimited(),
///     "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6.50697261746531303836"
/// );
///
/// // Parse from various formats
/// let from_concat: AssetId = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f650697261746531303836".parse().unwrap();
/// let from_dotted: AssetId = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6.50697261746531303836".parse().unwrap();
/// assert_eq!(from_concat, from_dotted);
///
/// // UTF-8 decoded asset name
/// assert_eq!(asset_id.asset_name(), "Pirate1086");
///
/// // Asset names cannot be empty
/// assert!(AssetId::new("policy_id".to_string(), "".to_string()).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssetId {
    /// Policy ID as 56-character hex string (28 bytes)
    pub policy_id: String,
    /// Asset name as hex-encoded string (variable length)
    pub asset_name_hex: String,
}

impl AssetId {
    /// Create a new AssetId from policy_id and asset_name_hex
    pub fn new(policy_id: String, asset_name_hex: String) -> Result<Self, AssetIdError> {
        Self::validate_policy_id(&policy_id)?;
        Self::validate_asset_name_hex(&asset_name_hex)?;

        if asset_name_hex.is_empty() {
            return Err(AssetIdError::EmptyAssetName);
        }

        Ok(Self {
            policy_id,
            asset_name_hex,
        })
    }

    /// Create AssetId without validation (use with caution)
    pub fn new_unchecked(policy_id: String, asset_name_hex: String) -> Self {
        Self {
            policy_id,
            asset_name_hex,
        }
    }

    /// Get the concatenated format: policy_id + asset_name_hex
    pub fn concatenated(&self) -> String {
        format!("{}{}", self.policy_id, self.asset_name_hex)
    }

    /// Get the dot-delimited format: policy_id.asset_name_hex
    pub fn dot_delimited(&self) -> String {
        format!("{}.{}", self.policy_id, self.asset_name_hex)
    }

    /// Get the policy ID
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    /// Get the asset name hex
    pub fn asset_name_hex(&self) -> &str {
        &self.asset_name_hex
    }

    /// Get the asset name decoded as UTF-8 (falls back to hex if invalid)
    pub fn asset_name(&self) -> String {
        if self.asset_name_hex.is_empty() {
            return String::new();
        }

        match hex::decode(&self.asset_name_hex) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(name) => name,
                Err(_) => self.asset_name_hex.clone(),
            },
            Err(_) => self.asset_name_hex.clone(),
        }
    }

    pub fn strip_cip67(&self) -> AssetId {
        let stripped_hex = Asset::strip_metadata_prefix(
            &self.asset_name_hex,
            &MetadataKind::guess_id_kind(&self.asset_name_hex),
        );

        Self::new_unchecked(self.policy_id.clone(), stripped_hex)
    }

    /// Create AssetId from hex-encoded asset name
    pub fn from_hex_name(policy_id: String, asset_name_hex: String) -> Result<Self, AssetIdError> {
        Self::new(policy_id, asset_name_hex)
    }

    /// Create AssetId from UTF-8 asset name (encodes to hex)
    pub fn from_utf8_name(policy_id: String, asset_name: String) -> Result<Self, AssetIdError> {
        let asset_name_hex = hex::encode(asset_name.as_bytes());
        Self::new(policy_id, asset_name_hex)
    }

    /// Get the policy ID as bytes
    pub fn policy_id_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(&self.policy_id)
    }

    /// Get the asset name as bytes
    pub fn asset_name_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(&self.asset_name_hex)
    }

    /// Get the full asset ID as bytes (policy_id + asset_name concatenated)
    pub fn as_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
        let mut bytes = self.policy_id_bytes()?;
        bytes.extend(self.asset_name_bytes()?);
        Ok(bytes)
    }

    /// Parse from concatenated format with smart format detection
    pub fn parse_smart(input: &str) -> Result<Self, AssetIdError> {
        // Try dot-delimited format first
        if let Some(dot_pos) = input.find('.') {
            let policy_id = input[..dot_pos].to_string();
            let asset_name_hex = input[dot_pos + 1..].to_string();
            return Self::new(policy_id, asset_name_hex);
        }

        // Fall back to concatenated format
        Self::parse_concatenated(input)
    }

    /// Parse from concatenated format: policy_id + asset_name_hex
    pub fn parse_concatenated(input: &str) -> Result<Self, AssetIdError> {
        if input.len() < POLICY_ID_LENGTH {
            return Err(AssetIdError::InvalidLength {
                expected_min: POLICY_ID_LENGTH,
                actual: input.len(),
            });
        }

        let policy_id = input[..POLICY_ID_LENGTH].to_string();
        let asset_name_hex = input[POLICY_ID_LENGTH..].to_string();

        Self::new(policy_id, asset_name_hex)
    }

    /// Parse from dot-delimited format: policy_id.asset_name_hex
    pub fn parse_dot_delimited(input: &str) -> Result<Self, AssetIdError> {
        let parts: Vec<&str> = input.split('.').collect();
        if parts.len() != 2 {
            return Err(AssetIdError::InvalidDotDelimitedFormat);
        }

        let policy_id = parts[0].to_string();
        let asset_name_hex = parts[1].to_string();

        Self::new(policy_id, asset_name_hex)
    }

    /// Validate policy ID format
    fn validate_policy_id(policy_id: &str) -> Result<(), AssetIdError> {
        if policy_id.len() != POLICY_ID_LENGTH {
            return Err(AssetIdError::InvalidPolicyIdLength {
                expected: POLICY_ID_LENGTH,
                actual: policy_id.len(),
            });
        }

        if !policy_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AssetIdError::InvalidPolicyIdFormat);
        }

        Ok(())
    }

    /// Validate asset name hex format
    fn validate_asset_name_hex(asset_name_hex: &str) -> Result<(), AssetIdError> {
        if asset_name_hex.is_empty() {
            return Err(AssetIdError::EmptyAssetName);
        }

        if !asset_name_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AssetIdError::InvalidAssetNameHexFormat);
        }

        if asset_name_hex.len() % 2 != 0 {
            return Err(AssetIdError::InvalidAssetNameHexLength);
        }

        Ok(())
    }
}

impl TryFrom<&str> for AssetId {
    type Error = AssetIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse_smart(value)
    }
}

/// Error types for AssetId operations
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum AssetIdError {
    InvalidLength { expected_min: usize, actual: usize },
    InvalidPolicyIdLength { expected: usize, actual: usize },
    InvalidPolicyIdFormat,
    InvalidAssetNameHexFormat,
    InvalidAssetNameHexLength,
    InvalidDotDelimitedFormat,
    EmptyAssetName,
}

impl fmt::Display for AssetIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetIdError::InvalidLength {
                expected_min,
                actual,
            } => {
                write!(
                    f,
                    "Invalid asset ID length: expected at least {}, got {}",
                    expected_min, actual
                )
            }
            AssetIdError::InvalidPolicyIdLength { expected, actual } => {
                write!(
                    f,
                    "Invalid policy ID length: expected {}, got {}",
                    expected, actual
                )
            }
            AssetIdError::InvalidPolicyIdFormat => {
                write!(f, "Invalid policy ID format: must be hexadecimal")
            }
            AssetIdError::InvalidAssetNameHexFormat => {
                write!(f, "Invalid asset name hex format: must be hexadecimal")
            }
            AssetIdError::InvalidAssetNameHexLength => {
                write!(
                    f,
                    "Invalid asset name hex length: must be even number of characters"
                )
            }
            AssetIdError::InvalidDotDelimitedFormat => {
                write!(
                    f,
                    "Invalid dot-delimited format: must contain exactly one dot"
                )
            }
            AssetIdError::EmptyAssetName => {
                write!(f, "Asset name cannot be empty")
            }
        }
    }
}

impl std::error::Error for AssetIdError {}

/// Default display shows concatenated format for backward compatibility
impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.concatenated())
    }
}

/// FromStr implementation with smart format detection
impl FromStr for AssetId {
    type Err = AssetIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_smart(s)
    }
}

/// Convert to concatenated string (backward compatibility)
impl From<AssetId> for String {
    fn from(asset_id: AssetId) -> Self {
        asset_id.concatenated()
    }
}

/// Serialize as concatenated string by default for backward compatibility
impl Serialize for AssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // For JSON and other human-readable formats, use structured representation
            #[derive(Serialize)]
            struct AssetIdJson<'a> {
                policy_id: &'a str,
                asset_name_hex: &'a str,
            }

            let json_repr = AssetIdJson {
                policy_id: &self.policy_id,
                asset_name_hex: &self.asset_name_hex,
            };
            json_repr.serialize(serializer)
        } else {
            // For binary formats, use concatenated string
            self.concatenated().serialize(serializer)
        }
    }
}

/// Deserialize from either concatenated string or structured JSON
impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum AssetIdFormat {
            Structured {
                policy_id: String,
                asset_name_hex: String,
            },
            String(String),
        }

        let format = AssetIdFormat::deserialize(deserializer)?;

        match format {
            AssetIdFormat::Structured {
                policy_id,
                asset_name_hex,
            } => AssetId::new(policy_id, asset_name_hex)
                .map_err(|e| serde::de::Error::custom(format!("Invalid AssetId: {}", e))),
            AssetIdFormat::String(s) => AssetId::parse_smart(&s)
                .map_err(|e| serde::de::Error::custom(format!("Invalid AssetId string: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";
    const TEST_ASSET_NAME_HEX: &str = "50697261746531303836"; // "Pirate1086" - real Blackflag asset
    const TEST_CONCATENATED: &str =
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f650697261746531303836";
    const TEST_DOT_DELIMITED: &str =
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6.50697261746531303836";
    const CIP_68_POLICY: &str =
        "29728939434a25e57ef6a9b94ba3215508264fee665bbb35b16a2d56000de1404d4432393230";

    #[test]
    fn test_new_valid() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_concatenated_format() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        assert_eq!(asset_id.concatenated(), TEST_CONCATENATED);
    }

    #[test]
    fn test_dot_delimited_format() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        assert_eq!(asset_id.dot_delimited(), TEST_DOT_DELIMITED);
    }

    #[test]
    fn test_parse_concatenated() {
        let asset_id = AssetId::parse_concatenated(TEST_CONCATENATED)
            .expect("Should parse concatenated format");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_parse_dot_delimited() {
        let asset_id = AssetId::parse_dot_delimited(TEST_DOT_DELIMITED)
            .expect("Should parse dot-delimited format");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_parse_smart_dot_delimited() {
        let asset_id =
            AssetId::parse_smart(TEST_DOT_DELIMITED).expect("Should parse dot-delimited format");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_parse_smart_concatenated() {
        let asset_id =
            AssetId::parse_smart(TEST_CONCATENATED).expect("Should parse concatenated format");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_parse_smart_concatenated_try_into() {
        let asset_id: AssetId = TEST_CONCATENATED
            .try_into()
            .expect("Should parse concatenated format");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_strip_cip67_prefix() {
        let asset_id =
            AssetId::parse_concatenated(CIP_68_POLICY).expect("Should create valid AssetId");

        assert_eq!(asset_id.concatenated(), CIP_68_POLICY);
        assert_eq!(
            asset_id.strip_cip67().concatenated(),
            "29728939434a25e57ef6a9b94ba3215508264fee665bbb35b16a2d564d4432393230"
        );
    }

    #[test]
    fn test_from_str() {
        let asset_id: AssetId = TEST_DOT_DELIMITED
            .parse()
            .expect("Should parse from string");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_display() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        assert_eq!(asset_id.to_string(), TEST_CONCATENATED);
    }

    #[test]
    fn test_asset_name_utf8() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        assert_eq!(asset_id.asset_name(), "Pirate1086");
    }

    #[test]
    fn test_empty_asset_name_rejected() {
        let result = AssetId::new(TEST_POLICY_ID.to_string(), String::new());
        assert!(matches!(result, Err(AssetIdError::EmptyAssetName)));
    }

    #[test]
    fn test_json_serialization() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        let json = serde_json::to_string(&asset_id).expect("Should serialize to JSON");
        let expected = format!(
            r#"{{"policy_id":"{}","asset_name_hex":"{}"}}"#,
            TEST_POLICY_ID, TEST_ASSET_NAME_HEX
        );

        assert_eq!(json, expected);
    }

    #[test]
    fn test_json_deserialization_structured() {
        let json = format!(
            r#"{{"policy_id":"{}","asset_name_hex":"{}"}}"#,
            TEST_POLICY_ID, TEST_ASSET_NAME_HEX
        );

        let asset_id: AssetId = serde_json::from_str(&json).expect("Should deserialize from JSON");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_json_deserialization_string() {
        let json = format!(r#""{}""#, TEST_DOT_DELIMITED);

        let asset_id: AssetId =
            serde_json::from_str(&json).expect("Should deserialize from JSON string");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_parsing_from_string() {
        let asset_id: AssetId = TEST_CONCATENATED
            .parse()
            .expect("Should parse valid concatenated string");

        assert_eq!(asset_id.policy_id(), TEST_POLICY_ID);
        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
    }

    #[test]
    fn test_conversion_to_string() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        let s: String = asset_id.into();
        assert_eq!(s, TEST_CONCATENATED);
    }

    #[test]
    fn test_invalid_policy_id_length() {
        let result = AssetId::new("short".to_string(), TEST_ASSET_NAME_HEX.to_string());
        assert!(matches!(
            result,
            Err(AssetIdError::InvalidPolicyIdLength { .. })
        ));
    }

    #[test]
    fn test_invalid_policy_id_format() {
        // Create a 56-character string with invalid hex characters
        let invalid_policy_id = "zzzzcf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b491";
        assert_eq!(invalid_policy_id.len(), 56); // Ensure correct length
        let result = AssetId::new(
            invalid_policy_id.to_string(),
            TEST_ASSET_NAME_HEX.to_string(),
        );
        assert!(matches!(result, Err(AssetIdError::InvalidPolicyIdFormat)));
    }

    #[test]
    fn test_invalid_asset_name_hex_format() {
        let result = AssetId::new(TEST_POLICY_ID.to_string(), "invalid_hex!".to_string());
        assert!(matches!(
            result,
            Err(AssetIdError::InvalidAssetNameHexFormat)
        ));
    }

    #[test]
    fn test_invalid_asset_name_hex_length() {
        let result = AssetId::new(TEST_POLICY_ID.to_string(), "42F".to_string()); // Odd length
        assert!(matches!(
            result,
            Err(AssetIdError::InvalidAssetNameHexLength)
        ));
    }

    #[test]
    fn test_from_utf8_name() {
        let asset_id =
            AssetId::from_utf8_name(TEST_POLICY_ID.to_string(), "Pirate1086".to_string())
                .expect("Should create from UTF-8 name");

        assert_eq!(asset_id.asset_name_hex(), TEST_ASSET_NAME_HEX);
        assert_eq!(asset_id.asset_name(), "Pirate1086");
    }

    #[test]
    fn test_policy_id_bytes() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        let bytes = asset_id.policy_id_bytes().expect("Should decode policy ID");
        assert_eq!(bytes.len(), 28); // 28 bytes = 56 hex chars
        assert_eq!(hex::encode(&bytes), TEST_POLICY_ID);
    }

    #[test]
    fn test_asset_name_bytes() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        let bytes = asset_id
            .asset_name_bytes()
            .expect("Should decode asset name");
        assert_eq!(String::from_utf8(bytes).unwrap(), "Pirate1086");
    }

    #[test]
    fn test_as_bytes() {
        let asset_id = AssetId::new(TEST_POLICY_ID.to_string(), TEST_ASSET_NAME_HEX.to_string())
            .expect("Should create valid AssetId");

        let bytes = asset_id.as_bytes().expect("Should get full bytes");
        assert_eq!(hex::encode(&bytes), TEST_CONCATENATED);
    }
}
