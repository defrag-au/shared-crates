use http_client::{HttpClient, HttpMethod};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::PinataError;

const BASE_URL: &str = "https://api.pinata.cloud/v3";

pub struct PinataApi {
    pub jwt: Option<String>,
    pub client: HttpClient,
}

impl Default for PinataApi {
    fn default() -> Self {
        Self::new()
    }
}

impl PinataApi {
    pub fn new() -> Self {
        Self {
            jwt: None,
            client: HttpClient::new()
                .with_user_agent("DefragPinataClient/1.0 (+https://defrag.au)")
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json"),
        }
    }

    pub fn with_jwt(jwt: impl Into<String>) -> Self {
        let jwt = jwt.into();
        let auth_header = format!("Bearer {jwt}");
        Self {
            jwt: Some(jwt),
            client: HttpClient::new()
                .with_user_agent("DefragPinataClient/1.0 (+https://defrag.au)")
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json")
                .with_header("Authorization", &auth_header),
        }
    }

    pub async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<R, PinataError> {
        self.client
            .request(method, url, body)
            .await
            .map_err(PinataError::Request)
    }

    /// Pin an existing CID to Pinata.
    ///
    /// This tells Pinata to fetch and pin content that already exists on IPFS.
    /// Useful for preserving content hosted by others.
    pub async fn pin_by_cid(
        &self,
        cid: &str,
        name: Option<&str>,
    ) -> Result<PinByCidResponse, PinataError> {
        let url = format!("{BASE_URL}/files/public/pin_by_cid");

        let request = PinByCidRequest {
            cid: cid.to_string(),
            name: name.map(|s| s.to_string()),
            group_id: None,
            keyvalues: None,
            host_nodes: None,
        };

        let response: PinByCidApiResponse =
            self.request(HttpMethod::POST, &url, Some(&request)).await?;

        Ok(response.data)
    }
}

/// Request body for pin_by_cid
#[derive(Debug, Serialize)]
pub struct PinByCidRequest {
    pub cid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyvalues: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_nodes: Option<Vec<String>>,
}

/// API wrapper response
#[derive(Debug, Deserialize)]
struct PinByCidApiResponse {
    data: PinByCidResponse,
}

/// Response from pin_by_cid
#[derive(Debug, Clone, Deserialize)]
pub struct PinByCidResponse {
    pub id: String,
    pub name: Option<String>,
    pub cid: String,
    pub status: String,
    pub date_queued: Option<String>,
    #[serde(default)]
    pub keyvalues: Option<serde_json::Value>,
    pub group_id: Option<String>,
}
