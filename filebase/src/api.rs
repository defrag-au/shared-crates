use std::collections::BTreeMap;

use http_client::{HttpClient, HttpMethod};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::FilebaseError;

/// Filebase's implementation of the IPFS Pinning Service API spec.
/// The bucket is determined by the bearer token, not the request body.
const BASE_URL: &str = "https://api.filebase.io/v1/ipfs";

pub struct FilebaseApi {
    pub token: Option<String>,
    pub client: HttpClient,
}

impl Default for FilebaseApi {
    fn default() -> Self {
        Self::new()
    }
}

impl FilebaseApi {
    pub fn new() -> Self {
        Self {
            token: None,
            client: HttpClient::new()
                .with_user_agent("DefragFilebaseClient/1.0 (+https://defrag.au)")
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json"),
        }
    }

    pub fn with_token(token: impl Into<String>) -> Self {
        let token = token.into();
        let auth_header = format!("Bearer {token}");
        Self {
            token: Some(token),
            client: HttpClient::new()
                .with_user_agent("DefragFilebaseClient/1.0 (+https://defrag.au)")
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json")
                .with_header("Authorization", &auth_header),
        }
    }

    /// Make a request and return the parsed body.
    ///
    /// Uses `request_with_details` under the hood so that non-2xx responses
    /// surface as `HttpError::HttpStatus` with headers intact — callers can
    /// inspect `Retry-After` on 429 via `HttpError::retry_after_seconds()`.
    pub async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<R, FilebaseError> {
        let details = self
            .client
            .request_with_details::<T, R>(method, url, body)
            .await
            .map_err(FilebaseError::Request)?;
        Ok(details.data)
    }

    /// Pin an existing CID to Filebase.
    ///
    /// The pin is added to the bucket associated with the API token.
    /// `name` accepts `/`-separated segments to organize pins into folders
    /// in the bucket dashboard. `meta` is round-tripped to the server and is
    /// queryable via `GET /pins?meta=...`.
    pub async fn pin_by_cid(
        &self,
        cid: &str,
        name: Option<&str>,
        meta: Option<&BTreeMap<String, String>>,
    ) -> Result<PinByCidResponse, FilebaseError> {
        let url = format!("{BASE_URL}/pins");

        let request = PinByCidRequest {
            cid: cid.to_string(),
            name: name.map(|s| s.to_string()),
            origins: None,
            meta: meta.cloned(),
        };

        self.request(HttpMethod::POST, &url, Some(&request)).await
    }

    /// List pins matching the given query.
    ///
    /// The response's `count` is the full filtered total regardless of how
    /// small `limit` is, so a `limit` of 1 is enough for a pure count query.
    pub async fn list_pins(
        &self,
        query: &PinListQuery,
    ) -> Result<PinListResponse, FilebaseError> {
        let mut url = format!("{BASE_URL}/pins?limit={}", query.limit);
        if let Some(s) = &query.status {
            url.push_str("&status=");
            url.push_str(s);
        }
        if let Some(n) = &query.name {
            url.push_str("&name=");
            url.push_str(&urlencoding::encode(n));
        }
        if let Some(m) = &query.name_match {
            url.push_str("&match=");
            url.push_str(m);
        }
        if let Some(meta) = &query.meta {
            let json = serde_json::to_string(meta).unwrap_or_default();
            url.push_str("&meta=");
            url.push_str(&urlencoding::encode(&json));
        }
        if let Some(before) = &query.before {
            url.push_str("&before=");
            url.push_str(&urlencoding::encode(before));
        }
        self.request::<(), _>(HttpMethod::GET, &url, None).await
    }

    /// Delete (unpin) a pin by its request id.
    ///
    /// Drops the pin record; the underlying block stays resident as long as
    /// another pin references it. Filebase responds with an empty body, so
    /// the response is read as text and discarded — only the status matters.
    pub async fn delete_pin(&self, requestid: &str) -> Result<(), FilebaseError> {
        let url = format!("{BASE_URL}/pins/{requestid}");
        self.client
            .request_text_with_details::<()>(HttpMethod::DELETE, &url, None)
            .await
            .map(|_| ())
            .map_err(FilebaseError::Request)
    }
}

/// Query filters for `GET /pins`.
///
/// All fields are optional except `limit`. `name` + `name_match` together
/// drive name-based filtering — `name_match` accepts the Pinning Service
/// strategies `exact`, `iexact`, `partial`, `ipartial`. `before` is the
/// pagination cursor: results are ordered newest-first, so passing the
/// oldest `created` timestamp seen so far fetches the next page.
#[derive(Debug, Clone)]
pub struct PinListQuery {
    pub status: Option<String>,
    pub name: Option<String>,
    pub name_match: Option<String>,
    pub meta: Option<BTreeMap<String, String>>,
    pub before: Option<String>,
    pub limit: u32,
}

impl Default for PinListQuery {
    fn default() -> Self {
        Self {
            status: None,
            name: None,
            name_match: None,
            meta: None,
            before: None,
            limit: 10,
        }
    }
}

/// Request body for POST /pins.
///
/// Follows the IPFS Pinning Service API spec:
/// <https://ipfs.github.io/pinning-services-api-spec/>
#[derive(Debug, Serialize)]
pub struct PinByCidRequest {
    pub cid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<BTreeMap<String, String>>,
}

/// Response from POST /pins.
///
/// `status` is one of `queued`, `pinning`, `pinned`, or `failed`.
#[derive(Debug, Clone, Deserialize)]
pub struct PinByCidResponse {
    pub requestid: String,
    pub status: String,
    pub created: String,
    pub pin: PinDetails,
    #[serde(default)]
    pub delegates: Vec<String>,
    #[serde(default)]
    pub info: Option<serde_json::Value>,
}

/// Pin details echoed back in the response.
#[derive(Debug, Clone, Deserialize)]
pub struct PinDetails {
    pub cid: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub origins: Option<Vec<String>>,
    #[serde(default)]
    pub meta: Option<BTreeMap<String, String>>,
}

/// Response from `GET /pins`.
///
/// `count` is the total number of pins matching the query filters — not
/// the number of entries in `results`, which is capped by `limit`.
#[derive(Debug, Clone, Deserialize)]
pub struct PinListResponse {
    pub count: u32,
    #[serde(default)]
    pub results: Vec<PinStatusEntry>,
}

/// One entry in a `GET /pins` response.
#[derive(Debug, Clone, Deserialize)]
pub struct PinStatusEntry {
    pub requestid: String,
    pub status: String,
    pub created: String,
    pub pin: PinDetails,
}
