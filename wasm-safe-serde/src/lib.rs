//! WASM-safe serialization helpers for serde
//!
//! This crate provides serialization modules that handle JavaScript's
//! Number.MAX_SAFE_INTEGER limit (2^53 - 1 = 9007199254740991) by
//! automatically converting large u64 values to strings.

/// Serializes u64 values as strings when they exceed JavaScript's safe integer limit
/// (Number.MAX_SAFE_INTEGER = 2^53 - 1 = 9007199254740991)
///
/// Use with `#[serde(with = "wasm_safe_serde::u64_option")]`
pub mod u64_option {
    use serde::{Deserialize, Deserializer, Serializer};

    const MAX_SAFE_JS_INTEGER: u64 = 9007199254740991;

    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(val) if *val > MAX_SAFE_JS_INTEGER => serializer.serialize_str(&val.to_string()),
            Some(val) => serializer.serialize_u64(*val),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_json::Value;
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(Some(u))
                } else {
                    Err(serde::de::Error::custom("Invalid number for u64"))
                }
            }
            Value::String(s) => s
                .parse::<u64>()
                .map(Some)
                .map_err(|_| serde::de::Error::custom("Invalid string for u64")),
            Value::Null => Ok(None),
            _ => Err(serde::de::Error::custom("Expected number, string, or null")),
        }
    }
}

/// WASM-safe serialization for non-optional large integers
///
/// Use with `#[serde(with = "wasm_safe_serde::u64_required")]`
pub mod u64_required {
    use serde::{Deserialize, Deserializer, Serializer};

    const MAX_SAFE_JS_INTEGER: u64 = 9007199254740991;

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if *value > MAX_SAFE_JS_INTEGER {
            serializer.serialize_str(&value.to_string())
        } else {
            serializer.serialize_u64(*value)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_json::Value;
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Number(n) => {
                if let Some(u) = n.as_u64() {
                    Ok(u)
                } else {
                    Err(serde::de::Error::custom("Invalid number for u64"))
                }
            }
            Value::String(s) => s
                .parse::<u64>()
                .map_err(|_| serde::de::Error::custom("Invalid string for u64")),
            _ => Err(serde::de::Error::custom("Expected number or string")),
        }
    }
}

/// WASM-safe serialization for i64 values (handles negative numbers)
///
/// Use with `#[serde(with = "wasm_safe_serde::i64")]`
pub mod i64 {
    use serde::{Deserialize, Deserializer, Serializer};

    const MAX_SAFE_JS_INTEGER: i64 = 9007199254740991;
    const MIN_SAFE_JS_INTEGER: i64 = -9007199254740991;

    pub fn serialize<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if *value > MAX_SAFE_JS_INTEGER || *value < MIN_SAFE_JS_INTEGER {
            serializer.serialize_str(&value.to_string())
        } else {
            serializer.serialize_i64(*value)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_json::Value;
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(i)
                } else {
                    Err(serde::de::Error::custom("Invalid number for i64"))
                }
            }
            Value::String(s) => s
                .parse::<i64>()
                .map_err(|_| serde::de::Error::custom("Invalid string for i64")),
            _ => Err(serde::de::Error::custom("Expected number or string")),
        }
    }
}

/// WASM-safe serialization for HashMap<String, u64> containing asset quantities
///
/// Use with `#[serde(with = "wasm_safe_serde::asset_map")]`
pub mod asset_map {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde_json::{Map, Value};
    use std::collections::HashMap;

    const MAX_SAFE_JS_INTEGER: u64 = 9007199254740991;

    pub fn serialize<S>(value: &HashMap<String, u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(value.len()))?;
        for (k, v) in value {
            if *v > MAX_SAFE_JS_INTEGER {
                map.serialize_entry(k, &v.to_string())?;
            } else {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = Map::deserialize(deserializer)?;
        let mut result = HashMap::new();

        for (key, value) in map {
            let parsed_value = match value {
                Value::Number(n) => {
                    if let Some(u) = n.as_u64() {
                        u
                    } else {
                        return Err(serde::de::Error::custom("Invalid number for u64"));
                    }
                }
                Value::String(s) => s
                    .parse::<u64>()
                    .map_err(|_| serde::de::Error::custom("Invalid string for u64"))?,
                _ => return Err(serde::de::Error::custom("Expected number or string")),
            };
            result.insert(key, parsed_value);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Serialize, Deserialize)]
    struct TestStruct {
        #[serde(with = "u64_option")]
        optional_value: Option<u64>,
        #[serde(with = "u64_required")]
        required_value: u64,
        #[serde(with = "asset_map")]
        assets: HashMap<String, u64>,
        #[serde(with = "i64")]
        signed_value: i64,
    }

    #[test]
    fn test_large_numbers_serialized_as_strings() {
        let large_number = 12738606488933375_u64; // > MAX_SAFE_JS_INTEGER
        let small_number = 1_000_000_u64;

        let mut assets = HashMap::new();
        assets.insert("large_asset".to_string(), large_number);
        assets.insert("small_asset".to_string(), small_number);

        let test_data = TestStruct {
            optional_value: Some(large_number),
            required_value: large_number,
            assets,
            signed_value: 9223372036854775807i64, // i64::MAX - the problematic value
        };

        let json = serde_json::to_string(&test_data).expect("Should serialize successfully");

        // Large numbers should be serialized as strings
        assert!(
            json.contains(&format!("\"{}\"", large_number)),
            "Large numbers should be serialized as strings"
        );

        // Verify we can deserialize back
        let deserialized: TestStruct =
            serde_json::from_str(&json).expect("Should deserialize successfully");
        assert_eq!(deserialized.optional_value, Some(large_number));
        assert_eq!(deserialized.required_value, large_number);
        assert_eq!(deserialized.assets.get("large_asset"), Some(&large_number));
    }

    #[test]
    fn test_small_numbers_remain_numeric() {
        let small_number = 1_000_000_u64;

        let mut assets = HashMap::new();
        assets.insert("small_asset".to_string(), small_number);

        let test_data = TestStruct {
            optional_value: Some(small_number),
            required_value: small_number,
            assets,
            signed_value: -100_000i64, // Small negative number
        };

        let json = serde_json::to_string(&test_data).expect("Should serialize successfully");

        // Small numbers should remain as numbers, not strings
        assert!(
            json.contains(&small_number.to_string())
                && !json.contains(&format!("\"{}\"", small_number)),
            "Small numbers should remain as numbers"
        );
    }
}
