use http_client::{HttpClient, HttpError, ResponseDetails};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "api.openai.com/v1";

pub struct Api {
    client: HttpClient,
    base_url: String,
}

impl Api {
    pub fn for_api_key(api_key: &str) -> Self {
        Self {
            client: HttpClient::new().with_header("Authorization", &format!("Bearer {api_key}")),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        let url = base_url
            .trim_end_matches('/')
            .strip_prefix("https://")
            .or_else(|| base_url.trim_end_matches('/').strip_prefix("http://"))
            .unwrap_or(base_url.trim_end_matches('/'));
        self.base_url = url.to_string();
        self
    }

    #[allow(dead_code)]
    pub async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, HttpError> {
        let url = format!("https://{}{path}", self.base_url);
        self.client.post(&url, body).await
    }

    pub async fn post_with_details<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<ResponseDetails<T>, HttpError> {
        let url = format!("https://{}{path}", self.base_url);
        self.client.post_with_details(&url, body).await
    }
}
