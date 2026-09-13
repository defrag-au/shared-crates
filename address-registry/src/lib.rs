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

pub mod action_protocol;
pub mod naming;
pub mod registry;
pub mod utils;

pub use registry::*;

// Not glob re-exported: `ScriptRole` and `ProtocolScript` are general enough
// names to collide with the marketplace tables above, and a deployment record
// should be reached for deliberately.
pub use action_protocol::{
    ACTION_PROTOCOL_DEPLOYMENTS, ActionProtocolDeployment, lookup_action_protocol,
};
