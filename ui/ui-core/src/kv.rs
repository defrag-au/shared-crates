//! A browser localStorage wrapper with a Workers KV-like API surface.
//!
//! Values are stored as JSON with an optional `expires_at` timestamp.
//! Expired entries are not automatically ejected from localStorage but
//! are treated as stale on read — `get()` returns `None` for expired keys
//! and lazily removes them.
//!
//! # Example
//!
//! ```ignore
//! use ui_core::kv::KvStore;
//!
//! let kv = KvStore::new("my-app").unwrap();
//! kv.put("greeting", "hello").expiration_ttl(3600).execute().unwrap();
//!
//! if let Some(val) = kv.get::<String>("greeting").unwrap() {
//!     tracing::info!("got: {val}");
//! }
//! ```

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A namespaced key-value store backed by `localStorage`.
///
/// All keys are prefixed with `{namespace}:` to avoid collisions
/// between different stores sharing the same `localStorage`.
pub struct KvStore {
    namespace: String,
    storage: web_sys::Storage,
}

/// Wrapper stored in localStorage around each value.
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    value: T,
    /// Unix timestamp (seconds) at which this entry expires.
    /// `None` means the entry never expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<f64>,
}

/// Builder for a `put` operation, allowing optional expiration.
pub struct PutBuilder<'a, T: Serialize> {
    store: &'a KvStore,
    key: &'a str,
    value: T,
    expires_at: Option<f64>,
}

/// Metadata about a stored key.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub name: String,
    pub expires_at: Option<f64>,
    pub expired: bool,
}

/// Result of a `get_with_metadata` call.
#[derive(Debug)]
pub struct ValueWithMetadata<T> {
    pub value: T,
    pub expires_at: Option<f64>,
}

#[derive(Debug)]
pub enum KvError {
    /// Could not access `window.localStorage`.
    StorageUnavailable,
    /// JSON serialization/deserialization failed.
    Serde(String),
    /// The underlying `Storage.setItem` or `Storage.removeItem` threw.
    StorageWrite(String),
}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KvError::StorageUnavailable => write!(f, "localStorage is not available"),
            KvError::Serde(msg) => write!(f, "serialization error: {msg}"),
            KvError::StorageWrite(msg) => write!(f, "storage write error: {msg}"),
        }
    }
}

impl std::error::Error for KvError {}

impl KvStore {
    /// Create a new namespaced KV store.
    ///
    /// All keys will be prefixed with `{namespace}:` in localStorage.
    pub fn new(namespace: &str) -> Result<Self, KvError> {
        let window = web_sys::window().ok_or(KvError::StorageUnavailable)?;
        let storage = window
            .local_storage()
            .map_err(|_| KvError::StorageUnavailable)?
            .ok_or(KvError::StorageUnavailable)?;

        Ok(Self {
            namespace: namespace.to_string(),
            storage,
        })
    }

    /// Get a value by key. Returns `None` if the key doesn't exist or has expired.
    ///
    /// Expired entries are lazily removed from storage on access.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, KvError> {
        let full_key = self.full_key(key);
        let raw = match self.storage.get_item(&full_key) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Ok(None),
        };

        let envelope: Envelope<T> =
            serde_json::from_str(&raw).map_err(|e| KvError::Serde(e.to_string()))?;

        if self.is_expired(&envelope) {
            let _ = self.storage.remove_item(&full_key);
            return Ok(None);
        }

        Ok(Some(envelope.value))
    }

    /// Get a value along with its expiration metadata.
    ///
    /// Returns `None` if the key doesn't exist or has expired.
    pub fn get_with_metadata<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<ValueWithMetadata<T>>, KvError> {
        let full_key = self.full_key(key);
        let raw = match self.storage.get_item(&full_key) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(_) => return Ok(None),
        };

        let envelope: Envelope<T> =
            serde_json::from_str(&raw).map_err(|e| KvError::Serde(e.to_string()))?;

        if self.is_expired(&envelope) {
            let _ = self.storage.remove_item(&full_key);
            return Ok(None);
        }

        Ok(Some(ValueWithMetadata {
            value: envelope.value,
            expires_at: envelope.expires_at,
        }))
    }

    /// Begin building a `put` operation for the given key and value.
    ///
    /// Call `.expiration_ttl(seconds)` or `.expiration(unix_ts)` on the
    /// returned builder, then `.execute()` to commit.
    ///
    /// ```ignore
    /// kv.put("key", "value").expiration_ttl(3600).execute()?;
    /// ```
    pub fn put<'a, T: Serialize>(&'a self, key: &'a str, value: T) -> PutBuilder<'a, T> {
        PutBuilder {
            store: self,
            key,
            value,
            expires_at: None,
        }
    }

    /// Delete a key from the store.
    pub fn delete(&self, key: &str) -> Result<(), KvError> {
        self.storage
            .remove_item(&self.full_key(key))
            .map_err(|e| KvError::StorageWrite(format!("{e:?}")))?;
        Ok(())
    }

    /// List all keys in this namespace.
    ///
    /// If `include_expired` is false, expired keys are excluded (and lazily removed).
    pub fn list(&self, include_expired: bool) -> Result<Vec<KeyInfo>, KvError> {
        let prefix = format!("{}:", self.namespace);
        let now = now_secs();
        let len = self
            .storage
            .length()
            .map_err(|e| KvError::StorageWrite(format!("{e:?}")))?;

        let mut keys = Vec::new();
        for i in 0..len {
            let full_key = match self.storage.key(i) {
                Ok(Some(k)) => k,
                _ => continue,
            };

            if let Some(short_key) = full_key.strip_prefix(&prefix) {
                let raw = match self.storage.get_item(&full_key) {
                    Ok(Some(v)) => v,
                    _ => continue,
                };

                let meta: EnvelopeMeta = match serde_json::from_str(&raw) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let expired = meta.expires_at.is_some_and(|exp| exp <= now);

                if expired && !include_expired {
                    let _ = self.storage.remove_item(&full_key);
                    continue;
                }

                keys.push(KeyInfo {
                    name: short_key.to_string(),
                    expires_at: meta.expires_at,
                    expired,
                });
            }
        }

        Ok(keys)
    }

    /// Remove all expired entries in this namespace.
    ///
    /// Returns the number of entries removed.
    pub fn purge_expired(&self) -> Result<usize, KvError> {
        let prefix = format!("{}:", self.namespace);
        let now = now_secs();
        let len = self
            .storage
            .length()
            .map_err(|e| KvError::StorageWrite(format!("{e:?}")))?;

        let mut to_remove = Vec::new();
        for i in 0..len {
            let full_key = match self.storage.key(i) {
                Ok(Some(k)) => k,
                _ => continue,
            };

            if !full_key.starts_with(&prefix) {
                continue;
            }

            let raw = match self.storage.get_item(&full_key) {
                Ok(Some(v)) => v,
                _ => continue,
            };

            let meta: EnvelopeMeta = match serde_json::from_str(&raw) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if meta.expires_at.is_some_and(|exp| exp <= now) {
                to_remove.push(full_key);
            }
        }

        let count = to_remove.len();
        for key in to_remove {
            let _ = self.storage.remove_item(&key);
        }

        Ok(count)
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}:{key}", self.namespace)
    }

    fn is_expired<T>(&self, envelope: &Envelope<T>) -> bool {
        envelope.expires_at.is_some_and(|exp| exp <= now_secs())
    }

    fn write_envelope<T: Serialize>(
        &self,
        key: &str,
        envelope: &Envelope<T>,
    ) -> Result<(), KvError> {
        let json = serde_json::to_string(envelope).map_err(|e| KvError::Serde(e.to_string()))?;
        self.storage
            .set_item(&self.full_key(key), &json)
            .map_err(|e| KvError::StorageWrite(format!("{e:?}")))?;
        Ok(())
    }
}

impl<'a, T: Serialize> PutBuilder<'a, T> {
    /// Set the expiration as a TTL in seconds from now.
    pub fn expiration_ttl(mut self, seconds: u64) -> Self {
        self.expires_at = Some(now_secs() + seconds as f64);
        self
    }

    /// Set the expiration as an absolute Unix timestamp (seconds).
    pub fn expiration(mut self, unix_timestamp: f64) -> Self {
        self.expires_at = Some(unix_timestamp);
        self
    }

    /// Write the value to the store.
    pub fn execute(self) -> Result<(), KvError> {
        let envelope = Envelope {
            value: self.value,
            expires_at: self.expires_at,
        };
        self.store.write_envelope(self.key, &envelope)
    }
}

/// Lightweight metadata-only deserialization (skips the value field).
#[derive(Deserialize)]
struct EnvelopeMeta {
    expires_at: Option<f64>,
}

/// Current time in seconds since Unix epoch.
fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}
