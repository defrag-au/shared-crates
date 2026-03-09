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

pub mod registry;
pub mod utils;

pub use registry::*;
