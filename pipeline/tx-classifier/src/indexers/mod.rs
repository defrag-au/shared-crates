use crate::TxClassifierError;
use pallas_addresses::Address;
use shared_types::ChainNetwork;
use tracing::{info, warn};
use worker::Env;

// Re-export submodules
pub mod maestro;
pub mod webhook_blockfrost;
pub mod webhook_oura;

// Re-export public types from transactions crate
pub use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

// Re-export webhook types
pub use webhook_blockfrost::*;
pub use webhook_oura::*;

/// Maestro blockchain indexer for transaction data
pub struct IndexerPool {
    maestro: maestro::MaestroApi,
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
    /// Create indexer pool from environment
    pub async fn from_env(env: &Env, network: &ChainNetwork) -> Result<Self, TxClassifierError> {
        let maestro = maestro::MaestroApi::for_env_with_network(env, &network.as_str())
            .await
            .map_err(|e| {
                TxClassifierError::ClassificationFailed(format!(
                    "Failed to initialize Maestro indexer: {e:?}"
                ))
            })?;

        info!("✅ Maestro indexer initialized");

        Ok(Self { maestro })
    }

    /// Create indexer pool with Maestro API
    pub fn new(maestro: maestro::MaestroApi) -> Self {
        Self { maestro }
    }

    /// Create a mock indexer pool for testing (requires maestro feature)
    #[cfg(test)]
    pub fn new_mock() -> Self {
        // Create a mock Maestro API with empty API key for testing
        let maestro = maestro::MaestroApi::new("".to_string(), "".to_string());

        Self { maestro }
    }

    /// Get transaction data from Maestro
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<RawTxData, TxClassifierError> {
        info!("Fetching transaction data for: {}", tx_hash);

        match maestro::get_tx_from_maestro(&self.maestro, tx_hash).await {
            Ok(raw_tx_data) => {
                info!("✅ Maestro successfully fetched transaction {}", tx_hash);
                Ok(raw_tx_data)
            }
            Err(e) => {
                warn!("❌ Maestro failed for {}: {:?}", tx_hash, e);
                Err(TxClassifierError::TransactionNotFound(tx_hash.to_string()))
            }
        }
    }

    /// Convert complete transaction to raw data format
    pub fn convert_complete_transaction_to_raw_data(
        &self,
        complete_tx: &::maestro::CompleteTransactionDetails,
    ) -> Result<RawTxData, TxClassifierError> {
        maestro::convert_complete_transaction_to_raw_data(complete_tx)
    }
}
