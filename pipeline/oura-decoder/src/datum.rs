//! CBOR parsing utilities for Cardano marketplace datums
//!
//! This module provides specialized parsing functions for different marketplace
//! datum formats using UTxORPC-compatible structures.

use crate::{DecoderError, Result};
use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_codec::minicbor::{Decode, Decoder};
use pallas_primitives::alonzo::PlutusData;
use serde::{Deserialize, Serialize};

/// UTxORPC-compatible datum representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxorpcDatum {
    pub hash: String,
    pub cbor: Option<String>,
}

impl UtxorpcDatum {
    /// Get CBOR bytes from the datum
    pub fn bytes(&self) -> Option<&str> {
        self.cbor.as_deref()
    }
}

/// Represents a payment distribution found in marketplace datums
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentDistribution {
    pub recipient_address: String,
    pub amount_lovelace: u64,
}

/// Extract payment distributions from JPG.store marketplace datums
///
/// JPG.store datums contain payment information structured as:
/// Array of [address_structure, amount] pairs
pub fn extract_jpg_store_payments(datum: &UtxorpcDatum) -> Result<Vec<PaymentDistribution>> {
    // Get the CBOR bytes from the datum
    let bytes_str = datum
        .bytes()
        .ok_or_else(|| DecoderError::MissingField("CBOR bytes".to_string()))?;

    // Decode hex string to bytes
    let datum_bytes = hex::decode(bytes_str)?;

    // Decode using Pallas
    let mut decoder = Decoder::new(&datum_bytes);
    let plutus_data = PlutusData::decode(&mut decoder, &mut ())
        .map_err(|e| DecoderError::CborDecode(format!("CBOR decode error: {e}")))?;

    #[cfg(debug_assertions)]
    tracing::debug!("Parsing JPG.store datum: {plutus_data:?}");

    let mut payments = Vec::new();
    find_jpg_store_payments(&plutus_data, &mut payments)?;

    Ok(payments)
}

/// Recursively search for JPG.store payment structures in PlutusData
fn find_jpg_store_payments(
    data: &PlutusData,
    payments: &mut Vec<PaymentDistribution>,
) -> Result<()> {
    // Collect all address bytes and amounts separately, then try to match them
    let mut all_addresses = Vec::new();
    let mut all_amounts = Vec::new();

    collect_addresses_and_amounts(data, &mut all_addresses, &mut all_amounts);

    #[cfg(debug_assertions)]
    tracing::debug!(
        "Found {} addresses and {} amounts",
        all_addresses.len(),
        all_amounts.len()
    );

    // Try to pair addresses with amounts
    // For JPG.store, we typically expect 2-3 payments: royalty, seller, maybe marketplace fee
    if all_addresses.len() >= 2 && all_amounts.len() >= 2 {
        // Sort amounts to help with pairing (smaller amounts typically royalties)
        let mut amount_indices: Vec<_> = (0..all_amounts.len()).collect();
        amount_indices.sort_by_key(|&i| all_amounts[i]);

        // Take up to the number of addresses we have
        let num_payments = std::cmp::min(all_addresses.len(), all_amounts.len());

        for i in 0..num_payments {
            let addr_bytes = &all_addresses[i];
            let amount = all_amounts[amount_indices[i]];

            match convert_bytes_to_bech32(addr_bytes) {
                Ok(address) => {
                    #[cfg(debug_assertions)]
                    tracing::debug!(
                        "Payment found: {} receives ₳{:.2}",
                        address,
                        amount as f64 / 1_000_000.0
                    );

                    payments.push(PaymentDistribution {
                        recipient_address: address,
                        amount_lovelace: amount,
                    });
                }
                #[allow(unused_variables)]
                Err(e) => {
                    #[cfg(debug_assertions)]
                    tracing::debug!("Failed to convert address bytes: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Helper function to collect all address bytes and amounts from PlutusData
fn collect_addresses_and_amounts(
    data: &PlutusData,
    addresses: &mut Vec<Vec<u8>>,
    amounts: &mut Vec<u64>,
) {
    match data {
        PlutusData::BoundedBytes(bytes) => {
            // Check if this looks like an address (28 bytes)
            if bytes.len() == 28 {
                addresses.push(bytes.as_slice().to_vec());
            }
        }
        PlutusData::BigInt(big_int) => {
            // Check if this looks like an amount
            if let Some(amount) = extract_amount_from_plutus(&PlutusData::BigInt(big_int.clone())) {
                // Filter out very small amounts (likely not payments) and very large amounts (likely not realistic)
                if amount > 1_000_000 && amount < 10_000_000_000 {
                    // Between 1 ADA and 10,000 ADA
                    amounts.push(amount);
                }
            }
        }
        PlutusData::Array(arr) => {
            for item in arr.iter() {
                collect_addresses_and_amounts(item, addresses, amounts);
            }
        }
        PlutusData::Constr(constr) => {
            for field in constr.fields.iter() {
                collect_addresses_and_amounts(field, addresses, amounts);
            }
        }
        PlutusData::Map(map) => {
            for (_, v) in map.iter() {
                collect_addresses_and_amounts(v, addresses, amounts);
            }
        }
    }
}

/// Extract amount from PlutusData BigInt
fn extract_amount_from_plutus(data: &PlutusData) -> Option<u64> {
    match data {
        PlutusData::BigInt(pallas_primitives::alonzo::BigInt::Int(int)) => {
            let val: i128 = (*int).into();
            if val >= 0 && val <= u64::MAX as i128 {
                Some(val as u64)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Convert 28-byte credential to Cardano bech32 address
fn convert_bytes_to_bech32(bytes: &[u8]) -> Result<String> {
    #[cfg(debug_assertions)]
    tracing::debug!(
        "Converting address bytes ({}): {}",
        bytes.len(),
        hex::encode(bytes)
    );

    if bytes.len() != 28 {
        return Err(DecoderError::InvalidStructure(format!(
            "Expected 28 bytes for credential, got {}",
            bytes.len()
        )));
    }

    // Create payment credential from the hash
    let payment_part = ShelleyPaymentPart::Key(bytes.into());

    // Create Shelley address for mainnet without stake delegation (simple payment address)
    let delegation_part = ShelleyDelegationPart::Null;
    let shelley_addr = ShelleyAddress::new(Network::Mainnet, payment_part, delegation_part);
    let address = Address::Shelley(shelley_addr);

    // Convert to bech32 string
    let bech32_str = address
        .to_bech32()
        .map_err(|e| DecoderError::InvalidStructure(format!("Failed to convert to bech32: {e}")))?;

    #[cfg(debug_assertions)]
    tracing::debug!("Successfully converted to address: {}", bech32_str);

    Ok(bech32_str)
}

/// Extract potential prices from marketplace datums using Pallas CBOR parsing
///
/// This is a general-purpose price extraction function that can be used by
/// various marketplace patterns (ListingCreate, etc.) to find price values
/// in CBOR-encoded datums.
pub fn extract_potential_prices(datum: &UtxorpcDatum) -> Vec<u64> {
    // Get the CBOR bytes from the datum
    let bytes_str = match datum.bytes() {
        Some(bytes) => bytes,
        None => return Vec::new(),
    };

    // Decode hex string to bytes
    let datum_bytes = match hex::decode(bytes_str) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    // Decode using Pallas
    let mut decoder = Decoder::new(&datum_bytes);
    let plutus_data = match PlutusData::decode(&mut decoder, &mut ()) {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };

    #[cfg(debug_assertions)]
    tracing::debug!("Extracting potential prices from datum: {plutus_data:?}");

    let mut prices = Vec::new();
    collect_potential_prices(&plutus_data, &mut prices);

    #[cfg(debug_assertions)]
    tracing::debug!("Found {} potential prices: {prices:?}", prices.len());

    prices
}

/// Recursively collect potential price values from PlutusData
fn collect_potential_prices(data: &PlutusData, prices: &mut Vec<u64>) {
    match data {
        PlutusData::BigInt(big_int) => {
            if let Some(amount) = extract_amount_from_plutus(&PlutusData::BigInt(big_int.clone())) {
                // Filter for reasonable price ranges (between 0.1 ADA and 100,000 ADA)
                if (100_000..=100_000_000_000).contains(&amount) {
                    prices.push(amount);
                }
            }
        }
        PlutusData::Array(arr) => {
            for item in arr.iter() {
                collect_potential_prices(item, prices);
            }
        }
        PlutusData::Constr(constr) => {
            for field in constr.fields.iter() {
                collect_potential_prices(field, prices);
            }
        }
        PlutusData::Map(map) => {
            for (_, v) in map.iter() {
                collect_potential_prices(v, prices);
            }
        }
        // Ignore other types like BoundedBytes for price extraction
        _ => {}
    }
}
