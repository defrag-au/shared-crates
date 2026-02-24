//! Splash DEX order beacon calculation
//!
//! The beacon is a deterministic order ID derived from:
//! - The input UTxO reference (tx_hash + index)
//! - The order index within the transaction
//! - A hash of the datum with an empty (zeroed) beacon field
//!
//! This ensures each order has a unique, verifiable identifier.
//!
//! Reference: protocol-sdk spotOrderBeacon.ts

use pallas_crypto::hash::Hasher;

/// Empty beacon placeholder (28 zero bytes) used during beacon calculation
pub const EMPTY_BEACON: [u8; 28] = [0u8; 28];

/// Calculate the beacon for a Splash spot order.
///
/// ```text
/// beacon = blake2b_224(
///     tx_hash          (32 bytes)
///  ++ be_u64(utxo_idx) (8 bytes)
///  ++ be_u64(order_idx)(8 bytes)
///  ++ blake2b_224(datum_with_empty_beacon_cbor)  (28 bytes)
/// )
/// ```
///
/// # Arguments
/// * `input_tx_hash` - The transaction hash of the input UTxO (32 bytes)
/// * `input_index` - The index of the input UTxO
/// * `order_index` - The order index (typically 0 for single-order transactions)
/// * `datum_with_empty_beacon` - CBOR-encoded datum where the beacon field is 28 zero bytes
pub fn calculate_beacon(
    input_tx_hash: &[u8; 32],
    input_index: u64,
    order_index: u64,
    datum_with_empty_beacon: &[u8],
) -> [u8; 28] {
    let datum_hash = Hasher::<224>::hash(datum_with_empty_beacon);

    let mut preimage = Vec::with_capacity(76); // 32 + 8 + 8 + 28
    preimage.extend_from_slice(input_tx_hash);
    preimage.extend_from_slice(&input_index.to_be_bytes());
    preimage.extend_from_slice(&order_index.to_be_bytes());
    preimage.extend_from_slice(datum_hash.as_ref());

    *Hasher::<224>::hash(&preimage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_beacon_is_28_zero_bytes() {
        assert_eq!(EMPTY_BEACON.len(), 28);
        assert!(EMPTY_BEACON.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_beacon_deterministic() {
        let tx_hash = [0xABu8; 32];
        let datum_cbor = vec![0xd8, 0x79, 0x82, 0x40, 0x40]; // some dummy CBOR

        let beacon1 = calculate_beacon(&tx_hash, 0, 0, &datum_cbor);
        let beacon2 = calculate_beacon(&tx_hash, 0, 0, &datum_cbor);

        assert_eq!(beacon1, beacon2);
        assert_eq!(beacon1.len(), 28);
    }

    #[test]
    fn test_beacon_differs_with_different_index() {
        let tx_hash = [0xABu8; 32];
        let datum_cbor = vec![0xd8, 0x79, 0x82, 0x40, 0x40];

        let beacon_idx0 = calculate_beacon(&tx_hash, 0, 0, &datum_cbor);
        let beacon_idx1 = calculate_beacon(&tx_hash, 1, 0, &datum_cbor);

        assert_ne!(beacon_idx0, beacon_idx1);
    }

    #[test]
    fn test_beacon_differs_with_different_order_index() {
        let tx_hash = [0xABu8; 32];
        let datum_cbor = vec![0xd8, 0x79, 0x82, 0x40, 0x40];

        let beacon_order0 = calculate_beacon(&tx_hash, 0, 0, &datum_cbor);
        let beacon_order1 = calculate_beacon(&tx_hash, 0, 1, &datum_cbor);

        assert_ne!(beacon_order0, beacon_order1);
    }
}
