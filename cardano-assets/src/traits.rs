use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// One value a trait may take.
///
/// `Plain` is every value stated directly ("Black", "Royal Thor") — and every
/// value already on disk deserializes into it. `Altered` is a value the
/// metadata stated as a base plus an alteration — GOOFS World's
/// `{"trait": "Puffy Jacket", "alteration": "Green"}` attribute objects — kept
/// as a pair in the model and rendered as `"<value> (<alteration>)"`: the form
/// the market shows (`"bath robe (the dude)"` in jpg.store's preserved
/// taxonomy).
///
/// The pair is one value: one filter entry under its category, scored as one
/// value by rarity, exactly how the marketplaces present it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitValue {
    /// The value as stated.
    Plain(String),
    /// A base value with an alteration applied.
    Altered {
        /// The base value, e.g. `Puffy Jacket`.
        value: String,
        /// What the base was altered to, e.g. `Green`.
        alteration: String,
    },
}

impl TraitValue {
    /// The value as a single string. `Altered` renders as
    /// `"<value> (<alteration>)"` — the market's format, and the form the
    /// value takes anywhere it leaves this crate (JSON, D1 rows, bundles).
    #[must_use]
    pub fn rendered(&self) -> String {
        match self {
            Self::Plain(value) => value.clone(),
            Self::Altered { value, alteration } => format!("{value} ({alteration})"),
        }
    }
}

impl Display for TraitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain(value) => write!(f, "{value}"),
            Self::Altered { value, alteration } => write!(f, "{value} ({alteration})"),
        }
    }
}

/// Compare against the value's rendered string: `Plain("Black")` equals
/// `"Black"`, and an altered value equals its `"<value> (<alteration>)"`
/// form. The comparison is the wire's — what a JSON consumer sees is what the
/// value equals — and it keeps every per-collection test that pins a trait
/// value as the string the market shows meaning exactly that, untouched.
impl PartialEq<String> for TraitValue {
    fn eq(&self, other: &String) -> bool {
        self.rendered() == *other
    }
}

impl PartialEq<&str> for TraitValue {
    fn eq(&self, other: &&str) -> bool {
        self.rendered() == *other
    }
}

/// Serialize as the rendered string: the wire keeps one shape, and a pair
/// travels as `"<value> (<alteration>)"`, the form every marketplace shows.
impl Serialize for TraitValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.rendered())
    }
}

/// Deserialize from the shapes that exist in the wild:
///
/// - a string — every value written so far: `"Black"`;
/// - a pair object — `{"value": "Puffy Jacket", "alteration": "Green"}`, with
///   `trait` accepted as the base key (the on-chain spelling) and
///   `alteration` optional (`{"value": "X"}` is `Plain("X")`).
impl<'de> Deserialize<'de> for TraitValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ValueFormat {
            Plain(String),
            // A map, deliberately — not a struct. Serde reads a struct from a
            // *sequence* positionally, so a value of `["a", "b"]` would match
            // a two-field pair struct and silently become an alteration.
            Pair(serde_json::Map<String, serde_json::Value>),
        }

        match ValueFormat::deserialize(deserializer)? {
            ValueFormat::Plain(value) => Ok(Self::Plain(value.trim().to_string())),
            ValueFormat::Pair(mut pair) => {
                let base = pair
                    .remove("value")
                    .or_else(|| pair.remove("trait"))
                    .ok_or_else(|| D::Error::custom("trait value object has no `value`/`trait`"))?;
                let base = base
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| D::Error::custom("trait `value`/`trait` is not a string"))?;
                let alteration = pair
                    .get("alteration")
                    .and_then(|a| a.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                Ok(match alteration {
                    Some(alteration) => Self::Altered {
                        value: base.to_string(),
                        alteration: alteration.to_string(),
                    },
                    None => Self::Plain(base.to_string()),
                })
            }
        }
    }
}

/// The canonical traits type: a list of [`TraitValue`]s per category, ordered.
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
/// The wire format is unchanged (`{"Category": ["Value", …]}`): an `Altered`
/// value serializes as its rendered string, so consumers that only ever see
/// JSON see what they saw before, and data written in any earlier era
/// deserializes (into `Plain`); see the `Deserialize` impl for the shapes
/// accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = std::collections::HashMap<String, Vec<String>>))]
pub struct Traits(BTreeMap<String, Vec<TraitValue>>);

impl Display for Traits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| {
                let values: Vec<String> = v.iter().map(TraitValue::rendered).collect();
                format!("{}: {}", k, values.join(", "))
            })
            .collect();
        write!(f, "{{{}}}", formatted.join(", "))
    }
}

/// Serialize as the map itself — one shape, values rendered to strings, in the
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

/// Custom deserializer: every shape that exists in the wild, one arm.
///
/// A category's value is either one value or a list of them — and in a mixed
/// map (some categories scalar, some arrays) each is read independently. A
/// value itself is a plain string or a pair object. Everything lands in the
/// ordered map, normalising into one shape; data written in any earlier era
/// keeps deserializing.
impl<'de> Deserialize<'de> for Traits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ValuesFormat {
            One(TraitValue),
            Many(Vec<TraitValue>),
        }

        let map: HashMap<String, ValuesFormat> = HashMap::deserialize(deserializer)?;
        Ok(Traits(
            map.into_iter()
                .map(|(key, values)| {
                    let values = match values {
                        ValuesFormat::One(value) => vec![value],
                        ValuesFormat::Many(values) => values,
                    };
                    (key, values)
                })
                .collect(),
        ))
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
        I: IntoIterator<Item = (String, Vec<TraitValue>)>,
    {
        Self(map.into_iter().collect())
    }

    /// Get a reference to the inner ordered map.
    pub fn inner(&self) -> &BTreeMap<String, Vec<TraitValue>> {
        &self.0
    }

    /// Get a mutable reference to the inner ordered map.
    pub fn inner_mut(&mut self) -> &mut BTreeMap<String, Vec<TraitValue>> {
        &mut self.0
    }

    /// Insert a trait with a single plain value.
    pub fn insert_single(&mut self, key: String, value: String) {
        self.0.insert(key, vec![TraitValue::Plain(value)]);
    }

    /// Insert a trait with multiple plain values.
    pub fn insert_multi(&mut self, key: String, values: Vec<String>) {
        self.0
            .insert(key, values.into_iter().map(TraitValue::Plain).collect());
    }

    /// Insert one value — plain or altered.
    pub fn insert(&mut self, key: String, value: TraitValue) {
        self.0.insert(key, vec![value]);
    }

    /// Insert values directly.
    pub fn insert_vec(&mut self, key: String, values: Vec<TraitValue>) {
        self.0.insert(key, values);
    }

    /// Get a category's values.
    pub fn get(&self, key: &str) -> Option<&Vec<TraitValue>> {
        self.0.get(key)
    }

    /// Get a category's values rendered to one string (joins multiple values
    /// with ", "), as every JSON-shaped consumer sees them.
    pub fn get_single(&self, key: &str) -> Option<String> {
        self.0.get(key).map(|values| {
            let rendered: Vec<String> = values.iter().map(TraitValue::rendered).collect();
            rendered.join(", ")
        })
    }

    /// Check if trait exists
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Iterate over traits
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<TraitValue>)> {
        self.0.iter()
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// Get all values
    pub fn values(&self) -> impl Iterator<Item = &Vec<TraitValue>> {
        self.0.values()
    }
}

impl Default for Traits {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for Traits {
    type Item = (String, Vec<TraitValue>);
    type IntoIter = std::collections::btree_map::IntoIter<String, Vec<TraitValue>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Traits {
    type Item = (&'a String, &'a Vec<TraitValue>);
    type IntoIter = std::collections::btree_map::Iter<'a, String, Vec<TraitValue>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<BTreeMap<String, Vec<TraitValue>>> for Traits {
    fn from(map: BTreeMap<String, Vec<TraitValue>>) -> Self {
        Self(map)
    }
}

impl From<HashMap<String, Vec<TraitValue>>> for Traits {
    fn from(map: HashMap<String, Vec<TraitValue>>) -> Self {
        Self(map.into_iter().collect())
    }
}

impl From<BTreeMap<String, Vec<String>>> for Traits {
    fn from(map: BTreeMap<String, Vec<String>>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (k, v.into_iter().map(TraitValue::Plain).collect()))
                .collect(),
        )
    }
}

impl From<HashMap<String, Vec<String>>> for Traits {
    fn from(map: HashMap<String, Vec<String>>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (k, v.into_iter().map(TraitValue::Plain).collect()))
                .collect(),
        )
    }
}

impl From<Traits> for BTreeMap<String, Vec<TraitValue>> {
    fn from(traits: Traits) -> Self {
        traits.0
    }
}

/// The render seam: a `Traits` becomes the string-shaped map every JSON
/// consumer (and the trait-schema bitmap encoder) reads.
impl From<Traits> for BTreeMap<String, Vec<String>> {
    fn from(traits: Traits) -> Self {
        traits
            .0
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|value| value.rendered()).collect()))
            .collect()
    }
}

/// The render seam, `HashMap` spelling — this is the conversion
/// `collection-ownership`'s `parse_asset_metadata` uses.
impl From<Traits> for HashMap<String, Vec<String>> {
    fn from(traits: Traits) -> Self {
        traits
            .0
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|value| value.rendered()).collect()))
            .collect()
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

            (
                k.into(),
                values.into_iter().map(TraitValue::Plain).collect(),
            )
        }))
    }
}

/// If you need a flat HashMap<String, String> (joins multi-values with ", ")
pub fn flatten_traits(traits: &Traits) -> HashMap<String, String> {
    traits
        .iter()
        .map(|(k, v)| {
            let rendered: Vec<String> = v.iter().map(TraitValue::rendered).collect();
            (k.clone(), rendered.join(", "))
        })
        .collect()
}

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
            Some(&vec![
                TraitValue::Plain("Feline".to_string()),
                TraitValue::Plain("Scar".to_string())
            ])
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
    fn a_pair_value_deserializes_keeps_its_parts_and_renders() {
        let traits: Traits = serde_json::from_str(
            r#"{"Overcoat": [{"value": "Puffy Jacket", "alteration": "Green"}], "Eyes": ["Scar"]}"#,
        )
        .expect("pair shape deserializes");

        assert_eq!(
            traits.get("Overcoat"),
            Some(&vec![TraitValue::Altered {
                value: "Puffy Jacket".to_string(),
                alteration: "Green".to_string(),
            }])
        );
        assert_eq!(
            traits.get_single("Overcoat").as_deref(),
            Some("Puffy Jacket (Green)")
        );
        // The on-chain spelling (`trait`) is accepted as the base key.
        let onchain: Traits = serde_json::from_str(
            r#"{"Overcoat": {"trait": "Puffy Jacket", "alteration": "Green"}}"#,
        )
        .expect("datum-shaped pair deserializes");
        assert_eq!(
            onchain.get_single("Overcoat").as_deref(),
            Some("Puffy Jacket (Green)")
        );
    }

    #[test]
    fn an_alteration_equal_to_the_value_is_kept() {
        // jpg.store keeps the parenthetical even when the two are equal
        // ("latte (latte)"), so fidelity means we do too.
        let traits: Traits =
            serde_json::from_str(r#"{"Fur Color": [{"trait": "Latte", "alteration": "Latte"}]}"#)
                .expect("deserializes");
        assert_eq!(
            traits.get_single("Fur Color").as_deref(),
            Some("Latte (Latte)")
        );
    }

    #[test]
    fn an_empty_alteration_is_a_plain_value() {
        let traits: Traits =
            serde_json::from_str(r#"{"Overcoat": [{"trait": "Puffy Jacket", "alteration": " "}]}"#)
                .expect("deserializes");
        assert_eq!(
            traits.get("Overcoat"),
            Some(&vec![TraitValue::Plain("Puffy Jacket".to_string())])
        );
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
            Some(&vec![
                TraitValue::Plain("Scar".to_string()),
                TraitValue::Plain("Feline".to_string())
            ])
        );
    }

    #[test]
    fn a_round_trip_is_stable() {
        let traits: Traits = serde_json::from_str(r#"{"Eyes": ["Scar"], "Background": ["Black"]}"#)
            .expect("deserializes");
        let json = serde_json::to_string(&traits).expect("serializes");
        assert_eq!(json, r#"{"Background":["Black"],"Eyes":["Scar"]}"#);
    }

    /// An altered value leaves the crate as its rendered string — the wire
    /// shape every JSON consumer (D1 rows, bundles, `AssetV2`) already reads.
    #[test]
    fn altered_values_render_into_the_string_shaped_maps() {
        let mut traits = Traits::new();
        traits.insert(
            "Overcoat".to_string(),
            TraitValue::Altered {
                value: "Puffy Jacket".to_string(),
                alteration: "Green".to_string(),
            },
        );
        traits.insert_single("Background".to_string(), "Black".to_string());

        let rendered: HashMap<String, Vec<String>> = traits.clone().into();
        assert_eq!(
            rendered.get("Overcoat"),
            Some(&vec!["Puffy Jacket (Green)".to_string()])
        );

        let json = serde_json::to_string(&traits).expect("serializes");
        assert_eq!(
            json,
            r#"{"Background":["Black"],"Overcoat":["Puffy Jacket (Green)"]}"#
        );
    }
}
