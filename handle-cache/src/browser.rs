//! The localStorage-backed [`Store`], and the browser clock.
//!
//! Deliberately the only part of this crate that touches `web_sys`, and the
//! only part behind the `browser` feature. Everything that decides anything —
//! the TTLs, the eviction, the snapshot format — is in the crate root and runs
//! on a plain `cargo test`. A macroquad target can depend on this crate as
//! long as it leaves the feature off: miniquad has no wasm-bindgen glue, so
//! anything reaching `web_sys` is unusable there.

use crate::{CacheConfig, NameBook, Store};

/// One localStorage entry holding the whole snapshot.
///
/// One entry rather than one per wallet: a per-key store costs a read and a
/// JSON parse per wallet on every view, and its eviction has to scan the whole
/// origin's storage to find its own keys.
pub struct LocalStore {
    key: String,
}

impl LocalStore {
    /// `namespace` distinguishes one app's names from another's on a shared
    /// origin.
    pub fn new(namespace: &str) -> Self {
        Self {
            key: format!("handle-cache:{namespace}"),
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
    /// entirely — none of them are worth failing a render over, because every
    /// name in here can be fetched again. The cost of a failed write is one
    /// slow session, not a broken one.
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

impl NameBook {
    /// A book backed by localStorage under `namespace`, on the browser clock.
    ///
    /// The one-liner every frontend wants:
    ///
    /// ```ignore
    /// let names = NameBook::browser("flow_names", CacheConfig::default())
    ///     .with_batch_size(25);
    /// ```
    pub fn browser(namespace: &str, config: CacheConfig) -> Self {
        Self::new(config, Box::new(now_secs)).with_store(Box::new(LocalStore::new(namespace)))
    }
}
