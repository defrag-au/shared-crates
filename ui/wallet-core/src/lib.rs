//! Cardano wallet core functionality
//!
//! Provides CIP-30 wallet detection, connection, and signing capabilities.
//! Framework-agnostic - can be used with any UI framework or web components.

mod cip30;
mod error;
mod storage;
mod types;

pub use cip30::*;
pub use error::*;
pub use storage::*;
pub use types::*;
