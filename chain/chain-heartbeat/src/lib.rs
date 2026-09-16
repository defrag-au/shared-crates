//! `chain-heartbeat`: Cardano block production as a live signal.
//!
//! Built to ride along inside a Durable Object that is already awake for
//! another reason (the augminted gateway today, a Nostr relay later), so the
//! heartbeat adds a TCP socket and a few kilobytes of state, not a new bill.
//! When the host sleeps, the heartbeat sleeps with it, and the snapshot says so.
//!
//! # Layers
//!
//! - **Pure, always compiled:** [`Network`] (magic, relay, slot time, epoch),
//!   [`BlockHeader`] and [`split_block`] (decode), [`BlockBeat`] and
//!   [`ChainEvent`] (what happened), and [`Heartbeat`], which folds events into
//!   a [`HeartbeatSnapshot`]. No clock, no I/O: the host passes `now_ms`.
//! - **`follow` feature (default):** [`follow::follow`] drives an Ouroboros
//!   node-to-node connection over any `futures-io` stream and emits
//!   [`ChainEvent`]s. The host supplies the stream and a sleep function.
//!
//! A host that already receives whole blocks some other way (an Oura webhook,
//! a mitos push) skips the follower and builds beats with
//! [`BlockBeat::from_block`].
//!
//! # Honesty rules the snapshot enforces
//!
//! - **A silent feed is not a quiet chain.** [`FeedHealth`] separates the two:
//!   a connection that has not even answered keep-alive for
//!   [`SILENT_AFTER_SECS`] is `Silent`, and only a `Following` feed at the tip
//!   reports [`HeartbeatSnapshot::block_due_probability`].
//! - **Catch-up is not activity.** Blocks replayed after a reconnect arrive as
//!   [`SyncState::CatchingUp`]; a UI should not pulse for them.
//! - **Block production is memoryless.** The due-probability rises with elapsed
//!   time but never promises a block, so render it as a likelihood, never as a
//!   countdown.
//! - **Rates come from a contiguous run.** A gap in heights (a reconnect that
//!   could not resume) cuts the window rather than being averaged over.

//!
//! # Relaying to subscribers
//!
//! - [`FrameBatcher`] (host) turns events into few [`HeartbeatFrame`]s, and
//!   [`Heartbeat::apply_frame`] (subscriber) folds them back into the same
//!   chain view with the subscriber's own clock.
//! - [`SubscriberRegistry`] (host) and [`SubscriptionLease`] (subscriber)
//!   manage who gets frames: leased, renewed only while someone listens.
//! - [`BlockPoller`] (browser) turns frames into backend polls, only while
//!   something is awaited and spread out so tabs don't synchronise.

mod beat;
mod block;
mod frame;
mod header;
mod heartbeat;
mod network;
mod poll;
mod relay;
mod vrf;

#[cfg(feature = "follow")]
pub mod follow;

pub use frame::{FrameApplied, FrameBatcher, HeartbeatFrame, MAX_BATCH, PULSE_AFTER_SECS};
pub use poll::{BlockPoller, PollDecision, PollReason, PollTiming};
pub use relay::{
    AfterDelivery, CHAIN_DOMAIN, DROP_AFTER_FAILURES, DeliveryOutcome, LeaseAction, MAX_LEASE_SECS,
    MAX_SUBSCRIBERS, MIN_LEASE_SECS, Rejection, Removal, SubscribeRequest, SubscribeResponse,
    Subscriber, SubscriberId, SubscriberRegistry, SubscriptionLease, TokenCheck,
    UnsubscribeRequest, check_bearer,
};

pub use beat::{
    BeatError, BlockBeat, BlockTxs, ChainEvent, ChainPoint, SyncState, TX_PREFIX_BYTES, TxInBlock,
};
pub use block::{BlockError, BlockParts, BlockTransactions, block_transactions, split_block};
pub use header::{BlockHeader, HeaderError, HeaderVrf, VrfCert};
pub use heartbeat::{
    Checkpoint, DEFAULT_CAPACITY, FeedHealth, Heartbeat, HeartbeatSnapshot, Restore,
    SILENT_AFTER_SECS, WindowStats,
};
pub use network::{
    ACTIVE_SLOT_COEFFICIENT, EpochPosition, Network, Relay, UnknownNetwork,
    block_probability_within,
};
pub use vrf::VrfValue;

pub(crate) fn blake2b<const N: usize>(data: &[u8]) -> [u8; N] {
    use blake2::digest::{Update, VariableOutput};
    let mut hasher = blake2::Blake2bVar::new(N).expect("blake2b output size is 1..=64");
    hasher.update(data);
    let mut out = [0u8; N];
    hasher
        .finalize_variable(&mut out)
        .expect("output buffer matches the requested size");
    out
}
