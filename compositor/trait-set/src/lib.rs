//! The trait catalogue: the traits a collection is built from, the named values
//! each may take, and the art that fills them.
//!
//! A [`TraitSet`] is what a studio hands a compositor, and what a compositor
//! needs before it can render: the base everything is worn on, and for every trait,
//! which values exist, what each is worth, and where its art lives. The unit is
//! **components, not tokens** — nothing here describes a finished piece.
//!
//! A full-canvas backdrop is an ordinary trait. The one thing that is *not* a trait is
//! the [`TraitSet::anchor`] — the subject the traits are worn on — because a trait is
//! what the collection varies and the subject is what it does not.
//!
//! ## What is deliberately absent
//!
//! **No constraints.** Linking, mutual exclusion, variant flow, dependencies and
//! none-percentages are the *evaluator's* vocabulary, and they stay with the
//! solver that enforces them. This crate says what exists and what it weighs;
//! which of it may combine is a question the solver answers. A catalogue that
//! grew those would be a second, competing collection config — the drift this
//! crate exists to prevent.
//!
//! **No pixels.** An [`ArtRef`] is a pointer, and the crate is `serde` only, so
//! every runtime can hold a catalogue without a codec between them.
//!
//! ## Neighbours, not to be merged
//!
//! - `cardano_assets::Traits` is the *instance*: the values one minted token
//!   actually got. This is the domain those values are drawn from.
//! - `mint_manifest` describes a *mint-ready piece*. This is the input to the
//!   selection that produces one.
//!
//! ## Datum-shaped
//!
//! Small, versioned and deterministic: field order is the serialisation order,
//! defaults are skipped, and nothing nests where a `project.toml` could not carry
//! it. A catalogue can therefore ride a datum later without a schema rewrite.
//! Nothing here knows about chains; it just declines to make that impossible.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Bumped when a stored catalogue would be read wrongly by old code.
///
/// Carried in the encoding rather than inferred, because the first consumer of an
/// old catalogue is a compositor that has to decide whether it can trust it.
pub const SCHEMA_VERSION: u16 = 1;

/// A region of the canvas, as `[x0, y0, x1, y1]` fractions of width and height.
///
/// The far corner rather than a size, because that is the shape the compositor's
/// own `mask_box` takes — so a catalogue and a `project.toml` describe the same
/// region in the same words. A producer holding `x/y/w/h` adds the width and
/// height when it writes.
pub type Region = [f32; 4];

/// Whether a region is a real area of the canvas: inside it, finite, non-empty.
fn is_region([x0, y0, x1, y1]: Region) -> bool {
    [x0, y0, x1, y1]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        && x0 < x1
        && y0 < y1
}

/// A content address on the wire: lower-case hex.
///
/// The hash stays a `u64` in memory — it is a name, and arithmetic on one is meaningless —
/// but it cannot travel as a *number*: TOML integers are signed 64-bit and a real hash
/// fills the unsigned range. A harness run produced `10033872954211784963`, which TOML
/// answers with `OutOfRange("u64")` — so every catalogue carrying real art failed to
/// write, while the crate's own tests passed on invented eight-bit hashes.
///
/// Hex is also what the house already writes for a digest: see `meme_layout::ImageRef`.
mod hash_hex {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(hash: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{hash:016x}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)
    }
}

/// Where a value's art lives.
///
/// Two shapes because there are two producers: a studio addresses art by content
/// in its own store, while a laid-out collection has files. One type rather than
/// one per producer, so a consumer writes one match.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtRef {
    /// Content address in whichever store the producer keeps, as lower-case hex.
    Hash(#[serde(with = "hash_hex")] u64),
    /// Path in the consumer's asset tree, relative to the collection root.
    Path(String),
}

/// One named value a trait may take, and what it is worth.
///
/// A value may exist before its art does — that is the ordinary state of a
/// catalogue being worked on, and the reason [`Self::art`] is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitValue {
    /// The value's name, and its identity. A compositor uses it as the asset
    /// filename and as the trait value in metadata, so renaming it is a different
    /// value rather than a relabelled one.
    pub name: String,
    /// Relative likelihood within the trait. Higher is more common.
    #[serde(default = "default_weight", skip_serializing_if = "is_default_weight")]
    pub weight: f32,
    /// How to make this one, when it differs from the trait's own prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The art, or `None` while the value is only declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<ArtRef>,
}

fn default_weight() -> f32 {
    1.0
}

fn is_default_weight(weight: &f32) -> bool {
    *weight == default_weight()
}

/// One trait: something the collection varies, and the values it can take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trait {
    /// Stable name, and the metadata trait name. Renaming one is a different
    /// trait, not a relabelled one.
    pub name: String,
    /// What this trait *is*, in words, for a reader that has not seen the
    /// picture — "the hat worn on the head". Not a prompt and not a routing tag:
    /// it is the thing a model or an artist needs in order to contribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Where on the canvas this trait lives, for generation guidance. `None` for
    /// a trait that is not region-bound, such as a whole-canvas backdrop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Region>,
    /// The standing instruction for making this trait's values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The values this trait may take. Empty is a normal state: a catalogue
    /// declares its traits before anything has been riffed into them.
    #[serde(default, rename = "value")]
    pub values: Vec<TraitValue>,
}

impl Trait {
    pub fn value_by_name(&self, name: &str) -> Option<&TraitValue> {
        self.values.iter().find(|value| value.name == name)
    }
}

/// A collection's traits, listed back to front.
///
/// The order of [`Self::traits`] *is* the composite order — there is no z-index
/// field, because a list that is already ordered does not need one, and an
/// override beside the list is an invitation for the two to disagree. Reordering
/// is reordering the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitSet {
    /// See [`SCHEMA_VERSION`].
    pub schema: u16,
    /// Slug. What a caller types to ask for this set.
    pub name: String,
    /// Human title, for saying what was made.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The square canvas edge every region is a fraction of. A resolution rather
    /// than a shape, because regions are normalized.
    pub canvas: u32,
    /// The base every trait is worn on, where the collection has a fixed one.
    ///
    /// Not a trait, and deliberately not expressed as one: a trait is something the
    /// collection *varies*, and the subject is the thing that does not vary — every
    /// token is the same character with different traits on it. Reading it as a slot
    /// would make it a value the solver could pick, which is how a collection ends up
    /// with two subjects in one token.
    ///
    /// Absent while a collection is being worked on and the frame has not been made
    /// yet, which is why it is optional rather than required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<ArtRef>,
    /// `[[trait]]` in TOML, as the compositor's own project config spells it.
    #[serde(default, rename = "trait")]
    pub traits: Vec<Trait>,
}

impl TraitSet {
    pub fn new(name: impl Into<String>, title: impl Into<String>, canvas: u32) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            name: name.into(),
            title: title.into(),
            canvas,
            anchor: None,
            traits: Vec::new(),
        }
    }

    pub fn trait_by_name(&self, name: &str) -> Option<&Trait> {
        self.traits.iter().find(|t| t.name == name)
    }

    /// Every problem that would make this catalogue read wrongly.
    ///
    /// Checked rather than trusted: a catalogue can arrive from an editor, a
    /// hand-written file, or a model — and the failures it prevents are invisible
    /// downstream, where the art still arrives and is simply wrong. Two traits
    /// sharing a name means the first shadows the second; two values sharing one
    /// is one PNG over another.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();

        if self.schema > SCHEMA_VERSION {
            problems.push(format!(
                "catalogue is schema {} but this build understands {SCHEMA_VERSION}",
                self.schema
            ));
        }
        if self.name.trim().is_empty() {
            problems.push("catalogue has no name".to_string());
        }
        if self.canvas == 0 {
            problems.push("catalogue canvas has no size".to_string());
        }

        let mut named = HashSet::new();
        for (position, t) in self.traits.iter().enumerate() {
            if t.name.trim().is_empty() {
                problems.push(format!("trait {position} has no name"));
            } else if !named.insert(t.name.as_str()) {
                problems.push(format!(
                    "`{}` is declared twice, and a reader would only ever find the first",
                    t.name
                ));
            }

            if let Some(rect) = t.rect
                && !is_region(rect)
            {
                problems.push(format!(
                    "trait `{}` has the region {rect:?}, which is not an area of the canvas",
                    t.name
                ));
            }

            let mut values = HashSet::new();
            for value in &t.values {
                if value.name.trim().is_empty() {
                    problems.push(format!("trait `{}` has a value with no name", t.name));
                } else if !values.insert(value.name.as_str()) {
                    problems.push(format!(
                        "trait `{}` names `{}` twice, and the two would share one filename",
                        t.name, value.name
                    ));
                }

                if !value.weight.is_finite() || value.weight < 0.0 {
                    problems.push(format!(
                        "trait `{}` value `{}` has weight {}",
                        t.name, value.name, value.weight
                    ));
                }
            }
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headwear() -> Trait {
        Trait {
            name: "headwear".to_string(),
            role: Some("the hat worn on the head".to_string()),
            rect: Some([0.30, 0.05, 0.70, 0.30]),
            prompt: Some("headwear".to_string()),
            values: vec![
                TraitValue {
                    name: "Beanie".to_string(),
                    weight: 3.0,
                    prompt: None,
                    art: Some(ArtRef::Hash(42)),
                },
                TraitValue {
                    name: "Top Hat".to_string(),
                    weight: 1.0,
                    prompt: Some("a tall black top hat".to_string()),
                    art: None,
                },
            ],
        }
    }

    fn catalogue() -> TraitSet {
        let mut set = TraitSet::new("degen-dragon", "Degen Dragons", 1024);
        set.anchor = Some(ArtRef::Hash(9));
        set.traits.push(headwear());
        set
    }

    /// The point of the crate: a catalogue is declared, then filled. A trait with
    /// no values and a value with no art are ordinary states, not errors, or the
    /// studio could not hold a plan it has not executed yet.
    #[test]
    fn a_catalogue_is_valid_before_any_art_exists() {
        let mut set = TraitSet::new("empty", "Empty", 512);
        set.traits.push(Trait {
            name: "headwear".to_string(),
            role: None,
            rect: None,
            prompt: None,
            values: vec![TraitValue {
                name: "Beanie".to_string(),
                weight: 1.0,
                prompt: None,
                art: None,
            }],
        });

        assert_eq!(set.validate(), Ok(()));
    }

    #[test]
    fn a_catalogue_round_trips_through_json() {
        let set = catalogue();
        let json = serde_json::to_string(&set).expect("a catalogue serialises");
        let back = serde_json::from_str::<TraitSet>(&json).expect("and deserialises");
        assert_eq!(back, set);
    }

    /// The property that matters is not that it round-trips through TOML, but
    /// that it *can sit in a `project.toml`-shaped world at all* — which is where
    /// the compositor will read it.
    #[test]
    fn a_catalogue_round_trips_through_toml() {
        let set = catalogue();
        let text = toml::to_string(&set).expect("a catalogue serialises");
        let back = toml::from_str::<TraitSet>(&text).expect("and deserialises");
        assert_eq!(back, set, "{text}");
    }

    /// Field order is the serialisation order and a default is not written, so the
    /// encoding is stable enough to compare — which is what makes a stored
    /// catalogue safe to trust.
    #[test]
    fn the_encoding_is_the_field_order_with_defaults_skipped() {
        let mut set = catalogue();
        set.traits[0].values[1].weight = 1.0;

        let text = toml::to_string(&set).expect("a catalogue serialises");
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle} in:\n{text}"))
        };

        assert!(at("schema") < at("name"));
        assert!(at("name") < at("canvas"));
        assert!(at("canvas") < at("anchor"), "{text}");
        assert!(at("anchor") < at("[[trait]]"), "{text}");
        assert!(at("[[trait]]") < at("[[trait.value]]"), "{text}");
        assert!(
            at("title =") < at("canvas"),
            "a title that is set is written:\n{text}"
        );
        assert_eq!(
            text.matches("weight =").count(),
            1,
            "only the weight that differs from the default is written:\n{text}"
        );

        set.title = String::new();
        let text = toml::to_string(&set).expect("a catalogue serialises");
        assert!(
            !text.contains("title ="),
            "an empty title is not written:\n{text}"
        );
    }

    /// The base is what makes the traits placeable at all, so it travels with them —
    /// and it is not a trait, because a trait is what the collection varies.
    #[test]
    fn the_base_travels_with_the_traits_and_is_not_one_of_them() {
        let set = catalogue();

        assert_eq!(set.anchor, Some(ArtRef::Hash(9)));
        assert!(
            set.traits.iter().all(|t| t.name != "anchor"),
            "the base is a field, not a trait"
        );

        let json = serde_json::to_string(&set).expect("a catalogue serialises");
        let back = serde_json::from_str::<TraitSet>(&json).expect("and deserialises");
        assert_eq!(back.anchor, Some(ArtRef::Hash(9)), "{json}");
    }

    /// A collection whose frame has not been made yet is still a catalogue — the traits
    /// and their regions are declared first, and the base arrives when it does.
    #[test]
    fn a_catalogue_without_a_base_writes_no_anchor() {
        let mut set = catalogue();
        set.anchor = None;

        let text = toml::to_string(&set).expect("a catalogue serialises");
        assert_eq!(set.validate(), Ok(()), "an absent base is a normal state");
        assert!(
            !text.contains("anchor"),
            "nothing to say, so nothing said:\n{text}"
        );
    }

    /// A hash that fills the *unsigned* 64-bit range, which is the half a signed TOML
    /// integer cannot hold. This is the value a harness run produced, and the reason every
    /// catalogue carrying real art failed to write while the rest of these tests passed on
    /// invented hashes of one or two digits.
    const BIG: u64 = 10_033_872_954_211_784_963;

    /// A catalogue carrying real art has to survive the compositor's own format, which is
    /// TOML — and TOML integers are signed. The hash travels as hex, so it does not matter
    /// which half of the range it lands in.
    #[test]
    fn a_hash_too_big_for_a_toml_integer_still_round_trips() {
        let mut set = TraitSet::new("harness-fills", "", 1024);
        set.anchor = Some(ArtRef::Hash(BIG));
        set.traits = vec![
            Trait {
                name: "headwear".into(),
                role: None,
                rect: Some([0.24, 0.03, 0.76, 0.25]),
                prompt: Some("A hat.".into()),
                values: vec![
                    TraitValue {
                        name: "Beanie".into(),
                        weight: 1.0,
                        prompt: None,
                        art: Some(ArtRef::Hash(BIG)),
                    },
                    TraitValue {
                        name: "Top Hat".into(),
                        weight: 1.0,
                        prompt: None,
                        art: Some(ArtRef::Hash(2)),
                    },
                ],
            },
            // A trait nothing has been riffed into yet: the empty half of a real projection,
            // and the shape that follows values carrying an inline table.
            Trait {
                name: "collar".into(),
                role: None,
                rect: Some([0.18, 0.57, 0.82, 0.81]),
                prompt: Some("A collar.".into()),
                values: Vec::new(),
            },
        ];

        let text = toml::to_string(&set).expect("a hash too big for an i64 still writes");
        assert!(
            text.contains(&format!("{BIG:016x}")),
            "the address travels as hex, not as a number TOML cannot hold:\n{text}"
        );

        let back: TraitSet = toml::from_str(&text).expect("and parses back");
        assert_eq!(back, set, "{text}");
    }

    /// A catalogue is a plan, and the traps are silent: the art still arrives, it
    /// is just the wrong art. Two values in one trait would share a filename.
    #[test]
    fn two_values_that_would_share_a_filename_are_reported() {
        let mut set = catalogue();
        set.traits[0].values.push(TraitValue {
            name: "Beanie".to_string(),
            weight: 1.0,
            prompt: None,
            art: None,
        });

        let problems = set.validate().expect_err("the clash is a problem");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("Beanie"), "{problems:?}");
    }

    #[test]
    fn a_trait_declared_twice_is_reported() {
        let mut set = catalogue();
        set.traits.push(headwear());

        let problems = set.validate().expect_err("the shadow is a problem");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("headwear"), "{problems:?}");
    }

    #[test]
    fn a_region_that_is_not_an_area_of_the_canvas_is_reported() {
        let mut set = catalogue();

        for rect in [
            [0.9, 0.1, 1.2, 0.3],
            [0.5, 0.1, 0.5, 0.3],
            [-0.1, 0.0, 0.2, 0.2],
        ] {
            set.traits[0].rect = Some(rect);
            let problems = set.validate().expect_err("the region is a problem");
            assert_eq!(problems.len(), 1, "{rect:?}: {problems:?}");
            assert!(problems[0].contains("headwear"), "{problems:?}");
        }
    }

    #[test]
    fn a_catalogue_from_the_future_is_reported() {
        let mut set = catalogue();
        set.schema = SCHEMA_VERSION + 1;

        let problems = set.validate().expect_err("an unknown schema is a problem");
        assert!(problems[0].contains("schema"), "{problems:?}");
    }

    /// Order is the list, so the values a compositor stacks are the order the
    /// studio declared — there is no second place for it to disagree with.
    #[test]
    fn the_list_is_the_composite_order() {
        let mut set = catalogue();
        set.traits.push(Trait {
            name: "background".to_string(),
            role: None,
            rect: None,
            prompt: None,
            values: Vec::new(),
        });

        assert_eq!(
            set.traits
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["headwear", "background"]
        );
        assert_eq!(set.trait_by_name("headwear").unwrap().values.len(), 2);
    }
}
