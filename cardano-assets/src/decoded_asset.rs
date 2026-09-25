//! `DecodedAsset` — what a mint's metadata document says about an asset, with
//! the media kept and the marketplace state left out.
//!
//! # Why this exists beside `Asset`
//!
//! `Asset` is what a metadata document flattens to, and three things about the
//! flatten cost more than they were worth:
//!
//! - **It keeps one medium.** `AssetEnvelope` parses `files[]` — it has to, for
//!   the `mediaType` fallback — and `into_asset` discards the rest, so an asset
//!   with a still and a video arrives with the still only. The workaround is
//!   `AssetWithId::cids`, and it is narrower than the problem: CIDs are
//!   extracted from IPFS URLs only (`cid::extract_cids`, with `skips_http_image_urls`
//!   pinning it), so a `https://` or `data:` entry is dropped twice over.
//! - **`name: String` cannot say "the document has no name".** `Untitled`
//!   documents arrive as `""`, which is indistinguishable from a document that
//!   really does declare an empty name — and the crate's own comment on that
//!   variant says callers must fill the display name in from the `AssetId`.
//!   The on-chain name and the metadata name are two facts; `Option<String>`
//!   keeps them apart.
//! - **`rarity_rank` and `tags` are never set by a decoder.** Every
//!   `From<AssetMetadata>` arm and `into_asset` pass `None`/`vec![]`; on the
//!   API path they are filled from the cnft.io cache table, and rarity already
//!   has a first-class home in `EnrichmentRarityScore`. A decode type that
//!   carries them is a second home for one fact.
//!
//! # The media type is not a closed set
//!
//! Measured over a 20-chunk spread of mainnet (36,423 decoded assets): **seven**
//! distinct media types, 32.4% declaring none, and the tail includes
//! `image/jpg` — a misspelling of the standard `image/jpeg` that mints really
//! publish. So [`MediaType`] names no MIME types: it distinguishes *declared*
//! from *not declared*, and everything else is carried as written. A future
//! named variant is a `of` change, not a representation change — and it must go
//! through [`MediaType::of`], or one spelling of a value would have two homes,
//! which is the failure this crate keeps having to fix.

use crate::{AssetId, Traits};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The media type of a [`Media`] entry.
///
/// See the module docs for why the MIME types are not named here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum MediaType {
    /// The document declares no media type for this entry — the case
    /// `Option<String>` spelled as `None`, made a variant so a `match` has to
    /// say what it does with it.
    #[default]
    None,
    /// A media type carried as the document wrote it, misspellings included.
    Custom(String),
}

impl MediaType {
    /// Read a declared media type. An empty string is not a type.
    #[must_use]
    pub fn of(declared: &str) -> Self {
        if declared.is_empty() {
            MediaType::None
        } else {
            MediaType::Custom(declared.to_string())
        }
    }

    /// The declared type as written, or `None` when the document declares none.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MediaType::None => None,
            MediaType::Custom(declared) => Some(declared),
        }
    }

    /// Is there no declared type?
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, MediaType::None)
    }
}

/// On the wire a `MediaType` is the string the document wrote, and `None` is an
/// omitted field — which is what `Media` does with it, and what `Asset`'s
/// `Option<String>` already looked like. `AssetTag` set this precedent in-crate:
/// a string-valued enum renders through `Display`.
impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaType::None => Ok(()),
            MediaType::Custom(declared) => f.write_str(declared),
        }
    }
}

impl Serialize for MediaType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MediaType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let declared = String::deserialize(deserializer)?;
        Ok(MediaType::of(&declared))
    }
}

/// Where a [`Media`] entry sits in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRole {
    /// The asset's headline media — the document's `image`.
    Headline,
    /// An entry from `files[]`.
    File,
}

/// One medium of a decoded asset.
///
/// Field names follow the document's: `src` is CIP-25's `src`, and `name` is
/// CIP-25's `files[].name` — what the entry is *for* ("the still", "video"),
/// not the asset's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    /// The source, with CIP-25's >64-byte chunking resolved.
    pub src: String,
    /// The declared media type, if any.
    #[serde(default, skip_serializing_if = "MediaType::is_none")]
    pub media_type: MediaType,
    /// `files[].name` — the entry's role in words, when the document says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Headline or additional.
    pub role: MediaRole,
}

/// An asset as its metadata document describes it.
///
/// Not an `Asset`: the media list replaces `image`/`media_type`, `name` can be
/// absent, and the marketplace fields are gone. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedAsset {
    /// The on-chain identity — policy and raw asset name.
    pub id: AssetId,
    /// The document's `name`/`Name`/`title`, or `None` when it declares none.
    /// An empty string here means the document really declared one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The headline first, then `files[]` in document order.
    pub media: Vec<Media>,
    /// The collection-ownership trait set.
    pub traits: Traits,
}

impl DecodedAsset {
    /// The headline medium, if the document carries one.
    #[must_use]
    pub fn headline(&self) -> Option<&Media> {
        self.media.iter().find(|m| m.role == MediaRole::Headline)
    }

    /// The headline's source — the projection of `Asset::image`, and `None`
    /// where that field would have been `""`.
    #[must_use]
    pub fn image(&self) -> Option<&str> {
        self.headline().map(|m| m.src.as_str())
    }

    /// The headline's declared type — the projection of `Asset::media_type`.
    #[must_use]
    pub fn media_type(&self) -> MediaType {
        self.headline()
            .map_or(MediaType::None, |m| m.media_type.clone())
    }

    /// The additional media, in document order.
    pub fn files(&self) -> impl Iterator<Item = &Media> {
        self.media.iter().filter(|m| m.role == MediaRole::File)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AssetEnvelope;

    fn envelope(json: &str) -> AssetEnvelope {
        serde_json::from_str(json).expect("envelope")
    }

    fn id() -> AssetId {
        AssetId::new_unchecked("ab".repeat(28), hex::encode(b"AL0001"))
    }

    /// A declared type travels as written — including the misspelling mints
    /// really publish, which a closed enum would have to rewrite or reject.
    #[test]
    fn a_declared_type_is_carried_as_written() {
        for declared in ["image/png", "image/jpg", "application/octet-stream"] {
            let media = Media {
                src: "ipfs://x".to_string(),
                media_type: MediaType::of(declared),
                name: None,
                role: MediaRole::File,
            };
            let json = serde_json::to_string(&media).expect("serializes");
            assert!(json.contains(declared), "json was {json}");
            assert_eq!(serde_json::from_str::<Media>(&json).expect("round"), media);
        }
        assert_eq!(MediaType::of(""), MediaType::None);
    }

    /// `None` is an *omitted* field, so a `Media` with no declared type and one
    /// with `Custom("")` cannot both exist.
    #[test]
    fn no_declared_type_is_an_omitted_field() {
        let media = Media {
            src: "ipfs://x".to_string(),
            media_type: MediaType::None,
            name: None,
            role: MediaRole::Headline,
        };
        let json = serde_json::to_string(&media).expect("serializes");
        assert_eq!(json, r#"{"src":"ipfs://x","role":"headline"}"#);
        let back: Media = serde_json::from_str(&json).expect("round");
        assert!(back.media_type.is_none());
        assert_eq!(back.media_type.as_str(), None);
    }

    /// The headline is the `image`; `files[]` are the additional media; and a
    /// file the fallback consumed to type the headline is that same medium, not
    /// a second one — its name is folded in rather than dropped.
    #[test]
    fn the_headline_is_the_image_and_the_consumed_file_is_folded_in() {
        let asset = envelope(
            r#"{
                "name": "AL0001",
                "image": "ipfs://Qmstill",
                "files": [
                    { "mediaType": "image/png", "name": "the still", "src": "ipfs://Qmstill" },
                    { "mediaType": "video/mp4", "name": "the piece", "src": "ipfs://Qmvideo" }
                ]
            }"#,
        )
        .into_decoded_asset(id());

        assert_eq!(asset.media.len(), 2, "the consumed file is not repeated");
        let headline = asset.headline().expect("headline");
        assert_eq!(headline.src, "ipfs://Qmstill");
        assert_eq!(headline.media_type.as_str(), Some("image/png"));
        assert_eq!(headline.name.as_deref(), Some("the still"));
        assert_eq!(asset.image(), Some("ipfs://Qmstill"));
        assert_eq!(asset.media_type().as_str(), Some("image/png"));

        let video: Vec<&Media> = asset.files().collect();
        assert_eq!(video.len(), 1);
        assert_eq!(video[0].src, "ipfs://Qmvideo");
        assert_eq!(video[0].media_type.as_str(), Some("video/mp4"));
    }

    /// A top-level `mediaType` describes the headline, and the file pointing at
    /// that same source is the same medium — folded, not repeated.
    #[test]
    fn a_top_level_media_type_describes_the_headline() {
        let asset = envelope(
            r#"{
                "image": "ipfs://Qmstill",
                "mediaType": "image/gif",
                "files": [ { "mediaType": "image/png", "src": "ipfs://Qmstill" } ]
            }"#,
        )
        .into_decoded_asset(id());
        assert_eq!(asset.media.len(), 1, "one entry per medium");
        assert_eq!(asset.media_type().as_str(), Some("image/gif"));
        assert_eq!(asset.files().count(), 0);
    }

    /// No `image` is no headline — not an empty one, and not the first file
    /// promoted behind the document's back.
    #[test]
    fn no_image_means_no_headline() {
        let asset =
            envelope(r#"{ "files": [ { "mediaType": "image/png", "src": "ipfs://Qmonly" } ] }"#)
                .into_decoded_asset(id());
        assert!(asset.headline().is_none());
        assert_eq!(asset.image(), None);
        assert!(asset.media_type().is_none());
        assert_eq!(asset.files().count(), 1);
    }

    /// The distinction `Asset` cannot make: no name field at all versus a name
    /// field that is empty.
    #[test]
    fn a_document_without_a_name_says_so() {
        let absent = envelope(r#"{ "image": "ipfs://x" }"#).into_decoded_asset(id());
        assert_eq!(absent.name, None);
        let empty = envelope(r#"{ "name": "", "image": "ipfs://x" }"#).into_decoded_asset(id());
        assert_eq!(empty.name.as_deref(), Some(""));
    }

    /// A declared type with no `image` has no medium to describe. `Asset`
    /// carried it anyway (it kept `media_type` as a free-standing field);
    /// `DecodedAsset` drops it, and the corpus comparison counts how often that
    /// happens rather than papering over it.
    #[test]
    fn a_declared_type_without_an_image_has_no_headline_to_type() {
        let json = r#"{ "name": "x", "mediaType": "image/png" }"#;
        let decoded = envelope(json).into_decoded_asset(id());
        assert!(decoded.media.is_empty());
        assert!(decoded.media_type().is_none());
        assert_eq!(
            envelope(json).into_asset().media_type.as_deref(),
            Some("image/png"),
            "the old shape carried a type no medium described"
        );
    }

    /// The id is the on-chain identity, so the asset name survives a document
    /// that never mentions it.
    #[test]
    fn the_id_is_the_on_chain_identity() {
        let asset = envelope(r#"{ "image": "ipfs://x" }"#).into_decoded_asset(id());
        assert_eq!(asset.id.policy_id, "ab".repeat(28));
        assert_eq!(asset.id.asset_name_hex, hex::encode(b"AL0001"));
    }
}
