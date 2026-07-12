//! Koios Ogmios `queryLedgerState/protocolParameters` — current protocol
//! parameters for transaction fee/size sizing.
//!
//! Ogmios surfaces execution prices as exact ratio strings (`"577/10000"`) and
//! `minFeeConstant` as `{ada:{lovelace}}` — the same shape Maestro used, so this
//! maps cleanly. Plutus cost models are intentionally **not** surfaced: they're
//! hardcoded in `cardano-tx` (script-hash correctness lives there), and no
//! cached-config consumer reads them.

use serde::{Deserialize, Serialize};

use crate::{KoiosApi, KoiosError};

/// Protocol parameters from Ogmios `queryLedgerState/protocolParameters`
/// (fee/size fields only).
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosProtocolParams {
    #[serde(rename = "minFeeCoefficient", with = "wasm_safe_serde::u64_required")]
    pub min_fee_coefficient: u64,
    #[serde(rename = "minFeeConstant")]
    pub min_fee_constant: OgmiosAda,
    #[serde(
        rename = "minUtxoDepositCoefficient",
        with = "wasm_safe_serde::u64_required"
    )]
    pub min_utxo_deposit_coefficient: u64,
    #[serde(rename = "scriptExecutionPrices", default)]
    pub script_execution_prices: Option<OgmiosPrices>,
    #[serde(rename = "maxExecutionUnitsPerTransaction", default)]
    pub max_execution_units_per_transaction: Option<OgmiosExUnits>,
    #[serde(rename = "maxTransactionSize", default)]
    pub max_transaction_size: Option<OgmiosByteSize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OgmiosAda {
    pub ada: OgmiosLovelace,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OgmiosLovelace {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub lovelace: u64,
}

/// Execution prices as exact ratio strings, e.g. `"577/10000"`.
#[derive(Debug, Clone, Deserialize)]
pub struct OgmiosPrices {
    pub memory: String,
    pub cpu: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OgmiosExUnits {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub memory: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub cpu: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OgmiosByteSize {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub bytes: u64,
}

#[derive(Serialize, Debug)]
struct OgmiosQueryRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
}

#[derive(Deserialize, Debug)]
struct OgmiosParamsResponse {
    #[serde(default)]
    result: Option<KoiosProtocolParams>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

impl KoiosApi {
    /// Fetch current protocol parameters via Ogmios
    /// `queryLedgerState/protocolParameters` (`POST /ogmios`).
    pub async fn get_protocol_params(&self) -> Result<KoiosProtocolParams, KoiosError> {
        let url = format!("{}/ogmios", self.base_url);
        let request = OgmiosQueryRequest {
            jsonrpc: "2.0",
            method: "queryLedgerState/protocolParameters",
        };
        let response: OgmiosParamsResponse = self.post_json(&url, &request).await?;

        if let Some(err) = response.error {
            return Err(KoiosError::KoiosResponse {
                status: 400,
                body: format!("ogmios protocolParameters error: {err}"),
            });
        }
        response.result.ok_or_else(|| KoiosError::KoiosResponse {
            status: 500,
            body: "ogmios protocolParameters: empty result".to_string(),
        })
    }
}
