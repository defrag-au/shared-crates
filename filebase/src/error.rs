use http_client::HttpError;
use std::{error::Error, fmt};

#[cfg(feature = "worker")]
use worker_stack::worker;

#[derive(Debug, Default)]
pub enum FilebaseError {
    #[default]
    Unknown,
    Request(HttpError),
}

impl Error for FilebaseError {}

impl fmt::Display for FilebaseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown Filebase api error"),
            Self::Request(err) => write!(f, "Filebase request fail: {err:?}"),
        }
    }
}

impl From<HttpError> for FilebaseError {
    fn from(err: HttpError) -> Self {
        Self::Request(err)
    }
}

#[cfg(feature = "worker")]
impl From<FilebaseError> for worker::Error {
    fn from(err: FilebaseError) -> Self {
        Self::RustError(err.to_string())
    }
}
