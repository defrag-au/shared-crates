//! Generic UTxO **tag datum** — a small, versioned inline datum that classifies
//! a UTxO's role for an application, so coin-selection can distinguish purposes
//! (e.g. delivery fuel vs operator float vs an inbound payment) without amount
//! heuristics or a second address.
//!
//! The mechanism is app-agnostic; the *meaning* of `flags` is owned by the
//! reader. A `namespace` byte string scopes the tag to one purpose, so an
//! unrelated `Constr 0 [..]` datum never decodes as a foreign app's tag.
//!
//! Datum: `Constr 0 [ Bytes(namespace), Int(version), Int(flags: u32) ]`.
//!
//! Inline datums are not script-only — a normal key/enterprise address output
//! can carry one (CIP-32) and is spent with just the key witness.

use pallas_primitives::alonzo::{BigInt, Constr, PlutusData};
use pallas_primitives::{Fragment, MaybeIndefArray};

/// A decoded UTxO tag datum. `flags` is an application-defined `u32` bitfield —
/// the reader interprets it (after filtering by its expected `namespace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtxoTagDatum {
    pub namespace: Vec<u8>,
    pub version: u16,
    pub flags: u32,
}

impl UtxoTagDatum {
    pub fn new(namespace: impl Into<Vec<u8>>, version: u16, flags: u32) -> Self {
        Self {
            namespace: namespace.into(),
            version,
            flags,
        }
    }

    /// Encode to inline-datum CBOR for `Output::set_inline_datum`.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let data = PlutusData::Constr(Constr {
            tag: 121, // Constr 0
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(self.namespace.clone().into()),
                PlutusData::BigInt(BigInt::Int((self.version as i64).into())),
                PlutusData::BigInt(BigInt::Int((self.flags as i64).into())),
            ]),
        });
        data.encode_fragment()
            .map_err(|e| format!("encode tag datum: {e}"))
    }

    /// Decode a UTxO's inline-datum CBOR. Returns `None` when it isn't a tag
    /// datum of this shape (a foreign datum is simply "untagged").
    pub fn decode(cbor: &[u8]) -> Option<Self> {
        let data = PlutusData::decode_fragment(cbor).ok()?;
        let PlutusData::Constr(constr) = data else {
            return None;
        };
        if constr.tag != 121 || constr.fields.len() != 3 {
            return None;
        }
        let namespace = as_bytes(&constr.fields[0])?;
        let version = u16::try_from(as_u64(&constr.fields[1])?).ok()?;
        let flags = u32::try_from(as_u64(&constr.fields[2])?).ok()?;
        Some(Self {
            namespace,
            version,
            flags,
        })
    }

    /// True when this tag is in `namespace` — the per-app scope guard a reader
    /// applies before trusting `flags`.
    pub fn in_namespace(&self, namespace: &[u8]) -> bool {
        self.namespace == namespace
    }
}

fn as_u64(d: &PlutusData) -> Option<u64> {
    match d {
        PlutusData::BigInt(BigInt::Int(int)) => {
            let v: i128 = (*int).into();
            (0..=u64::MAX as i128).contains(&v).then_some(v as u64)
        }
        _ => None,
    }
}

fn as_bytes(d: &PlutusData) -> Option<Vec<u8>> {
    match d {
        PlutusData::BoundedBytes(b) => Some(b.to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let tag = UtxoTagDatum::new(b"fuel".to_vec(), 1, 0b1011);
        let cbor = tag.encode().unwrap();
        let back = UtxoTagDatum::decode(&cbor).unwrap();
        assert_eq!(tag, back);
        assert!(back.in_namespace(b"fuel"));
        assert!(!back.in_namespace(b"float"));
    }

    #[test]
    fn rejects_foreign_datum() {
        // A 2-field Constr 0 (the old fuel-only shape / a Splash-ish datum) is
        // NOT a tag datum — must not false-positive.
        let foreign = PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BigInt(BigInt::Int(1i64.into())),
                PlutusData::BigInt(BigInt::Int(2i64.into())),
            ]),
        });
        let cbor = foreign.encode_fragment().unwrap();
        assert!(UtxoTagDatum::decode(&cbor).is_none());
    }
}
