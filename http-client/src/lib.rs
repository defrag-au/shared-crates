use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use tracing::debug;

mod error;
pub use error::*;

#[derive(Debug)]
pub struct ResponseDetails {
    pub status_code: u16,
    pub body: String,
    pub headers: std::collections::HashMap<String, String>,
}

/// Response with both parsed data and metadata (headers, status)
#[derive(Debug)]
pub struct ResponseWithHeaders<T> {
    pub data: T,
    pub status_code: u16,
    pub headers: std::collections::HashMap<String, String>,
}

impl<T> ResponseWithHeaders<T> {
    /// Get a specific header value (case-insensitive)
    pub fn get_header(&self, name: &str) -> Option<&String> {
        let name_lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == name_lower)
            .map(|(_, v)| v)
    }

    /// Get the Retry-After header value (in seconds)
    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.get_header("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
    }

    /// Get rate limit remaining requests (OpenAI format)
    pub fn rate_limit_remaining_requests(&self) -> Option<u64> {
        self.get_header("x-ratelimit-remaining-requests")
            .and_then(|v| v.parse::<u64>().ok())
    }

    /// Get rate limit reset time (OpenAI format - Unix timestamp)
    pub fn rate_limit_reset_requests(&self) -> Option<u64> {
        self.get_header("x-ratelimit-reset-requests")
            .and_then(|v| v.parse::<u64>().ok())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[derive(Debug, Clone)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl HttpMethod {
    #[cfg(not(target_arch = "wasm32"))]
    fn to_reqwest(&self) -> reqwest::Method {
        match self {
            HttpMethod::GET => reqwest::Method::GET,
            HttpMethod::POST => reqwest::Method::POST,
            HttpMethod::PUT => reqwest::Method::PUT,
            HttpMethod::DELETE => reqwest::Method::DELETE,
            HttpMethod::PATCH => reqwest::Method::PATCH,
        }
    }
}

pub struct HttpClient {
    #[cfg(not(target_arch = "wasm32"))]
    inner: reqwest::Client,
    default_headers: HashMap<String, String>,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: reqwest::Client::new(),
            default_headers: HashMap::new(),
        }
    }

    pub fn with_bearer_token(token: String) -> Self {
        let mut client = Self::new();
        client
            .default_headers
            .insert("Authorization".to_string(), format!("Bearer {token}"));
        client
    }

    pub fn with_bot_token(token: String) -> Self {
        let mut client = Self::new();
        client
            .default_headers
            .insert("Authorization".to_string(), format!("Bot {token}"));
        client
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.default_headers
            .insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_user_agent(self, user_agent: &str) -> Self {
        self.with_header("User-Agent", user_agent)
    }

    /// Generic request method that handles serialization, headers, and logging
    pub async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<R, HttpError> {
        if let Some(data) = body {
            debug!(
                "{:?} request to: {}, body: {}",
                method,
                url,
                serde_json::to_string(data).unwrap_or_else(|_| "serialization failed".to_string())
            );
        } else {
            debug!("{:?} request to: {} (no body)", method, url);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            native::make_request(&self.inner, &self.default_headers, method, url, body).await
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm::make_request(&self.default_headers, method, url, body).await
        }
    }

    /// Convenience method for GET requests
    pub async fn get<R: DeserializeOwned>(&self, url: &str) -> Result<R, HttpError> {
        self.request::<(), R>(HttpMethod::GET, url, None).await
    }

    /// Convenience method for POST requests with JSON body
    pub async fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, HttpError> {
        self.request(HttpMethod::POST, url, Some(body)).await
    }

    /// Convenience method for PUT requests with JSON body
    pub async fn put<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, HttpError> {
        self.request(HttpMethod::PUT, url, Some(body)).await
    }

    /// Convenience method for DELETE requests
    pub async fn delete<R: DeserializeOwned>(&self, url: &str) -> Result<R, HttpError> {
        self.request::<(), R>(HttpMethod::DELETE, url, None).await
    }

    /// Convenience method for PATCH requests with JSON body
    pub async fn patch<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R, HttpError> {
        self.request(HttpMethod::PATCH, url, Some(body)).await
    }

    /// Advanced request method that returns response details for custom handling (retry logic, etc.)
    pub async fn request_with_response_details<T: Serialize>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<ResponseDetails, HttpError> {
        debug!("{:?} request with response details to: {}", method, url);

        #[cfg(not(target_arch = "wasm32"))]
        {
            native::make_request_with_details(&self.inner, &self.default_headers, method, url, body)
                .await
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm::make_request_with_details(&self.default_headers, method, url, body).await
        }
    }

    /// Request that returns both parsed response data AND headers
    /// Useful for APIs that include important metadata in headers (rate limits, pagination, etc.)
    pub async fn request_with_headers<T: Serialize, R: DeserializeOwned>(
        &self,
        method: HttpMethod,
        url: &str,
        body: Option<&T>,
    ) -> Result<ResponseWithHeaders<R>, HttpError> {
        debug!("{:?} request with headers to: {}", method, url);

        #[cfg(not(target_arch = "wasm32"))]
        {
            native::make_request_with_headers(&self.inner, &self.default_headers, method, url, body)
                .await
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm::make_request_with_headers(&self.default_headers, method, url, body).await
        }
    }

    /// Convenience method for POST with headers
    pub async fn post_with_headers<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<ResponseWithHeaders<R>, HttpError> {
        self.request_with_headers(HttpMethod::POST, url, Some(body))
            .await
    }

    /// Convenience method for GET with headers
    pub async fn get_with_headers<R: DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<ResponseWithHeaders<R>, HttpError> {
        self.request_with_headers::<(), R>(HttpMethod::GET, url, None)
            .await
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
