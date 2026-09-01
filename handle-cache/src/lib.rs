//! Persistent naming — what a previous session learned about who a wallet is.
//!
//! [`handle_resolver::HandleResolver`] is the in-session queue: it batches,
//! deduplicates and gives up sensibly. It is also amnesiac. Close the tab and
//! every name is asked for again, which on the surfaces that need this most — a
//! 300-holder table, a counterparty graph — is hundreds of lookups a returning
//! reader has already paid for once.
//!
//! This crate is the naming-shaped adapter over [`local_cache`], plus
//! [`NameBook`], which owns both halves so a consumer has one thing to call.
//! The storage machinery — TTL, eviction, the snapshot, the browser store —
//! all lives in `local-cache` and is not about handles at all.
//!
//! ## The naming-specific part, which is the TTL policy
//!
//! - **Absences are cached too, and they are the point.** Most wallets have no
//!   handle. A cache that only remembers the ones that resolved spares the
//!   lookups for the minority and re-asks about everything else — which is most
//!   of a holder table. "Asked, nothing there" is a real answer and it is
//!   stored as one. [`handle_resolver::HandleResolver::insert_nameless`] is
//!   what lets it be restored.
//! - **Two TTLs, because the two facts decay differently.** A name is stable:
//!   handles move between wallets, but rarely, and the handle service caps its
//!   own cache at a day. An absence is the fragile one — a wallet with no
//!   handle today may buy one tomorrow, and it is the answer we would most like
//!   to be wrong about. So absences expire sooner, which under storage pressure
//!   also makes them the first evicted. See [`NameCacheConfig`].
//!
//! ## Shape of use
//!
//! ```
//! use handle_cache::{NameBook, NameCacheConfig};
//! use local_cache::MemoryStore;
//! use std::collections::HashMap;
//!
//! // A browser app calls `NameBook::browser("flow", NameCacheConfig::default())`.
//! let mut book = NameBook::new(NameCacheConfig::default(), Box::new(|| 1_000_000))
//!     .with_store(Box::new(MemoryStore::default()));
//!
//! book.want("stake1a");
//! let batch = book.next_batch().expect("one queued");
//! let mut found = HashMap::new();
//! found.insert("stake1a".to_string(), "$alice".to_string());
//! book.apply(&batch, found);
//!
//! assert_eq!(book.name("stake1a"), Some("$alice"));
//! ```

use std::collections::HashMap;

pub use handle_resolver::{HandleResolver, Status, MAX_BATCH};
pub use local_cache::{MemoryStore, Store};

use local_cache::{Cache, CacheConfig};

/// A remembered answer: a name, or `None` for "asked, and there is no handle".
type Name = Option<String>;

/// How long each kind of answer is trusted, and how many are kept.
#[derive(Debug, Clone, Copy)]
pub struct NameCacheConfig {
    /// Seconds a resolved name is trusted. A day, matching the handle service's
    /// own ceiling — trusting a name for longer than the service that serves it
    /// would be inventing freshness nobody promised.
    pub named_ttl: u64,
    /// Seconds an absence is trusted.
    ///
    /// Much shorter than [`Self::named_ttl`] on purpose. An absence is the
    /// answer most likely to become wrong — a wallet acquires a handle, and
    /// until this expires the reader keeps seeing a raw key for a wallet that
    /// now has a name. Cheap to be wrong about briefly, and re-asking an hour
    /// later is a fraction of the cost of never caching it at all.
    pub nameless_ttl: u64,
    /// Maximum entries kept. Roughly 100 bytes each, so the default is well
    /// under a megabyte of the origin's quota.
    pub capacity: usize,
}

impl Default for NameCacheConfig {
    fn default() -> Self {
        Self {
            named_ttl: 86_400,
            nameless_ttl: 3_600,
            capacity: 5_000,
        }
    }
}

impl NameCacheConfig {
    fn ttl_for(&self, name: Option<&str>) -> u64 {
        if name.is_some() {
            self.named_ttl
        } else {
            self.nameless_ttl
        }
    }
}

/// A [`HandleResolver`] with a memory.
///
/// The whole reason this type exists rather than leaving consumers to wire the
/// two together: the cache has to be consulted at the moment a key is first
/// WANTED, not at startup. Seeding everything up front would push thousands of
/// keys nobody asked about into the resolver and make its progress readout —
/// "naming wallets — 0/5000" — a statement about the cache rather than about
/// the view. Consulting on demand keeps the denominator honest: it counts what
/// is on screen, and a cache hit lands already settled.
///
/// Mirrors the resolver's API, so a consumer swaps the type and changes nothing
/// else.
pub struct NameBook {
    resolver: HandleResolver,
    cache: Cache<Name>,
    config: NameCacheConfig,
    store: Option<Box<dyn Store>>,
    clock: Box<dyn Fn() -> u64>,
    /// Answers learned since the last write.
    unsaved: usize,
}

impl NameBook {
    /// `clock` yields unix seconds. [`NameBook::browser`] supplies the
    /// browser's.
    pub fn new(config: NameCacheConfig, clock: Box<dyn Fn() -> u64>) -> Self {
        Self {
            resolver: HandleResolver::new(),
            cache: Cache::new(CacheConfig {
                capacity: config.capacity,
            }),
            config,
            store: None,
            clock,
            unsaved: 0,
        }
    }

    /// Attach a store and restore whatever it holds.
    pub fn with_store(mut self, store: Box<dyn Store>) -> Self {
        let now = (self.clock)();
        if let Some(blob) = store.load() {
            self.cache = Cache::restore(
                &blob,
                now,
                CacheConfig {
                    capacity: self.config.capacity,
                },
            );
        }
        self.store = Some(store);
        self
    }

    /// Keys per request. See [`HandleResolver::with_batch_size`].
    pub fn with_batch_size(mut self, n: usize) -> Self {
        let resolver = std::mem::replace(&mut self.resolver, HandleResolver::new());
        self.resolver = resolver.with_batch_size(n);
        self
    }

    /// Ask for a key's name, answering from cache where one is held.
    ///
    /// Safe to call every frame for everything on screen, exactly as
    /// [`HandleResolver::want`] is.
    pub fn want(&mut self, key: &str) {
        if self.resolver.status(key) != Status::Unknown {
            return;
        }
        match self.cache.get(key, (self.clock)()) {
            Some(Some(name)) => {
                let name = name.clone();
                self.resolver.insert_known(key, &name);
            }
            Some(None) => self.resolver.insert_nameless(key),
            None => self.resolver.want(key),
        }
    }

    pub fn want_all<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        for key in keys {
            self.want(key);
        }
    }

    /// Record an answer, into both the session and the cache.
    ///
    /// `requested` is the batch that was sent: every key in it absent from
    /// `found` has no handle, and remembering THAT is most of the value here.
    ///
    /// What gets cached is read back off the resolver AFTER it settles, not
    /// taken from `found` — the two are not the same set. A transport whose
    /// names and acknowledgement arrive separately settles the batch with an
    /// empty `found` and relies on [`Self::insert_known`] having already landed
    /// the names (flow-explorer's gateway does exactly this). Caching straight
    /// from `found` would write every one of those down as an absence, and the
    /// wallet would then go unnamed for the whole TTL — a persistent bug out of
    /// a transient ordering.
    pub fn apply(&mut self, requested: &[String], found: HashMap<String, String>) {
        self.resolver.apply(requested, found);

        let settled: Vec<(String, Name)> = requested
            .iter()
            .filter_map(|key| match self.resolver.status(key) {
                Status::Named => Some((key.clone(), self.resolver.name(key).map(str::to_string))),
                Status::Nameless => Some((key.clone(), None)),
                // Abandoned is "we could not find out", which is not an answer
                // and must not be written down as one. Pending means another
                // request owns this key now.
                _ => None,
            })
            .collect();

        let now = (self.clock)();
        self.unsaved += settled.len();
        for (key, name) in settled {
            let ttl = self.config.ttl_for(name.as_deref());
            self.cache.put(&key, name, ttl, now);
        }
    }

    /// Seed a name learned out of band — another view, a snapshot that arrived
    /// with the data. Cached like any other answer: it was true enough to draw.
    pub fn insert_known(&mut self, key: &str, name: &str) {
        self.resolver.insert_known(key, name);
        self.cache.put(
            key,
            Some(name.to_string()),
            self.config.named_ttl,
            (self.clock)(),
        );
        self.unsaved += 1;
    }

    /// The batch did not come back. Nothing is cached — a failure is not an
    /// answer, and writing it down would turn an outage into a persisted
    /// absence that outlives it.
    pub fn fail(&mut self, requested: &[String]) {
        self.resolver.fail(requested);
    }

    /// Write the snapshot if anything has been learned and nothing is in
    /// flight.
    ///
    /// Called every frame. The idle check is what batches it: a burst of
    /// requests lands as one write at the end rather than one per reply, and
    /// serialising a few thousand entries mid-scroll is exactly the kind of
    /// hitch nobody attributes to a cache.
    pub fn flush_when_idle(&mut self) {
        if self.unsaved == 0 || self.resolver.working() {
            return;
        }
        self.flush();
    }

    /// Write the snapshot now.
    pub fn flush(&mut self) {
        if let Some(store) = &self.store {
            store.save(&self.cache.snapshot((self.clock)()));
        }
        self.unsaved = 0;
    }

    // ─── Pass-throughs ───────────────────────────────────────────────────────

    pub fn name(&self, key: &str) -> Option<&str> {
        self.resolver.name(key)
    }
    pub fn status(&self, key: &str) -> Status {
        self.resolver.status(key)
    }
    pub fn next_batch(&mut self) -> Option<Vec<String>> {
        self.resolver.next_batch()
    }
    pub fn total(&self) -> usize {
        self.resolver.total()
    }
    pub fn settled(&self) -> usize {
        self.resolver.settled()
    }
    pub fn working(&self) -> bool {
        self.resolver.working()
    }
    pub fn progress(&self) -> Option<f32> {
        self.resolver.progress()
    }
    pub fn queued(&self) -> usize {
        self.resolver.queued()
    }
    pub fn in_flight(&self) -> usize {
        self.resolver.in_flight()
    }
    pub fn take_dirty(&mut self) -> bool {
        self.resolver.take_dirty()
    }
    pub fn names(&self) -> HashMap<&str, &str> {
        self.resolver.names()
    }
    /// Entries held across sessions — for a diagnostics readout.
    pub fn cached(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(feature = "browser")]
impl NameBook {
    /// A book backed by localStorage under `namespace`, on the browser clock.
    ///
    /// The one-liner every frontend wants:
    ///
    /// ```ignore
    /// let names = NameBook::browser("flow", NameCacheConfig::default())
    ///     .with_batch_size(25);
    /// ```
    pub fn browser(namespace: &str, config: NameCacheConfig) -> Self {
        Self::new(config, Box::new(local_cache::browser::now_secs)).with_store(Box::new(
            local_cache::browser::LocalStore::new(&format!("handles:{namespace}")),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn found(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// A clock the test moves by hand.
    fn test_clock() -> (Rc<Cell<u64>>, Box<dyn Fn() -> u64>) {
        let now = Rc::new(Cell::new(1_000_000u64));
        let handle = now.clone();
        (now, Box::new(move || handle.get()))
    }

    fn book(store: &MemoryStore) -> NameBook {
        let (_now, clock) = test_clock();
        NameBook::new(NameCacheConfig::default(), clock).with_store(Box::new(store.clone()))
    }

    #[test]
    fn a_cached_name_is_never_looked_up_again() {
        let store = MemoryStore::default();
        let mut b = book(&store);
        b.want("stake1a");
        b.want("stake1b");
        let batch = b.next_batch().expect("both queued");
        b.apply(&batch, found(&[("stake1a", "$alice")]));

        b.want("stake1a");
        b.want("stake1b");
        assert!(b.next_batch().is_none());
    }

    /// The whole point, end to end: a reload asks for nothing it asked for last
    /// time — including everything that had no handle, which is most of it.
    #[test]
    fn a_reload_asks_for_nothing_it_already_learned() {
        let store = MemoryStore::default();

        let mut first = book(&store);
        first.want("stake1a");
        first.want("stake1b");
        let batch = first.next_batch().expect("batch");
        first.apply(&batch, found(&[("stake1a", "$alice")]));
        first.flush();

        let mut reloaded = book(&store);
        reloaded.want("stake1a");
        reloaded.want("stake1b");

        assert_eq!(reloaded.name("stake1a"), Some("$alice"));
        assert_eq!(reloaded.status("stake1b"), Status::Nameless);
        assert!(
            reloaded.next_batch().is_none(),
            "a returning reader paid for these already"
        );
        assert_eq!(reloaded.progress(), Some(1.0));
    }

    /// flow-explorer's shape: the gateway sends the names on a delta and the op
    /// id on the ack that follows, so the batch settles with an EMPTY map and
    /// the names have already been seeded. Caching from `found` here would
    /// write a freshly-named wallet down as having no handle, and hide it for
    /// the whole TTL.
    #[test]
    fn a_name_seeded_out_of_band_is_not_cached_as_an_absence() {
        let store = MemoryStore::default();

        let mut first = book(&store);
        first.want("stake1a");
        first.want("stake1b");
        let batch = first.next_batch().expect("batch");
        first.insert_known("stake1a", "$alice");
        first.apply(&batch, HashMap::new());
        first.flush();

        let mut reloaded = book(&store);
        reloaded.want("stake1a");
        reloaded.want("stake1b");
        assert_eq!(reloaded.name("stake1a"), Some("$alice"));
        assert_eq!(reloaded.status("stake1b"), Status::Nameless);
    }

    /// An absence is the answer most likely to go wrong, so it is trusted for
    /// the shortest time — and a reader who comes back tomorrow re-asks about
    /// the unnamed wallets while keeping the named ones.
    #[test]
    fn an_absence_expires_long_before_a_name_does() {
        let store = MemoryStore::default();
        let config = NameCacheConfig::default();
        let (now, clock) = test_clock();

        let mut first = NameBook::new(config, clock).with_store(Box::new(store.clone()));
        first.want("named");
        first.want("nameless");
        let batch = first.next_batch().expect("batch");
        first.apply(&batch, found(&[("named", "$alice")]));
        first.flush();

        // An hour and a bit later.
        let (later, clock2) = test_clock();
        later.set(now.get() + config.nameless_ttl + 1);
        let mut reloaded = NameBook::new(config, clock2).with_store(Box::new(store.clone()));
        reloaded.want("named");
        reloaded.want("nameless");

        assert_eq!(reloaded.name("named"), Some("$alice"));
        assert_eq!(
            reloaded.status("nameless"),
            Status::Pending,
            "worth asking again — it may have a handle by now"
        );
    }

    /// An outage must not be written down. A cached absence outlives the
    /// failure that produced it, and would keep a real name hidden long after
    /// the service came back.
    #[test]
    fn a_failed_batch_is_not_cached_as_an_absence() {
        let store = MemoryStore::default();
        let mut b = book(&store);
        b.want("stake1a");
        let batch = b.next_batch().expect("batch");
        b.fail(&batch);
        b.flush();

        assert_eq!(b.cached(), 0);
    }

    /// Consulting the cache at `want` rather than seeding at startup is what
    /// keeps the progress readout about the view instead of about the cache.
    /// Seeding up front would report "naming wallets — 0/100" on a page showing
    /// two wallets.
    #[test]
    fn progress_counts_the_view_not_the_cache() {
        let store = MemoryStore::default();

        let mut first = book(&store);
        for i in 0..100 {
            first.insert_known(&format!("stake{i}"), "$x");
        }
        first.flush();

        let mut view = book(&store);
        assert_eq!(view.cached(), 100);
        view.want("stake0");
        view.want("stake1");
        assert_eq!(view.total(), 2, "the denominator is what is on screen");
        assert_eq!(view.progress(), Some(1.0));
    }

    #[test]
    fn a_write_is_held_back_while_a_batch_is_outstanding() {
        let store = MemoryStore::default();
        let mut b = book(&store);
        b.want("stake1a");
        b.want("stake1b");
        let first = b.next_batch().expect("batch");
        b.apply(&first, found(&[("stake1a", "$alice")]));
        b.flush_when_idle();

        assert_eq!(b.cached(), 2, "the answers are held in memory either way");
    }
}
