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
//! # The media type: a catalogue, plus the mess it catalogues
//!
//! Measured over the whole of mainnet: **118 distinct declared values**, 41.2%
//! of assets declaring none. They fall into three groups, and [`MediaType`] has a
//! variant for each:
//!
//! - **17 media types a consumer can act on** ([`NftMimeType`]) — the images, the
//!   video containers, the two piece formats — reached through the spellings
//!   mints actually publish: `image/jpg`, `image/PNG`, `img/png`, `.png`, `png`,
//!   `iamge/png`, `image/image/png`, `video/mp4\t`, all of which resolve to the
//!   one variant. Case is folded (RFC 2045) and the spellings are declared as
//!   serde aliases on the variant itself, so a promotion is one `alias = "…"`.
//! - **[`MediaType::Custom`]** — `type/subtype`, but not a type this catalogue
//!   knows: `text/svg+xml`, `application/lpf+zip`, `image/mp4`. Carried as
//!   written, because each of those is a judgement nobody has made yet.
//! - **[`MediaType::Undefined`]** — declared, but not a type at all: nothing,
//!   `Video`, `image/*`, `image/`, `<mime_type>`, `JPEG image`, a collection
//!   name. Nothing here is a guess about what was meant.
//!
//! The tail is where the documents are: `<mime_type>` and `image/{{FILE_TYPE}}`
//! are unsubstituted template variables, `null`/`undefined`/`string` are JS
//! serialisation artefacts, and a few minters pasted a CID or a whole filename
//! into the slot.

use crate::{AssetId, Traits};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A media type an NFT can carry, as the chain spells it.
///
/// Built from the corpus rather than from a spec: every variant is a type mainnet
/// publishes, and every alias is a spelling it publishes it as. The catalogue
/// grows by adding a variant or an alias — never by guessing which of two
/// near-miss strings a mint meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NftMimeType {
    // Images — everything an `<img>` renders.
    #[serde(
        rename = "image/png",
        alias = "png",
        alias = ".png",
        alias = "img/png",
        alias = "iamge/png",
        alias = "image/image/png"
    )]
    ImagePng,
    #[serde(
        rename = "image/jpeg",
        alias = "image/jpg",
        alias = "jpg",
        alias = ".jpg",
        alias = "jpeg",
        alias = "img/jpg",
        alias = "img/jpeg",
        alias = "image/jepg"
    )]
    ImageJpeg,
    #[serde(rename = "image/gif", alias = "gif", alias = ".gif")]
    ImageGif,
    #[serde(
        rename = "image/svg+xml",
        alias = "image/svg",
        alias = "svg",
        alias = ".svg"
    )]
    ImageSvg,
    #[serde(rename = "image/webp", alias = "webp")]
    ImageWebp,
    #[serde(rename = "image/avif")]
    ImageAvif,
    #[serde(rename = "image/apng")]
    ImageApng,
    // `x-ms-bmp` is the registered name for the same format, not a typo.
    #[serde(rename = "image/bmp", alias = "image/x-ms-bmp")]
    ImageBmp,
    #[serde(rename = "image/tiff")]
    ImageTiff,
    #[serde(rename = "image/heic")]
    ImageHeic,
    // Video — the containers a `<video>` plays.
    #[serde(rename = "video/mp4")]
    VideoMp4,
    // ⚠️ `image/webm` is a webm filed under `image/`; a webm is a container, so
    // the medium is the video either way.
    #[serde(rename = "video/webm", alias = "image/webm")]
    VideoWebm,
    #[serde(rename = "video/quicktime")]
    VideoQuicktime,
    // 3D — the format the `mugz`/Blender pieces ship in.
    #[serde(rename = "model/gltf-binary", alias = "model/glb")]
    ModelGltfBinary,
    // A fully on-chain generative piece: an HTML document, not an image.
    #[serde(rename = "text/html")]
    TextHtml,
}

impl NftMimeType {
    /// Every variant, for iterating the catalogue.
    pub const ALL: [NftMimeType; 15] = [
        NftMimeType::ImagePng,
        NftMimeType::ImageJpeg,
        NftMimeType::ImageGif,
        NftMimeType::ImageSvg,
        NftMimeType::ImageWebp,
        NftMimeType::ImageAvif,
        NftMimeType::ImageApng,
        NftMimeType::ImageBmp,
        NftMimeType::ImageTiff,
        NftMimeType::ImageHeic,
        NftMimeType::VideoMp4,
        NftMimeType::VideoWebm,
        NftMimeType::VideoQuicktime,
        NftMimeType::ModelGltfBinary,
        NftMimeType::TextHtml,
    ];

    /// The canonical spelling — what this writes back.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            NftMimeType::ImagePng => "image/png",
            NftMimeType::ImageJpeg => "image/jpeg",
            NftMimeType::ImageGif => "image/gif",
            NftMimeType::ImageSvg => "image/svg+xml",
            NftMimeType::ImageWebp => "image/webp",
            NftMimeType::ImageAvif => "image/avif",
            NftMimeType::ImageApng => "image/apng",
            NftMimeType::ImageBmp => "image/bmp",
            NftMimeType::ImageTiff => "image/tiff",
            NftMimeType::ImageHeic => "image/heic",
            NftMimeType::VideoMp4 => "video/mp4",
            NftMimeType::VideoWebm => "video/webm",
            NftMimeType::VideoQuicktime => "video/quicktime",
            NftMimeType::ModelGltfBinary => "model/gltf-binary",
            NftMimeType::TextHtml => "text/html",
        }
    }

    /// Read a declared type through the alias table the variants declare.
    ///
    /// Case is folded first — a MIME type is case-insensitive (RFC 2045) — which
    /// is what collapses `image/PNG`, `IMAGE/GIF` and `Image/jpg` without an
    /// alias each. Whitespace is not part of a type either, so the one
    /// `video/mp4\t` on chain is a `video/mp4`.
    #[must_use]
    pub fn of(declared: &str) -> Option<Self> {
        serde_plain::from_str(&declared.trim().to_ascii_lowercase()).ok()
    }
}

/// The media type of a [`Media`] entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum MediaType {
    /// A media type from the catalogue, however the document spelled it.
    Mime(NftMimeType),
    /// The document declares no usable media type — nothing at all, an empty
    /// string, or something that is not a type (`Video`, `image/*`, `image/`,
    /// `<mime_type>`, `JPEG image`).
    #[default]
    Undefined,
    /// A media type the catalogue does not know yet: `type/subtype`, carried as
    /// written so it can be promoted to a variant without a refetch.
    Custom(String),
}

impl MediaType {
    /// Read a declared media type.
    ///
    /// The order matters: the catalogue first (through its aliases), then the
    /// shape test. A spelling the catalogue knows is never a `Custom`, and a
    /// `Custom` is always shaped like a type — which is what makes the two
    /// counts mean something: `Custom` is the promotion list, `Undefined` is the
    /// corpus telling us the document had nothing to say.
    #[must_use]
    pub fn of(declared: &str) -> Self {
        let declared = declared.trim();
        match NftMimeType::of(declared) {
            Some(mime) => MediaType::Mime(mime),
            None if looks_like_a_media_type(declared) => MediaType::Custom(declared.to_string()),
            None => MediaType::Undefined,
        }
    }

    /// The canonical spelling, or `None` when the document declares no type.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MediaType::Mime(mime) => Some(mime.as_str()),
            MediaType::Custom(declared) => Some(declared),
            MediaType::Undefined => None,
        }
    }

    /// Is there no usable declared type?
    #[must_use]
    pub fn is_undefined(&self) -> bool {
        matches!(self, MediaType::Undefined)
    }
}

/// `type/subtype`: one slash, both sides tokens, no wildcard. This is the line
/// between "a type we have not catalogued" and "not a type at all" —
/// `application/lpf+zip` is a type somebody invented, `image/*` is not a type of
/// anything, and `image/gif/png` is neither.
fn looks_like_a_media_type(declared: &str) -> bool {
    let Some((top, sub)) = declared.split_once('/') else {
        return false;
    };
    // A token is the RFC 6838 charset, which excludes `/`, `*` and whitespace —
    // so the separator is the only slash, a wildcard subtype is not a type of
    // anything, and `image/` has an empty subtype.
    let token = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$&^_.+-".contains(c))
    };
    token(top) && token(sub)
}

/// A string on the wire: the canonical spelling for a catalogue type, what the
/// document wrote for a `Custom`, and an omitted field for `Undefined`.
/// `AssetTag` set this precedent in-crate.
impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_str() {
            Some(declared) => f.write_str(declared),
            None => Ok(()),
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
    #[serde(default, skip_serializing_if = "MediaType::is_undefined")]
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
            .map_or(MediaType::Undefined, |m| m.media_type.clone())
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

    /// The catalogue's aliases, and the folding that saves writing one per case:
    /// the head of the measured distribution resolves to one variant each.
    #[test]
    fn every_spelling_of_a_catalogued_type_resolves() {
        for (declared, expected) in [
            ("image/png", NftMimeType::ImagePng),
            ("image/PNG", NftMimeType::ImagePng),
            ("img/png", NftMimeType::ImagePng),
            ("iamge/png", NftMimeType::ImagePng),
            (".png", NftMimeType::ImagePng),
            ("png", NftMimeType::ImagePng),
            ("image/image/png", NftMimeType::ImagePng),
            ("image/jpeg", NftMimeType::ImageJpeg),
            ("image/jpg", NftMimeType::ImageJpeg),
            ("image/JPG", NftMimeType::ImageJpeg),
            ("img/jpg", NftMimeType::ImageJpeg),
            (".jpg", NftMimeType::ImageJpeg),
            ("gif", NftMimeType::ImageGif),
            ("IMAGE/GIF", NftMimeType::ImageGif),
            ("image/svg", NftMimeType::ImageSvg),
            ("svg", NftMimeType::ImageSvg),
            ("video/mp4\t", NftMimeType::VideoMp4),
            ("image/x-ms-bmp", NftMimeType::ImageBmp),
            ("model/glb", NftMimeType::ModelGltfBinary),
            ("text/html", NftMimeType::TextHtml),
        ] {
            assert_eq!(NftMimeType::of(declared), Some(expected), "{declared:?}");
            assert_eq!(
                MediaType::of(declared),
                MediaType::Mime(expected),
                "{declared:?}"
            );
        }
        assert_eq!(NftMimeType::of("image/web"), None, "not a guess at webp");
    }

    /// Every variant round-trips through its own canonical spelling, so `as_str`
    /// and the alias table cannot drift apart as the catalogue grows.
    #[test]
    fn every_variant_round_trips() {
        for mime in NftMimeType::ALL {
            assert_eq!(NftMimeType::of(mime.as_str()), Some(mime));
            assert_eq!(MediaType::of(mime.as_str()).as_str(), Some(mime.as_str()));
        }
    }

    /// **The line the catalogue rests on.** A `Custom` is shaped like a type and
    /// is the promotion list; an `Undefined` is the corpus saying the document
    /// had nothing to say. Guessing between the two would make both counts
    /// meaningless.
    #[test]
    fn shaped_but_uncatalogued_is_custom_and_the_rest_is_undefined() {
        for declared in [
            "text/svg+xml",
            "application/lpf+zip",
            "image/mp4",
            "application/octet-stream",
        ] {
            assert_eq!(
                MediaType::of(declared),
                MediaType::Custom(declared.to_string()),
                "{declared:?}"
            );
        }
        for declared in [
            "",
            "   ",
            "Video",
            "Image",
            "image",
            "image/",
            "image/*",
            "img/*",
            "image/gif/png",
            "JPEG image",
            "<mime_type>",
            "null",
            "undefined",
            "Disco Holiday",
            "\"image/jpg\"",
        ] {
            assert_eq!(
                MediaType::of(declared),
                MediaType::Undefined,
                "{declared:?}"
            );
        }
    }

    /// On the wire a type is the string: the canonical spelling for a catalogue
    /// type, what the document wrote for a `Custom`, nothing for an undefined.
    #[test]
    fn the_wire_is_the_canonical_string() {
        let jpeg = MediaType::Mime(NftMimeType::ImageJpeg);
        assert_eq!(
            serde_json::to_string(&jpeg).expect("serializes"),
            r#""image/jpeg""#
        );
        assert_eq!(
            serde_json::from_str::<MediaType>(r#""image/jpg""#).expect("reads"),
            jpeg
        );
        let custom = MediaType::of("application/lpf+zip");
        assert_eq!(
            serde_json::to_string(&custom).expect("serializes"),
            r#""application/lpf+zip""#
        );
        assert_eq!(
            serde_json::from_str::<MediaType>(r#""image/web""#).expect("reads"),
            MediaType::Custom("image/web".to_string())
        );
    }

    /// A `Media` whose type is undefined omits the field, so a document with no
    /// declared type and one with an empty declaration are the same entry.
    #[test]
    fn no_declared_type_is_an_omitted_field() {
        let media = Media {
            src: "ipfs://x".to_string(),
            media_type: MediaType::Undefined,
            name: None,
            role: MediaRole::Headline,
        };
        let json = serde_json::to_string(&media).expect("serializes");
        assert_eq!(json, r#"{"src":"ipfs://x","role":"headline"}"#);
        let back: Media = serde_json::from_str(&json).expect("round");
        assert!(back.media_type.is_undefined());
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
        assert!(asset.media_type().is_undefined());
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
        assert!(decoded.media_type().is_undefined());
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
