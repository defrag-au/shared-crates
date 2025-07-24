use std::{error::Error, fmt};
use http_client::HttpError;

#[derive(Debug)]
pub enum PinataError {
    Unknown,
    Request(HttpError),
}

impl Default for PinataError {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Error for PinataError {}

impl fmt::Display for PinataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown Pinata api error"),
            Self::Request(err) => write!(f, "Pinata request fail: {:?}", err),
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
        Self::RustError(err.display())
    }
}
