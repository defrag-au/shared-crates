mod client;
mod error;
mod types;

#[cfg(test)]
mod test;

pub use client::AnvilClient;
pub use error::AnvilError;
pub use types::*;

// Re-export Stream trait for convenience
pub use futures::Stream;

pub type Result<T> = std::result::Result<T, AnvilError>;
