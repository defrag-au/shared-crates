//! The PlutusData codec — the minicbor *design* (integer field ids, absent
//! means default, unknown preserved) on the PlutusData *target*.
//!
//! Why not serde: `pallas_primitives::PlutusData` is exactly five things
//! (`Constr`, `Map`, `Array`, `BigInt`, `BoundedBytes`) with no text
//! strings, no bools, no nulls and no field names, and serde's data model
//! cannot see per-field integer ids. Why not positional `Constr`: every
//! hand-rolled datum decoder in the tree is positional, which is right for
//! reading someone else's fixed layout and exactly wrong for a schema we
//! intend to evolve for years. See `ACTION_DEFINITION_SCHEMA.md` §1.

mod envelope;
mod error;
mod map;
mod scalar;

pub use envelope::{Cip68Envelope, Envelope, constr, constr_zero, read_constr, read_constr_zero};
pub use error::{DecodeError, DecodeErrorKind, UnknownReport};
pub use map::{MapReader, MapWriter, UnknownFields};
pub use scalar::{
    Bytes, PlutusCodec, as_array, as_bytes, as_constr, as_i128, as_map, int_data, shape_of,
};
