//! CIP-68 reference-token datum decoding.
//!
//! CIP-68 metadata lives on-chain in the inline datum of the `000643b0`
//! reference token, encoded as a Plutus
//! `Constr 0 [metadata_map, version, extra?]`. [`decode_cip68_datum`]
//! turns that raw datum CBOR into the same typed [`AssetMetadata`] this
//! crate already produces from CIP-25 JSON — the CIP-68 metadata map
//! mirrors the CIP-25 shape, so once it is rendered to a JSON value the
//! existing decoder handles every known variant. Downstream consumers
//! (trait extraction, [`crate::cid`] CID extraction) are unchanged.
//!
//! Behind the `cip68` feature; pulls in `pallas-codec` / `pallas-primitives`.

use crate::{AssetMetadata, AssetMetadata68, NftPurpose};
use pallas_primitives::{BigInt, PlutusData};
use serde_json::{Map, Number, Value};
use std::fmt;

/// CBOR tag of a Plutus `Constructor 0` value.
const CONSTR_0_TAG: u64 = 121;

/// Failure modes when decoding a CIP-68 reference-token datum.
#[derive(Debug)]
pub enum Cip68Error {
    /// The datum bytes were not valid `PlutusData` CBOR.
    Cbor(String),
    /// The datum was valid CBOR but not a constructor value.
    NotConstructor,
    /// The constructor was not `Constructor 0`, which CIP-68 requires.
    WrongConstructor,
    /// The constructor carried no fields — no metadata map present.
    EmptyDatum,
    /// The metadata map did not match any known `AssetMetadata` shape.
    Metadata(serde_json::Error),
}

impl fmt::Display for Cip68Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cip68Error::Cbor(e) => write!(f, "invalid PlutusData CBOR: {e}"),
            Cip68Error::NotConstructor => f.write_str("datum is not a constructor value"),
            Cip68Error::WrongConstructor => f.write_str("datum constructor is not Constructor 0"),
            Cip68Error::EmptyDatum => f.write_str("datum constructor has no fields"),
            Cip68Error::Metadata(e) => {
                write!(f, "metadata map did not match a known shape: {e}")
            }
        }
    }
}

impl std::error::Error for Cip68Error {}

/// Decode a CIP-68 reference-token datum (raw Plutus data CBOR) into a
/// typed [`AssetMetadata68`].
///
/// The datum is expected to be `Constr 0 [metadata_map, version, extra?]`.
/// The `version` field is read when present (defaulting to `1`); the
/// optional `extra` field is ignored. The returned
/// [`AssetMetadata68::purpose`] is always [`NftPurpose::ReferenceNft`] —
/// CIP-68 metadata datums only ever live on the reference token.
pub fn decode_cip68_datum(datum_cbor: &[u8]) -> Result<AssetMetadata68, Cip68Error> {
    let plutus: PlutusData =
        pallas_codec::minicbor::decode(datum_cbor).map_err(|e| Cip68Error::Cbor(e.to_string()))?;

    let constr = match plutus {
        PlutusData::Constr(c) => c,
        _ => return Err(Cip68Error::NotConstructor),
    };
    if constr.tag != CONSTR_0_TAG && constr.any_constructor != Some(0) {
        return Err(Cip68Error::WrongConstructor);
    }

    let fields: Vec<PlutusData> = constr.fields.into();
    let metadata_pd = fields.first().ok_or(Cip68Error::EmptyDatum)?;
    let version = fields.get(1).and_then(plutus_as_u32).unwrap_or(1);

    let metadata: AssetMetadata =
        serde_json::from_value(plutus_to_json(metadata_pd)).map_err(Cip68Error::Metadata)?;

    Ok(AssetMetadata68 {
        purpose: NftPurpose::ReferenceNft,
        version,
        metadata,
    })
}

/// Read a `PlutusData` integer as a `u32`, if it fits.
fn plutus_as_u32(pd: &PlutusData) -> Option<u32> {
    match pd {
        PlutusData::BigInt(BigInt::Int(n)) => u32::try_from(i128::from(*n)).ok(),
        _ => None,
    }
}

/// Render a `PlutusData` value to a `serde_json::Value`.
///
/// Byte strings become UTF-8 strings when valid, otherwise `0x…` hex
/// strings. Maps become objects (keys string-coerced via
/// [`plutus_key`]). Constructors become `{ "__constructor": n,
/// "fields": [...] }`. Integers become JSON numbers, falling back to a
/// decimal string outside the `i64` range.
fn plutus_to_json(pd: &PlutusData) -> Value {
    match pd {
        PlutusData::Constr(c) => {
            let mut obj = Map::new();
            obj.insert(
                "__constructor".to_owned(),
                Value::Number(Number::from(c.any_constructor.unwrap_or(0))),
            );
            obj.insert(
                "fields".to_owned(),
                Value::Array(c.fields.iter().map(plutus_to_json).collect()),
            );
            Value::Object(obj)
        }
        PlutusData::Map(entries) => {
            let mut obj = Map::new();
            for (k, v) in entries.iter() {
                obj.insert(plutus_key(k), plutus_to_json(v));
            }
            Value::Object(obj)
        }
        PlutusData::BigInt(BigInt::Int(n)) => {
            let raw = i128::from(*n);
            i64::try_from(raw).map_or_else(
                |_| Value::String(raw.to_string()),
                |v| Value::Number(v.into()),
            )
        }
        PlutusData::BigInt(BigInt::BigUInt(b) | BigInt::BigNInt(b)) => {
            Value::String(format!("0x{}", hex::encode(&**b)))
        }
        PlutusData::BoundedBytes(b) => bytes_to_json(b),
        PlutusData::Array(items) => Value::Array(items.iter().map(plutus_to_json).collect()),
    }
}

/// Render bytes as a UTF-8 string, or a `0x…` hex string if not valid UTF-8.
fn bytes_to_json(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => Value::String(s.to_owned()),
        Err(_) => Value::String(format!("0x{}", hex::encode(bytes))),
    }
}

/// Coerce a `PlutusData` map key to a JSON object key. CIP-68 metadata
/// keys are byte strings; integer keys are stringified; anything more
/// exotic (a spec violation) is preserved as `0x…` CBOR hex.
fn plutus_key(pd: &PlutusData) -> String {
    match pd {
        PlutusData::BoundedBytes(b) => match std::str::from_utf8(b) {
            Ok(s) => s.to_owned(),
            Err(_) => format!("0x{}", hex::encode(&**b)),
        },
        PlutusData::BigInt(BigInt::Int(n)) => i128::from(*n).to_string(),
        other => {
            let mut buf = Vec::new();
            if pallas_codec::minicbor::encode(other, &mut buf).is_ok() {
                format!("0x{}", hex::encode(&buf))
            } else {
                "<unencodable>".to_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Asset, CidRole};

    // Real CIP-68 reference-token datums lifted from the mitos
    // `asset-metadata-update` Nikeverse1501 fixture: the metadata of a
    // dynamic CIP-68 NFT before and after an on-chain update. The
    // `prev` datum carries a CIDv0 image, the `new` datum a CIDv1 image.
    const NIKEVERSE_PREV: &[u8] =
        include_bytes!("../resources/cip68/nikeverse1501_prev.datum.cbor");
    const NIKEVERSE_NEW: &[u8] = include_bytes!("../resources/cip68/nikeverse1501_new.datum.cbor");

    #[test]
    fn decodes_reference_datum_with_cidv0_image() {
        let decoded = decode_cip68_datum(NIKEVERSE_PREV).expect("prev datum decodes");
        assert_eq!(decoded.purpose, NftPurpose::ReferenceNft);
        assert_eq!(decoded.version, 1);

        let asset = Asset::from(decoded.metadata);
        assert_eq!(asset.name, "Guthix");
        assert_eq!(
            asset.image,
            "ipfs://QmQVcekGM1VRMHZnDjNPvYiEQ3mYYRaXiEAPs2qwHHk8kv"
        );
        assert_eq!(asset.media_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn decodes_reference_datum_with_cidv1_image() {
        let decoded = decode_cip68_datum(NIKEVERSE_NEW).expect("new datum decodes");
        let asset = Asset::from(decoded.metadata);
        assert_eq!(asset.name, "Guthix");
        assert_eq!(
            asset.image,
            "ipfs://bafybeidw54qa6bcbbjnztbbj6cd7qzazr33instef33ql4lws45mp6uw3e"
        );
    }

    #[test]
    fn extracts_cid_from_decoded_datum() {
        // The `prev` datum's CIDv0 image is normalised to CIDv1 on the
        // way out, so the index sees a single stable representation.
        let decoded = decode_cip68_datum(NIKEVERSE_PREV).expect("prev datum decodes");
        let cids = decoded.extract_cids();
        assert_eq!(cids.len(), 1);
        assert_eq!(cids[0].role, CidRole::Image);
        assert_eq!(
            cids[0].cid,
            "bafybeibaanddhnz7v7quubnv3dlngzkl2x56zxllbyhtfck3kybrepaee4"
        );
    }

    #[test]
    fn rejects_non_constructor_datum() {
        // A bare CBOR integer (`1`) is valid CBOR but not a constructor.
        assert!(matches!(
            decode_cip68_datum(&[0x01]),
            Err(Cip68Error::NotConstructor)
        ));
    }

    #[test]
    fn rejects_garbage_cbor() {
        assert!(matches!(
            decode_cip68_datum(&[0xff, 0xfe, 0xfd]),
            Err(Cip68Error::Cbor(_))
        ));
    }
}
