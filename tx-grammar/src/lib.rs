//! `tx-grammar` — one transaction as a compositional phrase, not a sentence.
//!
//! # The problem
//!
//! A transaction is not one event. A batched marketplace submission can list
//! three items and create three collection offers at once; a wallet's mint is a
//! payment followed by an arrival; an EVM sale can settle in WETH and emit
//! several transfers. The estate's older models each collapsed that into one
//! shape with a bespoke set of fields, and the view then reconstructed the rest
//! in prose.
//!
//! This crate keeps the composition. A [`Step`] is one transaction's
//! [`Primitive`]s; a [`Sequence`] is one or more steps; a [`Phrase`] is the
//! renderer-neutral projection of readings and [`Cell`]s.
//!
//! # The three layers
//!
//! 1. **Primitives** — nine physical actions: move, create, destroy, hold,
//!    declare, retract, revise, grant, revoke. A sale is two moves; a listing is
//!    a hold plus a declare; a swap is two moves in opposite directions.
//! 2. **Readings** — the domain vocabulary derived from primitives: sale,
//!    listing, offer, mint, burn, swap, lock, approval, and so on. A reading is
//!    a label over the same axes, never a struct with its own fields.
//! 3. **Phrase** — the forward projection. [`Cell`]s carry the semantic marks a
//!    renderer needs (direction, custody, certainty, absence, visibility,
//!    outcome) and nothing about how to draw them.
//!
//! # What it is not
//!
//! It does not fetch, classify, score, resolve names, choose colours, or know a
//! collection's name. It has no thresholds and no policy IDs. Detection belongs
//! to `tx-classifier`; the social-card shape belongs to `tx-verdict`; the
//! notification shape belongs to `tx-insights`. Those become projections of this
//! grammar, not parallel dialects.
//!
//! # Multichain
//!
//! Identity comes from [`chains`]: CAIP-2 for a chain and CAIP-19 for an asset,
//! including Midnight. A [`Phrase`] is single-chain; a bridge is an
//! [`Composition::AcrossChains`] sequence of steps linked across chains. The
//! grammar never invents a party or an amount: [`Party`] has separate absences
//! for a scan floor, an untraced transfer and a shielded counterparty, and a
//! hidden object is [`Object::Hidden`], not a zero.
//!
//! # The text renderings
//!
//! [`Phrase::tokens`] is the compact canonical line; [`Phrase::sentence`] is the
//! text-only interface and the oracle the visual renderers are tested against.
//! Prose lives in [`text`] and nowhere else.

pub mod model;
pub mod phrase;
pub mod primitive;
pub mod reading;
pub mod text;

pub use model::*;
pub use phrase::*;
pub use primitive::{Action, Primitive};
pub use reading::Reading;

/// Bumped when a change to these types changes the wire meaning. A consumer
/// that persists a [`Phrase`] must carry this beside it.
pub const GRAMMAR_VERSION: u8 = 1;
