//! Typed OpenSea API v2 client.
//!
//! Requests go through [`http_client`], which selects reqwest on native and the
//! worker/gloo path on wasm32. Nothing here names a runtime, so the same client
//! compiles for a Cloudflare Worker and for a native service.
//!
//! ## The key is passed in, never read from the environment
//!
//! [`OpenseaClient::new`] writes the key into the `X-API-KEY` header. Sourcing it
//! is the caller's job — `worker_utils::secrets` in a Worker, a CLI flag in a
//! tool. This crate has no notion of an environment, which is also what lets its
//! tests run offline.
//!
//! **The key is a server-side credential.** The crate will compile for a
//! browser, because `http-client` does — but a frontend that calls OpenSea
//! directly publishes the key to every visitor. Keep the calls worker-side.
//!
//! ## Errors keep their reason
//!
//! OpenSea explains a refusal in the body — `{"errors":["Missing an API Key,
//! which is required for this request."]}` — and `http-client`'s plain `get`
//! discards it, because it calls `error_for_status` and only the status survives.
//! Every call here goes through `request_text_with_details` and branches on the
//! status itself, so [`OpenseaError::Api`] carries the sentence rather than just
//! "400".
//!
//! ## Only observed endpoints are modelled
//!
//! The types in [`types`] are copied from captured responses, one fixture per
//! endpoint per shape. Endpoints not modelled yet — listings, offers, events,
//! account NFTs — are reachable through the [`OpenseaClient::get_json`] escape
//! hatch with your own type, which is how a shape gets added here: capture it,
//! model it, and add the fixture.
//!
//! ## Authentication, as observed
//!
//! Without a key, `/chains`, `/collections` and `/collections/{slug}` answer on
//! this API. Pagination, `/stats`, `/chain/{chain}/contract/{address}` and the
//! NFT pages answer `401`. [`OpenseaClient::without_key`] exists for that public
//! subset — it is not a way to avoid the key for the rest.

mod types;

pub use types::*;

use std::time::Duration;

use http_client::{HttpClient, HttpMethod};
use serde::de::DeserializeOwned;

/// The API root. Version is part of the path on OpenSea v2.
pub const BASE_URL: &str = "https://api.opensea.io/api/v2";

/// Anything that can go wrong talking to OpenSea.
#[derive(Debug, thiserror::Error)]
pub enum OpenseaError {
    /// The request never got a response, or the transport failed.
    #[error(transparent)]
    Http(#[from] http_client::HttpError),
    /// OpenSea answered non-2xx, other than a rate limit. `errors` is its own
    /// `{"errors":[…]}` text, so the caller sees the reason and not just a code.
    #[error("OpenSea returned {status}: {}", .errors.join("; "))]
    Api { status: u16, errors: Vec<String> },
    /// OpenSea answered 429. `retry_after_seconds` is its `retry-after` header,
    /// absent when it did not send one.
    #[error("OpenSea rate limited this request (retry-after: {retry_after_seconds:?} seconds)")]
    RateLimited { retry_after_seconds: Option<u64> },
    /// A 2xx body that did not match the modelled shape — usually a field whose
    /// type is not what the capture showed.
    #[error("could not decode the OpenSea response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// An OpenSea v2 client. Cheap to clone: the underlying HTTP client is shared.
#[derive(Clone)]
pub struct OpenseaClient {
    client: HttpClient,
}

impl OpenseaClient {
    /// Authenticate with an API key, sent as `X-API-KEY`.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: HttpClient::new().with_header("X-API-KEY", &api_key.into()),
        }
    }

    /// No key. Only the unauthenticated subset answers — see the crate docs.
    pub fn without_key() -> Self {
        Self {
            client: HttpClient::new(),
        }
    }

    /// Bound every request, body included. Unbounded by default, which is a bad
    /// default in a Worker: `fetch` has no timeout of its own.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = self.client.with_timeout(timeout);
        self
    }

    /// `GET /chains` — the chains OpenSea indexes, as slugs.
    pub async fn chains(&self) -> Result<Vec<ChainInfo>, OpenseaError> {
        let list: ChainList = self.get_json("/chains").await?;
        Ok(list.chains)
    }

    /// `GET /collections?chain=…` — one page of a chain's collections.
    ///
    /// `cursor` is the `next` of the previous page, passed back unchanged; the
    /// client encodes it. `None` starts from the first page.
    pub async fn collections_page(
        &self,
        chain: &ChainSlug,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<CollectionPage, OpenseaError> {
        let mut path = format!("/collections?chain={}", encode_query_value(chain.as_str()));
        if let Some(limit) = limit {
            path.push_str(&format!("&limit={limit}"));
        }
        if let Some(cursor) = cursor {
            path.push_str(&format!("&next={}", encode_query_value(cursor)));
        }
        self.get_json(&path).await
    }

    /// `GET /collections/{slug}` — detail, including supply, fees and the
    /// currencies the collection prices in.
    pub async fn collection(&self, slug: &str) -> Result<CollectionDetails, OpenseaError> {
        self.get_json(&format!("/collections/{}", encode_query_value(slug)))
            .await
    }

    /// `GET /collections/{slug}/stats` — lifetime totals and the day/week/month
    /// buckets.
    pub async fn collection_stats(&self, slug: &str) -> Result<CollectionStats, OpenseaError> {
        self.get_json(&format!("/collections/{}/stats", encode_query_value(slug)))
            .await
    }

    /// `GET /chain/{chain}/contract/{address}` — which collection a contract
    /// belongs to, its standard, and its name.
    pub async fn contract(
        &self,
        chain: &ChainSlug,
        address: &str,
    ) -> Result<Contract, OpenseaError> {
        self.get_json(&format!(
            "/chain/{}/contract/{}",
            encode_query_value(chain.as_str()),
            encode_query_value(address)
        ))
        .await
    }

    /// `GET /chain/{chain}/contract/{address}/nfts` — one page of a contract's
    /// tokens, traits included.
    pub async fn contract_nfts_page(
        &self,
        chain: &ChainSlug,
        address: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<NftPage, OpenseaError> {
        let mut path = format!(
            "/chain/{}/contract/{}/nfts?limit={}",
            encode_query_value(chain.as_str()),
            encode_query_value(address),
            limit.unwrap_or(50)
        );
        if let Some(cursor) = cursor {
            path.push_str(&format!("&next={}", encode_query_value(cursor)));
        }
        self.get_json(&path).await
    }

    /// The escape hatch: any `/api/v2` path, with your own type, using this
    /// client's key, timeouts and error handling.
    ///
    /// This is how an endpoint gets adopted — call it with a candidate type,
    /// confirm the shape against the live answer, then move it into [`types`]
    /// with a fixture. `path` is relative to [`BASE_URL`] and includes its own
    /// query string.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, OpenseaError> {
        let url = format!("{BASE_URL}{path}");
        let response = self
            .client
            .request_text_with_details::<()>(HttpMethod::GET, &url, None)
            .await?;

        if !(200..300).contains(&response.status_code) {
            if response.status_code == 429 {
                return Err(OpenseaError::RateLimited {
                    retry_after_seconds: response.retry_after_seconds(),
                });
            }
            return Err(OpenseaError::Api {
                status: response.status_code,
                errors: parse_errors(&response.data),
            });
        }

        Ok(serde_json::from_str(&response.data)?)
    }
}

/// The reason a request was refused: OpenSea's `{"errors":[…]}` when the body is
/// that shape, and the body itself when it is not — a 502 from an intermediary is
/// HTML, and losing it turns a diagnosable failure into "nonzero exit".
fn parse_errors(body: &str) -> Vec<String> {
    if body.trim().is_empty() {
        return vec!["no response body".to_owned()];
    }
    match serde_json::from_str::<ApiErrorBody>(body) {
        Ok(parsed) => parsed.errors,
        Err(_) => vec![body.chars().take(400).collect()],
    }
}

/// Percent-encode one URL segment or query value, leaving the unreserved set
/// alone.
///
/// `next` cursors are base64: the captures happen to be alphanumeric, but `+`,
/// `/` and `=` are legal in base64 and mean something else in a query string, so
/// they are encoded rather than trusted.
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreserved_characters_pass_through_unchanged() {
        // The shape of a real cursor and a real contract address: nothing to encode.
        assert_eq!(
            encode_query_value("WyIyMDI2LTA5LTI2VDIzOjM4OjMyWiJd"),
            "WyIyMDI2LTA5LTI2VDIzOjM4OjMyWiJd"
        );
        assert_eq!(
            encode_query_value("0x539cdd042c2f3d93ebc5be7dfff0c79f3b4fabf0"),
            "0x539cdd042c2f3d93ebc5be7dfff0c79f3b4fabf0"
        );
        assert_eq!(encode_query_value("heritage-hood"), "heritage-hood");
    }

    #[test]
    fn base64_padding_and_plus_are_encoded() {
        // `+` would decode as a space and `=` would end the value early.
        assert_eq!(encode_query_value("a+b/c=="), "a%2Bb%2Fc%3D%3D");
    }

    #[test]
    fn a_json_error_body_yields_its_messages() {
        let errors = parse_errors(r#"{"errors":["Missing an API Key","and another"]}"#);
        assert_eq!(errors, vec!["Missing an API Key", "and another"]);
    }

    #[test]
    fn a_non_json_error_body_is_kept_as_text() {
        assert_eq!(parse_errors("<html>502</html>"), vec!["<html>502</html>"]);
    }

    #[test]
    fn an_empty_error_body_says_so() {
        assert_eq!(parse_errors("   "), vec!["no response body"]);
    }
}
