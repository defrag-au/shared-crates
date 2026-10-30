//! The asset's content changed.
//!
//! What the two sources have in common is a **before/after hash**, not a document:
//! Cardano's change is a burn-and-remint of the CIP-68 label-100 reference twin, and
//! OpenSea sends the whole current metadata and never says what changed. So the event
//! says which fingerprint was current and when, and the document stays in
//! `ASSET_METADATA_INDEX`.
//!
//! Two hashes are only comparable if they were computed the same way, and they are not
//! comparable across chains — a datum hash and a JSON fingerprint are different
//! algorithms. Hence [`HashAlgo`] on every event.

use serde::ser::Serializer;
use serde::{Deserialize, Deserializer, Serialize};

use crate::id::{self, OFFSET};

/// The wire name for the fingerprint this crate computes.
pub const IMAGE_METADATA_V1: &str = "image_metadata_v1";

/// The wire name for a hash over a CIP-68 reference-twin datum.
pub const CIP68_DATUM_V1: &str = "cip68_datum_v1";

/// One content change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetEvent {
    /// Which attribute changed.
    pub kind: AssetKind,
    /// Which attribute, when the kind is not the whole document.
    pub attribute: Option<String>,
    /// How `after_hash` (and `before_hash`, when present) were computed.
    pub hash_algo: HashAlgo,
    /// What was current before, when the source knows. `None` on the first set.
    pub before_hash: Option<String>,
    /// What is current from this event.
    pub after_hash: String,
}

impl AssetEvent {
    /// A change to `after_hash`, with no known before-state.
    pub fn new(kind: AssetKind, hash_algo: HashAlgo, after_hash: impl Into<String>) -> Self {
        Self {
            kind,
            attribute: None,
            hash_algo,
            before_hash: None,
            after_hash: after_hash.into(),
        }
    }

    /// The same change, stated as a transition.
    pub fn change(
        kind: AssetKind,
        hash_algo: HashAlgo,
        before_hash: impl Into<Option<String>>,
        after_hash: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            attribute: None,
            hash_algo,
            before_hash: before_hash.into(),
            after_hash: after_hash.into(),
        }
    }

    /// A stable string for [`EventId::derive`](crate::EventId::derive).
    ///
    /// The algorithm and the resulting hash together: the same bytes under two
    /// algorithms are two different facts, and only the pair identifies one.
    pub fn discriminator(&self) -> String {
        format!("{}:{}", self.hash_algo.as_str(), self.after_hash)
    }
}

/// Which attribute changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// The metadata document as a whole.
    Metadata,
    /// The image.
    Image,
    /// The token URI.
    Uri,
    /// The royalty policy.
    Royalty,
    /// The token standard.
    Standard,
}

impl AssetKind {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Image => "image",
            Self::Uri => "uri",
            Self::Royalty => "royalty",
            Self::Standard => "standard",
        }
    }
}

/// How a content hash was computed.
///
/// [`HashAlgo::Custom`] is the arm that stops a future algorithm being a parse error:
/// an event written before the algorithm was named still reads back.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HashAlgo {
    /// [`ImageMetadataV1`]'s fingerprint.
    ImageMetadataV1,
    /// A hash over a CIP-68 reference-twin datum.
    Cip68DatumV1,
    /// An algorithm this crate does not name, carried verbatim.
    Custom(String),
}

impl HashAlgo {
    /// The wire value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::ImageMetadataV1 => IMAGE_METADATA_V1,
            Self::Cip68DatumV1 => CIP68_DATUM_V1,
            Self::Custom(name) => name,
        }
    }
}

impl Serialize for HashAlgo {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HashAlgo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            IMAGE_METADATA_V1 => Self::ImageMetadataV1,
            CIP68_DATUM_V1 => Self::Cip68DatumV1,
            _ => Self::Custom(value),
        })
    }
}

/// The fingerprint input for [`HashAlgo::ImageMetadataV1`].
///
/// Deliberately **not** the whole document. Hashing everything a decoder happens to
/// parse means a release that adds a field re-hashes every item in the collection,
/// which reads as a redraw of every asset — a false event for the entire supply. This
/// is the subset the store acts on, in a fixed order, with traits sorted because the
/// feed does not promise an order and two consumers must agree.
///
/// The version is in the algorithm name (`image_metadata_v1`), so widening the input
/// later is a new name rather than a silent reinterpretation of old hashes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageMetadataV1<'a> {
    /// Where the image lives. Content-addressed on OpenSea's CDN, so it changes when
    /// the image does.
    pub image_url: Option<&'a str>,
    /// An animated form, when there is one.
    pub animation_url: Option<&'a str>,
    /// The token's own metadata document.
    pub metadata_url: Option<&'a str>,
    /// A declared background colour.
    pub background_color: Option<&'a str>,
    /// The complete trait set, in any order.
    pub traits: &'a [(&'a str, &'a str)],
}

impl ImageMetadataV1<'_> {
    /// The 32-character hex fingerprint.
    pub fn fingerprint(&self) -> String {
        let mut state = OFFSET;
        id::feed(&mut state, b"image_metadata_v1");
        id::feed_opt_str(&mut state, self.image_url);
        id::feed_opt_str(&mut state, self.animation_url);
        id::feed_opt_str(&mut state, self.metadata_url);
        id::feed_opt_str(&mut state, self.background_color);

        let mut traits = self.traits.to_vec();
        traits.sort_unstable();
        id::feed(&mut state, &(traits.len() as u64).to_le_bytes());
        for (trait_type, value) in traits {
            id::feed(&mut state, trait_type.as_bytes());
            id::feed(&mut state, value.as_bytes());
        }

        format!("{state:032x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits() -> [(&'static str, &'static str); 3] {
        [("Eyes", "Laser"), ("Hat", "Crown"), ("Body", "Chrome")]
    }

    #[test]
    fn the_fingerprint_does_not_depend_on_trait_order() {
        let forward = ImageMetadataV1 {
            image_url: Some("ipfs://a"),
            traits: &traits(),
            ..ImageMetadataV1::default()
        };
        let mut reversed = traits();
        reversed.reverse();
        let backward = ImageMetadataV1 {
            image_url: Some("ipfs://a"),
            traits: &reversed,
            ..ImageMetadataV1::default()
        };
        assert_eq!(forward.fingerprint(), backward.fingerprint());
    }

    #[test]
    fn a_changed_image_is_a_changed_fingerprint() {
        let before = ImageMetadataV1 {
            image_url: Some("ipfs://a"),
            ..ImageMetadataV1::default()
        };
        let after = ImageMetadataV1 {
            image_url: Some("ipfs://b"),
            ..ImageMetadataV1::default()
        };
        assert_ne!(before.fingerprint(), after.fingerprint());
    }

    #[test]
    fn a_changed_trait_value_is_a_changed_fingerprint() {
        let one = [("Hat", "Crown")];
        let two = [("Hat", "Beanie")];
        let before = ImageMetadataV1 {
            traits: &one,
            ..ImageMetadataV1::default()
        };
        let after = ImageMetadataV1 {
            traits: &two,
            ..ImageMetadataV1::default()
        };
        assert_ne!(before.fingerprint(), after.fingerprint());
    }

    #[test]
    fn a_trait_boundary_cannot_be_moved_into_a_collision() {
        // ("ab", "c") and ("a", "bc") concatenate to the same bytes without the
        // length prefix that `feed` adds.
        let one = [("ab", "c")];
        let two = [("a", "bc")];
        let before = ImageMetadataV1 {
            traits: &one,
            ..ImageMetadataV1::default()
        };
        let after = ImageMetadataV1 {
            traits: &two,
            ..ImageMetadataV1::default()
        };
        assert_ne!(before.fingerprint(), after.fingerprint());
    }

    #[test]
    fn an_absent_value_is_not_the_empty_one() {
        let absent = ImageMetadataV1 {
            image_url: None,
            ..ImageMetadataV1::default()
        };
        let empty = ImageMetadataV1 {
            image_url: Some(""),
            ..ImageMetadataV1::default()
        };
        assert_ne!(absent.fingerprint(), empty.fingerprint());
    }

    #[test]
    fn every_algorithm_round_trips_as_a_plain_string() {
        for algo in [
            HashAlgo::ImageMetadataV1,
            HashAlgo::Cip68DatumV1,
            HashAlgo::Custom("ipfs-cid-v1".to_owned()),
        ] {
            let json = serde_json::to_string(&algo).unwrap();
            assert_eq!(json, format!("\"{}\"", algo.as_str()));
            assert_eq!(serde_json::from_str::<HashAlgo>(&json).unwrap(), algo);
        }
    }

    #[test]
    fn a_metadata_event_serialises_its_algorithm_with_the_hash() {
        let event = AssetEvent::change(
            AssetKind::Metadata,
            HashAlgo::ImageMetadataV1,
            Some("00".repeat(16)),
            "11".repeat(16),
        );
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["hash_algo"], "image_metadata_v1");
        assert_eq!(json["kind"], "metadata");
        assert_eq!(json["before_hash"], "00".repeat(16));
    }
}
