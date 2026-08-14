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

/// An asset image reference: `asset://{fingerprint}/{size}`.
///
/// Used as the `src` of an `<img>` in the markup. The renderer resolves it;
/// the sender never supplies a URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUri {
    pub fingerprint: Fingerprint,
    pub size: ImageSize,
}

impl AssetUri {
    pub fn new(fingerprint: Fingerprint, size: ImageSize) -> Self {
        Self { fingerprint, size }
    }
}

impl fmt::Display for AssetUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "asset://{}/{}",
            self.fingerprint.as_str(),
            self.size.pixels()
        )
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
        let rest = s.strip_prefix("asset://").ok_or(AssetUriError::MissingScheme)?;
        let (fp, size) = rest.split_once('/').ok_or(AssetUriError::MissingSize)?;

        Ok(Self {
            fingerprint: Fingerprint::new(fp).map_err(|_| AssetUriError::InvalidFingerprint)?,
            size: size.parse()?,
        })
    }
}

/// A width the image pipeline keeps warm.
///
/// Deliberately an enum rather than a free integer. Only these two widths are
/// pre-generated; any other value silently trades a cache hit for an on-demand
/// resize on every single render, which is the kind of cost that never shows up
/// as an error and never gets noticed.
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
        let uri = AssetUri::new(Fingerprint::new(FP).unwrap(), ImageSize::Thumb);

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
        let err = format!("asset://{FP}/1024").parse::<AssetUri>().unwrap_err();
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
