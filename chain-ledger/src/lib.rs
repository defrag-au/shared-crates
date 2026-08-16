//! Chain-agnostic wallet-flow ledger.
//!
//! The model a forensic wallet trace actually needs, extracted from the Mekka
//! analysis (see `docs/design/CHAIN_LEDGER_CASE_TOOL.md` in `cnft.dev-workers`).
//! Pure: no I/O, no wasm-bindgen, no chain SDKs — so the same types serve an
//! egui app, a Cloudflare worker, and `chain-forensics`. Chain access lives in
//! the adapter crates.
//!
//! # The two rules the API enforces
//!
//! **Attribute on the per-tx NET, never on gross outputs.** A wallet routinely
//! appears as a minor input in a batched marketplace or DEX transaction whose
//! outputs belong to other people; booking those outputs as its own produced a
//! 203,000,000 ADA figure out of a treasury that had banked 445,417. There is
//! deliberately no gross accessor on [`TxView`] — [`net_deltas`] is the only way
//! in.
//!
//! **Bounded traversals report their bound.** A walk that silently stops looks
//! exactly like a walk that finished. [`WalkOutcome`] therefore carries
//! [`Origin::BeyondDepth`] / [`Origin::BudgetExhausted`] as real terminals that
//! sum into the total, so a caller cannot present a truncated trace as complete.
//!
//! # Chain differences that must not be abstracted away
//!
//! On a UTxO chain each input names the output it consumes, so
//! [`walk_provenance`] answers "did *these* units become *those* units" —
//! proof. On an account chain there is no such link and the nearest analogue is
//! the instruction tree, which is inference. [`Provenance::available`] reports
//! which one a caller is looking at; rendering them identically invites
//! overclaiming.

#![forbid(unsafe_code)]

mod attribution;
mod model;
mod walk;

pub use attribution::{movements, net_deltas, round_trips, RoundTrip};
pub use model::{
    Basis, Chain, Movement, Party, PartyRef, Provenance, TxDelta, TxInput, TxOutput, TxView,
};
pub use walk::{walk_provenance, Origin, WalkBudget, WalkLeg, WalkOutcome};
