use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "wasm")]
mod wasm;

pub mod types;

#[cfg(feature = "native")]
pub use native::*;
#[cfg(feature = "wasm")]
pub use wasm::*;

pub use types::*;

#[derive(Error, Debug)]
pub enum DiscordError {
    #[error("Request failed: {0}")]
    Request(String),
    
    #[error("Serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Rate limited: retry after {retry_after:.2}s (global: {global})")]
    RateLimited { retry_after: f64, global: bool },
    
    #[error("Invalid attachment: {0}")]
    InvalidAttachment(String),
    
    #[error("Configuration error: {0}")]
    Config(String),

    #[cfg(feature = "native")]
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[cfg(feature = "wasm")]
    #[error("Gloo error: {0}")]
    Gloo(String),

    #[cfg(feature = "wasm")]
    #[error("Worker error: {0}")]
    Worker(#[from] worker::Error),
}