//! A bounded image cache.
//!
//! Every loader in the estate caches; **none of them evicts**. One reports its
//! own byte size and does nothing with the number; another hands out blob URLs
//! and never revokes them, which leaks for as long as the tab is open. On a
//! desktop with a dozen images that is invisible. On a phone browsing a
//! thousand-asset collection it is not.
//!
//! So this one has a budget and gives things up in least-recently-used order.

use std::collections::HashMap;

/// An entry the cache gave up, so the caller can release whatever it attached
/// — a GPU texture, a blob URL — that this crate deliberately knows nothing
/// about.
///
/// Returned rather than dropped silently because on most runtimes the *pixels*
/// are the small part: freeing 640 KB of RGBA while leaving the texture
/// uploaded would be a leak that looks like a working cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evicted<V> {
    pub key: String,
    pub value: V,
}

struct Slot<V> {
    value: V,
    bytes: usize,
    /// Monotonic tick of last use. A counter rather than a clock: this crate
    /// has no clock, and ordering is all that's needed.
    used: u64,
}

/// LRU cache with a byte budget.
///
/// Generic over the value so each runtime can store what suits it — decoded
/// pixels, an egui `ColorImage`, a `Texture2D` handle — while the eviction
/// policy stays here and stays tested.
pub struct ImageCache<V> {
    slots: HashMap<String, Slot<V>>,
    budget: usize,
    used_bytes: usize,
    tick: u64,
}

impl<V> ImageCache<V> {
    /// `budget_bytes` is a ceiling on the *reported* size of everything held.
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            slots: HashMap::new(),
            budget: budget_bytes,
            used_bytes: 0,
            tick: 0,
        }
    }

    /// Insert, evicting least-recently-used entries until the budget fits.
    ///
    /// `bytes` is what this entry costs — the caller's judgement, since only
    /// it knows whether it is holding pixels, a texture, or both.
    ///
    /// An entry larger than the whole budget is still stored, and everything
    /// else is evicted to make room. Refusing it would mean a screen that
    /// silently shows nothing on a small budget; one oversized image that
    /// pushes others out is the better failure, and the caller can see it in
    /// [`Self::used_bytes`].
    pub fn insert(&mut self, key: impl Into<String>, value: V, bytes: usize) -> Vec<Evicted<V>> {
        let key = key.into();
        let mut evicted = Vec::new();

        // Replacing an entry releases its bytes first, or a repeatedly-updated
        // key would inflate the total until everything else was evicted.
        if let Some(old) = self.slots.remove(&key) {
            self.used_bytes -= old.bytes;
            evicted.push(Evicted {
                key: key.clone(),
                value: old.value,
            });
        }

        self.tick += 1;
        self.slots.insert(
            key,
            Slot {
                value,
                bytes,
                used: self.tick,
            },
        );
        self.used_bytes += bytes;

        evicted.extend(self.trim());
        evicted
    }

    /// Fetch and mark as recently used.
    pub fn get(&mut self, key: &str) -> Option<&V> {
        self.tick += 1;
        let tick = self.tick;
        let slot = self.slots.get_mut(key)?;
        slot.used = tick;
        Some(&slot.value)
    }

    /// Is it held? Does **not** count as a use, so a caller polling
    /// availability can't keep something alive it never actually draws.
    pub fn contains(&self, key: &str) -> bool {
        self.slots.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        let slot = self.slots.remove(key)?;
        self.used_bytes -= slot.bytes;
        Some(slot.value)
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Drop everything, handing it all back for release.
    pub fn clear(&mut self) -> Vec<Evicted<V>> {
        self.used_bytes = 0;
        self.slots
            .drain()
            .map(|(key, slot)| Evicted {
                key,
                value: slot.value,
            })
            .collect()
    }

    /// Evict LRU-first until inside budget. Never evicts the last entry, so a
    /// budget smaller than one image degrades to a cache of one rather than to
    /// a cache that drops what it was just handed.
    fn trim(&mut self) -> Vec<Evicted<V>> {
        let mut evicted = Vec::new();
        while self.used_bytes > self.budget && self.slots.len() > 1 {
            let Some(oldest) = self
                .slots
                .iter()
                .min_by_key(|(key, slot)| (slot.used, (*key).clone()))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(slot) = self.slots.remove(&oldest) {
                self.used_bytes -= slot.bytes;
                evicted.push(Evicted {
                    key: oldest,
                    value: slot.value,
                });
            }
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Budget for exactly two 100-byte entries.
    fn cache() -> ImageCache<&'static str> {
        ImageCache::new(200)
    }

    #[test]
    fn evicts_least_recently_used_when_over_budget() {
        let mut c = cache();
        c.insert("a", "A", 100);
        c.insert("b", "B", 100);
        // Touching `a` makes `b` the oldest.
        assert_eq!(c.get("a"), Some(&"A"));

        let evicted = c.insert("c", "C", 100);
        assert_eq!(
            evicted,
            vec![Evicted {
                key: "b".into(),
                value: "B"
            }]
        );
        assert!(c.contains("a") && c.contains("c"));
        assert_eq!(c.used_bytes(), 200);
    }

    #[test]
    fn eviction_hands_the_value_back_for_release() {
        // The whole reason eviction returns anything: on most runtimes the
        // pixels are the cheap part and a GPU texture hangs off the value.
        // Dropping silently would be a leak wearing a cache's clothes.
        let mut c = ImageCache::new(100);
        c.insert("a", "texture-a", 100);
        let evicted = c.insert("b", "texture-b", 100);
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].value, "texture-a");
    }

    #[test]
    fn replacing_a_key_releases_the_old_bytes() {
        // Otherwise a key updated each frame inflates the total until it
        // evicts everything else — a leak that only shows under motion.
        let mut c = cache();
        c.insert("a", "A", 100);
        c.insert("a", "A2", 100);
        assert_eq!(c.used_bytes(), 100);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn contains_does_not_count_as_a_use() {
        // A caller polling "is it ready?" every frame must not keep something
        // alive that it never draws.
        let mut c = cache();
        c.insert("a", "A", 100);
        c.insert("b", "B", 100);
        assert!(c.contains("a"));
        // `a` is still the oldest despite the `contains`, so it goes first.
        let evicted = c.insert("c", "C", 100);
        assert_eq!(evicted[0].key, "a");
    }

    #[test]
    fn an_oversized_entry_is_kept_rather_than_silently_dropped() {
        // A cache that refuses what it was just handed shows a blank screen
        // and looks like a loading failure.
        let mut c = ImageCache::new(50);
        c.insert("big", "B", 500);
        assert!(c.contains("big"));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn clear_returns_everything_so_nothing_leaks() {
        let mut c = cache();
        c.insert("a", "A", 100);
        c.insert("b", "B", 100);
        let mut dropped: Vec<String> = c.clear().into_iter().map(|e| e.key).collect();
        dropped.sort();
        assert_eq!(dropped, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(c.used_bytes(), 0);
        assert!(c.is_empty());
    }
}
