//! What a player holds of a Cardano NFT collection, as a game sees it.
//!
//! A game built around CNFTs asks three questions of the chain, and this
//! crate answers each one without doing any I/O:
//!
//! - **What does this player hold?** [`HoldingsLoad`] plans the requests —
//!   one per page — and folds the responses into [`Holdings`]. The edge
//!   crate performs each request and hands the body back.
//! - **What is this asset called, and what is it like?** [`OwnedAsset`]
//!   carries a display name (never raw hex), decoded traits and a rarity
//!   rank, and two deterministic hooks for turning an asset into game
//!   numbers: [`OwnedAsset::seed`] and [`OwnedAsset::roll`].
//! - **What does it look like?** [`OwnedAsset::art`] is an IIIF URL builder
//!   aimed at the token whose image exists. For CIP-68 that is the reference
//!   token, not the one the player holds.
//!
//! ```text
//! cnft-holdings   ← this crate: pure. Builds natively, in a worker, in wasm.
//!   macroquad-cnft  ← macroquad edge: fetch + browser decode, textures
//! ```
//!
//! # Where holdings come from
//!
//! [`Source`] names the two paths, and they are not equally trustworthy:
//!
//! - [`Source::Stakes`] asks the public collection-ownership service about
//!   stake addresses the client supplied, typically from a CIP-30 wallet.
//!   Nothing proves the player controls them, so this is fine for cosmetics
//!   (a portrait, a name) and wrong for anything with stakes.
//! - [`Source::Worker`] asks the game's own worker, which resolves the
//!   signed-in player to the wallets they verified server-side. This is the
//!   only path inside a Discord Activity, where there is no wallet extension
//!   and the content security policy allows same-origin requests only.
//!
//! A reward, a leaderboard entry or anything else worth cheating for must be
//! re-checked on the server regardless of which path the client used.

#![forbid(unsafe_code)]

mod asset;
mod browse;
mod holdings;
mod profile;
mod source;
mod stats;

pub use asset::{OwnedAsset, decode_asset_name, display_name, image_asset_name};
pub use browse::{Browse, CollectionList, CollectionSummary, Order};
/// The rarity tiers, Common to Legendary.
pub use cardano_assets::AssetRarity;
pub use holdings::{Holdings, HoldingsLoad, LoadState, MAX_PAGES};
pub use profile::{CardProfile, RoleRule};
pub use source::{Get, OWNERSHIP_BASE, Source, SourceError, bundle_url};
pub use stats::{Stat, StatBlock, StatDef, StatSpec, TraitBonus, rarity_fraction, tier};

/// Re-exported so a caller can name a collection without depending on
/// `cardano-assets` directly.
pub use cardano_assets::PolicyId;
/// The wire types a page arrives in, for a worker that proxies them.
pub use ownership_client::{BundleEntry, BundleResponse};
