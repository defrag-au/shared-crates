//! Typed HTTP client for the policy-info service.
//!
//! Uses reqwest for HTTP — works on both native and WASM targets.
//! Mirrors the structure of `ownership-client`. The wallet-viewer worker
//! and any other consumer that needs collection/token metadata for a
//! Cardano policy should use this rather than rolling their own HTTP
//! calls against `policies.cnft.dev`.

mod types;

pub use types::*;

use reqwest::Client;
use serde::de::DeserializeOwned;

/// Default production base URL for the policy-info service.
pub const DEFAULT_BASE_URL: &str = "https://policies.cnft.dev";

/// Maximum subjects accepted by `POST /api/policies` per request.
/// The worker rejects anything above this with a 400; the client chunks
/// transparently in `resolve_batch`.
pub const MAX_BATCH_SIZE: usize = 50;

/// Error type for policy-info client operations.
#[derive(Debug)]
pub enum Error {
    Request(String),
    Http { status: u16, body: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Request(e) => write!(f, "HTTP request error: {e}"),
            Error::Http { status, body } => write!(f, "HTTP {status}: {body}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Request(e.to_string())
    }
}

/// Client for the policy-info service.
#[derive(Clone)]
pub struct PolicyInfoClient {
    base_url: String,
    client: Client,
}

impl PolicyInfoClient {
    /// Create a new client targeting the given base URL (e.g. `https://policies.cnft.dev`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    /// Create a client with a debug token attached as `X-Debug-Token` —
    /// required only for admin endpoints like cache invalidation.
    pub fn with_debug_token(base_url: impl Into<String>, token: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-Debug-Token",
            reqwest::header::HeaderValue::from_str(token).unwrap(),
        );
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder().default_headers(headers).build().unwrap(),
        }
    }

    /// Resolve a single subject. `subject` may be a bare `policy_id` (56 hex
    /// chars) or a full `policy_id + asset_name_hex`.
    ///
    /// Returns `Ok(None)` for 404 (no metadata anywhere), `Err` for transport
    /// or other HTTP failures.
    pub async fn resolve(&self, subject: &str) -> Result<Option<ResolvedPolicy>, Error> {
        let url = format!("{}/api/policy/{subject}", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http { status, body });
        }
        Ok(Some(resp.json().await?))
    }

    /// Batch-resolve subjects. Transparently chunks at `MAX_BATCH_SIZE`
    /// and merges the responses. Missing subjects are simply absent from
    /// the returned map (matching server behaviour).
    pub async fn resolve_batch(
        &self,
        subjects: &[String],
    ) -> Result<BatchPolicyResponse, Error> {
        let mut merged: BatchPolicyResponse = BatchPolicyResponse::new();
        for chunk in subjects.chunks(MAX_BATCH_SIZE) {
            let body = BatchPolicyRequest {
                subjects: chunk.to_vec(),
            };
            let url = format!("{}/api/policies", self.base_url);
            let chunk_resp: BatchPolicyResponse =
                self.post_json(&url, &body).await?;
            merged.extend(chunk_resp);
        }
        Ok(merged)
    }

    /// Invalidate cached entries for a subject. Requires the client to have
    /// been constructed via [`with_debug_token`].
    pub async fn invalidate(&self, policy_id: &str) -> Result<(), Error> {
        let url = format!("{}/api/policy/{policy_id}/cache", self.base_url);
        let resp = self.client.delete(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http { status, body });
        }
        Ok(())
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &BatchPolicyRequest,
    ) -> Result<T, Error> {
        let resp = self.client.post(url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http { status, body });
        }
        Ok(resp.json().await?)
    }
}
