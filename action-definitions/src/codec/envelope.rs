//! The three frozen envelopes (schema §2.1, §2.1a).
//!
//! These are the only positional structures in the format. Everything below
//! them is an integer-keyed map, because a positional field cannot be
//! optional, cannot be skipped by an older reader, and cannot be inserted
//! without renumbering everything after it.
//!
//! | Envelope | Shape | Read by |
//! |---|---|---|
//! | definition | `Constr 0 [version, owner, body: Map]` | `registry.ak` (owner only) |
//! | CIP-68 | `Constr 0 [metadata: Map, version, extra]` | `fuel.ak` (extra only), and every wallet |
//! | claim tag | `Constr 0 [definition_tx, claim_id, recipient, claim_slot]` | `escrow.ak` (all four) |
//!
//! A decoder tolerates **trailing** fields it does not know, so a future
//! addition to the end of an envelope does not break an older reader.

use pallas_codec::utils::MaybeIndefArray;
use pallas_primitives::{Constr, PlutusData};

use super::scalar::{as_constr, PlutusCodec};
use super::DecodeError;

/// `Constr 0` — the only constructor index any envelope here uses.
const CONSTR_ZERO: u64 = 121;

pub fn constr_zero(fields: Vec<PlutusData>) -> PlutusData {
    constr(0, fields)
}

/// A `Constr` at an arbitrary small constructor index.
///
/// Needed because a few structures in this format are **the ledger's own**,
/// not ours — an address credential is `Constr 0/1 [hash]` because that is
/// what a Plutus `Address` is, and a validator casts one straight out of a
/// transaction output. Our own enums use the integer-keyed map with the tag
/// at key 0; these do not get a choice.
pub fn constr(index: u64, fields: Vec<PlutusData>) -> PlutusData {
    PlutusData::Constr(Constr {
        tag: CONSTR_ZERO + index,
        any_constructor: None,
        fields: MaybeIndefArray::Def(fields),
    })
}

/// Read a `Constr` at any index, returning `(index, fields)`.
pub fn read_constr(data: &PlutusData) -> Result<(u64, &[PlutusData]), DecodeError> {
    let constr = as_constr(data)?;
    Ok((constr_index(constr)?, constr.fields.as_ref()))
}

/// Read a `Constr 0` and check it carries at least `min_fields`.
pub fn read_constr_zero(
    data: &PlutusData,
    min_fields: usize,
) -> Result<&[PlutusData], DecodeError> {
    let constr = as_constr(data)?;
    let index = constr_index(constr)?;
    if index != 0 {
        return Err(DecodeError::WrongConstructor {
            expected: 0,
            found: index,
        });
    }
    let fields: &[PlutusData] = constr.fields.as_ref();
    if fields.len() < min_fields {
        return Err(DecodeError::ShortEnvelope {
            expected: min_fields,
            actual: fields.len(),
        });
    }
    Ok(fields)
}

/// `Constr::constr_index` panics on a malformed tag; this returns an error.
fn constr_index(constr: &Constr<PlutusData>) -> Result<u64, DecodeError> {
    match constr.tag {
        121..=127 => Ok(constr.tag - 121),
        1280..=1400 => Ok(constr.tag - 1280 + 7),
        102 => constr.any_constructor.ok_or(DecodeError::WrongShape {
            expected: "constr with an explicit constructor index",
            found: "constr",
        }),
        _ => Err(DecodeError::WrongShape {
            expected: "a plutus constr tag",
            found: "constr",
        }),
    }
}

// ── the definition envelope ────────────────────────────────────────────────

/// `Constr 0 [version, owner, body]`, where the registry validator reads
/// `owner` and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Schema version — semantics only, never shape (§2.5).
    pub version: u64,
    /// 28-byte payment key hash. The only field the validator reads.
    pub owner: [u8; 28],
    /// Everything else, by integer id.
    pub body: PlutusData,
}

impl Envelope {
    /// The highest schema version this build understands.
    pub const SUPPORTED_VERSION: u64 = 1;

    pub fn to_data(&self) -> PlutusData {
        constr_zero(vec![
            self.version.to_data(),
            self.owner.to_data(),
            self.body.clone(),
        ])
    }

    pub fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let fields = read_constr_zero(data, 3)?;
        let version = u64::from_data(&fields[0])?;
        if version > Self::SUPPORTED_VERSION {
            // Never a partial decode presented as whole (§2.5). The caller
            // keeps the raw datum and says "needs a newer worker".
            return Err(DecodeError::Unsupported { version });
        }
        Ok(Self {
            version,
            owner: <[u8; 28]>::from_data(&fields[1])?,
            body: fields[2].clone(),
        })
    }
}

// ── the CIP-68 envelope ────────────────────────────────────────────────────

/// `Constr 0 [metadata, version, extra]` — CIP-68's, not ours.
///
/// `metadata` stays **opaque bytes to us**: the fuel validator compares it
/// unchanged across a `TopUp` or `Reconcile` continuing output, and wallets
/// render it. Our `FuelBody` is `extra`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cip68Envelope {
    pub metadata: PlutusData,
    pub version: u64,
    pub extra: PlutusData,
}

impl Cip68Envelope {
    /// CIP-68's own datum version, unrelated to our schema version.
    pub const CIP68_VERSION: u64 = 1;

    pub fn to_data(&self) -> PlutusData {
        constr_zero(vec![
            self.metadata.clone(),
            self.version.to_data(),
            self.extra.clone(),
        ])
    }

    pub fn from_data(data: &PlutusData) -> Result<Self, DecodeError> {
        let fields = read_constr_zero(data, 3)?;
        Ok(Self {
            metadata: fields[0].clone(),
            version: u64::from_data(&fields[1])?,
            extra: fields[2].clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{MapWriter, UnknownFields};
    use pallas_primitives::Fragment;

    fn body() -> PlutusData {
        let mut w = MapWriter::new();
        w.field(0, &1u64);
        w.finish()
    }

    #[test]
    fn the_definition_envelope_round_trips() {
        let env = Envelope {
            version: 1,
            owner: [9u8; 28],
            body: body(),
        };
        let data = env.to_data();
        assert_eq!(Envelope::from_data(&data).unwrap(), env);

        let bytes = data.encode_fragment().unwrap();
        let reparsed = PlutusData::decode_fragment(&bytes).unwrap();
        assert_eq!(Envelope::from_data(&reparsed).unwrap(), env);
    }

    #[test]
    fn a_newer_schema_version_is_refused_not_half_read() {
        let env = Envelope {
            version: Envelope::SUPPORTED_VERSION + 1,
            owner: [1u8; 28],
            body: body(),
        };
        assert_eq!(
            Envelope::from_data(&env.to_data()).unwrap_err(),
            DecodeError::Unsupported {
                version: Envelope::SUPPORTED_VERSION + 1
            }
        );
    }

    #[test]
    fn trailing_envelope_fields_are_tolerated() {
        // A future writer appends a fourth field; this build still reads it.
        let data = constr_zero(vec![
            1u64.to_data(),
            [2u8; 28].to_data(),
            body(),
            vec![7u8].to_data(),
        ]);
        let env = Envelope::from_data(&data).unwrap();
        assert_eq!(env.owner, [2u8; 28]);
    }

    #[test]
    fn a_short_envelope_is_an_error() {
        let data = constr_zero(vec![1u64.to_data(), [2u8; 28].to_data()]);
        assert_eq!(
            Envelope::from_data(&data).unwrap_err(),
            DecodeError::ShortEnvelope {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn a_wrong_constructor_is_an_error() {
        let data = PlutusData::Constr(Constr {
            tag: 122, // Constr 1
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![1u64.to_data(), [2u8; 28].to_data(), body()]),
        });
        assert_eq!(
            Envelope::from_data(&data).unwrap_err(),
            DecodeError::WrongConstructor {
                expected: 0,
                found: 1
            }
        );
    }

    #[test]
    fn the_cip68_envelope_leaves_metadata_untouched() {
        let mut meta = MapWriter::new();
        meta.field(0, &"name".to_string());
        let metadata = meta.finish();

        let env = Cip68Envelope {
            metadata: metadata.clone(),
            version: Cip68Envelope::CIP68_VERSION,
            extra: body(),
        };
        let back = Cip68Envelope::from_data(&env.to_data()).unwrap();
        assert_eq!(back.metadata, metadata, "metadata must survive byte-exact");
        assert_eq!(back, env);
    }

    #[test]
    fn unknown_fields_default_to_empty() {
        assert!(UnknownFields::default().is_empty());
    }
}
