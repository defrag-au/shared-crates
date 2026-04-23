//! Lightweight Cardano datum parsing
//!
//! This crate provides a WASM-compatible approach to parsing Cardano datums using
//! TOML-based schema definitions. It supports multiple marketplace protocols
//! with type-safe parsing and validation while avoiding heavy dependencies.
//!
//! # Architecture
//!
//! - **TOML Schema Definitions**: Lightweight schema definitions embedded at compile time
//! - **CBOR-First Parsing**: Direct PlutusData parsing using pallas/minicbor
//! - **WASM Compatible**: No filesystem dependencies, works in Cloudflare Workers
//! - **Marketplace-Specific**: Dedicated parsers for JPG.store, Wayup, etc.
//!
//! # Bundle Size Optimization
//!
//! This implementation deliberately avoids heavy dependencies like CDDL parsers
//! and ciborium to keep bundle sizes small for WASM deployment. TOML schemas
//! are embedded at compile time using `include_str!` instead of runtime loading.
//!
//! # Usage
//!
//! ```rust
//! use datum_parsing::{TomlSchemaLoader, CborExtractor, MarketplaceDatumParser, MarketplaceType};
//!
//! // Load TOML schemas (embedded at compile time)
//! let mut loader = TomlSchemaLoader::new();
//!
//! // Use schema-based extraction
//! if let Some(schema) = loader.get_schema("jpg_store_v3_ask") {
//!     let extractor = CborExtractor::new(schema);
//!     // let result = extractor.extract_marketplace_operation(&cbor_bytes);
//! }
//!
//! // Or use marketplace-specific parser
//! let mut parser = MarketplaceDatumParser::new(MarketplaceType::JpgStoreV3);
//! // let result = parser.parse_cbor(&cbor_bytes);
//! ```

pub mod error;
pub mod marketplace_parsers;
pub mod toml_schema;

pub use error::DatumParsingError;
pub use marketplace_parsers::{
    LockedTarget, MarketplaceDatumParser, MarketplaceOperation, MarketplaceType,
};
pub use toml_schema::{CborExtractor, TomlSchema, TomlSchemaLoader};

/// Supported dApp protocols for datum parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    /// JPG.store NFT marketplace
    JpgStore,
    /// Minswap DEX
    Minswap,
    /// SundaeSwap DEX
    SundaeSwap,
    /// WingRiders DEX
    WingRiders,
    /// Unknown or unsupported protocol
    Unknown,
}

/// Result type for datum parsing operations
pub type Result<T> = std::result::Result<T, DatumParsingError>;

/// Version of the datum parsing library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
