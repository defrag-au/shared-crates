//! UTxORPC transaction parsing using Pallas
//!
//! Converts raw CBOR block data from Oura into UTxORPC format using pallas-utxorpc.
//! This establishes a clear domain boundary where oura-decoder works with UTxORPC types,
//! separate from RawTxData used in the tx-classifier domain.

use crate::{DecoderError, Result};
use pallas_utxorpc::{LedgerContext, Mapper, TxoRef, UtxoMap};
use tracing::debug;
use utxorpc_spec::utxorpc::v1alpha::cardano as u5c;

/// No-op ledger context for simple block parsing without UTXO lookups
#[derive(Clone)]
struct NoLedger;

impl LedgerContext for NoLedger {
    fn get_utxos(&self, _refs: &[TxoRef]) -> Option<UtxoMap> {
        None
    }

    fn get_slot_timestamp(&self, _slot: u64) -> Option<u64> {
        None
    }
}

/// Parse CBOR block data and convert to UTxORPC format
///
/// This is the main entry point for converting Oura block CBOR data
/// into UTxORPC format using pallas-utxorpc's map_block_cbor function.
pub fn parse_block_cbor_to_utxorpc(cbor_bytes: &[u8]) -> Result<u5c::Block> {
    // Create mapper with NoLedger context (no UTXO lookups needed)
    let mapper = Mapper::new(NoLedger);

    // Use pallas-utxorpc's map_block_cbor function - this is the correct approach!
    let utxorpc_block = mapper.map_block_cbor(cbor_bytes);

    debug!("Successfully converted block to UTxORPC format using pallas-utxorpc");

    Ok(utxorpc_block)
}

/// Parse block CBOR from hex string and convert to UTxORPC format
pub fn parse_block_cbor_hex_to_utxorpc(cbor_hex: &str) -> Result<u5c::Block> {
    let cbor_bytes = hex::decode(cbor_hex).map_err(DecoderError::InvalidHex)?;
    parse_block_cbor_to_utxorpc(&cbor_bytes)
}

/// Parse block CBOR from base64-encoded data and convert to UTxORPC format
pub fn parse_block_cbor_base64_to_utxorpc(cbor_base64: &str) -> Result<u5c::Block> {
    use base64::{engine::general_purpose, Engine as _};
    let cbor_bytes = general_purpose::STANDARD
        .decode(cbor_base64)
        .map_err(|e| DecoderError::CborDecode(format!("Base64 decode error: {e}")))?;
    parse_block_cbor_to_utxorpc(&cbor_bytes)
}

/// Parse a single transaction CBOR (hex) into a UTxORPC Tx.
///
/// Uses pallas-traverse to auto-detect the era, then maps via pallas-utxorpc.
/// Useful for testing with TX CBOR from Maestro or other indexers.
pub fn parse_tx_cbor_hex_to_utxorpc(cbor_hex: &str) -> Result<u5c::Tx> {
    let cbor_bytes = hex::decode(cbor_hex).map_err(DecoderError::InvalidHex)?;
    let multi_era_tx = pallas_traverse::MultiEraTx::decode(&cbor_bytes)
        .map_err(|e| DecoderError::CborDecode(format!("TX CBOR decode error: {e}")))?;
    let mapper = Mapper::new(NoLedger);
    let utxorpc_tx = mapper.map_tx(&multi_era_tx);
    debug!("Successfully converted TX to UTxORPC format");
    Ok(utxorpc_tx)
}

/// Auto-detect format and parse block CBOR to UTxORPC
pub fn parse_block_cbor_auto_to_utxorpc(data: &str) -> Result<u5c::Block> {
    // Auto-detect format: hex contains only hex chars, base64 can contain +/=
    if data.chars().all(|c| c.is_ascii_hexdigit()) {
        // Looks like hex
        parse_block_cbor_hex_to_utxorpc(data)
    } else {
        // Assume base64 (Oura's default)
        parse_block_cbor_base64_to_utxorpc(data)
    }
}
