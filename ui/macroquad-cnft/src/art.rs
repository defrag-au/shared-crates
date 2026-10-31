//! `art` — where an asset's art lives, on the IIIF service.
//!
//! The loader itself is not NFT-specific and lives in `image-miniquad`; this
//! module keeps the URL policy, which is: ask the service for `auto` on the
//! web so it can send whatever is smallest, and for PNG natively because
//! that is what macroquad decodes.

use cnft_holdings::OwnedAsset;
use image_core::{Format, ImageSize};

/// The queue, the cache and the texture states. Re-exported under the names
/// this crate has always used, so a consumer of `ArtLoader` needs no change.
pub use image_miniquad::{ImageLoader as ArtLoader, ImageState as ArtView, ImageStats as ArtStats};

/// The URL to draw an asset's art from, on the default IIIF host.
///
/// On the web the service picks the format (`auto`): the browser decodes
/// whatever comes back, and the service knows whether the collection is
/// pixel art. Natively macroquad decodes PNG and TGA only, so it asks for
/// PNG — several times the bytes, which only a developer's machine pays.
pub fn art_url(asset: &OwnedAsset, size: ImageSize) -> String {
    asset.art().size(size).format(platform_format()).build()
}

/// As [`art_url`], against another IIIF base: a same-origin proxy for a
/// surface under a strict content security policy, such as a Discord
/// Activity. `base` is everything up to and including `/iiif/3`.
pub fn art_url_on(base: &str, asset: &OwnedAsset, size: ImageSize) -> String {
    asset
        .art()
        .on(base)
        .size(size)
        .format(platform_format())
        .build()
}

fn platform_format() -> Format {
    if cfg!(target_arch = "wasm32") {
        Format::Auto
    } else {
        Format::Png
    }
}
