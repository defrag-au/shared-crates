use crate::TxClassifierError;
use pallas_addresses::Address;
use tracing::{info, warn};
use worker::Env;

// Re-export submodules
pub mod koios;
pub mod maestro;
pub mod webhook_blockfrost;
pub mod webhook_oura;

// Re-export public types from transactions crate
pub use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

// Re-export webhook types
pub use webhook_blockfrost::*;
pub use webhook_oura::*;

/// Which indexer serves transaction detail. Selected at runtime (env
/// `CLASSIFIER_INDEXER`) so Koios can be A/B'd against Maestro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexerProvider {
    Maestro,
    Koios,
}

/// Blockchain indexer pool for transaction data (Maestro + Koios).
pub struct IndexerPool {
    maestro: maestro::MaestroApi,
    koios: ::koios::KoiosApi,
    provider: IndexerProvider,
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
    /// Provider selection is network-aware:
    /// - **mainnet** → the `CLASSIFIER_INDEXER` env switch (`koios` → Koios,
    ///   else Maestro default), so Koios can be A/B'd against Maestro.
    /// - **preprod/testnet/preview** → always **Koios** — Maestro's testnet path
    ///   is gated (402) and preprod is served by mitos-native infra + Koios
    ///   (the same `KOIOS_API_KEY` works across all environments).
    ///
    /// The Koios client targets the network-appropriate host automatically.
    pub async fn from_env(env: &Env, network: &str) -> Result<Self, TxClassifierError> {
        let koios = ::koios::KoiosApi::for_env_with_network(env, network)
            .await
            .map_err(|e| {
                TxClassifierError::ClassificationFailed(format!(
                    "Failed to initialize Koios indexer: {e:?}"
                ))
            })?;

        let provider = if network == "cardano:mainnet" {
            match env.var("CLASSIFIER_INDEXER").ok().map(|v| v.to_string()) {
                Some(v) if v.eq_ignore_ascii_case("koios") => IndexerProvider::Koios,
                _ => IndexerProvider::Maestro,
            }
        } else {
            IndexerProvider::Koios
        };

        // Maestro is only reachable when it's the active provider. Off-mainnet
        // (or when its testnet key is absent) init can fail — that's fatal only
        // if Maestro is actually selected; otherwise keep a keyless placeholder
        // that is never called.
        let maestro = match maestro::MaestroApi::for_env_with_network(env, network).await {
            Ok(m) => m,
            Err(e) if provider == IndexerProvider::Maestro => {
                return Err(TxClassifierError::ClassificationFailed(format!(
                    "Failed to initialize Maestro indexer: {e:?}"
                )));
            }
            Err(e) => {
                warn!("Maestro indexer unavailable ({e:?}); using {provider:?}");
                maestro::MaestroApi::new(String::new(), "mainnet.gomaestro-api.org/v1".to_string())
            }
        };

        info!("✅ Indexer pool initialized (provider: {provider:?}, network: {network})");

        Ok(Self {
            maestro,
            koios,
            provider,
        })
    }

    /// Create indexer pool with a Maestro API (Koios defaults to the keyless
    /// free tier; provider defaults to Maestro).
    pub fn new(maestro: maestro::MaestroApi) -> Self {
        Self {
            maestro,
            koios: ::koios::KoiosApi::default(),
            provider: IndexerProvider::Maestro,
        }
    }

    /// Create a mock indexer pool for testing (requires maestro feature)
    #[cfg(test)]
    pub fn new_mock() -> Self {
        // Create a mock Maestro API with empty API key for testing
        let maestro = maestro::MaestroApi::new("".to_string(), "".to_string());

        Self {
            maestro,
            koios: ::koios::KoiosApi::default(),
            provider: IndexerProvider::Maestro,
        }
    }

    /// The active transaction-detail provider.
    pub fn provider(&self) -> IndexerProvider {
        self.provider
    }

    /// Get transaction data from the active provider (Maestro or Koios).
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<RawTxData, TxClassifierError> {
        info!(
            "Fetching transaction data for {tx_hash} via {:?}",
            self.provider
        );

        let result = match self.provider {
            IndexerProvider::Maestro => {
                maestro::get_tx_from_maestro(&self.maestro, tx_hash).await
            }
            IndexerProvider::Koios => koios::get_tx_from_koios(&self.koios, tx_hash).await,
        };

        match result {
            Ok(raw_tx_data) => {
                info!("✅ {:?} fetched transaction {tx_hash}", self.provider);
                Ok(raw_tx_data)
            }
            Err(e) => {
                warn!("❌ {:?} failed for {tx_hash}: {e:?}", self.provider);
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
