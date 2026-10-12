//! Wallet core functionality
//!
//! Provides CIP-30 wallet detection, connection, and signing capabilities, and
//! the EIP-1193 provider bridge next to it ([`EvmProvider`]) — one wasm-bindgen
//! wallet-API crate rather than two bindings layers.
//! Framework-agnostic - can be used with any UI framework or web components.

mod cip30;
mod error;
mod evm;
mod storage;
mod types;

pub use cip30::*;
pub use error::*;
pub use evm::*;
pub use storage::*;
pub use types::*;
