//! Koios `GET /tip` — the chain head.
//!
//! This is where "what epoch is it now" lands now that Maestro's
//! `/epochs/current` is gone: `epoch_no` is on the tip, so reading it needs no
//! `/epoch_info` round-trip.
//!
//! Koios answers the endpoint with a one-row array, and the row carries both
//! `block_height` and a duplicate `block_no` plus an `era` label. Only the
//! fields below are modelled; serde ignores the rest.

use serde::{Deserialize, Serialize};

use crate::{KoiosApi, KoiosError};

/// The chain tip (`GET /tip`). Field names match Koios's wire shape.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KoiosTip {
    pub hash: String,
    pub epoch_no: u32,
    pub abs_slot: u64,
    pub epoch_slot: u32,
    pub block_height: u64,
    pub block_time: u64,
}

impl KoiosApi {
    /// Current chain tip (`GET /tip`).
    ///
    /// An empty array is an error rather than a `None`: a live network always
    /// has a tip, so no rows means the indexer has not caught up.
    pub async fn get_tip(&self) -> Result<KoiosTip, KoiosError> {
        let url = format!("{}/tip", self.base_url);
        let rows: Vec<KoiosTip> = self.get_json(&url).await?;

        rows.into_iter()
            .next()
            .ok_or_else(|| KoiosError::KoiosResponse {
                status: 500,
                body: "GET /tip returned no rows".to_string(),
            })
    }
}
