//! Transaction submission — `POST /submittx`.
//!
//! Koios fronts `cardano-submit-api`: the body is the signed transaction as
//! RAW CBOR bytes (`Content-Type: application/cbor`), and a `202 Accepted`
//! answers with the transaction hash. The JSON [`http_client::HttpClient`]
//! cannot send a binary body, so this goes through the Worker `Fetch` API
//! directly — the same route the org's `tx-submit` Koios submitter takes —
//! authenticating with the client's own `KOIOS_API_KEY` bearer token.
//!
//! Deliberately NOT retried, unlike the JSON reads: a submit that timed out
//! may still have reached the mempool, and whether to resubmit is the
//! caller's decision, not the transport's.

use tracing::{info, warn};
use worker_stack::worker::{Fetch, Headers, Method, Request, RequestInit};

use crate::{KoiosApi, KoiosError};

impl KoiosApi {
    /// Submit a signed transaction (raw CBOR bytes) and return its hash.
    ///
    /// A non-2xx answer — the ledger rejecting the transaction, a malformed
    /// body, a rate limit — surfaces as [`KoiosError::KoiosResponse`] with the
    /// status and Koios's body, which for a rejection names the ledger rule
    /// that failed. A transport failure surfaces as [`KoiosError::Worker`].
    ///
    /// Uses the Worker `Fetch` API, so it only runs inside a Worker.
    pub async fn submit_transaction(&self, tx_cbor: &[u8]) -> Result<String, KoiosError> {
        let url = format!("{}/submittx", self.base_url);

        let headers = Headers::new();
        headers.set("Content-Type", "application/cbor")?;
        if let Some(key) = &self.api_key {
            headers.set("Authorization", &format!("Bearer {key}"))?;
        }

        let mut init = RequestInit::new();
        init.method = Method::Post;
        init.headers = headers;
        init.body = Some(tx_cbor.to_vec().into());

        info!("submitting {} byte transaction to {url}", tx_cbor.len());
        let request = Request::new_with_init(&url, &init)?;
        let mut response = Fetch::Request(request).send().await?;

        let status = response.status_code();
        let body = response.text().await.unwrap_or_default();
        if !(200..300).contains(&status) {
            warn!("Koios submittx rejected ({status}): {body}");
            return Err(KoiosError::KoiosResponse { status, body });
        }

        let tx_hash = submitted_tx_hash(&body);
        if !is_tx_hash(&tx_hash) {
            // Accepted, so the transaction is on its way regardless; the hash
            // is passed through as-is rather than failing a submit that worked.
            warn!("Koios submittx accepted but answered with an unexpected body: {body}");
        }
        Ok(tx_hash)
    }
}

/// The transaction hash out of a `/submittx` success body.
///
/// `cardano-submit-api` answers with a JSON string (`"ab12…"`, quotes
/// included); a bare hash is accepted too, so a proxy that unwraps it does not
/// break submission.
fn submitted_tx_hash(body: &str) -> String {
    let body = body.trim();
    serde_json::from_str::<String>(body)
        .unwrap_or_else(|_| body.to_string())
        .trim()
        .to_string()
}

/// A Cardano transaction hash: 32 bytes, hex.
fn is_tx_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "fdba011794eef2717512a534e3b751b4b44fdbd11e7b8b3e9875ebb9a86b65e6";

    #[test]
    fn reads_a_json_string_body() {
        let body = format!("\"{HASH}\"\n");
        assert_eq!(submitted_tx_hash(&body), HASH);
        assert!(is_tx_hash(&submitted_tx_hash(&body)));
    }

    #[test]
    fn reads_a_bare_hash_body() {
        assert_eq!(submitted_tx_hash(&format!("  {HASH}\n")), HASH);
    }

    #[test]
    fn an_unexpected_body_is_not_a_hash() {
        assert!(!is_tx_hash(&submitted_tx_hash("{\"error\":\"x\"}")));
        assert!(!is_tx_hash(&submitted_tx_hash("")));
    }
}
