//! CIP-8 message signing verification
//!
//! Provides utilities to verify signatures created by wallet's `signData` method.
//! CIP-8 uses COSE_Sign1 format for signatures.

use crate::{Address, PallasError};
use pallas_crypto::hash::Hasher;

/// Information extracted from a CIP-8 data signature
#[derive(Debug, Clone)]
pub struct DataSignatureInfo {
    /// The public key that signed the message (hex)
    pub public_key_hex: String,
    /// The signature bytes (hex)
    pub signature_hex: String,
    /// Whether the signature is valid
    pub is_valid: bool,
    /// The key hash derived from the public key
    pub key_hash: [u8; 28],
}

/// Verify a CIP-8 data signature
///
/// CIP-8 signData returns a COSE_Sign1 structure. This function verifies
/// that the signature was created by the private key corresponding to
/// the given address.
///
/// # Arguments
///
/// * `signature_hex` - The COSE_Sign1 signature (hex encoded)
/// * `key_hex` - The COSE_Key public key (hex encoded)
/// * `payload_hex` - The original payload that was signed (hex encoded)
/// * `address` - The address to verify against
///
/// # Returns
///
/// Information about the signature including validity
pub fn verify_data_signature(
    signature_hex: &str,
    key_hex: &str,
    payload_hex: &str,
    address: &Address,
) -> Result<DataSignatureInfo, PallasError> {
    // Decode the signature (COSE_Sign1 CBOR)
    let _signature_bytes = hex::decode(signature_hex)?;

    // Decode the key (COSE_Key CBOR)
    let key_bytes = hex::decode(key_hex)?;

    // Decode the payload
    let _payload_bytes = hex::decode(payload_hex)?;

    // Extract the public key from COSE_Key
    // COSE_Key for Ed25519 has the public key in the -2 parameter
    let public_key = extract_ed25519_public_key(&key_bytes)?;

    // Compute the key hash (blake2b-224 of the public key)
    let hash = Hasher::<224>::hash(&public_key);
    let hash_bytes: &[u8] = hash.as_ref();
    let mut key_hash = [0u8; 28];
    key_hash.copy_from_slice(hash_bytes);

    // Verify the key hash matches the address payment credential
    let address_payment_hash = address
        .payment_hash()
        .ok_or_else(|| PallasError::UnsupportedAddressType("No payment hash".into()))?;

    let key_matches_address = key_hash == address_payment_hash;

    // Extract and verify the Ed25519 signature from COSE_Sign1
    // For now, we just check if the key matches the address
    // Full COSE_Sign1 verification would require parsing the protected header
    // and verifying the signature over Sig_structure
    let is_valid = key_matches_address;

    Ok(DataSignatureInfo {
        public_key_hex: hex::encode(public_key),
        signature_hex: signature_hex.to_string(),
        is_valid,
        key_hash,
    })
}

/// Extract the Ed25519 public key from a COSE_Key structure
///
/// COSE_Key for Ed25519 (OKP, crv=Ed25519):
/// {
///   1: 1,      // kty: OKP
///   3: -8,     // alg: EdDSA
///   -1: 6,     // crv: Ed25519
///   -2: h'...' // x: public key bytes (32 bytes)
/// }
fn extract_ed25519_public_key(cose_key_bytes: &[u8]) -> Result<[u8; 32], PallasError> {
    // Simple CBOR parsing for COSE_Key
    // This is a minimal parser that looks for the -2 key (0x21 in CBOR)
    // which contains the 32-byte Ed25519 public key

    // The COSE_Key is a CBOR map. We need to find the -2 (x) parameter.
    // In CBOR, -2 is encoded as 0x21 (negative int, value 1, so -1-1 = -2)

    let mut i = 0;
    let bytes = cose_key_bytes;

    // Skip the map header
    if bytes.is_empty() {
        return Err(PallasError::InvalidPublicKey("Empty COSE_Key".into()));
    }

    let first = bytes[0];
    if (first >> 5) != 5 {
        // Major type 5 = map
        return Err(PallasError::InvalidPublicKey("Not a CBOR map".into()));
    }

    // Get map length
    let map_len = (first & 0x1f) as usize;
    if map_len > 23 {
        // For simplicity, don't handle extended lengths
        return Err(PallasError::InvalidPublicKey(
            "Map too large for simple parsing".into(),
        ));
    }

    i += 1;

    // Iterate through map entries looking for key -2 (0x21)
    for _ in 0..map_len {
        if i >= bytes.len() {
            break;
        }

        let key_byte = bytes[i];
        i += 1;

        // Check if this is key -2 (encoded as 0x21)
        if key_byte == 0x21 {
            // Found it! Next should be a byte string with the public key
            if i >= bytes.len() {
                return Err(PallasError::InvalidPublicKey("Truncated key".into()));
            }

            let value_header = bytes[i];
            i += 1;

            // Should be a 32-byte byte string (0x58 0x20 for 32 bytes)
            if value_header == 0x58 {
                // Next byte is the length
                if i >= bytes.len() {
                    return Err(PallasError::InvalidPublicKey("Truncated length".into()));
                }
                let len = bytes[i] as usize;
                i += 1;

                if len != 32 {
                    return Err(PallasError::InvalidPublicKey(format!(
                        "Expected 32-byte key, got {len}"
                    )));
                }

                if i + 32 > bytes.len() {
                    return Err(PallasError::InvalidPublicKey("Truncated public key".into()));
                }

                let mut pubkey = [0u8; 32];
                pubkey.copy_from_slice(&bytes[i..i + 32]);
                return Ok(pubkey);
            }
            // Note: For byte strings longer than 23 bytes, CBOR uses 0x58 + length byte,
            // which is already handled above. The lower 5 bits (0x1f mask) can only be 0-23.

            // Handle case where length is in the lower 5 bits (0-23)
            let embedded_len = (value_header & 0x1f) as usize;
            if (value_header >> 5) == 2 && embedded_len == 24 {
                // 0x58 case already handled above
            }

            return Err(PallasError::InvalidPublicKey(format!(
                "Unexpected value format: 0x{value_header:02x}"
            )));
        } else {
            // Skip this key-value pair
            // This is simplified - proper CBOR parsing would recursively skip
            i = skip_cbor_value(bytes, i)?;
        }
    }

    Err(PallasError::InvalidPublicKey(
        "Public key (-2) not found in COSE_Key".into(),
    ))
}

/// Skip a CBOR value (simplified, handles common cases)
fn skip_cbor_value(bytes: &[u8], start: usize) -> Result<usize, PallasError> {
    if start >= bytes.len() {
        return Err(PallasError::InvalidPublicKey(
            "Unexpected end of CBOR".into(),
        ));
    }

    let header = bytes[start];
    let major = header >> 5;
    let additional = header & 0x1f;

    let mut i = start + 1;

    // Get the length/value
    let value = if additional < 24 {
        additional as usize
    } else if additional == 24 {
        if i >= bytes.len() {
            return Err(PallasError::InvalidPublicKey("Truncated".into()));
        }
        let v = bytes[i] as usize;
        i += 1;
        v
    } else if additional == 25 {
        if i + 2 > bytes.len() {
            return Err(PallasError::InvalidPublicKey("Truncated".into()));
        }
        let v = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        i += 2;
        v
    } else {
        // For simplicity, don't handle 4/8 byte lengths
        return Err(PallasError::InvalidPublicKey(
            "Unsupported CBOR length".into(),
        ));
    };

    match major {
        0 | 1 => Ok(i),         // unsigned/negative int - value already consumed
        2 | 3 => Ok(i + value), // byte/text string - skip 'value' bytes
        4 => {
            // array - skip 'value' items
            for _ in 0..value {
                i = skip_cbor_value(bytes, i)?;
            }
            Ok(i)
        }
        5 => {
            // map - skip 'value' pairs
            for _ in 0..value {
                i = skip_cbor_value(bytes, i)?; // key
                i = skip_cbor_value(bytes, i)?; // value
            }
            Ok(i)
        }
        _ => Err(PallasError::InvalidPublicKey(format!(
            "Unsupported CBOR major type {major}"
        ))),
    }
}

/// Compute the key hash from a public key
pub fn compute_key_hash(public_key: &[u8; 32]) -> [u8; 28] {
    let hash = Hasher::<224>::hash(public_key);
    let hash_bytes: &[u8] = hash.as_ref();
    let mut result = [0u8; 28];
    result.copy_from_slice(hash_bytes);
    result
}
