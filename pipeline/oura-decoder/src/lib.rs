//! UTxORPC CBOR decoding utilities for Oura block data using Pallas
//!
//! This crate provides high-level interfaces for decoding CBOR block data from Oura
//! into UTxORPC format using the Pallas cryptographic primitives library.
//!
//! Oura is a Cardano data pipeline tool that extracts and filters blockchain data.
//! This decoder specifically handles CBOR-encoded block data from Oura's output format
//! and converts it to UTxORPC format for compatibility with the UTxORPC ecosystem.

use serde::Deserialize;
use thiserror::Error;

pub mod datum;
mod tests;
pub mod transaction;

pub use datum::{extract_jpg_store_payments, extract_potential_prices, PaymentDistribution};
pub use transaction::*;

#[derive(Deserialize)]
pub struct OuraBlock {
    #[serde(alias = "block")]
    pub hex: String,
}

/// Decoder error types
#[derive(Error, Debug)]
pub enum DecoderError {
    #[error("Invalid hex encoding: {0}")]
    InvalidHex(#[from] hex::FromHexError),

    #[error("CBOR decode error: {0}")]
    CborDecode(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid block structure: {0}")]
    InvalidStructure(String),

    #[error("Unsupported block format: {0}")]
    UnsupportedFormat(String),
}

pub type Result<T> = std::result::Result<T, DecoderError>;
