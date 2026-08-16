//! Case-file schema, records and checksummed export for [`chain_ledger`].
//!
//! A *case* is one investigation: the parties you have identified, how firmly,
//! the clusters you grouped them into, the cached per-transaction ledger, and
//! the claims you have drawn from it.
//!
//! # No database driver here
//!
//! The same case lives in two places — a local file (rusqlite, native) and a
//! per-case Durable Object (`storage().sql()`, wasm). Those cannot share a
//! driver, but they must share a schema, so [`schema::DDL`] and [`records`] live
//! here and each edge binds them to its own executor. A schema written twice is
//! a schema that drifts, and a case exported from one that will not open in the
//! other is worthless.
//!
//! # A case is meant to leave the machine
//!
//! [`export`] is content-addressed: the payload serialises canonically and the
//! digest covers those exact bytes, so a recipient can re-check what they were
//! handed. A finding nobody else can verify is an assertion with citations.

#![forbid(unsafe_code)]

pub mod export;
pub mod records;
pub mod schema;

pub use export::{export, verify, CaseExport, CasePayload, VerifyError};
pub use records::{
    basis_str, chain_str, ClaimRecord, ClusterRecord, NoteRecord, PartyRecord, SupportRecord,
};
pub use schema::{can_open, DDL, SCHEMA_VERSION};
