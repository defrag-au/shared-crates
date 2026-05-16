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
