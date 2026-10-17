//! Wallet core functionality
//!
//! Provides CIP-30 wallet detection, connection, and signing capabilities, and
//! the EIP-1193 provider bridge next to it ([`EvmProvider`]) — one wasm-bindgen
//! wallet-API crate rather than two bindings layers.
//! Framework-agnostic - can be used with any UI framework or web components.
//!
//! The *state* of a connected browser is [`WalletConnections`]: a set of active
//! connections, one per chain, keyed by [`chains::ChainFamily`]. [`ConnectionState`]
//! is the single-slot predecessor it replaces.

mod cip30;
mod connections;
mod error;
mod evm;
mod storage;
mod types;

pub use cip30::*;
pub use connections::*;
pub use error::*;
pub use evm::*;
pub use storage::*;
pub use types::*;
