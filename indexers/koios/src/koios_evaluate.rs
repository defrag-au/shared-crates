//! Koios Ogmios `evaluateTransaction` support — script execution-unit evaluation
//! for the write path.
//!
//! Dolos/mitos can *submit* transactions but do not run phase-2, so calculating
//! script execution costs still needs an external evaluator. Koios exposes an
//! Ogmios v6 JSON-RPC passthrough at `POST /ogmios`; `evaluateTransaction`
//! returns the real per-redeemer memory/CPU budget for an unsigned tx.
//!
//! With the `evaluator` feature enabled this also provides
//! `impl cardano_tx::evaluate::TxEvaluator for KoiosApi`, so
//! `UnsignedTxBuilder::build_evaluated(&koios)` sources ExUnits from Koios.

use serde::{Deserialize, Serialize};

use crate::{KoiosApi, KoiosError, BASE_URL};

/// Real execution-unit budget for one redeemer, mapped from Ogmios onto the
/// Maestro-compatible tag/index shape a tx builder patches against.
#[derive(Debug, Clone)]
pub struct KoiosRedeemerBudget {
    /// Redeemer purpose: `"spend"`, `"mint"`, `"withdraw"`, `"publish"`,
    /// `"vote"`, or `"propose"`.
    pub redeemer_tag: String,
    /// Index within the purpose group.
    pub redeemer_index: u64,
    /// Memory units.
    pub mem: u64,
    /// CPU / step units.
    pub steps: u64,
}

#[derive(Serialize, Debug)]
struct OgmiosRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: OgmiosEvalParams<'a>,
}

#[derive(Serialize, Debug)]
struct OgmiosEvalParams<'a> {
    transaction: OgmiosTx<'a>,
}

#[derive(Serialize, Debug)]
struct OgmiosTx<'a> {
    cbor: &'a str,
}

#[derive(Deserialize, Debug)]
struct OgmiosEvalResponse {
    #[serde(default)]
    result: Option<Vec<OgmiosBudgetEntry>>,
    #[serde(default)]
    error: Option<OgmiosError>,
}

#[derive(Deserialize, Debug)]
struct OgmiosBudgetEntry {
    validator: OgmiosValidator,
    budget: OgmiosBudget,
}

#[derive(Deserialize, Debug)]
struct OgmiosValidator {
    purpose: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    index: u64,
}

#[derive(Deserialize, Debug)]
struct OgmiosBudget {
    #[serde(with = "wasm_safe_serde::u64_required")]
    memory: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    cpu: u64,
}

#[derive(Deserialize, Debug)]
struct OgmiosError {
    code: i64,
    message: String,
}

impl KoiosApi {
    /// Evaluate the scripts in an unsigned transaction and return the real
    /// per-redeemer execution-unit budget, via Koios's Ogmios v6
    /// `evaluateTransaction` passthrough (`POST /ogmios`).
    ///
    /// A JSON-RPC `error` in the response (e.g. a script that fails phase-2, or
    /// malformed CBOR) surfaces as [`KoiosError::KoiosResponse`].
    pub async fn evaluate_transaction(
        &self,
        tx_cbor_hex: &str,
    ) -> Result<Vec<KoiosRedeemerBudget>, KoiosError> {
        let url = format!("{BASE_URL}/ogmios");
        let request = OgmiosRequest {
            jsonrpc: "2.0",
            method: "evaluateTransaction",
            params: OgmiosEvalParams {
                transaction: OgmiosTx { cbor: tx_cbor_hex },
            },
        };

        let response: OgmiosEvalResponse = self.post_json(&url, &request).await?;

        if let Some(err) = response.error {
            return Err(KoiosError::KoiosResponse {
                status: 400,
                body: format!(
                    "ogmios evaluateTransaction error {}: {}",
                    err.code, err.message
                ),
            });
        }

        Ok(response
            .result
            .unwrap_or_default()
            .into_iter()
            .map(|entry| KoiosRedeemerBudget {
                redeemer_tag: entry.validator.purpose,
                redeemer_index: entry.validator.index,
                mem: entry.budget.memory,
                steps: entry.budget.cpu,
            })
            .collect())
    }
}

#[cfg(feature = "evaluator")]
mod evaluator_impl {
    use super::KoiosApi;
    use async_trait::async_trait;
    use cardano_tx::evaluate::{EvalError, EvalExUnits, RedeemerEvaluation, TxEvaluator};

    #[async_trait(?Send)]
    impl TxEvaluator for KoiosApi {
        fn name(&self) -> &str {
            "koios"
        }

        async fn evaluate(&self, tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError> {
            let budgets = self.evaluate_transaction(tx_cbor_hex).await.map_err(|e| {
                // A 4xx / JSON-RPC error means the tx itself won't evaluate
                // (invalid everywhere → stop); a 5xx or transport error means the
                // provider is unreachable (a fallback may try the next evaluator).
                match &e {
                    crate::KoiosError::KoiosResponse { status, .. } if *status < 500 => {
                        EvalError::Failed(e.to_string())
                    }
                    _ => EvalError::Unavailable(e.to_string()),
                }
            })?;

            Ok(budgets
                .into_iter()
                .map(|b| RedeemerEvaluation {
                    redeemer_tag: b.redeemer_tag,
                    redeemer_index: b.redeemer_index,
                    ex_units: EvalExUnits {
                        mem: b.mem,
                        steps: b.steps,
                    },
                })
                .collect())
        }
    }
}
