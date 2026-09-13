//! The vocabulary — schema §3 for intent, §2.8 for every id and tag.

pub mod claim;
pub mod definition;
pub mod fuel;
pub mod grant;
pub mod scalars;
pub mod trigger;

pub use claim::ClaimTag;
pub use definition::{
    Accepts, ActorPredicate, DEFAULT_CONFIRM_DEPTH, Definition, Filter, Limits, MIN_CONFIRM_DEPTH,
    TraitPredicate, Window,
};
pub use fuel::{CostEntry, Currency, FuelBody, ProtocolConfigBody, Scope};
pub use grant::{
    Deliverer, Effect, EffectKind, EntitlementGrant, Grant, Mode, ModeKind, PolicyFilter, Stacking,
};
pub use scalars::{
    Address, AssetId, ChainAddress, ClaimId, PaymentKeyHash, PolicyId, RouteRef, ScriptHash, TxHash,
};
pub use trigger::{Trigger, TriggerKind, Venue};
