//! Images in a macroquad game, without a browser API in sight.
//!
//! The macroquad edge of [`image_core`]: what to fetch is the pure crate's
//! decision, and this one performs the request and hands back pixels.
//!
//! ```text
//! image-core       ← URLs, sizes, cache + queue. Pure, testable.
//!   image-web      ← web-sys edge      → egui ColorImage
//!   image-miniquad ← plugin-JS edge    → this crate, raw RGBA for macroquad
//! ```
//!
//! Two layers, usable separately:
//!
//! - [`get_image`] / [`get_text`] — one request, which the caller polls each
//!   frame. That is all a tile cache or a one-off banner needs.
//! - [`ImageLoader`] — a priority queue, retries, a byte budget and LRU
//!   eviction over those requests, for a screen showing many images.
//!
//! ```no_run
//! use image_miniquad::ImageLoader;
//!
//! # fn demo(clock: f32, urls: &[String]) {
//! let mut images = ImageLoader::default();
//! for (i, url) in urls.iter().enumerate() {
//!     images.request(url, i as i32);
//! }
//! images.update(clock);
//! for url in urls {
//!     if images.ready(url) {
//!         // `peek` does not count a use; the draw pass marks one with `touch`.
//!         let _texture = images.peek(url);
//!     }
//! }
//! # }
//! ```
//!
//! # Setup
//!
//! Stamp [`PLUGIN_JS`] into the web build from `build.rs`, the same way as
//! `wallet-miniquad`, `discord-miniquad` and `miniquad-platform`:
//!
//! ```no_run
//! # fn main() -> std::io::Result<()> {
//! std::fs::write("web/image.js", image_miniquad::PLUGIN_JS)?;
//! # Ok(()) }
//! ```
//!
//! Natively there is nothing to set up: requests run on threads, and images are
//! decoded as PNG because that is what macroquad decodes. A caller that builds
//! its own URLs should therefore ask for PNG on native and let the service pick
//! the format on the web — the browser decodes JPEG, PNG, WebP, AVIF and GIF,
//! where a decoder in wasm costs hundreds of KB.
//!
//! # Why not `quad-net`
//!
//! Its HTTP reply carries no status. A 404 and a 200 arrive as the same "here
//! is some text", so a caller cannot tell "there is nothing here" from "the
//! service is down". Every failure here says which it was, in stages, and
//! [`FetchError::is_permanent`] says whether a retry could help.

pub mod fetch;
pub mod loader;

pub use fetch::{FetchError, ImageRequest, Stage, TextRequest, get_image, get_text};
pub use image_core::DecodedImage;
pub use loader::{ImageLoader, ImageState, ImageStats};

/// The JS half of the bridge, for stamping into a web build as `image.js`.
///
/// Shipped from the crate so it cannot drift from the Rust that calls it: a
/// stale copy fails on a missing import at instantiate.
pub const PLUGIN_JS: &str = include_str!("../js/image.js");

/// Plugin version handshake. miniquad's `gl.js` compares the export
/// `<plugin>_crate_version` with the plugin's JS `version`; `image.js` declares
/// `version: 1`. Bump both together when the JS surface changes.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn image_crate_version() -> u32 {
    1
}
