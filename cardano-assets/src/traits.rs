use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};

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

/// The canonical traits type: `Vec<String>` per category, ordered.
///
/// `BTreeMap` rather than `HashMap` so the order is a property of the type
/// rather than of whichever path built it: categories iterate — and so
/// serialize — in one stable order whether the map came from the extractor,
/// a persisted payload or a fixture. A `HashMap`'s iteration order is seeded
/// per process, which made a fresh trait-schema derive assign bit positions
/// differently between environments and kept API responses from being
/// byte-stable. Values *within* a category keep their stated order — the map
/// is the trait set, the `Vec` is what the metadata said.
///
/// The wire format is unchanged (`{"Category": ["Value", …]}`), so data
/// written in the `HashMap` era still deserializes; see the `Deserialize`
/// impl for the shapes accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = std::collections::HashMap<String, Vec<String>>))]
pub struct Traits(BTreeMap<String, Vec<String>>);

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

/// Serialize as the map itself — one shape, `Vec<String>` values, in the
/// map's canonical order.
impl Serialize for Traits {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Always serialize as HashMap<String, Vec<String>> to ensure consistency
        self.0.serialize(serializer)
    }
}

/// Custom deserializer to handle multiple formats:
/// - HashMap<String, Vec<String>> - new canonical format
/// - HashMap<String, TraitValue> - old format with Single/Multi enum
/// - HashMap<String, String> - simple string map (most common onchain)
///
/// Every shape lands in the ordered map, so persisted data from any era
/// keeps deserializing and simply normalises into canonical order.
impl<'de> Deserialize<'de> for Traits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TraitsFormat {
            // Try Vec<String> first (most specific)
            VecStrings(HashMap<String, Vec<String>>),
            // Then try plain String (very common for onchain data)
            PlainStrings(HashMap<String, String>),
            // Finally try TraitValue (legacy format)
            TraitValues(HashMap<String, TraitValue>),
        }

        let format = TraitsFormat::deserialize(deserializer)?;

        match format {
            TraitsFormat::VecStrings(map) => Ok(Traits::from(map)),
            TraitsFormat::PlainStrings(string_map) => {
                // Convert HashMap<String, String> to HashMap<String, Vec<String>>
                // Trim values to normalize data quality issues (trailing spaces, etc)
                let vec_map = string_map
                    .into_iter()
                    .map(|(k, v)| (k, vec![v.trim().to_string()]))
                    .collect();
                Ok(Traits(vec_map))
            }
            TraitsFormat::TraitValues(trait_values) => {
                // Convert old TraitValue format to new format
                let vec_map = trait_values
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect();
                Ok(Traits(vec_map))
            }
        }
    }
}

impl Traits {
    /// Create new empty traits
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Build from any iterator of `(category, values)` pairs — a map a record
    /// arrived in, a `collect()`, a filter of another set. The pairs are the
    /// input; the order is the map's own.
    pub fn from_map<I>(map: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<String>)>,
    {
        Self(map.into_iter().collect())
    }

    /// Get a reference to the inner ordered map.
    pub fn inner(&self) -> &BTreeMap<String, Vec<String>> {
        &self.0
    }

    /// Get a mutable reference to the inner ordered map.
    pub fn inner_mut(&mut self) -> &mut BTreeMap<String, Vec<String>> {
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
    type IntoIter = std::collections::btree_map::IntoIter<String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Traits {
    type Item = (&'a String, &'a Vec<String>);
    type IntoIter = std::collections::btree_map::Iter<'a, String, Vec<String>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<BTreeMap<String, Vec<String>>> for Traits {
    fn from(map: BTreeMap<String, Vec<String>>) -> Self {
        Self(map)
    }
}

impl From<HashMap<String, Vec<String>>> for Traits {
    fn from(map: HashMap<String, Vec<String>>) -> Self {
        Self(map.into_iter().collect())
    }
}

impl From<Traits> for BTreeMap<String, Vec<String>> {
    fn from(traits: Traits) -> Self {
        traits.0
    }
}

impl From<Traits> for HashMap<String, Vec<String>> {
    fn from(traits: Traits) -> Self {
        traits.0.into_iter().collect()
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
        Traits::from_map(self.into_iter().map(|(k, v)| {
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
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the compatibility contract: every persisted shape still loads ---

    #[test]
    fn deserializes_the_canonical_map() {
        let traits: Traits =
            serde_json::from_str(r#"{"Eyes": ["Feline", "Scar"], "Background": ["Black"]}"#)
                .expect("canonical map deserializes");
        assert_eq!(traits.get_single("Background").as_deref(), Some("Black"));
        assert_eq!(
            traits.get("Eyes"),
            Some(&vec!["Feline".to_string(), "Scar".to_string()])
        );
    }

    #[test]
    fn deserializes_a_plain_string_map() {
        let traits: Traits =
            serde_json::from_str(r#"{"Background": "Black "}"#).expect("string map deserializes");
        // The string arm trims — the same normalisation it always applied.
        assert_eq!(traits.get_single("Background").as_deref(), Some("Black"));
    }

    #[test]
    fn a_legacy_trait_value_map_converts() {
        let legacy = HashMap::from([
            (
                "Background".to_string(),
                TraitValue::Single("Black".to_string()),
            ),
            (
                "Eyes".to_string(),
                TraitValue::Multi(vec!["Feline".to_string(), "Scar".to_string()]),
            ),
        ]);
        let traits: Traits = legacy.into();
        assert_eq!(traits.get_single("Eyes").as_deref(), Some("Feline, Scar"));
    }

    // ---- order is canonical, values are not ---------------------------------

    #[test]
    fn categories_are_canonical_regardless_of_insertion_order() {
        let mut traits = Traits::new();
        traits.insert_single("Zebra".to_string(), "1".to_string());
        traits.insert_single("alpha".to_string(), "2".to_string());
        traits.insert_single("Background".to_string(), "3".to_string());

        let order: Vec<&str> = traits.keys().map(String::as_str).collect();
        assert_eq!(order, vec!["Background", "Zebra", "alpha"]);

        // ...and serialisation agrees with iteration.
        let json = serde_json::to_string(&traits).expect("serializes");
        assert_eq!(json, r#"{"Background":["3"],"Zebra":["1"],"alpha":["2"]}"#);
    }

    #[test]
    fn values_keep_their_stated_order() {
        let traits: Traits =
            serde_json::from_str(r#"{"Eyes": ["Scar", "Feline"]}"#).expect("deserializes");
        assert_eq!(
            traits.get("Eyes"),
            Some(&vec!["Scar".to_string(), "Feline".to_string()])
        );
    }

    #[test]
    fn a_round_trip_is_stable() {
        let traits: Traits = serde_json::from_str(r#"{"Eyes": ["Scar"], "Background": ["Black"]}"#)
            .expect("deserializes");
        let json = serde_json::to_string(&traits).expect("serializes");
        assert_eq!(json, r#"{"Background":["Black"],"Eyes":["Scar"]}"#);
    }
}
