//! Error types for datum parsing operations

use thiserror::Error;

/// Errors that can occur during datum parsing
#[derive(Error, Debug)]
pub enum DatumParsingError {
    #[error("Invalid CBOR data: {0}")]
    InvalidCbor(String),

    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("Unknown protocol for address: {address}")]
    UnknownProtocol { address: String },

    #[error("Unsupported datum version: {version} for protocol: {protocol:?}")]
    UnsupportedVersion {
        version: u32,
        protocol: crate::Protocol,
    },

    #[error("Missing required field: {field} in {schema}")]
    MissingRequiredField { field: String, schema: String },

    #[error("Invalid field type: expected {expected}, got {actual} in field {field}")]
    InvalidFieldType {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("Hex decoding error: {0}")]
    HexDecode(#[from] hex::FromHexError),

    #[error("CBOR decoding error: {0}")]
    CborDecode(String),

    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("Address parsing error: {0}")]
    AddressParsing(String),

    #[error("Payout extraction failed: {reason}")]
    PayoutExtraction { reason: String },
}
