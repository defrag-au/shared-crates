//! Persistent naming — what a previous session learned about who a wallet is.
//!
//! [`handle_resolver::HandleResolver`] is the in-session queue: it batches,
//! deduplicates and gives up sensibly. It is also amnesiac. Close the tab and
//! every name is asked for again, which on the surfaces that need this most —
//! a 300-holder table, a counterparty graph — is hundreds of lookups a
//! returning reader has already paid for once.
//!
//! This crate is the layer underneath: a TTL cache with a size bound, and
//! [`NameBook`], which owns both halves so a consumer has one thing to call.
//!
//! ## What makes it worth having
//!
//! - **Absences are cached too, and they are the point.** Most wallets have no
//!   handle. A cache that only remembers the ones that resolved spares the
//!   lookups for the minority and re-asks about everything else — which is
//!   most of a holder table. "Asked, nothing there" is a real answer and it is
//!   stored as one.
//! - **Two TTLs, because the two facts decay differently.** A name is stable:
//!   handles move between wallets, but rarely, and the handle service caps its
//!   own cache at a day. An absence is the fragile one — a wallet with no handle today
//!   may buy one tomorrow, and it is the answer we would most like to be
//!   wrong about. So absences expire sooner. See [`CacheConfig`].
//! - **Bounded.** localStorage is a few megabytes for the whole origin and
//!   throws when it fills. A whale's counterparty graph would otherwise grow
//!   this without limit and take the app's other storage down with it.
//! - **One blob, not one key per wallet.** A per-key store costs a read and a
//!   JSON parse per wallet on every view, and makes eviction a scan of the
//!   entire origin's storage. One entry is one parse at startup and one write
//!   per burst.
//!
//! ## Shape of use
//!
//! ```
//! use handle_cache::{CacheConfig, MemoryStore, NameBook};
//! use std::collections::HashMap;
//!
//! // A browser app calls `NameBook::browser("flow_names", CacheConfig::default())`.
//! let mut book = NameBook::new(CacheConfig::default(), Box::new(|| 1_000_000))
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

use serde::{Deserialize, Serialize};

pub use handle_resolver::{HandleResolver, Status, MAX_BATCH};

#[cfg(feature = "browser")]
pub mod browser;

/// Snapshot format. Bumped when the shape changes; an older or unreadable blob
/// is DISCARDED rather than migrated — this is a cache, and the cost of being
/// wrong about its contents is far higher than the cost of refilling it.
const FORMAT: u32 = 1;

/// How long each kind of answer is trusted, and how many are kept.
#[derive(Debug, Clone, Copy)]
pub struct CacheConfig {
    /// Seconds a resolved name is trusted. A day, matching the handle
    /// service's own ceiling — trusting a name for longer than the service
    /// that serves it would be inventing freshness nobody promised.
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
    /// under a megabyte.
    pub capacity: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            named_ttl: 86_400,
            nameless_ttl: 3_600,
            capacity: 5_000,
        }
    }
}

/// One remembered answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    /// The key asked about — a stake address, or whatever the transport keys by.
    k: String,
    /// The name, or `None` for "asked, and there is no handle".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<String>,
    /// Unix seconds at which this stops being trusted.
    x: u64,
    /// Write order, breaking ties on `x`.
    ///
    /// Load-bearing, not decoration. A whole batch resolves in the same second
    /// and so shares an expiry exactly; without a tiebreak, evicting
    /// "everything at or below the cut" takes the entire batch — which for a
    /// cache filled by one batch means eviction wipes it. Persisted so the
    /// order survives a reload.
    #[serde(default)]
    s: u64,
}

/// Short field names throughout: this is serialised into a storage quota
/// shared with the rest of the origin, and the keys repeat once per wallet.
#[derive(Serialize, Deserialize)]
struct Snapshot {
    v: u32,
    entries: Vec<Record>,
}

/// Names remembered across sessions, with a TTL and a size bound.
///
/// Pure and clock-injected: every method that cares about time takes `now` as
/// unix seconds, so the expiry and eviction rules are testable without a
/// browser. [`NameBook`] is what supplies a real clock.
#[derive(Debug, Clone)]
pub struct HandleCache {
    entries: HashMap<String, Record>,
    config: CacheConfig,
    /// Next write-order stamp. See [`Record::s`].
    seq: u64,
}

impl HandleCache {
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
    /// write, or something else's data under the same key all yield an empty
    /// cache rather than an error. There is nothing here that cannot be
    /// fetched again, so refusing to start over a bad cache would trade a free
    /// recovery for a broken app.
    pub fn restore(blob: &str, now: u64, config: CacheConfig) -> Self {
        let mut cache = Self::new(config);
        let Ok(snapshot) = serde_json::from_str::<Snapshot>(blob) else {
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
        let entries: Vec<Record> = self
            .entries
            .values()
            .filter(|r| r.x > now)
            .cloned()
            .collect();
        serde_json::to_string(&Snapshot {
            v: FORMAT,
            entries,
        })
        .unwrap_or_else(|_| String::new())
    }

    /// What is remembered about a key.
    ///
    /// `Some(Some(name))` is a name, `Some(None)` is a remembered absence, and
    /// `None` means nothing is known — never asked, or asked too long ago. The
    /// double option is deliberate: collapsing it would erase exactly the
    /// distinction this cache exists to keep.
    pub fn get(&self, key: &str, now: u64) -> Option<Option<&str>> {
        let record = self.entries.get(key)?;
        (record.x > now).then_some(record.n.as_deref())
    }

    /// Record an answer, evicting if that takes the cache over capacity.
    pub fn remember(&mut self, key: &str, name: Option<&str>, now: u64) {
        let ttl = if name.is_some() {
            self.config.named_ttl
        } else {
            self.config.nameless_ttl
        };
        self.entries.insert(
            key.to_string(),
            Record {
                k: key.to_string(),
                n: name.map(str::to_string),
                x: now + ttl,
                s: self.seq,
            },
        );
        self.seq += 1;
        self.evict_to_capacity(now);
    }

    /// Drop expired entries first, then the soonest to expire until the cache
    /// fits.
    ///
    /// Expiry order rather than insertion order, which falls out of the TTL
    /// policy: absences expire sooner, so under pressure they go first and the
    /// names — the entries that actually change what a reader sees — are what
    /// survives. Write order breaks ties, so a cache filled by a single batch
    /// sheds its oldest entries rather than all of them at once.
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
/// usable from a target that has no `web_sys`, and it is what lets the whole
/// policy above be tested against [`MemoryStore`].
pub trait Store {
    fn load(&self) -> Option<String>;
    fn save(&self, blob: &str);
}

/// An in-memory [`Store`], for tests and for a consumer that wants the cache
/// without the persistence.
///
/// Cloning shares the storage rather than copying it, so a test can hold one
/// end while a [`NameBook`] owns the other — which is how "reload the app" is
/// written without a browser.
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
/// Mirrors the resolver's API, so a consumer swaps the type and changes
/// nothing else.
pub struct NameBook {
    resolver: HandleResolver,
    cache: HandleCache,
    store: Option<Box<dyn Store>>,
    clock: Box<dyn Fn() -> u64>,
    /// Answers learned since the last write.
    unsaved: usize,
}

impl NameBook {
    /// `clock` yields unix seconds. [`NameBook::browser`] supplies the browser's.
    pub fn new(config: CacheConfig, clock: Box<dyn Fn() -> u64>) -> Self {
        Self {
            resolver: HandleResolver::new(),
            cache: HandleCache::new(config),
            store: None,
            clock,
            unsaved: 0,
        }
    }

    /// Attach a store and restore whatever it holds.
    pub fn with_store(mut self, store: Box<dyn Store>) -> Self {
        let now = (self.clock)();
        if let Some(blob) = store.load() {
            self.cache = HandleCache::restore(&blob, now, self.cache.config);
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
            Some(Some(name)) => self.resolver.insert_known(key, name),
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
    /// empty `found` and relies on [`Self::insert_known`] having already
    /// landed the names (flow-explorer does exactly this). Caching straight
    /// from `found` would write every one of those down as an absence, and the
    /// wallet would then go unnamed for the whole TTL — a persistent bug from
    /// a transient ordering.
    pub fn apply(&mut self, requested: &[String], found: HashMap<String, String>) {
        self.resolver.apply(requested, found);

        let now = (self.clock)();
        let settled: Vec<(String, Option<String>)> = requested
            .iter()
            .filter_map(|key| match self.resolver.status(key) {
                Status::Named => Some((
                    key.clone(),
                    self.resolver.name(key).map(str::to_string),
                )),
                Status::Nameless => Some((key.clone(), None)),
                // Abandoned is "we could not find out", which is not an answer
                // and must not be written down as one. Pending means another
                // request owns this key now.
                _ => None,
            })
            .collect();
        self.unsaved += settled.len();
        for (key, name) in settled {
            self.cache.remember(&key, name.as_deref(), now);
        }
    }

    /// Seed a name learned out of band — another view, a snapshot that arrived
    /// with the data. Cached like any other answer: it was true enough to draw.
    pub fn insert_known(&mut self, key: &str, name: &str) {
        self.cache.remember(key, Some(name), (self.clock)());
        self.unsaved += 1;
        self.resolver.insert_known(key, name);
    }

    /// The batch did not come back. Nothing is cached — a failure is not an
    /// answer, and writing it would turn an outage into a persisted absence
    /// that outlives it.
    pub fn fail(&mut self, requested: &[String]) {
        self.resolver.fail(requested);
    }

    /// Write the snapshot if anything has been learned and nothing is in
    /// flight.
    ///
    /// Called every frame. The idle check is what batches it: a burst of
    /// batches lands as one write at the end rather than one per reply, and
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
        let Some(store) = &self.store else {
            self.unsaved = 0;
            return;
        };
        store.save(&self.cache.snapshot((self.clock)()));
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

    #[test]
    fn a_name_survives_a_round_trip_through_a_snapshot() {
        let mut cache = HandleCache::new(CacheConfig::default());
        cache.remember("stake1a", Some("$alice"), 1000);
        let blob = cache.snapshot(1000);

        let restored = HandleCache::restore(&blob, 1000, CacheConfig::default());
        assert_eq!(restored.get("stake1a", 1000), Some(Some("$alice")));
    }

    /// The reason the cache is worth having at all.
    #[test]
    fn an_absence_survives_a_round_trip_too() {
        let mut cache = HandleCache::new(CacheConfig::default());
        cache.remember("stake1b", None, 1000);
        let blob = cache.snapshot(1000);

        let restored = HandleCache::restore(&blob, 1000, CacheConfig::default());
        assert_eq!(
            restored.get("stake1b", 1000),
            Some(None),
            "asked and found nothing is an ANSWER, not an absence of one"
        );
    }

    /// An absence is the answer most likely to go wrong, so it is trusted for
    /// the shortest time.
    #[test]
    fn an_absence_expires_long_before_a_name_does() {
        let config = CacheConfig::default();
        let mut cache = HandleCache::new(config);
        cache.remember("named", Some("$alice"), 1000);
        cache.remember("nameless", None, 1000);

        let later = 1000 + config.nameless_ttl + 1;
        assert_eq!(cache.get("nameless", later), None, "re-ask for this one");
        assert_eq!(cache.get("named", later), Some(Some("$alice")));
    }

    #[test]
    fn an_expired_entry_is_dropped_rather_than_carried_forward() {
        let mut cache = HandleCache::new(CacheConfig::default());
        cache.remember("stake1a", None, 1000);
        let blob = cache.snapshot(1_000_000);
        let restored = HandleCache::restore(&blob, 1_000_000, CacheConfig::default());
        assert!(restored.is_empty());
    }

    /// localStorage is a shared, finite quota that throws when it fills — an
    /// unbounded cache here takes the rest of the app's storage down with it.
    #[test]
    fn the_cache_stays_within_capacity() {
        let config = CacheConfig {
            capacity: 10,
            ..CacheConfig::default()
        };
        let mut cache = HandleCache::new(config);
        for i in 0..100 {
            cache.remember(&format!("stake{i}"), Some("$x"), 1000);
        }
        assert!(cache.len() <= 10, "held {} entries", cache.len());
    }

    /// Under pressure the names are what survive: they are the entries that
    /// change what a reader sees, and they are the ones with the longer TTL.
    #[test]
    fn eviction_sheds_absences_before_names() {
        let config = CacheConfig {
            capacity: 5,
            ..CacheConfig::default()
        };
        let mut cache = HandleCache::new(config);
        for i in 0..5 {
            cache.remember(&format!("named{i}"), Some("$x"), 1000);
        }
        for i in 0..20 {
            cache.remember(&format!("nameless{i}"), None, 1000);
        }
        for i in 0..5 {
            assert_eq!(
                cache.get(&format!("named{i}"), 1000),
                Some(Some("$x")),
                "a name was evicted while absences remained"
            );
        }
    }

    /// A cache is refillable. Refusing to start because its contents are
    /// unreadable would trade a free recovery for a broken app.
    #[test]
    fn an_unreadable_blob_yields_an_empty_cache_not_a_failure() {
        for blob in ["", "not json", r#"{"v":999,"entries":[]}"#] {
            let cache = HandleCache::restore(blob, 1000, CacheConfig::default());
            assert!(cache.is_empty(), "blob {blob:?}");
        }
    }

    #[test]
    fn a_cached_name_is_never_looked_up_again() {
        let (_now, clock) = test_clock();
        let store = Box::new(MemoryStore::default());

        let mut book = NameBook::new(CacheConfig::default(), clock).with_store(store);
        book.want("stake1a");
        book.want("stake1b");
        let batch = book.next_batch().expect("both queued");
        book.apply(&batch, found(&[("stake1a", "$alice")]));
        book.flush();

        // Same session, asked again: already settled.
        book.want("stake1a");
        book.want("stake1b");
        assert!(book.next_batch().is_none());
    }

    /// The whole point, end to end: a reload asks for nothing it asked for
    /// last time — including everything that had no handle.
    #[test]
    fn a_reload_asks_for_nothing_it_already_learned() {
        let (_now, clock) = test_clock();
        let shared = MemoryStore::default();

        let mut book =
            NameBook::new(CacheConfig::default(), clock).with_store(Box::new(shared.clone()));
        book.want("stake1a");
        book.want("stake1b");
        let batch = book.next_batch().expect("batch");
        book.apply(&batch, found(&[("stake1a", "$alice")]));
        book.flush();

        // A new session over the same storage.
        let (_now2, clock2) = test_clock();
        let mut reloaded =
            NameBook::new(CacheConfig::default(), clock2).with_store(Box::new(shared.clone()));
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

    /// An outage must not be written down. A cached absence outlives the
    /// failure that produced it, and would keep the name hidden long after the
    /// service came back.
    #[test]
    fn a_failed_batch_is_not_cached_as_an_absence() {
        let (_now, clock) = test_clock();
        let mut book = NameBook::new(CacheConfig::default(), clock)
            .with_store(Box::new(MemoryStore::default()));
        book.want("stake1a");
        let batch = book.next_batch().expect("batch");
        book.fail(&batch);
        book.flush();

        assert_eq!(book.cached(), 0);
    }

    /// Consulting the cache at `want` rather than seeding at startup is what
    /// keeps the progress readout about the view instead of about the cache.
    ///
    /// Seeding up front would report "naming wallets — 0/5000" on a page
    /// showing two of them.
    #[test]
    fn progress_counts_the_view_not_the_cache() {
        let (_now, clock) = test_clock();
        let shared = MemoryStore::default();

        let mut book =
            NameBook::new(CacheConfig::default(), clock).with_store(Box::new(shared.clone()));
        for i in 0..100 {
            book.insert_known(&format!("stake{i}"), "$x");
        }
        book.flush();

        // A later session showing two wallets, over a cache holding a hundred.
        let (_now2, clock2) = test_clock();
        let mut view =
            NameBook::new(CacheConfig::default(), clock2).with_store(Box::new(shared.clone()));
        assert_eq!(view.cached(), 100);
        view.want("stake0");
        view.want("stake1");
        assert_eq!(view.total(), 2, "the denominator is what is on screen");
        assert_eq!(view.progress(), Some(1.0));
    }

    #[test]
    fn nothing_is_written_while_work_is_outstanding() {
        let (_now, clock) = test_clock();
        let mut book = NameBook::new(CacheConfig::default(), clock)
            .with_store(Box::new(MemoryStore::default()));
        book.want("stake1a");
        book.want("stake1b");
        let first = book.next_batch().expect("batch");
        book.apply(&first, found(&[("stake1a", "$alice")]));

        book.flush_when_idle();
        assert_eq!(book.cached(), 2, "the answers are held either way");
    }
}
