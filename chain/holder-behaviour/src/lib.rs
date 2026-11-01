//! Holder behaviour: what a wallet *did* with a collection, folded from chain events.
//!
//! An achievement is a predicate over history, not a snapshot of holdings. "Still holds
//! one it minted" and "held for a year" are facts a balance column cannot express, and a
//! balance-weighted score cannot rank — which is why this crate stores **intervals** and
//! answers questions over them.
//!
//! # The shape
//!
//! ```text
//! chain-events  ──apply──▶  Store (lots, mint order, market history)  ──evaluate──▶  Outcome
//! ```
//!
//! Two functions and one trait. [`apply`] folds an event into a backend, [`evaluate`] reads
//! one subject's lots as achievements, and [`Store`] is the point operations a backend
//! provides. The Cloudflare Durable Object and the in-memory test store implement the same
//! trait, and the batch replay and the live fold run the same [`apply`] — there is no
//! second algorithm for a rebuild to disagree with.
//!
//! # Chain-neutral, by construction
//!
//! The input is [`chain_events::ChainEvent`], not a Cardano ledger row. A Cardano archive
//! and an OpenSea feed are both translated upstream into that envelope, so the predicate
//! that says "held for a year" is written once and scores both. Nothing here names a
//! policy, a slot or a marketplace.
//!
//! # Facts, not rollups
//!
//! The store holds acquisitions and the interval each one covers. It holds no totals, no
//! leaderboard and no progress. Progress is derived at read time, which is what lets a
//! `Pending` answer carry the slot it will complete at: the open interval is already the
//! clock, so nothing needs refreshing and no nightly job can fall behind.
//!
//! # What this crate is not
//!
//! Not a ledger (it stores only what an achievement reads — the subject's lots, the
//! collection's mint order, and the marketplace placements and fills a subject was party
//! to), and not a catalogue loader (the TOML is read by the caller — the predicate type
//! deserialises from the same `{ kind = … }` table the published rules use). It does own
//! [`allocation`], the threshold policy that turns an earned set into a phase, because the
//! tool that models a mint and the site that shows an estimate must not each carry a copy.
//! The design is `cnft.dev-workers/docs/design/HOLDER_BEHAVIOUR.md`; the first client is
//! the frozen Perps catalogue.

mod allocation;
mod bundle;
mod evaluate;
mod fold;
mod memory;
mod predicate;
mod state;
mod status;

pub use allocation::{
    Allocation, Counts, Phase, Role, Score, ScoreError, held_at, peak_concurrent, phase, score,
    score_earned,
};
pub use bundle::{
    Bundle, BundleError, BundleStore, Earned, Lot, Rarity, Snapshot, SubjectRecord, SubjectView,
    VERSION as BUNDLE_VERSION, catalogue_digest, evaluate_row, fnv1a, write as write_bundle,
};
pub use evaluate::{
    Assessment, Definition, EarnedSince, Evidence, Gap, Outcome, Progress, SECONDS_PER_DAY,
    evaluate,
};
pub use fold::{Anomaly, Applied, Report, apply, rebuild};
pub use memory::MemoryStore;
pub use predicate::Predicate;
pub use status::{StatusError, StatusPredicate, StatusView, status_holds};
// The chain-qualified asset id, so a gate config can name a collection without also taking
// `chains` as a dependency — the same reason `chain-events` re-exports it.
pub use chain_events::TokenRef;
// And the walk's coverage, which [`Snapshot`] has as a public field: without this a crate
// outside this one can read a snapshot but never write the struct that produces one.
pub use chain_events::Coverage;
pub use state::{
    AcquiredBy, Ask, BookChange, Drawdown, Fill, FloorPoint, FloorSeries, Holding, LotId,
    MarketHistory, MarketSide, MintFact, MintSummary, NewFill, NewLot, Placement, Slot, Store,
    SubjectId, UnitId,
};
