//! CIP-68 datum building for reference tokens
//!
//! CIP-68 defines a standard for storing NFT metadata in inline datums on reference tokens.
//! The datum structure is: `#6.121([metadata, version, extra])`
//!
//! Reference: <https://cips.cardano.org/cip/CIP-0068>

use cardano_assets::AssetMetadata;
use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

use crate::helpers::is_hex_encoded;

use super::MetadataError;

/// CIP-68 datum version (1 = initial version)
const CIP68_VERSION: i64 = 1;

/// Build a CIP-68 datum from [`AssetMetadata`], with an empty `extra`.
///
/// The datum structure follows CIP-68:
/// ```text
/// datum = #6.121([metadata_map, version, extra])
/// ```
///
/// Returns CBOR bytes suitable for use as an inline datum on reference tokens.
///
/// Use [`build_cip68_datum_with_extra`] when the third field carries
/// something — a mint that leaves it empty and a mint that fills it are the
/// same datum shape, and this is the thin wrapper so callers that do not care
/// are not made to say so.
pub fn build_cip68_datum(metadata: &AssetMetadata) -> Result<Vec<u8>, MetadataError> {
    build_cip68_datum_with_extra(metadata, empty_extra())
}

/// The `extra` a datum carries when it carries nothing.
///
/// An empty ARRAY rather than an absent field or a unit: CIP-68 fixes the
/// envelope at three fields, so the third has to be *something*, and this is
/// the shape already on chain for every reference token this crate has ever
/// minted. Changing it would change those datums' bytes.
pub fn empty_extra() -> PlutusData {
    PlutusData::Array(MaybeIndefArray::Def(vec![]))
}

/// Build a CIP-68 datum whose `extra` field carries `extra`.
///
/// CIP-68's third field is free-form PlutusData, and this is what makes it
/// usable: a reference token can be a rendered NFT to every wallet and
/// explorer — name, image, description, out of `metadata` — *and* carry
/// application state a validator reads, in the same datum, with no second
/// UTxO and no parallel registry.
///
/// The on-chain action protocol's fuel tank is the case this was added for
/// (`ONCHAIN_ACTION_PROTOCOL.md` §1.2): `metadata` renders the subscription
/// in the holder's wallet while `extra` holds the credit balance the fuel
/// validator decodes. **The validator reads `extra` only and treats
/// `metadata` as opaque bytes that must be unchanged across every continuing
/// output** — so whatever goes in here must encode identically on every
/// rebuild, which is why the schema crate's codec is canonical by
/// construction.
pub fn build_cip68_datum_with_extra(
    metadata: &AssetMetadata,
    extra: PlutusData,
) -> Result<Vec<u8>, MetadataError> {
    let metadata_map = metadata_to_plutus_map(metadata)?;

    // Build the CIP-68 constructor: Constr 0 with [metadata, version, extra]
    // #6.121 is represented as Constr with alternative = 0
    let datum = PlutusData::Constr(Constr {
        tag: 121, // Constr 0 uses tag 121
        any_constructor: None,
        fields: MaybeIndefArray::Def(vec![
            metadata_map,
            PlutusData::BigInt(BigInt::Int(CIP68_VERSION.into())),
            extra,
        ]),
    });

    datum
        .encode_fragment()
        .map_err(|e| MetadataError::EncodeError(format!("Failed to encode CIP-68 datum: {e}")))
}

/// Convert [`AssetMetadata`] to a PlutusData Map.
fn metadata_to_plutus_map(metadata: &AssetMetadata) -> Result<PlutusData, MetadataError> {
    let json_value = serde_json::to_value(metadata)
        .map_err(|e| MetadataError::EncodeError(format!("Failed to serialize metadata: {e}")))?;

    json_to_plutus_data(&json_value)
}

/// Convert a JSON value to [`PlutusData`].
///
/// Type mapping:
/// - `null` → Constr 0 (empty)
/// - `bool` → Constr 0 (false) / Constr 1 (true)
/// - `number` → BigInt (floats stored as string bytes)
/// - `string` → BoundedBytes (split into 64-byte chunks if >64 bytes)
/// - `array` → Array
/// - `object` → Map (null values skipped)
pub fn json_to_plutus_data(value: &serde_json::Value) -> Result<PlutusData, MetadataError> {
    match value {
        serde_json::Value::Null => Ok(PlutusData::Constr(Constr {
            tag: 121, // Constr 0
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![]),
        })),
        serde_json::Value::Bool(b) => {
            let tag = if *b { 122 } else { 121 };
            Ok(PlutusData::Constr(Constr {
                tag,
                any_constructor: None,
                fields: MaybeIndefArray::Def(vec![]),
            }))
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PlutusData::BigInt(BigInt::Int(i.into())))
            } else if let Some(u) = n.as_u64() {
                Ok(PlutusData::BigInt(BigInt::Int((u as i64).into())))
            } else {
                // Floating point: store as string bytes
                Ok(PlutusData::BoundedBytes(n.to_string().into_bytes().into()))
            }
        }
        serde_json::Value::String(s) => {
            let bytes = s.as_bytes();
            if bytes.len() <= 64 {
                Ok(PlutusData::BoundedBytes(bytes.to_vec().into()))
            } else {
                // CIP-68: split into 64-byte chunks
                let chunks: Vec<PlutusData> = bytes
                    .chunks(64)
                    .map(|chunk| PlutusData::BoundedBytes(chunk.to_vec().into()))
                    .collect();
                Ok(PlutusData::Array(MaybeIndefArray::Def(chunks)))
            }
        }
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<PlutusData>, MetadataError> =
                arr.iter().map(json_to_plutus_data).collect();
            Ok(PlutusData::Array(MaybeIndefArray::Def(items?)))
        }
        serde_json::Value::Object(obj) => {
            let mut map_items: Vec<(PlutusData, PlutusData)> = Vec::new();

            for (key, val) in obj {
                // Skip null values in objects
                if val.is_null() {
                    continue;
                }

                let key_data = PlutusData::BoundedBytes(key.as_bytes().to_vec().into());
                let val_data = json_to_plutus_data(val)?;
                map_items.push((key_data, val_data));
            }

            Ok(PlutusData::Map(map_items.into()))
        }
    }
}

/// Build a CIP-67 prefixed asset name for user or reference tokens.
///
/// The `prefix_hex` is the 4-byte label (e.g. [`cip67::NFT_USER`](super::cip67::NFT_USER),
/// [`cip67::REFERENCE`](super::cip67::REFERENCE)). The `base_name` is either a raw UTF-8
/// name or an already hex-encoded name.
///
/// # Example
/// ```
/// use cardano_tx::metadata::{cip67, cip68};
///
/// let user_name = cip68::get_prefixed_asset_name("TestNFT", cip67::NFT_USER);
/// assert!(hex::encode(&user_name).starts_with("000de140"));
///
/// let ref_name = cip68::get_prefixed_asset_name("TestNFT", cip67::REFERENCE);
/// assert!(hex::encode(&ref_name).starts_with("000643b0"));
/// ```
pub fn get_prefixed_asset_name(base_name: &str, prefix_hex: &str) -> Vec<u8> {
    let prefix = hex::decode(prefix_hex).expect("Valid hex prefix constant");
    let name_bytes = if is_hex_encoded(base_name) {
        hex::decode(base_name).unwrap_or_else(|_| base_name.as_bytes().to_vec())
    } else {
        base_name.as_bytes().to_vec()
    };

    let mut result = prefix;
    result.extend(name_bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::cip67;
    use cardano_assets::{PrimitiveOrList, Traits};

    /// The smallest well-formed metadata, for tests about the ENVELOPE
    /// rather than about metadata contents.
    fn sample_metadata(name: &str) -> AssetMetadata {
        AssetMetadata::Flattened {
            name: name.to_string(),
            image: PrimitiveOrList::Primitive("ipfs://QmTest123".to_string()),
            media_type: Some("image/png".to_string()),
            project: None,
            description: None,
            files: None,
            publisher: None,
            discord: None,
            twitter: None,
            website: None,
            github: None,
            medium: None,
            sha256: None,
            url: None,
            traits: Traits::new(),
        }
    }

    #[test]
    fn test_build_simple_cip68_datum() {
        let metadata = AssetMetadata::Flattened {
            name: "Test NFT #1".to_string(),
            image: PrimitiveOrList::Primitive("ipfs://QmTest123".to_string()),
            media_type: Some("image/png".to_string()),
            project: None,
            description: Some(PrimitiveOrList::Primitive("A test NFT".to_string())),
            files: None,
            publisher: None,
            discord: None,
            twitter: None,
            website: None,
            github: None,
            medium: None,
            sha256: None,
            url: None,
            traits: Traits::new(),
        };

        let datum_bytes = build_cip68_datum(&metadata).expect("Should build datum");

        assert!(!datum_bytes.is_empty());
        // The datum should start with tag 121 (Constr 0) which is 0xd8 0x79
        assert_eq!(datum_bytes[0], 0xd8);
        assert_eq!(datum_bytes[1], 0x79);
    }

    /// Adding the `extra` parameter must not have moved a single byte for
    /// callers that never asked for one.
    ///
    /// `wallet-operations` mints CIP-68 reference tokens with this function,
    /// and those datums are on chain. A wrapper that produced even slightly
    /// different bytes would change the datum of every future mint away from
    /// the shape the existing ones use.
    #[test]
    fn the_empty_extra_wrapper_is_byte_identical_to_the_old_behaviour() {
        let metadata = sample_metadata("Test NFT");

        let wrapper = build_cip68_datum(&metadata).expect("build");
        let explicit =
            build_cip68_datum_with_extra(&metadata, empty_extra()).expect("build with extra");
        assert_eq!(wrapper, explicit);

        // …and the empty extra really is an empty definite array (0x80),
        // which is what the on-chain datums carry.
        assert_eq!(*wrapper.last().expect("non-empty"), 0x80);
    }

    /// End to end with the REAL type: a fuel tank's datum, built here,
    /// decoded back through the schema crate.
    ///
    /// This is the join the protocol depends on — `cardano-tx` writes the
    /// envelope, `action-definitions` owns what goes in `extra`, and the
    /// fuel validator decodes exactly what comes out. A test that used a
    /// hand-rolled map would prove the two agree with a third thing that
    /// nobody ships.
    #[test]
    fn a_real_fuel_body_round_trips_through_the_cip68_envelope() {
        use action_definitions::FuelBody;
        use action_definitions::codec::{Bytes, Cip68Envelope, PlutusCodec};
        use pallas_primitives::Fragment;

        let tank = FuelBody {
            balance: 1_000,
            reconciled_slot: 12_345,
            reconciled_seq: 7,
            receipts_hash: Bytes::from(vec![0xab; 32]),
            scope: None,
            unknown: Default::default(),
        };

        let metadata = sample_metadata("Defrag Fuel");
        let bytes = build_cip68_datum_with_extra(&metadata, tank.to_data()).expect("build");

        // What the fuel validator sees.
        let data = PlutusData::decode_fragment(&bytes).expect("ledger-valid");
        let envelope = Cip68Envelope::from_data(&data).expect("a real CIP-68 datum");
        assert_eq!(envelope.version, Cip68Envelope::CIP68_VERSION);
        assert_eq!(
            FuelBody::from_data(&envelope.extra).expect("extra decodes"),
            tank,
            "the balance the validator reads must be the balance we wrote"
        );

        // And `metadata` is still a map a wallet can render — the half the
        // validator treats as opaque and requires unchanged.
        assert_eq!(
            action_definitions::codec::shape_of(&envelope.metadata),
            "map"
        );
    }

    /// The case the parameter was added for: a fuel tank renders as an NFT
    /// and carries a balance a validator reads, in one datum.
    #[test]
    fn an_extra_payload_lands_in_the_third_field_without_disturbing_metadata() {
        use pallas_primitives::PlutusData;

        let metadata = sample_metadata("Defrag Fuel");

        // Stand-in for a `FuelBody`: an integer-keyed map, as the schema
        // crate encodes one.
        let extra = PlutusData::Map(pallas_codec::utils::KeyValuePairs::Def(vec![(
            PlutusData::BigInt(BigInt::Int(0.into())),
            PlutusData::BigInt(BigInt::Int(1_000.into())),
        )]));

        let with_extra = build_cip68_datum_with_extra(&metadata, extra).expect("build");
        let without = build_cip68_datum(&metadata).expect("build");

        assert_ne!(with_extra, without, "the payload must actually be carried");

        // The metadata half is untouched — the fuel validator compares those
        // bytes across every continuing output, so a change here would break
        // every TopUp.
        let shared_prefix = with_extra
            .iter()
            .zip(without.iter())
            .take_while(|(a, b)| a == b)
            .count();
        assert!(
            shared_prefix > 10,
            "metadata and version should be byte-identical; only the third field differs"
        );
    }

    #[test]
    fn test_user_asset_name_nft() {
        let name = get_prefixed_asset_name("TestNFT", cip67::NFT_USER);
        let hex_name = hex::encode(&name);
        assert!(hex_name.starts_with("000de140"));
        assert!(hex_name.ends_with(&hex::encode("TestNFT")));
    }

    #[test]
    fn test_reference_asset_name() {
        let name = get_prefixed_asset_name("TestNFT", cip67::REFERENCE);
        let hex_name = hex::encode(&name);
        assert!(hex_name.starts_with("000643b0"));
        assert!(hex_name.ends_with(&hex::encode("TestNFT")));
    }

    #[test]
    fn test_long_string_chunking() {
        let long_string = "a".repeat(100);
        let json_val = serde_json::Value::String(long_string);

        let plutus = json_to_plutus_data(&json_val).expect("Should convert");

        match plutus {
            PlutusData::Array(MaybeIndefArray::Def(chunks)) => {
                assert_eq!(chunks.len(), 2); // 100 bytes = 64 + 36
            }
            _ => panic!("Expected array for long string"),
        }
    }

    #[test]
    fn test_json_null_to_constr0() {
        let plutus = json_to_plutus_data(&serde_json::Value::Null).unwrap();
        match plutus {
            PlutusData::Constr(c) => assert_eq!(c.tag, 121),
            _ => panic!("Expected Constr for null"),
        }
    }

    #[test]
    fn test_json_bool_to_constr() {
        let t = json_to_plutus_data(&serde_json::json!(true)).unwrap();
        let f = json_to_plutus_data(&serde_json::json!(false)).unwrap();
        match (t, f) {
            (PlutusData::Constr(ct), PlutusData::Constr(cf)) => {
                assert_eq!(ct.tag, 122); // true = Constr 1
                assert_eq!(cf.tag, 121); // false = Constr 0
            }
            _ => panic!("Expected Constr for booleans"),
        }
    }
}
