//! Wire types shared between defrag platform services and their HTTP clients.
//!
//! Each submodule here mirrors a service in `cnft.dev-workers/workers/` and
//! holds the request / response shapes that both the service implementation
//! and the typed HTTP client agree on. Putting them here is the single
//! source of truth — `services/<name>/src/types.rs` re-exports from these
//! modules, and `services/<name>-client` does the same. No more drift
//! between server and client copies.
//!
//! New services should be added as new modules, not as separate crates.

pub mod policy_cid_index;
pub mod policy_info;
