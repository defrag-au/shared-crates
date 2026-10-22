//! Cardano NFTs in a macroquad game: a player's holdings and their art.
//!
//! The macroquad edge of `cnft-holdings`, and a companion to
//! `macroquad-gamekit`. The pure crate decides what to ask for and what the
//! answers mean; this crate performs the requests and turns art into
//! textures.
//!
//! ```no_run
//! use macroquad::prelude::Rect;
//! use macroquad_cnft::{ArtLoader, ArtTile, HoldingsLoader, ImageSize, LoadState, Source};
//! use macroquad_cnft::{OWNERSHIP_BASE, PolicyId, TileStyle, art_url};
//!
//! # fn demo(stake: &str, clock: f32) -> Result<(), Box<dyn std::error::Error>> {
//! let policy = PolicyId::new("285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4")?;
//! // A stake address from a CIP-30 wallet (`macroquad-wallet-flow`), or
//! // `Source::worker("api/holdings", token)` inside a Discord Activity.
//! let mut holdings = HoldingsLoader::new(Source::stakes(OWNERSHIP_BASE, [stake])?, policy);
//! let mut art = ArtLoader::default();
//! let tile = ArtTile::new(TileStyle::DARK);
//!
//! // Once per frame: ask for what is on screen, update, draw.
//! holdings.update();
//! let champion = holdings.holdings().rarest().map(|a| art_url(a, ImageSize::Thumb));
//! if let Some(url) = &champion {
//!     art.request(url, 0);
//! }
//! art.update(clock);
//! if let Some(url) = &champion {
//!     // A shimmer until it lands, then a fade.
//!     tile.draw(&mut art, url, Rect::new(10.0, 10.0, 120.0, 120.0), 12.0);
//! }
//! # Ok(()) }
//! ```
//!
//! # Setup
//!
//! Stamp [`PLUGIN_JS`] into the web build from `build.rs`, the same way as
//! `wallet-miniquad` and `discord-miniquad`:
//!
//! ```no_run
//! # fn main() -> std::io::Result<()> {
//! std::fs::write("web/cnft.js", macroquad_cnft::PLUGIN_JS)?;
//! # Ok(()) }
//! ```
//!
//! Natively there is nothing to set up: requests run on threads, and art is
//! fetched as PNG because that is what macroquad decodes.
//!
//! # Trust
//!
//! Holdings read on the client are for cosmetics. See `cnft-holdings` for
//! why, and re-check anything worth cheating for on the server.

pub mod art;
pub mod card;
pub mod collection;
pub mod fetch;
pub mod holdings;
pub mod tile;

pub use art::{ArtLoader, ArtStats, ArtView, art_url, art_url_on};
pub use card::{CardData, CardFx, CardLayout, CardPainter, CardPalette, draw_stat_icon, tier_name};
pub use collection::{BrowseLoader, CollectionsLoader};
pub use fetch::{FetchError, ImageRequest, Stage, TextRequest, get_image, get_text};
pub use holdings::HoldingsLoader;
pub use tile::{ArtTile, TileStyle};

pub use cnft_holdings::{
    AssetRarity, Browse, CardProfile, CollectionList, CollectionSummary, Holdings, LoadState,
    OWNERSHIP_BASE, Order, OwnedAsset, PolicyId, RoleRule, Source, SourceError, Stat, StatBlock,
    StatDef, StatSpec, TraitBonus,
};
pub use image_core::ImageSize;

/// The JS half of the bridge, for stamping into a web build as `cnft.js`.
///
/// Shipped from the crate so it cannot drift from the Rust that calls it: a
/// stale copy fails on a missing import at instantiate.
pub const PLUGIN_JS: &str = include_str!("../js/cnft.js");

/// Plugin version handshake. miniquad's `gl.js` compares the export
/// `<plugin>_crate_version` with the plugin's JS `version`; `cnft.js`
/// declares `version: 1`. Bump both together when the JS surface changes.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn cnft_crate_version() -> u32 {
    1
}
