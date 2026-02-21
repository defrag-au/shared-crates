//! Cardano-aware wallet utilities
//!
//! This crate extends `wallet-core` with Cardano serialization and verification
//! capabilities using the pallas library suite.
//!
//! ## Features
//!
//! - **Address utilities**: Bech32 encoding/decoding, network detection
//! - **CIP-8 verification**: Verify message signatures from `sign_data`
//! - **Transaction inspection**: Parse transactions and witness sets
//!
//! ## Example
//!
//! ```ignore
//! use wallet_core::{WalletApi, WalletProvider};
//! use wallet_pallas::{verify_data_signature, Address};
//!
//! // Connect and sign
//! let api = WalletApi::connect(WalletProvider::Eternl).await?;
//! let address_hex = api.change_address().await?;
//! let signature = api.sign_data(&address_hex, "48656c6c6f").await?;
//!
//! // Verify the signature
//! let address = Address::from_hex(&address_hex)?;
//! let valid = verify_data_signature(
//!     &signature.signature,
//!     &signature.key,
//!     "48656c6c6f",
//!     &address,
//! )?;
//! ```

mod address;
mod cip8;
mod error;
mod transaction;
mod value;

pub use address::Address;
pub use cip8::{compute_key_hash, verify_data_signature, DataSignatureInfo};
pub use error::PallasError;
pub use transaction::{
    extract_vkey_witnesses, parse_transaction, parse_witness_set, TransactionInfo, WitnessSetInfo,
};
pub use value::{decode_balance, NativeToken, PolicyGroup, WalletBalance, ADA_HANDLE_POLICY};
