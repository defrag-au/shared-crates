use http_client::HttpError;
use std::{error::Error, fmt};

#[cfg(feature = "worker")]
use worker_stack::worker;

#[derive(Debug, Default)]
pub enum PinataError {
    #[default]
    Unknown,
    Request(HttpError),
}

impl Error for PinataError {}

impl fmt::Display for PinataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown Pinata api error"),
            Self::Request(err) => write!(f, "Pinata request fail: {err:?}"),
        }
    }
}

impl From<HttpError> for PinataError {
    fn from(err: HttpError) -> Self {
        Self::Request(err)
    }
}

#[cfg(feature = "worker")]
impl From<PinataError> for worker::Error {
    fn from(err: PinataError) -> Self {
        Self::RustError(err.to_string())
    }
}
