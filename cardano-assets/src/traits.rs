use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, fmt::Display};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(untagged)]
pub enum TraitValue {
    Single(String),
    Multi(Vec<String>),
}

impl Display for TraitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(value) => write!(f, "{value}"),
            Self::Multi(values) => write!(f, "{}", values.join(", ")),
        }
    }
}

impl PartialEq<String> for TraitValue {
    fn eq(&self, other: &String) -> bool {
        match self {
            Self::Single(value) => value == other,
            Self::Multi(values) => values.join(", ") == *other,
        }
    }
}

impl From<TraitValue> for Vec<String> {
    fn from(value: TraitValue) -> Self {
        match value {
            TraitValue::Single(s) => vec![s.trim().to_string()],
            TraitValue::Multi(v) => v.into_iter().map(|s| s.trim().to_string()).collect(),
        }
    }
}

impl From<Vec<String>> for TraitValue {
    fn from(values: Vec<String>) -> Self {
        if values.len() == 1 {
            TraitValue::Single(values.into_iter().next().unwrap())
        } else {
            TraitValue::Multi(values)
        }
    }
}

impl From<String> for TraitValue {
    fn from(value: String) -> Self {
        TraitValue::Single(value)
    }
}

/// New consistent traits type using HashMap<String, Vec<String>>
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = std::collections::HashMap<String, Vec<String>>))]
pub struct Traits(HashMap<String, Vec<String>>);

impl Display for Traits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v.join(", ")))
            .collect();
        write!(f, "{{{}}}", formatted.join(", "))
    }
}

/// Custom serializer to ensure all traits are consistently serialized as Vec<String>
impl Serialize for Traits {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Always serialize as HashMap<String, Vec<String>> to ensure consistency
        self.0.serialize(serializer)
    }
}

/// Custom deserializer to handle both old TraitValue format and new Vec<String> format
impl<'de> Deserialize<'de> for Traits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TraitsFormat {
            Old(HashMap<String, TraitValue>),
            New(HashMap<String, Vec<String>>),
        }

        let format = TraitsFormat::deserialize(deserializer)?;

        match format {
            TraitsFormat::Old(old_traits) => {
                // Convert old format to new format
                let new_traits = old_traits.into_iter().map(|(k, v)| (k, v.into())).collect();
                Ok(Traits(new_traits))
            }
            TraitsFormat::New(new_traits) => Ok(Traits(new_traits)),
        }
    }
}

impl Traits {
    /// Create new empty traits
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Create from HashMap<String, Vec<String>>
    pub fn from_map(map: HashMap<String, Vec<String>>) -> Self {
        Self(map)
    }

    /// Get a reference to the inner HashMap
    pub fn inner(&self) -> &HashMap<String, Vec<String>> {
        &self.0
    }

    /// Get a mutable reference to the inner HashMap
    pub fn inner_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.0
    }

    /// Insert a trait with a single value
    pub fn insert_single(&mut self, key: String, value: String) {
        self.0.insert(key, vec![value]);
    }

    /// Insert a trait with multiple values
    pub fn insert_multi(&mut self, key: String, values: Vec<String>) {
        self.0.insert(key, values);
    }

    /// Insert a trait value (backward compatibility with TraitValue)
    pub fn insert(&mut self, key: String, value: TraitValue) {
        self.0.insert(key, value.into());
    }

    /// Insert trait values directly as Vec<String>
    pub fn insert_vec(&mut self, key: String, values: Vec<String>) {
        self.0.insert(key, values);
    }

    /// Get trait values as Vec<String>
    pub fn get(&self, key: &str) -> Option<&Vec<String>> {
        self.0.get(key)
    }

    /// Get trait as a single string (joins multiple values with ", ")
    pub fn get_single(&self, key: &str) -> Option<String> {
        self.0.get(key).map(|values| values.join(", "))
    }

    /// Check if trait exists
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Iterate over traits
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.0.iter()
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// Get all values
    pub fn values(&self) -> impl Iterator<Item = &Vec<String>> {
        self.0.values()
    }

    /// Convert to the old TraitValue format for backward compatibility
    pub fn to_trait_value_map(&self) -> HashMap<String, TraitValue> {
        self.0
            .iter()
            .map(|(k, v)| {
                let trait_value = if v.len() == 1 {
                    TraitValue::Single(v[0].clone())
                } else {
                    TraitValue::Multi(v.clone())
                };
                (k.clone(), trait_value)
            })
            .collect()
    }
}

impl Default for Traits {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for Traits {
    type Item = (String, Vec<String>);
    type IntoIter = std::collections::hash_map::IntoIter<String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Traits {
    type Item = (&'a String, &'a Vec<String>);
    type IntoIter = std::collections::hash_map::Iter<'a, String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<HashMap<String, Vec<String>>> for Traits {
    fn from(map: HashMap<String, Vec<String>>) -> Self {
        Self(map)
    }
}

impl From<Traits> for HashMap<String, Vec<String>> {
    fn from(traits: Traits) -> Self {
        traits.0
    }
}

/// Convert old TraitValue map to new Traits
impl From<HashMap<String, TraitValue>> for Traits {
    fn from(old_traits: HashMap<String, TraitValue>) -> Self {
        let new_map = old_traits.into_iter().map(|(k, v)| (k, v.into())).collect();
        Self(new_map)
    }
}

pub trait IntoTraits {
    fn into_traits(self) -> Traits;
}

impl<K, V> IntoTraits for HashMap<K, V>
where
    K: Into<String>,
    V: Into<String>,
{
    fn into_traits(self) -> Traits {
        let map = self
            .into_iter()
            .map(|(k, v)| {
                // Convert to owned String
                let raw = v.into();
                // Split on ", " and collect into Vec<String>
                let parts: Vec<String> = raw
                    .split(", ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                // If no parts after filtering, use trimmed raw value
                let values = if parts.is_empty() {
                    vec![raw.trim().to_string()]
                } else {
                    parts
                };

                (k.into(), values)
            })
            .collect();
        Traits::from_map(map)
    }
}

/// Convert HashMap<String, TraitValue> to new Traits format
impl IntoTraits for HashMap<String, TraitValue> {
    fn into_traits(self) -> Traits {
        Traits::from(self)
    }
}

/// If you need a flat HashMap<String, String> (joins multi-values with ", ")
pub fn flatten_traits(traits: &Traits) -> HashMap<String, String> {
    traits
        .iter()
        .map(|(k, v)| (k.clone(), v.join(", ")))
        .collect()
}

/// Legacy type alias for backward compatibility
#[deprecated(note = "Use Traits directly instead")]
pub type LegacyTraits = HashMap<String, TraitValue>;
