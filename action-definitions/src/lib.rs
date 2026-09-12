//! On-chain action definitions — the type, its two codecs, and the pure
//! rules that decide what a definition is worth.
//!
//! # What this crate is
//!
//! A **definition** is a trigger, a filter, a window and a list of grants:
//! "when someone burns 2,000,000 $PERP to this sink between these slots,
//! pay them one NFT and post an announcement." A collection's mint feed, a
//! burn-to-earn campaign and a sale alert are all the same type — whether a
//! given one lives on chain (the registry) or in KV (a mint feed) is a
//! storage choice, not a type.
//!
//! # The two non-negotiables
//!
//! 1. **A definition encoded today must decode forever.** Adding a field, a
//!    variant or a whole new trigger kind never breaks an older reader on a
//!    newer datum, or a newer reader on an older datum. That is what the
//!    [`codec`] module's integer field ids, absent-means-default rule and
//!    unknown-field preservation buy, and what the golden corpus in
//!    `tests/corpus/` enforces — **its fixtures are never edited or
//!    deleted**.
//! 2. **The vocabulary is comprehensive, not burn-specific.**
//!
//! # Where the numbers come from
//!
//! Every field id and variant tag is assigned in
//! `docs/design/ACTION_DEFINITION_SCHEMA.md` §2.8 (in the `cnft.dev-workers`
//! repo) and implemented here verbatim. **Ids are assigned once and never
//! reused or renumbered**; removing a field reserves its id. A change to a
//! number is a change to that table first.

// The derive macro emits `::action_definitions::…` paths so downstream
// crates can use it; this makes those paths resolve inside the crate that
// defines the types too.
extern crate self as action_definitions;

pub mod codec;
pub mod recognise;
pub mod types;
pub mod validate;

pub use codec::{
    Bytes, Cip68Envelope, DecodeError, DecodeErrorKind, Envelope, MapReader, MapWriter,
    PlutusCodec, UnknownFields, UnknownReport,
};
pub use recognise::{recognise, EscrowView, Recognition, RejectReason, TankView, UnfundedReason};
pub use types::*;
pub use validate::{validate, Reason, ReasonKind, Unsupported, ValidateCtx};

/// `#[derive(PlutusCodec)]` — see the derive crate's docs for the attribute
/// vocabulary.
pub use action_definitions_derive::PlutusCodec;
