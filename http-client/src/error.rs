use std::fmt;

#[derive(Debug)]
pub enum HttpError {
    #[cfg(not(target_arch = "wasm32"))]
    Reqwest(reqwest::Error),
    #[cfg(target_arch = "wasm32")]
    Gloo(gloo_net::Error),
    Serialization(serde_json::Error),
    Custom(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            HttpError::Reqwest(e) => write!(f, "HTTP request error: {}", e),
            #[cfg(target_arch = "wasm32")]
            HttpError::Gloo(e) => write!(f, "HTTP request error: {}", e),
            HttpError::Serialization(e) => write!(f, "JSON serialization error: {}", e),
            HttpError::Custom(e) => write!(f, "Custom HTTP error: {}", e),
        }
    }
}

impl std::error::Error for HttpError {}

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