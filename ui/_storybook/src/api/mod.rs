//! PFP City API client for storybook demos
//!
//! Provides WASM-compatible client for fetching NFT asset data from PFP City.
//! This is a local utility to avoid circular dependencies with augminted-bots.

pub mod pfp_city;

pub use pfp_city::*;
