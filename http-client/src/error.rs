use std::{collections::HashMap, fmt};

#[derive(Debug)]
pub enum HttpError {
    #[cfg(not(target_arch = "wasm32"))]
    Reqwest(reqwest::Error),
    #[cfg(target_arch = "wasm32")]
    Gloo(gloo_net::Error),
    Serialization(serde_json::Error),
    Custom(String),
    /// HTTP error with response details (status code, headers, body)
    /// Useful for handling rate limits and other non-2xx responses
    HttpStatus {
        status_code: u16,
        headers: HashMap<String, String>,
        body: String,
    },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            HttpError::Reqwest(e) => write!(f, "HTTP request error: {e}"),
            #[cfg(target_arch = "wasm32")]
            HttpError::Gloo(e) => write!(f, "HTTP request error: {e}"),
            HttpError::Serialization(e) => write!(f, "JSON serialization error: {e}"),
            HttpError::Custom(e) => write!(f, "Custom HTTP error: {e}"),
            HttpError::HttpStatus { status_code, .. } => {
                write!(f, "HTTP request failed with status: {status_code}")
            }
        }
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    /// Extract retry-after header value (in seconds) if this is an HttpStatus error
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            HttpError::HttpStatus { headers, .. } => headers
                .get("retry-after")
                .and_then(|v| v.parse::<u64>().ok()),
            _ => None,
        }
    }

    /// Get status code if this is an HttpStatus error
    pub fn status_code(&self) -> Option<u16> {
        match self {
            HttpError::HttpStatus { status_code, .. } => Some(*status_code),
            _ => None,
        }
    }

    /// Get all headers if this is an HttpStatus error
    pub fn headers(&self) -> Option<&HashMap<String, String>> {
        match self {
            HttpError::HttpStatus { headers, .. } => Some(headers),
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<reqwest::Error> for HttpError {
    fn from(e: reqwest::Error) -> Self {
        HttpError::Reqwest(e)
    }
}

#[cfg(target_arch = "wasm32")]
impl From<gloo_net::Error> for HttpError {
    fn from(e: gloo_net::Error) -> Self {
        HttpError::Gloo(e)
    }
}

impl From<serde_json::Error> for HttpError {
    fn from(e: serde_json::Error) -> Self {
        HttpError::Serialization(e)
    }
}
