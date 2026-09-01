//! What a service sends to ask for a graphic: [`RenderRequest`].
//!
//! # Why markup rather than a node tree
//!
//! The obvious design is for the requesting service to build the renderer's own
//! node tree and serialise it. That is not possible with takumi: `Node` and
//! `NodeKind` derive `Deserialize` but **not** `Serialize`, because the engine
//! is built to *consume* trees produced elsewhere. A Rust producer therefore
//! cannot emit one.
//!
//! Markup sidesteps that completely, and turns out to be the better boundary
//! anyway:
//!
//! - **The producer needs no renderer.** A plugin composing a graphic depends on
//!   this crate — serde and a fingerprint type — not on a rasteriser, a font
//!   stack, or a layout engine. That matters most for workers, where every
//!   dependency is bytes in a wasm bundle.
//! - **It is inspectable.** The same string can be pasted into a browser to see
//!   roughly what the renderer saw, which no bespoke IR gives you.
//! - **It has one owner.** The format is HTML; nobody has to keep a mirrored
//!   struct in step with anybody else's.
//!
//! Tailwind-style utility classes are supported through a `tw` attribute, so a
//! caller writes `<div tw="flex flex-wrap gap-2">` rather than computing
//! coordinates.
//!
//! # Images are named, not fetched
//!
//! A render request is foreign input. A renderer that resolves arbitrary URLs
//! on the sender's behalf is server-side request forgery with extra steps, so
//! images are referenced by [`AssetUri`] — `asset://{fingerprint}/{size}` — and
//! the *rendering* service resolves them through a path it controls. The
//! fingerprint is CIP-14, which is already the canonical key the IIIF image
//! pipeline stores under, so this names an existing key rather than inventing
//! an identifier.
//!
//! The other schemes work the same way and exist because they resolve to
//! genuinely different pictures, not because they are conveniences:
//! [`AvatarUri`] for a Discord user, [`R2Uri`] for a bucket this renderer is
//! bound to, and [`TokenUri`] for a token's registry brand mark — which is a
//! different image from the artwork `asset://` returns for the same pair.

use std::fmt;
use std::str::FromStr;

use cardano_assets::Fingerprint;
use serde::{Deserialize, Serialize};

/// A request to render markup to an image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderRequest {
    /// HTML fragment. Styling via `style` attributes or `tw` utility classes.
    ///
    /// A fragment is parsed as-is; a source starting with `<html>` is parsed as
    /// a document. Multiple roots are wrapped in a full-size container.
    pub html: String,

    /// Output dimensions in pixels. This is the canvas, not a hint — layout is
    /// computed against it.
    pub viewport: Viewport,

    #[serde(default)]
    pub format: OutputFormat,
}

impl RenderRequest {
    /// A PNG request at the given size.
    pub fn png(html: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            html: html.into(),
            viewport: Viewport { width, height },
            format: OutputFormat::Png,
        }
    }
}

/// Output canvas size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl From<Viewport> for (u32, u32) {
    fn from(v: Viewport) -> Self {
        (v.width, v.height)
    }
}

/// Encoding for the rendered image.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg {
        /// 1–100.
        quality: u8,
    },
    WebP {
        /// 1–100.
        quality: u8,
    },
}

/// How an asset image is named.
///
/// Both forms are accepted because they are not interchangeable in practice.
/// The image service stores by fingerprint but *resolves* a fingerprint through
/// an identity index (D1 → Cardanoscan), so fingerprint addressing only works
/// for collections that index has reached. `policy:hex` needs no lookup and
/// works for every collection — including ones indexed later.
///
/// A caller that has a fingerprint should send it; a caller that has the policy
/// and asset name should send those rather than deriving a fingerprint the
/// service may not be able to reverse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetIdentifier {
    /// CIP-14 fingerprint. Requires the image service's identity index to know it.
    Fingerprint(Fingerprint),
    /// Policy id and asset name, both hex. Always resolvable.
    PolicyAsset {
        policy_id: String,
        asset_name_hex: String,
    },
}

impl AssetIdentifier {
    /// The identifier as the image service expects it in a request path.
    pub fn as_path_segment(&self) -> String {
        match self {
            Self::Fingerprint(fp) => fp.as_str().to_string(),
            Self::PolicyAsset {
                policy_id,
                asset_name_hex,
            } => format!("{policy_id}:{asset_name_hex}"),
        }
    }
}

impl fmt::Display for AssetIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_path_segment())
    }
}

/// An asset image reference: `asset://{identifier}/{size}`, where the
/// identifier is a CIP-14 fingerprint or `policy_id:asset_name_hex`.
///
/// Used as the `src` of an `<img>` in the markup. The renderer resolves it;
/// the sender never supplies a URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUri {
    pub identifier: AssetIdentifier,
    pub size: ImageSize,
}

impl AssetUri {
    pub fn fingerprint(fingerprint: Fingerprint, size: ImageSize) -> Self {
        Self {
            identifier: AssetIdentifier::Fingerprint(fingerprint),
            size,
        }
    }

    pub fn policy_asset(
        policy_id: impl Into<String>,
        asset_name_hex: impl Into<String>,
        size: ImageSize,
    ) -> Self {
        Self {
            identifier: AssetIdentifier::PolicyAsset {
                policy_id: policy_id.into(),
                asset_name_hex: asset_name_hex.into(),
            },
            size,
        }
    }
}

impl fmt::Display for AssetUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "asset://{}/{}", self.identifier, self.size.pixels())
    }
}

/// Why an [`AssetUri`] could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetUriError {
    MissingScheme,
    MissingSize,
    InvalidFingerprint,
    /// A width that has no warm cache entry — see [`ImageSize`].
    UnsupportedSize(String),
}

impl fmt::Display for AssetUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingScheme => f.write_str("expected an `asset://` URI"),
            Self::MissingSize => f.write_str("expected `asset://{fingerprint}/{size}`"),
            Self::InvalidFingerprint => f.write_str("not a CIP-14 fingerprint"),
            Self::UnsupportedSize(s) => {
                write!(f, "unsupported size `{s}`: expected 400 or 1646")
            }
        }
    }
}

impl std::error::Error for AssetUriError {}

impl FromStr for AssetUri {
    type Err = AssetUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("asset://")
            .ok_or(AssetUriError::MissingScheme)?;
        // Split on the LAST slash: neither identifier form contains one, but a
        // future form might, and the size is always the final segment.
        let (id, size) = rest.rsplit_once('/').ok_or(AssetUriError::MissingSize)?;

        let identifier = match id.split_once(':') {
            Some((policy_id, asset_name_hex)) => {
                // Both halves land in a URL the renderer builds, so anything
                // that isn't hex is rejected rather than passed through.
                let hex_ok = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_hexdigit());
                if policy_id.len() != 56 || !hex_ok(policy_id) || !hex_ok(asset_name_hex) {
                    return Err(AssetUriError::InvalidFingerprint);
                }
                AssetIdentifier::PolicyAsset {
                    policy_id: policy_id.to_string(),
                    asset_name_hex: asset_name_hex.to_string(),
                }
            }
            None => AssetIdentifier::Fingerprint(
                Fingerprint::new(id).map_err(|_| AssetUriError::InvalidFingerprint)?,
            ),
        };

        Ok(Self {
            identifier,
            size: size.parse()?,
        })
    }
}

/// A Discord avatar reference: `avatar://{user_id}/{hash}`.
///
/// A second scheme rather than allowing `https://cdn.discordapp.com/...` in the
/// markup, so the rule stays absolute: markup never contains a fetchable URL.
/// An allowlisted host would be less code, but "no URLs except this one" is a
/// category that grows, where a scheme the renderer resolves does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarUri {
    pub user_id: String,
    /// Discord's avatar hash. `None` renders the default avatar.
    pub hash: Option<String>,
}

impl AvatarUri {
    pub fn new(user_id: impl Into<String>, hash: Option<String>) -> Self {
        Self {
            user_id: user_id.into(),
            hash,
        }
    }
}

impl fmt::Display for AvatarUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `default` rather than omitting the segment, so the URI always has the
        // same shape and the parser never has to guess.
        write!(
            f,
            "avatar://{}/{}",
            self.user_id,
            self.hash.as_deref().unwrap_or("default")
        )
    }
}

impl FromStr for AvatarUri {
    type Err = AssetUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("avatar://")
            .ok_or(AssetUriError::MissingScheme)?;
        let (user_id, hash) = rest.split_once('/').ok_or(AssetUriError::MissingSize)?;

        // Snowflake and hex-hash only: both become path segments in a URL the
        // renderer builds, so anything else is a path-traversal attempt.
        if user_id.is_empty() || !user_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(AssetUriError::InvalidFingerprint);
        }
        if hash != "default"
            && (hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return Err(AssetUriError::InvalidFingerprint);
        }

        Ok(Self {
            user_id: user_id.to_string(),
            hash: (hash != "default").then(|| hash.to_string()),
        })
    }
}

/// Artwork we own, in object storage: `r2://{binding}/{key}`.
///
/// One scheme for every kind of art we host — tier emblems, overlays, ship
/// templates, badges — rather than one scheme per kind doing the same thing
/// with a different prefix.
///
/// `binding` is the renderer's R2 **binding name**, not a bucket name, and that
/// is what bounds it: a binding the worker doesn't declare simply fails to
/// resolve, so the wrangler config is the allowlist. There is no separate map
/// to keep in step with reality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct R2Uri {
    pub binding: String,
    pub key: String,
}

impl R2Uri {
    pub fn new(binding: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            binding: binding.into(),
            key: key.into(),
        }
    }
}

impl fmt::Display for R2Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r2://{}/{}", self.binding, self.key)
    }
}

impl FromStr for R2Uri {
    type Err = AssetUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("r2://")
            .ok_or(AssetUriError::MissingScheme)?;
        let (binding, key) = rest.split_once('/').ok_or(AssetUriError::MissingSize)?;

        // Cloudflare binding names are SCREAMING_SNAKE_CASE. Requiring the
        // shape means a typo fails to parse rather than becoming a lookup for a
        // binding that will never exist.
        if binding.is_empty()
            || !binding
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(AssetUriError::InvalidFingerprint);
        }

        // The key is a path and may contain slashes, but must not climb: `..`
        // in an object key is meaningless to R2 but meaningful to anything that
        // later maps keys onto a filesystem or a URL.
        if key.is_empty()
            || key.starts_with('/')
            || key
                .split('/')
                .any(|segment| segment == ".." || segment.is_empty())
        {
            return Err(AssetUriError::InvalidFingerprint);
        }

        Ok(Self {
            binding: binding.to_string(),
            key: key.to_string(),
        })
    }
}

/// A token's registry logo: `token://{policy_id}/{asset_name_hex}`.
///
/// # Why this is not an [`AssetUri`]
///
/// They name the same pair and resolve to different pictures. The IIIF asset
/// service answers for a policy and name with the ARTWORK — for $Aliens, a
/// photorealistic head. The Cardano token registry holds what the project
/// actually brands itself with, which for $Aliens is a green plush toy. A card
/// about a token wants the second one, and there is no width or flag on
/// `asset://` that would get it.
///
/// # Why there is no size segment
///
/// `asset://` carries one because a resizer stands behind it. A registry logo
/// is a single bitmap at whatever size the project uploaded — usually 64–256px
/// — and nothing resizes it. Asking for `/1646` would be a request the
/// renderer could only ignore, so the shape does not offer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUri {
    pub policy_id: String,
    /// On-chain asset name, hex. NOT the registry's display name.
    pub asset_name_hex: String,
}

impl TokenUri {
    pub fn new(policy_id: impl Into<String>, asset_name_hex: impl Into<String>) -> Self {
        Self {
            policy_id: policy_id.into(),
            asset_name_hex: asset_name_hex.into(),
        }
    }
}

impl fmt::Display for TokenUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "token://{}/{}", self.policy_id, self.asset_name_hex)
    }
}

impl FromStr for TokenUri {
    type Err = AssetUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix("token://")
            .ok_or(AssetUriError::MissingScheme)?;
        let (policy_id, asset_name_hex) = rest.split_once('/').ok_or(AssetUriError::MissingSize)?;

        // Both halves become path segments in a URL the renderer builds, so
        // anything that is not hex of the right length is rejected here rather
        // than passed through. A policy id is exactly 28 bytes.
        let hex_ok = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_hexdigit());
        if policy_id.len() != 56 || !hex_ok(policy_id) || !hex_ok(asset_name_hex) {
            return Err(AssetUriError::InvalidFingerprint);
        }

        Ok(Self {
            policy_id: policy_id.to_string(),
            asset_name_hex: asset_name_hex.to_string(),
        })
    }
}

/// A width the image pipeline keeps warm.
///
/// Deliberately an enum rather than a free integer. The image service will
/// render *any* width on request, so an arbitrary value is not an error — it is
/// a 2-3 second on-demand render for the first caller, versus a warm hit for
/// these two. That is exactly the kind of cost that never surfaces as a failure
/// and so never gets noticed; restricting the type makes it a deliberate choice
/// to add a width rather than an accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSize {
    /// 400px — thumbnails, grid cells, composites.
    Thumb,
    /// 1646px — full-size single-asset display.
    Full,
}

impl ImageSize {
    pub fn pixels(self) -> u32 {
        match self {
            Self::Thumb => 400,
            Self::Full => 1646,
        }
    }
}

impl FromStr for ImageSize {
    type Err = AssetUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "400" | "thumb" => Ok(Self::Thumb),
            "1646" | "full" => Ok(Self::Full),
            other => Err(AssetUriError::UnsupportedSize(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real CIP-14 fingerprint — the bech32 checksum is verified on parse, so
    /// an invented one would fail for the wrong reason. From the CIP-14 test
    /// vectors in `cardano-assets`.
    const FP: &str = "asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3";

    #[test]
    fn asset_uri_round_trips() {
        let uri = AssetUri::fingerprint(Fingerprint::new(FP).unwrap(), ImageSize::Thumb);

        assert_eq!(uri.to_string(), format!("asset://{FP}/400"));
        assert_eq!(uri.to_string().parse::<AssetUri>().unwrap(), uri);
    }

    /// A plain URL must not be mistaken for an asset reference — that is the
    /// whole point of the scheme.
    #[test]
    fn rejects_a_bare_url() {
        assert_eq!(
            "https://example.com/evil.png".parse::<AssetUri>(),
            Err(AssetUriError::MissingScheme)
        );
    }

    #[test]
    fn rejects_a_cold_size() {
        let err = format!("asset://{FP}/1024")
            .parse::<AssetUri>()
            .unwrap_err();
        assert_eq!(err, AssetUriError::UnsupportedSize("1024".to_string()));
    }

    #[test]
    fn rejects_a_non_fingerprint() {
        assert_eq!(
            "asset://not-a-fingerprint/400".parse::<AssetUri>(),
            Err(AssetUriError::InvalidFingerprint)
        );
    }

    #[test]
    fn request_defaults_to_png() {
        let json = r#"{"html":"<div/>","viewport":{"width":400,"height":240}}"#;
        let req: RenderRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.format, OutputFormat::Png);
        assert_eq!(<(u32, u32)>::from(req.viewport), (400, 240));
    }
}

#[cfg(test)]
mod avatar_tests {
    use super::*;

    #[test]
    fn avatar_round_trips() {
        let uri = AvatarUri::new("179744071361757184", Some("a1b2c3d4".to_string()));
        assert_eq!(uri.to_string(), "avatar://179744071361757184/a1b2c3d4");
        assert_eq!(uri.to_string().parse::<AvatarUri>().unwrap(), uri);
    }

    /// A user with no avatar still produces a well-formed URI, so the renderer
    /// never has to handle a missing segment.
    #[test]
    fn a_missing_hash_becomes_default() {
        let uri = AvatarUri::new("179744071361757184", None);
        assert_eq!(uri.to_string(), "avatar://179744071361757184/default");
        assert_eq!(uri.to_string().parse::<AvatarUri>().unwrap().hash, None);
    }

    /// Both segments land in a URL the renderer builds, so traversal attempts
    /// must not parse.
    #[test]
    fn path_traversal_is_rejected() {
        assert!("avatar://../../etc/passwd".parse::<AvatarUri>().is_err());
        assert!("avatar://123/..%2f..%2fx".parse::<AvatarUri>().is_err());
        assert!("avatar://not-a-snowflake/abc".parse::<AvatarUri>().is_err());
    }

    #[test]
    fn an_asset_uri_is_not_an_avatar() {
        assert_eq!(
            "asset://asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3/400".parse::<AvatarUri>(),
            Err(AssetUriError::MissingScheme)
        );
    }
}

#[cfg(test)]
mod policy_asset_tests {
    use super::*;

    const POLICY: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";
    const NAME: &str = "50697261746531353039";

    #[test]
    fn policy_asset_round_trips() {
        let uri = AssetUri::policy_asset(POLICY, NAME, ImageSize::Thumb);
        assert_eq!(uri.to_string(), format!("asset://{POLICY}:{NAME}/400"));
        assert_eq!(uri.to_string().parse::<AssetUri>().unwrap(), uri);
    }

    /// The two forms must stay distinguishable — a fingerprint has no colon.
    #[test]
    fn the_two_forms_parse_to_different_identifiers() {
        let fp = "asset1rjklcrnsdzqp65wjgrg55sy9723kw09mlgvlc3";
        assert!(matches!(
            format!("asset://{fp}/400")
                .parse::<AssetUri>()
                .unwrap()
                .identifier,
            AssetIdentifier::Fingerprint(_)
        ));
        assert!(matches!(
            format!("asset://{POLICY}:{NAME}/400")
                .parse::<AssetUri>()
                .unwrap()
                .identifier,
            AssetIdentifier::PolicyAsset { .. }
        ));
    }

    /// Both halves become URL path segments, so non-hex must not parse.
    #[test]
    fn non_hex_identifiers_are_rejected() {
        assert!(format!("asset://{POLICY}:../../etc/x/400")
            .parse::<AssetUri>()
            .is_err());
        assert!("asset://short:abcd/400".parse::<AssetUri>().is_err());
        assert!(format!("asset://{POLICY}:zzzz/400")
            .parse::<AssetUri>()
            .is_err());
    }

    #[test]
    fn the_path_segment_is_what_the_image_service_expects() {
        assert_eq!(
            AssetUri::policy_asset(POLICY, NAME, ImageSize::Thumb)
                .identifier
                .as_path_segment(),
            format!("{POLICY}:{NAME}")
        );
    }
}

#[cfg(test)]
mod r2_tests {
    use super::*;

    const KEY: &str = "world/blackflag/tiers/icons/beards_bandits.png";

    #[test]
    fn r2_uri_round_trips() {
        let uri = R2Uri::new("STORAGE", KEY);
        assert_eq!(uri.to_string(), format!("r2://STORAGE/{KEY}"));
        assert_eq!(uri.to_string().parse::<R2Uri>().unwrap(), uri);
    }

    /// A key is a path and legitimately contains slashes.
    #[test]
    fn nested_keys_survive() {
        let uri: R2Uri = format!("r2://STORAGE/{KEY}").parse().unwrap();
        assert_eq!(uri.binding, "STORAGE");
        assert_eq!(uri.key, KEY);
    }

    /// `..` is meaningless to R2 but meaningful to anything that later maps a
    /// key onto a path or a URL, so it is refused at the boundary.
    #[test]
    fn traversal_is_rejected() {
        assert!("r2://STORAGE/../secrets".parse::<R2Uri>().is_err());
        assert!("r2://STORAGE/a/../../b".parse::<R2Uri>().is_err());
        assert!("r2://STORAGE//leading".parse::<R2Uri>().is_err());
        assert!("r2://STORAGE/".parse::<R2Uri>().is_err());
    }

    /// Binding names are SCREAMING_SNAKE_CASE; a lowercase bucket name is a
    /// confusion worth catching at parse rather than as a failed lookup.
    #[test]
    fn a_bucket_name_is_not_a_binding_name() {
        assert!(format!("r2://augminted-dev/{KEY}")
            .parse::<R2Uri>()
            .is_err());
    }

    #[test]
    fn other_schemes_are_not_r2() {
        assert_eq!(
            "asset://abc:def/400".parse::<R2Uri>(),
            Err(AssetUriError::MissingScheme)
        );
    }

}

#[cfg(test)]
mod token_tests {
    use super::*;

    /// $Aliens. The registry holds the project's green plush mark here, where
    /// `asset://` for the same pair returns the artwork — which is the whole
    /// reason this scheme exists.
    const ALIENS_POLICY: &str = "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7";
    const ALIENS_NAME: &str = "416c69656e73";

    #[test]
    fn a_token_uri_round_trips() {
        let uri = TokenUri::new(ALIENS_POLICY, ALIENS_NAME);
        let text = uri.to_string();
        assert_eq!(text, format!("token://{ALIENS_POLICY}/{ALIENS_NAME}"));
        assert_eq!(text.parse::<TokenUri>().unwrap(), uri);
    }

    /// Both halves land in a URL the renderer builds, so anything that is not
    /// hex of the right length is a traversal attempt rather than a typo.
    #[test]
    fn a_token_uri_refuses_anything_that_is_not_a_hex_pair() {
        for bad in [
            // Policy the wrong length.
            "token://abc/416c69656e73",
            &format!("token://{ALIENS_POLICY}0/416c69656e73"),
            // Not hex.
            &format!("token://{ALIENS_POLICY}/nothex!"),
            &format!("token://../{ALIENS_NAME}"),
            // Missing halves.
            &format!("token://{ALIENS_POLICY}"),
            &format!("token://{ALIENS_POLICY}/"),
            "token://",
        ] {
            assert!(bad.parse::<TokenUri>().is_err(), "{bad:?} should not parse");
        }
    }

    /// A registry logo is one bitmap and nothing resizes it, so the shape does
    /// not offer a size — a caller appending one is naming a different asset,
    /// not asking for a width.
    #[test]
    fn a_token_uri_has_no_size_segment() {
        let uri: TokenUri = format!("token://{ALIENS_POLICY}/{ALIENS_NAME}")
            .parse()
            .unwrap();
        assert_eq!(uri.asset_name_hex, ALIENS_NAME);
        // `/400` appended is read as part of the name, which is not hex.
        assert!(format!("token://{ALIENS_POLICY}/{ALIENS_NAME}/400")
            .parse::<TokenUri>()
            .is_err());
    }

    #[test]
    fn other_schemes_are_not_tokens() {
        assert_eq!(
            format!("asset://{ALIENS_POLICY}:{ALIENS_NAME}/400").parse::<TokenUri>(),
            Err(AssetUriError::MissingScheme)
        );
        assert_eq!(
            "r2://STORAGE/fonts/Anton.ttf".parse::<TokenUri>(),
            Err(AssetUriError::MissingScheme)
        );
    }
}
