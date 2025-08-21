mod client;
mod error;
mod types;

#[cfg(test)]
mod test;

pub use client::AnvilClient;
pub use error::AnvilError;
pub use types::*;

pub type Result<T> = std::result::Result<T, AnvilError>;