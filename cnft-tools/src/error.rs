use http_client::HttpError;
use std::{error::Error, fmt};

#[cfg(feature = "worker")]
use worker_stack::worker;

#[derive(Debug)]
pub enum CnftError {
    Unknown,
    UntrackedPolicy(String),
    Request(HttpError),
}

impl Default for CnftError {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Error for CnftError {}

impl fmt::Display for CnftError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown CNFT api error"),
            Self::UntrackedPolicy(policy_id) => write!(f, "Untracked policy: {policy_id}"),
            Self::Request(err) => write!(f, "CNFT tools request error: {err:?}"),
        }
    }
}

impl From<HttpError> for CnftError {
    fn from(err: HttpError) -> Self {
        Self::Request(err)
    }
}

#[cfg(feature = "worker")]
impl From<CnftError> for worker::Error {
    fn from(_: CnftError) -> Self {
        Self::BadEncoding
    }
}
