//! The localStorage-backed [`Store`], and the browser clock.
//!
//! Deliberately the only part of this crate that touches `web_sys`, and the
//! only part behind the `browser` feature. Everything that decides anything —
//! the TTLs, the eviction, the snapshot format — is in the crate root and runs
//! on a plain `cargo test`. A macroquad target can depend on this crate as
//! long as it leaves the feature off: miniquad has no wasm-bindgen glue, so
//! anything reaching `web_sys` is unusable there.

use crate::Store;

/// One localStorage entry holding a whole dataset's snapshot.
///
/// One entry rather than one per key: a per-key store costs a read and a JSON
/// parse per entry on every view, and its eviction has to scan the whole
/// origin's storage to find its own keys.
pub struct LocalStore {
    key: String,
}

impl LocalStore {
    /// `namespace` names the dataset — `"handles"`, `"prices"`. Distinct
    /// namespaces do not share a blob, so one dataset filling up or being
    /// discarded never disturbs another.
    pub fn new(namespace: &str) -> Self {
        Self {
            key: format!("local-cache:{namespace}"),
        }
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }
}

impl Store for LocalStore {
    fn load(&self) -> Option<String> {
        Self::storage()?.get_item(&self.key).ok()?
    }

    /// Best-effort. A quota error, a browser in private mode, storage disabled
    /// entirely — none of them are worth failing a render over, because
    /// everything in here can be fetched again. The cost of a failed write is
    /// one slow session, not a broken one.
    fn save(&self, blob: &str) {
        if let Some(storage) = Self::storage() {
            let _ = storage.set_item(&self.key, blob);
        }
    }
}

/// Unix seconds, from the browser.
pub fn now_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
