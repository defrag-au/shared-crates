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
    /// Whether the datum sits **inline** on the UTxO, as opposed to being
    /// referenced by hash.
    ///
    /// This decides whether `datum_cbor` is witnessed, and getting it wrong
    /// fails the transaction in one of two opposite ways: omitting the preimage
    /// for a hash datum is `MissingRequiredDatums`, while witnessing one that is
    /// already inline is `NotAllowedSupplementalDatums`. It cannot be inferred
    /// from the bytes — only the UTxO knows — so the resolver that fetched the
    /// datum must record it. jpg.store listings are hash-kind in practice.
    pub datum_is_inline: bool,
    /// Payout obligations to fulfill
    pub payouts: Vec<DatumPayout>,
    /// Marketplace contract version
    pub marketplace_version: MarketplaceType,
    /// The referenced validator as the chain describes it: its Plutus language
    /// and its serialised size.
    ///
    /// Resolved from the reference UTxO, never assumed — see [`ScriptRefInfo`].
    pub script_ref: ScriptRefInfo,
}

/// What the chain says about a deployed reference script.
///
/// Both fields are read off the reference UTxO (`reference_script.type` and
/// `.size`), because both are load-bearing at SUBMIT and neither is visible to
/// `evaluateTransaction`:
///
/// - **language** names the cost model in the transaction's language views,
///   which are hashed into the script-integrity hash. Guess it and the node
///   computes a different hash and rejects with `ScriptIntegrityHashMismatch`.
/// - **size** drives Conway's `minFeeRefScriptCoinsPerByte`. Omit it and the
///   node rejects with `FeeTooSmallUTxO`.
///
/// Both were previously guessed or ignored, and every jpg buy evaluated
/// perfectly and then failed on submit with exactly those two errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptRefInfo {
    pub language: pallas_txbuilder::ScriptKind,
    /// Serialised script size in bytes.
    pub size: u64,
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

    /// The real hash-kind datum behind SpaceBud #1224, listed for 5000 ₳ at the
    /// jpg.store V1 sale script (UTxO
    /// `1d5b18556e6aeec81ddd4eb3e5f1e9a3ec226bf03596498e20d522bcb2809887#0`,
    /// pulled from Koios `/datum_info` on 2026-09-07).
    ///
    /// This is the datum the previous two-level amount parser could not read —
    /// it reported "No payouts could be parsed from datum" and made every V1
    /// listing unbuyable.
    const SPACEBUD_1224_V1_DATUM: &str = "d8799f581c7332086de38a8697e4ae475b4f5f4135946e0f8b83ceeca788b4ae889fd8799fd8799fd8799f581c740e42a7823c3ba189a7b54ee61533ca65b261a75e143a0e280faab0ffd8799fd8799fd8799f581cc9d531ef19bd56d932dcc8b02c3092c0392a16b2909e7ffd0711209affffffffa140d8799f00a1401a07270e00ffffd8799fd8799fd8799f581c70e60f3b5ea7153e0acc7a803e4401d44b8ed1bae1c7baaad1a62a72ffd8799fd8799fd8799f581c1e78aae7c90cc36d624f7b3bb6d86b52696dc84e490f343eba89005fffffffffa140d8799f00a1401a05f5e100ffffd8799fd8799fd8799f581c7332086de38a8697e4ae475b4f5f4135946e0f8b83ceeca788b4ae88ffd8799fd8799fd8799f581cbbdd2ef5dd2ce6fbef843b2f90d8295183ebfd27032fb29681dceac1ffffffffa140d8799f00a1401b000000011ce90300ffffffff";

    #[test]
    fn test_parse_live_jpg_v1_listing_datum() {
        let bytes = hex::decode(SPACEBUD_1224_V1_DATUM).expect("fixture hex");
        let payouts = parse_listing_datum(&bytes, MarketplaceType::JpgStoreV1, 1)
            .expect("live V1 listing datum must parse");

        // Three payouts: marketplace fee, royalty, seller take. All three are
        // required — dropping any one builds a TX the validator rejects.
        let amounts: Vec<u64> = payouts.iter().map(|p| p.lovelace).collect();
        assert_eq!(amounts, vec![120_000_000, 100_000_000, 4_780_000_000]);

        // Sums to exactly the 5000 ₳ list price. This is the assertion that
        // proves we read the right integers: shape B nests a decoy `Int 0`
        // beside the real amount, and picking it would total 0.
        assert_eq!(amounts.iter().sum::<u64>(), 5_000_000_000);
    }

    #[test]
    fn test_live_datum_payout_addresses_are_mainnet_bech32() {
        let bytes = hex::decode(SPACEBUD_1224_V1_DATUM).expect("fixture hex");
        let payouts = parse_listing_datum(&bytes, MarketplaceType::JpgStoreV1, 1).unwrap();

        for payout in &payouts {
            let bech32 = payout.address.to_bech32().expect("payout address encodes");
            assert!(
                bech32.starts_with("addr1"),
                "expected a mainnet address, got {bech32}"
            );
        }

        // The seller's take is the largest payout, and it must go back to the
        // datum's own owner_pkh. Asserting on the payment credential rather
        // than a bech32 prefix keeps this about the datum's semantics instead
        // of the address encoding.
        const OWNER_PKH: &str = "7332086de38a8697e4ae475b4f5f4135946e0f8b83ceeca788b4ae88";
        let seller = payouts.last().unwrap();
        assert_eq!(seller.lovelace, 4_780_000_000);
        assert_eq!(
            hex::encode(&seller.address.to_vec()[1..29]),
            OWNER_PKH,
            "seller payout must pay the datum's owner_pkh"
        );

        // …and the fee/royalty payouts must NOT, or we'd be paying the seller
        // money the validator expects elsewhere.
        for other in &payouts[..payouts.len() - 1] {
            assert_ne!(hex::encode(&other.address.to_vec()[1..29]), OWNER_PKH);
        }
    }

    /// A partially-unreadable payouts list must fail, not silently drop the
    /// entry — see the "fail closed" comment in `parse_jpg_v1_v2_v3_datum`.
    #[test]
    fn test_unparseable_payout_fails_rather_than_being_skipped() {
        use pallas_codec::utils::MaybeIndefArray;
        use pallas_primitives::conway::{Constr, PlutusData};

        let junk_payout = PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(vec![0u8; 4].into()),
                PlutusData::BoundedBytes(vec![0u8; 4].into()),
            ]),
        });
        let datum = PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(vec![0u8; 28].into()),
                PlutusData::Array(MaybeIndefArray::Def(vec![junk_payout])),
            ]),
        });

        let mut encoded = Vec::new();
        minicbor::encode(&datum, &mut encoded).expect("encode fixture");

        let err = parse_listing_datum(&encoded, MarketplaceType::JpgStoreV1, 1)
            .expect_err("an unreadable payout must fail the whole parse");
        assert!(
            format!("{err:?}").contains("Payout 0 of 1"),
            "error should name the offending payout, got: {err:?}"
        );
    }

    /// The decoy-integer regression, isolated: shape B's constructor carries a
    /// leading `Int 0` before the map holding the real amount.
    #[test]
    fn test_amount_ignores_decoy_integer_in_wrapper_constructor() {
        use pallas_codec::utils::MaybeIndefArray;
        use pallas_primitives::conway::{BigInt, Constr, PlutusData};

        let inner = PlutusData::Map(
            MaybeIndefArray::Def(vec![(
                PlutusData::BoundedBytes(Vec::new().into()),
                PlutusData::BigInt(BigInt::Int(120_000_000.into())),
            )])
            .to_vec()
            .into(),
        );

        let wrapper = PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![PlutusData::BigInt(BigInt::Int(0.into())), inner]),
        });

        let outer =
            PlutusData::Map(vec![(PlutusData::BoundedBytes(Vec::new().into()), wrapper)].into());

        assert_eq!(parse_payout_amount(&outer).unwrap(), 120_000_000);
    }

    /// A multi-asset payout has no single lovelace answer — refuse rather than
    /// pick an arbitrary entry.
    #[test]
    fn test_ambiguous_multi_policy_amount_is_rejected() {
        use pallas_primitives::conway::{BigInt, PlutusData};

        let outer = PlutusData::Map(
            vec![
                (
                    PlutusData::BoundedBytes(vec![1u8; 28].into()),
                    PlutusData::BigInt(BigInt::Int(5.into())),
                ),
                (
                    PlutusData::BoundedBytes(vec![2u8; 28].into()),
                    PlutusData::BigInt(BigInt::Int(7.into())),
                ),
            ]
            .into(),
        );

        assert!(parse_payout_amount(&outer).is_err());
    }
}
