//! Address and script registry for Cardano transaction classification
//!
//! This crate provides registries for known addresses and smart contracts,
//! enabling classification of transactions based on address patterns.
//!
//! The registry contains:
//! - Known marketplace addresses and their purposes
//! - Smart contract addresses and their categories
//! - Marketplace-specific policy extraction logic
//! - Address category classification utilities

pub mod naming;
pub mod registry;
pub mod utils;

pub use registry::*;

// There was an `ACTION_PROTOCOL_DEPLOYMENTS` static here, and removing it drew
// a line worth keeping:
//
// `MARKETPLACE_DEPLOYMENTS` is right to be a constant because those are OTHER
// PEOPLE'S contracts. We discovered jpg.store's validator hash; we can only
// ever hard-code it, and it cannot be checked against anything.
//
// Deployments WE make are the opposite. We know them at the moment we create
// them, so compiling one in means a crate push and a rev bump to record a fact
// that already happened — and, worse, it cannot be verified: a mistyped hex
// character compiles perfectly and names an address that exists and is not
// ours. Those live in `script-depot` now, uploaded with the applied blueprint
// so every hash is re-derived from the bytes beside it before it is believed.
//
// The rule: constants for chain state we merely observed, records for chain
// state we caused.
