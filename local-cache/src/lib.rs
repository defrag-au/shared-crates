//! A client-side cache for things fetched over a transport: TTL'd, size-bounded,
//! and persisted as one blob per dataset.
//!
//! Every frontend here re-derives the same thing the moment it shows data it had
//! to ask a service for — holder names, price points, collection metadata. The
//! answers are expensive, they are the same on the next visit, and they go stale
//! at a rate the caller knows and this crate does not. So the policy is the
//! caller's (a TTL per write) and the mechanism is here.
//!
//! ## Not a replacement for [`ui_core::kv::KvStore`]
//!
//! That is a key-per-entry store with a Workers-KV-shaped API, and it is the
//! right thing for a handful of durable single values — a session token, a saved
//! wallet list. This is for the other case: a DATASET of hundreds or thousands of
//! entries, where per-key storage costs a read and a JSON parse per entry on
//! every view, and where eviction would have to scan the whole origin's storage
//! to find its own keys. One blob is one parse at startup and one write per burst.
//!
//! [`ui_core::kv::KvStore`]: https://docs.rs/ui-core
//!
//! ## What it gives you
//!
//! - **Per-write TTL.** Freshness is a property of the fact, not of the store.
//!   The caller that knows a name lasts a day and an absence lasts an hour says
//!   so at the point of writing; see `handle-cache` for that policy in practice.
//! - **A hard size bound.** localStorage is a few megabytes for the entire
//!   origin and throws when it fills — an unbounded cache takes the rest of the
//!   app's storage down with it. Eviction sheds the soonest-to-expire first, so
//!   the shorter TTL a caller assigns, the sooner that entry goes under pressure.
//! - **A store port.** The browser is one implementation. The policy above is
//!   pure and tested against [`MemoryStore`] on a plain `cargo test`.
//!
//! ## Shape of use
//!
//! ```
//! use local_cache::{Cache, CacheConfig, MemoryStore, Store};
//!
//! let store = MemoryStore::default();
//! let mut cache: Cache<String> = Cache::new(CacheConfig::default());
//! cache.put("ada", "0.42".to_string(), 300, 1_000);
//! store.save(&cache.snapshot(1_000));
//!
//! // Next session, over the same storage.
//! let restored: Cache<String> =
//!     Cache::restore(&store.load().unwrap_or_default(), 1_100, CacheConfig::default());
//! assert_eq!(restored.get("ada", 1_100), Some(&"0.42".to_string()));
//! // ...and once the TTL is up, it is simply not there.
//! assert_eq!(restored.get("ada", 9_999), None);
//! ```

use std::collections::HashMap;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "browser")]
pub mod browser;

/// Snapshot format. An older or unreadable blob is DISCARDED rather than
/// migrated — this is a cache, and the cost of being wrong about its contents
/// is far higher than the cost of refilling it.
const FORMAT: u32 = 1;

/// How much is kept.
#[derive(Debug, Clone, Copy)]
pub struct CacheConfig {
    /// Maximum entries. Past this, eviction runs on every write.
    pub capacity: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { capacity: 5_000 }
    }
}

/// One cached value.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record<T> {
    /// The key it was stored under.
    k: String,
    /// The value.
    v: T,
    /// Unix seconds at which this stops being trusted.
    x: u64,
    /// Write order, breaking ties on `x`.
    ///
    /// Load-bearing, not decoration. A whole batch of answers is written in the
    /// same second and so shares an expiry exactly; without a tiebreak,
    /// evicting "everything at or below the cut" takes the entire batch — which
    /// for a cache filled by one batch means eviction wipes it. Persisted so
    /// the order survives a reload.
    #[serde(default)]
    s: u64,
}

/// Short field names throughout: this is serialised into a storage quota shared
/// with the rest of the origin, and the keys repeat once per entry.
#[derive(Serialize, Deserialize)]
struct Snapshot<T> {
    v: u32,
    entries: Vec<Record<T>>,
}

/// A TTL'd, size-bounded map that survives a reload.
///
/// Pure and clock-injected: every method that cares about time takes `now` as
/// unix seconds, so expiry and eviction are testable without a browser.
/// [`browser::now_secs`] is what supplies a real clock.
#[derive(Debug, Clone)]
pub struct Cache<T> {
    entries: HashMap<String, Record<T>>,
    config: CacheConfig,
    /// Next write-order stamp. See [`Record::s`].
    seq: u64,
}

impl<T> Cache<T>
where
    T: Clone + Serialize + DeserializeOwned,
{
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: HashMap::new(),
            config,
            seq: 0,
        }
    }

    /// Restore from a snapshot, dropping anything already expired.
    ///
    /// Tolerant by construction: a blob from an older format, a truncated
    /// write, or another dataset's data under the same key all yield an empty
    /// cache rather than an error. There is nothing here that cannot be
    /// fetched again, so refusing to start over a bad cache would trade a free
    /// recovery for a broken app.
    pub fn restore(blob: &str, now: u64, config: CacheConfig) -> Self {
        let mut cache = Self::new(config);
        let Ok(snapshot) = serde_json::from_str::<Snapshot<T>>(blob) else {
            return cache;
        };
        if snapshot.v != FORMAT {
            return cache;
        }
        for record in snapshot.entries {
            if record.x > now {
                cache.seq = cache.seq.max(record.s + 1);
                cache.entries.insert(record.k.clone(), record);
            }
        }
        cache
    }

    /// Serialise for storage. Expired entries are dropped on the way out, so a
    /// blob never carries dead weight forward across a reload.
    pub fn snapshot(&self, now: u64) -> String {
        let entries: Vec<Record<T>> = self
            .entries
            .values()
            .filter(|r| r.x > now)
            .cloned()
            .collect();
        serde_json::to_string(&Snapshot { v: FORMAT, entries }).unwrap_or_default()
    }

    /// The value, if it is held and still fresh.
    pub fn get(&self, key: &str, now: u64) -> Option<&T> {
        let record = self.entries.get(key)?;
        (record.x > now).then_some(&record.v)
    }

    /// Is anything held for this key, fresh or not? Distinguishes "we have
    /// never asked" from "what we knew has expired", which a caller deciding
    /// whether to show something stale while it refetches may want.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Store a value for `ttl` seconds, evicting if that takes the cache over
    /// capacity.
    pub fn put(&mut self, key: &str, value: T, ttl: u64, now: u64) {
        self.entries.insert(
            key.to_string(),
            Record {
                k: key.to_string(),
                v: value,
                x: now + ttl,
                s: self.seq,
            },
        );
        self.seq += 1;
        self.evict_to_capacity(now);
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drop expired entries first, then the soonest to expire until the cache
    /// fits.
    ///
    /// Expiry order rather than insertion order, so the TTL a caller chose is
    /// also its eviction priority: the facts it declared short-lived are the
    /// first to go under pressure. Write order breaks ties, so a cache filled
    /// by a single batch sheds its oldest entries rather than all of them.
    ///
    /// `select_nth_unstable` rather than a sort: this runs on every write once
    /// the cache is full, and finding the cut is linear where ordering the
    /// whole thing is not.
    fn evict_to_capacity(&mut self, now: u64) {
        if self.entries.len() <= self.config.capacity {
            return;
        }
        self.entries.retain(|_, r| r.x > now);
        if self.entries.len() <= self.config.capacity {
            return;
        }
        let excess = self.entries.len() - self.config.capacity;
        let mut order: Vec<(u64, u64)> = self.entries.values().map(|r| (r.x, r.s)).collect();
        // The last entry that does NOT survive. `(x, s)` is unique per entry,
        // so keeping everything strictly above it leaves exactly `capacity`.
        let cut = *order.select_nth_unstable(excess - 1).1;
        self.entries.retain(|_, r| (r.x, r.s) > cut);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Somewhere a snapshot can live between sessions.
///
/// A port rather than a hard dependency on localStorage: it keeps the crate
/// usable from a target that has no `web_sys`, and it is what lets the policy
/// above be tested against [`MemoryStore`].
pub trait Store {
    fn load(&self) -> Option<String>;
    fn save(&self, blob: &str);
}

/// An in-memory [`Store`], for tests and for a consumer that wants the cache
/// without the persistence.
///
/// Cloning shares the storage rather than copying it, so a test can hold one
/// end while the code under test owns the other — which is how "reload the app"
/// is written without a browser.
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    blob: std::rc::Rc<std::cell::RefCell<Option<String>>>,
}

impl Store for MemoryStore {
    fn load(&self) -> Option<String> {
        self.blob.borrow().clone()
    }
    fn save(&self, blob: &str) {
        *self.blob.borrow_mut() = Some(blob.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> Cache<String> {
        Cache::new(CacheConfig::default())
    }

    #[test]
    fn a_value_survives_a_round_trip_through_a_snapshot() {
        let mut c = cache();
        c.put("a", "alpha".to_string(), 600, 1000);
        let restored = Cache::<String>::restore(&c.snapshot(1000), 1000, CacheConfig::default());
        assert_eq!(restored.get("a", 1000), Some(&"alpha".to_string()));
    }

    #[test]
    fn a_value_is_gone_once_its_ttl_is_up() {
        let mut c = cache();
        c.put("a", "alpha".to_string(), 600, 1000);
        assert_eq!(c.get("a", 1599), Some(&"alpha".to_string()));
        assert_eq!(c.get("a", 1601), None);
    }

    /// The TTL is per write, because freshness is a property of the fact rather
    /// than of the store — the caller is the only one who knows.
    #[test]
    fn each_write_carries_its_own_ttl() {
        let mut c = cache();
        c.put("brief", "x".to_string(), 60, 1000);
        c.put("durable", "y".to_string(), 86_400, 1000);
        let later = 1000 + 3600;
        assert_eq!(c.get("brief", later), None);
        assert_eq!(c.get("durable", later), Some(&"y".to_string()));
    }

    #[test]
    fn an_expired_entry_is_dropped_rather_than_carried_forward() {
        let mut c = cache();
        c.put("a", "alpha".to_string(), 60, 1000);
        let restored =
            Cache::<String>::restore(&c.snapshot(1_000_000), 1_000_000, CacheConfig::default());
        assert!(restored.is_empty());
    }

    /// localStorage is a shared, finite quota that throws when it fills — an
    /// unbounded cache here takes the rest of the app's storage down with it.
    #[test]
    fn the_cache_stays_within_capacity() {
        let mut c: Cache<String> = Cache::new(CacheConfig { capacity: 10 });
        for i in 0..100 {
            c.put(&format!("k{i}"), "v".to_string(), 600, 1000);
        }
        assert_eq!(c.len(), 10);
    }

    /// The regression that made eviction worth testing: a batch written in one
    /// second shares an expiry exactly, and a cut on expiry alone takes all of
    /// it — emptying the cache instead of trimming it.
    #[test]
    fn a_batch_written_in_one_second_is_trimmed_not_wiped() {
        let mut c: Cache<String> = Cache::new(CacheConfig { capacity: 10 });
        for i in 0..40 {
            c.put(&format!("k{i}"), "v".to_string(), 600, 1000);
        }
        assert_eq!(
            c.len(),
            10,
            "identical expiries must not evict as one group"
        );
        // ...and what survives is the most recently written.
        assert!(c.get("k39", 1000).is_some());
        assert!(c.get("k0", 1000).is_none());
    }

    /// The TTL a caller chose is also its eviction priority.
    #[test]
    fn eviction_sheds_the_shortest_lived_first() {
        let mut c: Cache<String> = Cache::new(CacheConfig { capacity: 5 });
        for i in 0..5 {
            c.put(&format!("durable{i}"), "v".to_string(), 86_400, 1000);
        }
        for i in 0..20 {
            c.put(&format!("brief{i}"), "v".to_string(), 600, 1000);
        }
        for i in 0..5 {
            assert!(
                c.get(&format!("durable{i}"), 1000).is_some(),
                "a long-lived entry was evicted while short-lived ones remained"
            );
        }
    }

    /// A cache is refillable. Refusing to start because its contents are
    /// unreadable would trade a free recovery for a broken app.
    #[test]
    fn an_unreadable_blob_yields_an_empty_cache_not_a_failure() {
        for blob in ["", "not json", r#"{"v":999,"entries":[]}"#, "{}"] {
            let c = Cache::<String>::restore(blob, 1000, CacheConfig::default());
            assert!(c.is_empty(), "blob {blob:?}");
        }
    }

    #[test]
    fn contains_sees_an_expired_entry_that_get_hides() {
        let mut c = cache();
        c.put("a", "alpha".to_string(), 60, 1000);
        assert_eq!(c.get("a", 5000), None);
        assert!(c.contains("a"), "asked before, just not recently enough");
    }
}
