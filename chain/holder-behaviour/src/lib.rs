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
//! # Reusing it for another collection
//!
//! The fold, the evaluator and the bundle name no collection: a store is scoped to one
//! collection's chain events, and a subject key is the text the producer interns (a stake
//! credential or an EVM address). What the first campaign shaped is worth knowing before
//! pointing the crate at a second:
//!
//! - **One collection per store, bundle and evaluation.** [`Definition`] carries no collection,
//!   and a cross-collection fact is a live [`StatusPredicate`] (a partner collection read at the
//!   claim), never folded history. A campaign over several collections is one fold each, joined
//!   by the caller.
//! - **Two points tables by design.** [`Track`] is [`Track::Bloodline`] and [`Track::Clout`] and
//!   [`Tracked`] is a pair. The names are the first campaign's; the mechanic — what cannot be
//!   bought before the snapshot versus what a holder can still chase — is general. A third axis
//!   is a different model, not a third variant.
//! - **The companion token and the staking programme are optional.** A store not given them
//!   answers `None`, and the predicates that read them report [`Gap::TokenFacts`] or
//!   [`Gap::StakingFacts`] rather than "never held". A collection with neither omits those
//!   definitions.
//! - **Identity linking belongs to the caller.** [`identity_sources`] is handed each wallet's
//!   counts, the pool they form and the merged reading; the crate stores no link.
//! - **The catalogue's TOML is authored, not read here.** `examples/support/catalogue.rs` is the
//!   loader the modelling tools share, and a consumer reads the published manifest
//!   (`examples/catalogue-manifest.rs`) instead. The crate parses the predicate table and the
//!   policy, not the file around them.
//!
//! A second collection therefore needs a catalogue, a producer that translates its chain and
//! marketplace facts into [`chain_events::ChainEvent`], a [`Store`], and a surface that reads
//! the bundle. A campaign with three points axes, a different staking mechanism, or a score that
//! spans collections' histories is an extension of this crate, not a configuration of it.
//!
//! # What this crate is not
//!
//! Not a ledger (it stores only what an achievement reads — the subject's lots, the
//! collection's mint order, and the marketplace placements and fills a subject was party
//! to), and not a catalogue loader (the TOML is read by the caller — the predicate type
//! deserialises from the same `{ kind = … }` table the published rules use). It does own
//! the allocation rule — [`score`] and [`phase`] over an [`Allocation`] — because the tool that
//! models a mint and the site that shows an estimate must not each carry a copy.
//! The design is `cnft.dev-workers/docs/design/HOLDER_BEHAVIOUR.md`; the first client is
//! the frozen Perps catalogue.

mod allocation;
mod bundle;
mod evaluate;
mod fold;
mod memory;
mod moves;
mod predicate;
mod state;
mod status;

pub use allocation::{
    Allocation, Counts, Grant, Grouping, Launch, LaunchPhase, Matured, Maturity,
    Outcome as ScoreOutcome, Paid, Phase, PhaseKind, RUNGS_MAX, Role, Rung, Rungs, Score,
    ScoreError, Seasoning, Slots, Sources, TOKEN_STEPS_MAX, Tail, TokenEarly, TokenRule, TokenStep,
    TokenTail, Trace, TraceRow, Track, Tracked, explain, explain_tracks, grants, held_at,
    holding_mints, identity_slots, identity_sources, matures_at, maturity_at, mints,
    peak_concurrent, phase, pieces_now, score, score_earned, score_tracks, slots, sources,
};
pub use bundle::{
    Bundle, BundleError, BundleStore, Earned, Lot, Rarity, Snapshot, SubjectRecord, SubjectView,
    VERSION as BUNDLE_VERSION, catalogue_digest, evaluate_row, fnv1a, write as write_bundle,
};
pub use evaluate::{
    Assessment, Definition, EarnedSince, Evidence, Gap, Outcome, Progress, SECONDS_PER_DAY,
    Standing, evaluate,
};
pub use fold::{Anomaly, Applied, Report, apply, rebuild};
pub use memory::MemoryStore;
pub use moves::{HOLD_SEARCH, Horizon, Linked, Move, Step, moves};
pub use predicate::{Pieces, Predicate};
pub use status::{StatusError, StatusPredicate, StatusView, status_holds};
// The chain-qualified asset id, so a gate config can name a collection without also taking
// `chains` as a dependency — the same reason `chain-events` re-exports it.
pub use chain_events::TokenRef;
// And the walk's coverage, which [`Snapshot`] has as a public field: without this a crate
// outside this one can read a snapshot but never write the struct that produces one.
pub use chain_events::Coverage;
pub use state::{
    AcquiredBy, Ask, BookChange, Campaign, Drawdown, Fill, FloorPoint, FloorSeries, Holding,
    Interval, LotId, MarketHistory, MarketSide, MintFact, MintSummary, NewFill, NewLot, Payout,
    Placement, Slot, StakingHistory, Store, SubjectId, TokenHistory, UnitId,
};
