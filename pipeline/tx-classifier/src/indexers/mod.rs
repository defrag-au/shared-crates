use crate::TxClassifierError;
use pallas_addresses::Address;
use tracing::{info, warn};
use worker::Env;

// Re-export submodules
mod datums;
pub mod koios;
pub mod webhook_blockfrost;
pub mod webhook_oura;

// Re-export public types from transactions crate
pub use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

// Re-export webhook types
pub use webhook_blockfrost::*;
pub use webhook_oura::*;

/// Blockchain indexer pool for transaction data.
///
/// Koios is the only provider. Maestro, which this used to default to on
/// mainnet (behind a `CLASSIFIER_INDEXER` switch), has shut down.
pub struct IndexerPool {
    koios: ::koios::KoiosApi,
}

/// Returns true if the given Shelley-style address has a *script* payment credential.
pub fn is_script_address(addr: &str) -> bool {
    try_is_script_address(addr).unwrap_or_default()
}

fn try_is_script_address(addr: &str) -> Result<bool, Box<dyn std::error::Error>> {
    // Parse the address using Pallas
    let address =
        Address::from_bech32(addr).map_err(|e| format!("Failed to parse address: {e}"))?;

    // Convert to bytes to inspect the header
    let bytes = address.to_vec();
    let header = bytes.first().ok_or("invalid address: payload empty")?;
    let addr_type = header >> 4;

    // Script-payment types per CIP-19: 1 = base script, 3 = base script+script,
    //    5 = pointer script, 7 = enterprise script
    Ok(matches!(addr_type, 1 | 3 | 5 | 7))
}

impl IndexerPool {
    /// Create indexer pool from environment.
    ///
    /// The Koios client targets the network-appropriate host and reads the
    /// `KOIOS_API_KEY` bearer token (the same key works across all
    /// environments).
    pub async fn from_env(env: &Env, network: &str) -> Result<Self, TxClassifierError> {
        let koios = ::koios::KoiosApi::for_env_with_network(env, network)
            .await
            .map_err(|e| {
                TxClassifierError::ClassificationFailed(format!(
                    "Failed to initialize Koios indexer: {e:?}"
                ))
            })?;

        info!("✅ Indexer pool initialized (provider: Koios, network: {network})");

        Ok(Self { koios })
    }

    /// Create indexer pool with a ready-made Koios client.
    pub fn new(koios: ::koios::KoiosApi) -> Self {
        Self { koios }
    }

    /// Get transaction data from Koios.
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<RawTxData, TxClassifierError> {
        info!("Fetching transaction data for {tx_hash} via Koios");

        match koios::get_tx_from_koios(&self.koios, tx_hash).await {
            Ok(raw_tx_data) => {
                info!("✅ Koios fetched transaction {tx_hash}");
                Ok(raw_tx_data)
            }
            Err(e) => {
                warn!("❌ Koios failed for {tx_hash}: {e:?}");
                Err(TxClassifierError::TransactionNotFound(tx_hash.to_string()))
            }
        }
    }
}
