use http_client::HttpError;
use std::{error::Error, fmt};

#[derive(Debug, Default)]
pub enum OpenAiError {
    #[default]
    Unknown,
    ApiFailure(HttpError),
    Serde(serde_json::Error),
    RateLimitExceeded {
        retry_after_seconds: Option<u64>,
        remaining_requests: Option<u64>,
    },
    InvalidInput(String),
}

impl fmt::Display for OpenAiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenAiError::Unknown => write!(f, "An unknown OpenAI error has occurred"),
            OpenAiError::ApiFailure(err) => write!(f, "API communication failed: {err}"),
            Self::Serde(err) => write!(f, "serde error: {err}"),
            Self::RateLimitExceeded {
                retry_after_seconds,
                remaining_requests,
            } => {
                write!(f, "OpenAI rate limit exceeded")?;
                if let Some(retry_after) = retry_after_seconds {
                    write!(f, ", retry after {retry_after}s")?;
                }
                if let Some(remaining) = remaining_requests {
                    write!(f, ", {remaining} requests remaining")?;
                }
                Ok(())
            }
            Self::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
        }
    }
}

impl Error for OpenAiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            OpenAiError::ApiFailure(err) => Some(err),
            _ => None,
        }
    }
}

// Optional: convert to a user-friendly string
impl From<OpenAiError> for String {
    fn from(err: OpenAiError) -> Self {
        err.to_string()
    }
}

impl From<serde_json::Error> for OpenAiError {
    fn from(value: serde_json::Error) -> Self {
        OpenAiError::Serde(value)
    }
}
