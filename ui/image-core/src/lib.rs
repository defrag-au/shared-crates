//! Image loading policy, with no runtime attached.
//!
//! Every frontend in the estate loads the same NFT art from the same IIIF
//! service, and — before this crate — every one of them decided independently
//! how to build the URL, which size to ask for, and whether to cache. A survey
//! found **~22 URL construction sites from ~13 distinct builders across 4
//! hosts, and 6 separate "warm size" enums**, one of which had drifted to a
//! size that isn't cached at all. That drift is invisible: the image still
//! arrives, just slowly, forever.
//!
//! So the policy lives here once, and the runtimes bring only a decoder:
//!
//! ```text
//! image-core       ← this crate: URLs, sizes, cache + queue. Pure, testable.
//!   image-web      ← web-sys edge  → egui ColorImage
//!   image-miniquad ← plugin-JS edge → raw RGBA for macroquad
//! ```
//!
//! Nothing here does I/O. [`LoadQueue`] decides *what to fetch next and
//! whether it is worth fetching at all*; the edge crate performs the fetch and
//! reports back. That split is what makes the interesting behaviour —
//! priority, de-duplication, concurrency limits, retry, eviction — testable
//! without a browser.

#![forbid(unsafe_code)]

mod cache;
mod queue;
mod url;

pub use cache::{Evicted, ImageCache};
pub use queue::{LoadQueue, Outcome, Slot};
pub use url::{iiif_asset_url, iiif_url_on, ImageSize, DEFAULT_IIIF_BASE};

/// Decoded pixels, tightly packed RGBA8.
///
/// The interchange type between an edge crate and its host. Chosen because it
/// is what five of the nine existing loaders already hand back — no one needed
/// persuading — and because it is the last representation before a GPU upload,
/// which is the one thing this crate must not know about.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes. Use [`Self::is_consistent`] before trusting
    /// it: a truncated transfer reaching the GPU is a much worse failure than
    /// a missing image.
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    /// Does the buffer match the declared dimensions?
    pub fn is_consistent(&self) -> bool {
        self.rgba.len() == self.expected_len()
    }

    pub fn expected_len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// Bytes held, for cache accounting.
    pub fn byte_size(&self) -> usize {
        self.rgba.len()
    }
}

impl std::fmt::Debug for DecodedImage {
    /// Never print the pixels — a single 400×400 image is 640 KB of noise that
    /// makes any log containing it unreadable.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}
