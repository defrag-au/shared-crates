//! Read an EVM chain over JSON-RPC.
//!
//! This is the **reconcile oracle** for a stream-fed ownership ledger, and the
//! reader behind it. The OpenSea stream is best-effort with no replay, so a dropped
//! socket leaves a permanent hole in an event-sourced ledger; the chain does not.
//! An [`EvmRpcClient`] is how that hole is filled:
//!
//! - [`EvmRpcClient::owner_of`] and [`EvmRpcClient::balance_of`] read current state
//!   — the truth a ledger is checked against.
//! - [`EvmRpcClient::transfers`] replays ERC-721 `Transfer` logs, which is the same
//!   movement OpenSea's `item_transferred` reports and therefore the check for
//!   whether the stream is a subset of the chain.
//! - [`EvmRpcClient::supports_erc4906`] answers whether metadata changes are a
//!   replayable log or only a marketplace signal — see [`erc4906`]'s module docs.
//! - [`EvmRpcClient::get_logs`] is the general log query all of the above are built
//!   on, because a reconcile also has to be able to ask its own question.
//!
//! # What it does not do
//!
//! No writes, no signing, no transactions — this crate reads. No ERC-1155: an
//! ERC-1155 `Transfer` carries a quantity and its event shape differs, and nothing
//! here has needed one yet. No dynamic ABI: every call returns one static 32-byte
//! word, which is why there is no ABI dependency (see [`abi`]).
//!
//! # The endpoint is passed in
//!
//! [`EvmRpcClient::new`] takes a URL. Sourcing it — from a worker secret, a config
//! file, a flag — is the caller's job, for the same reason the OpenSea key is: this
//! crate has no notion of an environment, which is also what lets its tests run
//! offline.
//!
//! # Every constant here was checked, not recalled
//!
//! The function selectors, the event topics and the ERC-4906 interface id are the
//! standard ones, and each was verified against a signature database rather than
//! written from memory: a wrong selector is a call that reverts, and a wrong topic
//! is a scan that silently returns nothing — the same silent-failure shape as
//! OpenSea's wrong slug.

mod abi;
mod address;
mod block;
mod erc4906;
mod erc721;
mod hex;
mod log;
mod rpc;
mod uint256;

pub use abi::AbiError;
pub use address::{Address, AddressError};
pub use block::BlockTag;
pub use erc721::{TRANSFER_TOPIC, Transfer, decode_transfer};
pub use erc4906::{
    BATCH_METADATA_UPDATE_TOPIC, INTERFACE_ID, METADATA_UPDATE_TOPIC, MetadataEvent, MetadataUpdate,
};
pub use hex::{HexError, Quantity};
pub use log::{HexData, Log, LogFilter, LogTopic};
pub use rpc::{EvmRpcClient, RpcError};
pub use uint256::{U256, U256Error};
