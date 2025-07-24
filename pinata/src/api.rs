use serde::{de::DeserializeOwned, Serialize};
use http_client::{HttpClient, HttpMethod};

use crate::PinataError;

const BASE_URL: &str = "https://api.pinata.cloud/v3";

pub struct PinataApi {
    pub jwt: Option<String>,
    pub client: HttpClient,
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

    pub fn with_jwt(jwt: String) -> Self {
        Self {
            jwt: Some(jwt.clone()),
            client: HttpClient::new()
                .with_user_agent("DefragPinataClient/1.0 (+https://defrag.au)")
                .with_header("Accept", "application/json")
                .with_header("Content-Type", "application/json")
                .with_header("Authorization", &jwt),
        }
    }

    async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<R, PinataError> {
        self.client.request(method, url, body).await.map_err(PinataError::Request)
    }
}
