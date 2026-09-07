//! Marketplace datum payout parser for buy TX construction.
//!
//! Extracts payout obligations from JPG.store listing datums so the buy builder
//! can create outputs satisfying all payout targets.
//!
//! This is a focused parser for TX construction — it works on PlutusData CBOR,
//! not on the TOML schema system used by `datum-parsing` (pipeline crate).

use address_registry::MarketplaceType;
use cardano_assets::utxo::UtxoApi;
use pallas_addresses::Address;
use pallas_codec::minicbor;
use pallas_primitives::conway::PlutusData;

use crate::error::TxBuildError;

/// A payout obligation from a listing datum.
#[derive(Debug, Clone)]
pub struct DatumPayout {
    /// Bech32 recipient address
    pub address: Address,
    /// Lovelace amount to pay
    pub lovelace: u64,
}

/// A parsed listing ready for buy TX construction.
#[derive(Debug, Clone)]
pub struct ParsedListing {
    /// The script UTxO to consume
    pub utxo: UtxoApi,
    /// Raw datum CBOR bytes (needed for script input)
    pub datum_cbor: Vec<u8>,
    /// Payout obligations to fulfill
    pub payouts: Vec<DatumPayout>,
    /// Marketplace contract version
    pub marketplace_version: MarketplaceType,
}

/// Parse a JPG.store listing datum (raw CBOR bytes) into payout obligations.
///
/// Supports V1/V2/V3 datum format:
/// ```text
/// Constructor(0) [
///   owner_pkh: ByteString,        // 28 bytes
///   payouts: [
///     Constructor(0) [
///       address: Constructor(0) [  // Shelley address components
///         payment_credential,
///         maybe_staking_credential,
///       ],
///       amount_map: Map {          // { "": { "": lovelace_amount } }
///         "": Map { "": amount }
///       }
///     ],
///     ...
///   ]
/// ]
/// ```
pub fn parse_listing_datum(
    datum_cbor: &[u8],
    version: MarketplaceType,
    network_id: u8,
) -> Result<Vec<DatumPayout>, TxBuildError> {
    match version {
        MarketplaceType::JpgStoreV1 | MarketplaceType::JpgStoreV2 | MarketplaceType::JpgStoreV3 => {
            parse_jpg_v1_v2_v3_datum(datum_cbor, network_id)
        }
        _ => Err(TxBuildError::BuildFailed(format!(
            "Unsupported marketplace version for buy TX: {version:?}"
        ))),
    }
}

/// Parse JPG.store V1/V2/V3 datum.
///
/// Two known layouts:
/// - Layout A: Constructor(0) [owner_pkh, payouts_list]   — owner first
/// - Layout B: Constructor(0) [payouts_list, owner_pkh]   — payouts first (observed in V2)
///
/// We detect which layout by checking field types.
fn parse_jpg_v1_v2_v3_datum(
    datum_cbor: &[u8],
    network_id: u8,
) -> Result<Vec<DatumPayout>, TxBuildError> {
    let data: PlutusData =
        minicbor::decode(datum_cbor).map_err(|e| TxBuildError::CborParse(format!("{e}")))?;

    let fields = extract_constr_fields(&data, Some(0))?;
    if fields.len() < 2 {
        return Err(TxBuildError::BuildFailed(
            "Datum has fewer than 2 fields".to_string(),
        ));
    }

    // Detect layout: if field[0] is an Array, payouts are first (Layout B)
    // If field[0] is BoundedBytes, owner is first (Layout A)
    let payouts_field_idx = match &fields[0] {
        PlutusData::Array(_) => 0,
        _ => 1,
    };

    let payouts_list = match &fields[payouts_field_idx] {
        PlutusData::Array(arr) => arr,
        _ => {
            return Err(TxBuildError::BuildFailed(format!(
                "Expected payouts array at datum field[{payouts_field_idx}]"
            )));
        }
    };

    // Fail closed: every payout must parse. Skipping an unparseable one would
    // build a TX that underpays a target the validator checks, so the failure
    // surfaces as a rejected script instead of a legible error — and if a
    // validator ever *didn't* check it, we'd silently rob a royalty recipient.
    let payouts = payouts_list
        .iter()
        .enumerate()
        .map(|(i, payout_data)| {
            parse_single_payout(payout_data, network_id).map_err(|e| {
                TxBuildError::BuildFailed(format!(
                    "Payout {i} of {} could not be parsed: {e}",
                    payouts_list.len()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if payouts.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "Datum carries an empty payouts list".to_string(),
        ));
    }

    Ok(payouts)
}

/// Parse a single payout entry: Constructor(0) [address_constr, amount_map]
fn parse_single_payout(data: &PlutusData, network_id: u8) -> Result<DatumPayout, TxBuildError> {
    let fields = extract_constr_fields(data, Some(0))?;
    if fields.len() < 2 {
        return Err(TxBuildError::BuildFailed(
            "Payout has fewer than 2 fields".to_string(),
        ));
    }

    let address = parse_payout_address(&fields[0], network_id)?;
    let lovelace = parse_payout_amount(&fields[1])?;

    Ok(DatumPayout { address, lovelace })
}

/// Parse the address from a payout's address constructor.
///
/// JPG.store datums encode addresses as:
/// ```text
/// Constructor(0) [
///   payment_credential: Constructor(0/1) [ByteString],  // 0=PubKeyHash, 1=ScriptHash
///   staking_credential: Constructor(0/1) [               // 0=Some, 1=None
///     Constructor(0) [                                    // StakingHash
///       Constructor(0/1) [ByteString]                     // 0=PubKeyHash, 1=ScriptHash
///     ]
///   ]
/// ]
/// ```
fn parse_payout_address(data: &PlutusData, network_id: u8) -> Result<Address, TxBuildError> {
    let addr_fields = extract_constr_fields(data, Some(0))?;
    if addr_fields.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "Address constructor has no fields".to_string(),
        ));
    }

    // Payment credential
    let (pay_tag, pay_inner) = extract_constr_tag_and_fields(&addr_fields[0])?;
    let pay_hash = extract_bytes(&pay_inner[0])?;
    if pay_hash.len() != 28 {
        return Err(TxBuildError::BuildFailed(format!(
            "Payment credential hash must be 28 bytes, got {}",
            pay_hash.len()
        )));
    }
    let pay_hash_arr: [u8; 28] = pay_hash.try_into().unwrap();

    // Staking credential (optional)
    let staking = if addr_fields.len() > 1 {
        parse_staking_credential(&addr_fields[1])?
    } else {
        None
    };

    // Build address bytes manually
    // Header byte: network_id | (type << 4)
    // Type 0: key pay + key stake, Type 1: script pay + key stake
    // Type 2: key pay + script stake, Type 3: script pay + script stake
    // Type 6: key pay + no stake, Type 7: script pay + no stake
    let mut addr_bytes = Vec::with_capacity(57);

    match (pay_tag, &staking) {
        (0, Some((0, _))) => addr_bytes.push(network_id), // key pay + key stake
        (1, Some((0, _))) => addr_bytes.push(0x10 | network_id), // script pay + key stake
        (0, Some((1, _))) => addr_bytes.push(0x20 | network_id), // key pay + script stake
        (1, Some((1, _))) => addr_bytes.push(0x30 | network_id), // script pay + script stake
        (0, None) => addr_bytes.push(0x60 | network_id),  // key pay + no stake (enterprise)
        (1, None) => addr_bytes.push(0x70 | network_id),  // script pay + no stake
        _ => {
            return Err(TxBuildError::BuildFailed(format!(
                "Unexpected address credential tags: pay={pay_tag}"
            )));
        }
    }

    addr_bytes.extend_from_slice(&pay_hash_arr);

    if let Some((_, stake_hash)) = staking {
        addr_bytes.extend_from_slice(&stake_hash);
    }

    Address::from_bytes(&addr_bytes)
        .map_err(|e| TxBuildError::BuildFailed(format!("Failed to construct address: {e}")))
}

/// Parse staking credential from datum.
/// Returns Some((tag, 28-byte-hash)) or None if the staking is "None" (Constructor(1) []).
fn parse_staking_credential(data: &PlutusData) -> Result<Option<(u64, [u8; 28])>, TxBuildError> {
    let (tag, fields) = extract_constr_tag_and_fields(data)?;

    if tag == 1 {
        // Constructor(1) [] means None
        return Ok(None);
    }

    // Constructor(0) [StakingHash]
    if fields.is_empty() {
        return Ok(None);
    }

    // StakingHash: Constructor(0) [Constructor(tag) [ByteString]]
    let staking_hash_fields = extract_constr_fields(&fields[0], Some(0))?;
    if staking_hash_fields.is_empty() {
        return Ok(None);
    }

    let (cred_tag, cred_inner) = extract_constr_tag_and_fields(&staking_hash_fields[0])?;
    let hash_bytes = extract_bytes(&cred_inner[0])?;
    if hash_bytes.len() != 28 {
        return Err(TxBuildError::BuildFailed(format!(
            "Staking credential hash must be 28 bytes, got {}",
            hash_bytes.len()
        )));
    }
    let hash_arr: [u8; 28] = hash_bytes.try_into().unwrap();

    Ok(Some((cred_tag, hash_arr)))
}

/// Maximum nesting to descend looking for the lovelace leaf. The deepest real
/// shape seen is 3 (`Map -> Constr -> Map -> Int`); the cap only stops a
/// malformed datum from recursing without bound.
const MAX_AMOUNT_DEPTH: u8 = 8;

/// Parse the lovelace amount from a payout.
///
/// The amount is a Plutus `Value`, and jpg.store has shipped three encodings of
/// it across V1–V3. All of them bottom out at the same place — the integer
/// under the ADA policy (empty ByteString) and the lovelace name (also empty):
///
/// ```text
/// A  Map { "" => Map { "" => Int } }                     // the documented shape
/// B  Map { "" => Constr _ [ Int, Map { "" => Int } ] }   // live V1, see the golden test
/// C  Int                                                 // bare lovelace
/// ```
///
/// Shape B is why this is a descent rather than two nested loops: the earlier
/// two-level version bailed on the intervening `Constr` and reported "no
/// payouts", making every V1 listing unbuyable. Note B's constructor also
/// carries a leading `Int 0` — so "find the first integer" is exactly wrong and
/// would value the payout at zero. Only integers reached as a *map value* count.
fn parse_payout_amount(data: &PlutusData) -> Result<u64, TxBuildError> {
    parse_payout_amount_at(data, 0)
}

fn parse_payout_amount_at(data: &PlutusData, depth: u8) -> Result<u64, TxBuildError> {
    if depth > MAX_AMOUNT_DEPTH {
        return Err(TxBuildError::BuildFailed(format!(
            "Payout amount nested deeper than {MAX_AMOUNT_DEPTH} levels"
        )));
    }

    match data {
        PlutusData::BigInt(big_int) => extract_big_int_value(big_int),

        // Prefer the ADA entry (empty-ByteString key). A single-entry map is
        // unambiguous whatever the key. Anything else is a genuine multi-asset
        // payout, which an ADA-only output cannot satisfy — fail rather than
        // guess which entry is the price.
        PlutusData::Map(map) => {
            let entries: Vec<_> = map.iter().collect();
            let chosen = entries
                .iter()
                .find(|(k, _)| matches!(k, PlutusData::BoundedBytes(b) if b.is_empty()))
                .or(if entries.len() == 1 {
                    entries.first()
                } else {
                    None
                });

            match chosen {
                Some((_, value)) => parse_payout_amount_at(value, depth + 1),
                None => Err(TxBuildError::BuildFailed(format!(
                    "Payout amount has {} map entries and none is the ADA (empty) key",
                    entries.len()
                ))),
            }
        }

        // Descend through wrapper constructors, skipping scalar fields — those
        // are tags/flags, never the amount.
        PlutusData::Constr(constr) => {
            for field in constr.fields.iter() {
                if matches!(field, PlutusData::Map(_) | PlutusData::Constr(_)) {
                    return parse_payout_amount_at(field, depth + 1);
                }
            }
            Err(TxBuildError::BuildFailed(
                "Payout amount constructor has no Map or Constr field to descend into".to_string(),
            ))
        }

        _ => Err(TxBuildError::BuildFailed(format!(
            "Expected Map, Constr or Int for payout amount, got: {data:?}"
        ))),
    }
}

// --- PlutusData helpers ---

/// Extract fields from a Constructor, optionally verifying the tag.
fn extract_constr_fields(
    data: &PlutusData,
    expected_tag: Option<u64>,
) -> Result<Vec<PlutusData>, TxBuildError> {
    match data {
        PlutusData::Constr(constr) => {
            if let Some(tag) = expected_tag
                && constr.tag != (121 + tag)
                && constr.tag != tag
            {
                // pallas uses raw CBOR tag (121 = Constructor 0, 122 = Constructor 1, etc.)
                // but also sometimes the "compact" form
                let effective_tag = if constr.tag >= 121 && constr.tag <= 127 {
                    constr.tag - 121
                } else {
                    constr.tag
                };
                if effective_tag != tag {
                    return Err(TxBuildError::BuildFailed(format!(
                        "Expected constructor tag {tag}, got {} (raw: {})",
                        effective_tag, constr.tag
                    )));
                }
            }
            Ok(constr.fields.iter().cloned().collect())
        }
        _ => Err(TxBuildError::BuildFailed(format!(
            "Expected Constructor, got: {data:?}"
        ))),
    }
}

/// Extract constructor tag and fields.
fn extract_constr_tag_and_fields(
    data: &PlutusData,
) -> Result<(u64, Vec<PlutusData>), TxBuildError> {
    match data {
        PlutusData::Constr(constr) => {
            let tag = if constr.tag >= 121 && constr.tag <= 127 {
                constr.tag - 121
            } else {
                constr.tag
            };
            Ok((tag, constr.fields.iter().cloned().collect()))
        }
        _ => Err(TxBuildError::BuildFailed(format!(
            "Expected Constructor, got: {data:?}"
        ))),
    }
}

/// Extract raw bytes from a PlutusData::BoundedBytes.
fn extract_bytes(data: &PlutusData) -> Result<Vec<u8>, TxBuildError> {
    match data {
        PlutusData::BoundedBytes(bytes) => Ok(bytes.to_vec()),
        _ => Err(TxBuildError::BuildFailed(format!(
            "Expected BoundedBytes, got: {data:?}"
        ))),
    }
}

/// Extract u64 from a BigInt PlutusData value.
fn extract_big_int_value(big_int: &pallas_primitives::conway::BigInt) -> Result<u64, TxBuildError> {
    use pallas_primitives::conway::BigInt;
    match big_int {
        BigInt::Int(int_val) => {
            let val: i128 = (*int_val).into();
            if val < 0 {
                Err(TxBuildError::BuildFailed(format!(
                    "Negative amount in payout: {val}"
                )))
            } else {
                Ok(val as u64)
            }
        }
        BigInt::BigUInt(bytes) => {
            // Big-endian unsigned integer
            let mut val = 0u64;
            for b in bytes.iter() {
                val = val
                    .checked_shl(8)
                    .and_then(|v| v.checked_add(*b as u64))
                    .ok_or_else(|| TxBuildError::BuildFailed("BigUInt overflow".to_string()))?;
            }
            Ok(val)
        }
        BigInt::BigNInt(_) => Err(TxBuildError::BuildFailed(
            "Negative BigNInt in payout".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real V2 listing datum from the analyzed sweep TX.
    /// This is a simplified test using manually constructed PlutusData.
    #[test]
    fn test_extract_constr_fields() {
        use pallas_codec::utils::MaybeIndefArray;
        use pallas_primitives::conway::{Constr, PlutusData};

        let data = PlutusData::Constr(Constr {
            tag: 121, // Constructor(0)
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(vec![0u8; 28].into()),
                PlutusData::Array(MaybeIndefArray::Def(vec![])),
            ]),
        });

        let fields = extract_constr_fields(&data, Some(0)).unwrap();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_extract_big_int_value() {
        use pallas_primitives::conway::BigInt;

        let val = BigInt::Int(3_000_000.into());
        assert_eq!(extract_big_int_value(&val).unwrap(), 3_000_000);
    }
}
