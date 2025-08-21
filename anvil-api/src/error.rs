use std::fmt;

#[derive(Debug)]
pub enum AnvilError {
    Http(http_client::HttpError),
    Serialization(serde_json::Error),
    InvalidInput(String),
}

impl fmt::Display for AnvilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnvilError::Http(err) => write!(f, "HTTP error: {err}"),
            AnvilError::Serialization(err) => write!(f, "Serialization error: {err}"),
            AnvilError::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
        }
    }
}

impl std::error::Error for AnvilError {}

impl From<http_client::HttpError> for AnvilError {
    fn from(err: http_client::HttpError) -> Self {
        AnvilError::Http(err)
    }
}

impl From<serde_json::Error> for AnvilError {
    fn from(err: serde_json::Error) -> Self {
        AnvilError::Serialization(err)
    }
}