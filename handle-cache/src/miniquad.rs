//! The macroquad end of the book: where it is kept, and what a reply means.
//!
//! [`crate::NameBook`] is runtime-free — it decides what to ask and when to
//! give up, and the browser store it ships is `web-sys`, which cannot run under
//! miniquad. This module supplies the two things a macroquad app needs and
//! neither frontend should write twice:
//!
//! - a [`Store`] over `miniquad-platform`'s storage plugin (`localStorage` in
//!   the browser, nothing natively), under `handles:<namespace>`;
//! - [`apply_reply`], which is where the one trap in the route lives. See its
//!   own docs — getting it wrong caches an outage as "this wallet has no
//!   handle" for an hour.
//!
//! The clock is the host's, not this crate's: `macroquad::miniquad::date::now()`
//! is one line at the call site and the platform's clock is the one a returning
//! reader expects a TTL to be measured against. Passing it also keeps this
//! crate free of macroquad, so a test can drive a book with a stopped clock.
//!
//! ```ignore
//! use handle_cache::miniquad::Names;
//!
//! let mut names = Names::new("holder-map", Box::new(now_secs));
//! names.want(holders.iter().map(|h| h.key.as_str()));
//! if let Some(batch) = names.next_batch() {
//!     // ...one GET to `/api/handles?stake=...`, then:
//!     names.apply(&batch, &body);
//! }
//! names.flush_when_idle();
//! ```

use std::collections::HashMap;

use crate::{NameBook, NameCacheConfig, Store};

/// Stakes in one `/api/handles` request: the route's own ceiling, stated here
/// so a caller can size a batch rather than discover a silent truncation.
pub const MAX_PER_REQUEST: usize = 100;

/// `localStorage` under `handles:<namespace>`.
///
/// A namespace is the page's, not this crate's: two surfaces that share a
/// browser share the answers, which is the point — a wallet named in the arena
/// is named in the holder map before the first frame. A surface that wants its
/// own shelf passes its own name.
struct PlatformStore {
    key: String,
}

impl Store for PlatformStore {
    fn load(&self) -> Option<String> {
        miniquad_platform::storage_get(&self.key)
    }

    fn save(&self, blob: &str) {
        miniquad_platform::storage_set(&self.key, blob);
    }
}

/// A [`NameBook`] with the macroquad store attached.
///
/// Batch size is the route's ceiling, so a full page of holders is one request
/// rather than a handful: the book already deduplicates and keeps one batch in
/// flight.
pub struct Names {
    book: NameBook,
}

impl Names {
    /// `clock` yields unix seconds.
    pub fn new(namespace: &str, clock: Box<dyn Fn() -> u64>) -> Self {
        Self {
            book: NameBook::new(NameCacheConfig::default(), clock)
                .with_store(Box::new(PlatformStore {
                    key: format!("handles:{namespace}"),
                }))
                .with_batch_size(MAX_PER_REQUEST),
        }
    }

    /// Ask for a key's name, answering from the cache where one is held. Safe
    /// to call every frame for everything on screen.
    pub fn want<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        self.book.want_all(keys);
    }

    /// What the book knows for `key`.
    pub fn name(&self, key: &str) -> Option<&str> {
        self.book.name(key)
    }

    /// The next batch to send, if one is queued and none is in flight.
    pub fn next_batch(&mut self) -> Option<Vec<String>> {
        self.book.next_batch()
    }

    /// Take in a batch's reply. True when names arrived, so a view is worth
    /// drawing again. See [`apply_reply`] for what a reply means.
    pub fn apply(&mut self, requested: &[String], body: &str) -> bool {
        apply_reply(&mut self.book, requested, body)
    }

    /// The request failed at the transport. Nothing is cached: an outage is not
    /// an absence.
    pub fn fail(&mut self, requested: &[String]) {
        self.book.fail(requested);
    }

    /// Write the snapshot if anything was learned and nothing is in flight.
    pub fn flush_when_idle(&mut self) {
        self.book.flush_when_idle();
    }

    pub fn progress(&self) -> Option<f32> {
        self.book.progress()
    }
}

/// Apply one `/api/handles` reply to `book`. True when a name was learned.
///
/// The answer is `{ stake: handle }`, where every stake in the batch absent
/// from the map has NO handle — remembering that is most of the saving.
///
/// # ⚠️ `{ "unavailable": reason }` is a FAILURE, not an absence
///
/// The route answers that body with a **200** when the handle service is out
/// (the client's HTTP drops every other status, so a 503 would read as a
/// transport error). Parsed as a map of stakes, it is a map with one unknown
/// key — so every stake in the batch reads as "no handle" and is cached as
/// such for [`NameCacheConfig::nameless_ttl`]. The symptom is a wallet that
/// goes unnamed for an hour after one bad minute, with nothing in any log.
///
/// An unparseable body is treated the same way: it is not an answer.
pub fn apply_reply(book: &mut NameBook, requested: &[String], body: &str) -> bool {
    let Ok(map) = serde_json::from_str::<HashMap<String, String>>(body) else {
        book.fail(requested);
        return false;
    };
    if map.contains_key("unavailable") {
        book.fail(requested);
        return false;
    }
    let arrived = !map.is_empty();
    book.apply(requested, map);
    arrived
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryStore, Status};

    fn book() -> NameBook {
        NameBook::new(NameCacheConfig::default(), Box::new(|| 1_000_000))
            .with_store(Box::new(MemoryStore::default()))
    }

    /// The trap this module exists for: an outage body, answered with a 200,
    /// must not be written down as "no handle".
    #[test]
    fn an_unavailable_answer_is_a_failure_and_is_never_cached_as_an_absence() {
        let mut book = book();
        let batch = vec!["stake1a".to_string()];
        book.want("stake1a");
        assert_eq!(book.next_batch().as_deref(), Some(batch.as_slice()));
        assert!(!apply_reply(
            &mut book,
            &batch,
            r#"{"unavailable":"the handle service is down"}"#
        ));
        assert_ne!(book.status("stake1a"), Status::Nameless);
        assert_eq!(book.cached(), 0, "a failure is not an answer");
        // Nothing was written, so the key is still queued: the next batch asks
        // again rather than answering from a cached absence.
        assert_eq!(book.next_batch().as_deref(), Some(batch.as_slice()));
    }

    #[test]
    fn a_map_answers_every_key_it_was_asked_about() {
        let mut book = book();
        let batch = vec!["stake1a".to_string(), "stake1b".to_string()];
        book.want("stake1a");
        book.want("stake1b");
        assert_eq!(book.next_batch().as_deref(), Some(batch.as_slice()));
        assert!(apply_reply(
            &mut book,
            &batch,
            r#"{"stake1a":"$alice"}"#
        ));
        assert_eq!(book.name("stake1a"), Some("$alice"));
        // Absent from the map is an answer too, and is settled as one.
        assert_eq!(book.status("stake1b"), Status::Nameless);
    }

    #[test]
    fn a_body_that_is_not_json_is_not_an_answer() {
        let mut book = book();
        let batch = vec!["stake1a".to_string()];
        book.want("stake1a");
        assert_eq!(book.next_batch().as_deref(), Some(batch.as_slice()));
        assert!(!apply_reply(&mut book, &batch, "<html>bad gateway</html>"));
        assert_ne!(book.status("stake1a"), Status::Nameless);
        assert_eq!(book.cached(), 0);
    }
}
