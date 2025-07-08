use std::{error::Error, fmt};

#[derive(Debug)]
pub enum CnftError {
    Unknown,
    UntrackedPolicy(String),
    Request(reqwest::Error),
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
            Self::UntrackedPolicy(policy_id) => write!(f, "Untracked policy: {}", policy_id),
            Self::Request(err) => write!(f, "CNFT tools request error: {:?}", err),
        }
    }
}

impl From<reqwest::Error> for CnftError {
    fn from(err: reqwest::Error) -> Self {
        Self::Request(err)
    }
}

#[cfg(feature = "worker")]
impl From<CnftError> for worker::Error {
    fn from(_: CnftError) -> Self {
        Self::BadEncoding
    }
}
