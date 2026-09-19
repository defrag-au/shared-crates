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

use crate::{KoiosApi, KoiosError};

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
    /// UTxOs to resolve inputs against IN ADDITION to the ledger's own set.
    ///
    /// This is how a CHAINED transaction is evaluated: its inputs are the
    /// outputs of a sibling that has been built but not submitted, so no
    /// indexer can resolve them. Omitted entirely when empty — an empty array
    /// is not the same as absent to every Ogmios version.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "additionalUtxo")]
    additional_utxo: Vec<OgmiosUtxo>,
}

#[derive(Serialize, Debug)]
struct OgmiosTx<'a> {
    cbor: &'a str,
}

/// One entry of Ogmios v6's `additionalUtxo`.
#[derive(Serialize, Debug)]
struct OgmiosUtxo {
    transaction: OgmiosTxId,
    index: u32,
    address: String,
    /// `{ "ada": { "lovelace": n }, "<policy>": { "<name>": n } }` — Ogmios's
    /// nested value encoding, with ADA under its own reserved key.
    value: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Debug)]
struct OgmiosTxId {
    id: String,
}

// Only the `evaluator` feature knows what a `PendingUtxo` is; the ungated
// evaluate path sends an empty `additionalUtxo` and needs none of this.
#[cfg(feature = "evaluator")]
impl OgmiosUtxo {
    fn from_pending(pending: &cardano_tx::evaluate::PendingUtxo) -> Self {
        let mut value = serde_json::Map::new();
        let mut ada = serde_json::Map::new();
        ada.insert("lovelace".to_string(), pending.lovelace.into());
        value.insert("ada".to_string(), serde_json::Value::Object(ada));
        for (policy, name, quantity) in &pending.assets {
            let entry = value
                .entry(policy.clone())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(map) = entry.as_object_mut() {
                map.insert(name.clone(), (*quantity).into());
            }
        }
        Self {
            transaction: OgmiosTxId {
                id: pending.tx_hash.clone(),
            },
            index: pending.index,
            address: pending.address.clone(),
            value,
        }
    }
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
        self.evaluate_with_additional(tx_cbor_hex, Vec::new()).await
    }

    /// As [`Self::evaluate_transaction`], but resolving inputs against
    /// `pending` as well as the ledger — the UTxOs an earlier, unsubmitted
    /// transaction in a chained plan will create.
    #[cfg(feature = "evaluator")]
    pub async fn evaluate_transaction_with(
        &self,
        tx_cbor_hex: &str,
        pending: &[cardano_tx::evaluate::PendingUtxo],
    ) -> Result<Vec<KoiosRedeemerBudget>, KoiosError> {
        self.evaluate_with_additional(
            tx_cbor_hex,
            pending.iter().map(OgmiosUtxo::from_pending).collect(),
        )
        .await
    }

    async fn evaluate_with_additional(
        &self,
        tx_cbor_hex: &str,
        additional_utxo: Vec<OgmiosUtxo>,
    ) -> Result<Vec<KoiosRedeemerBudget>, KoiosError> {
        let url = format!("{}/ogmios", self.base_url);
        let request = OgmiosRequest {
            jsonrpc: "2.0",
            method: "evaluateTransaction",
            params: OgmiosEvalParams {
                transaction: OgmiosTx { cbor: tx_cbor_hex },
                additional_utxo,
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
mod params_impl {
    use crate::koios_params::KoiosProtocolParams;
    use cardano_tx::builder::cost_models::PlutusCostModels;
    use cardano_tx::params::TxBuildParams;

    /// Parse an Ogmios exact-ratio string (`"577/10000"`) into `(num, den)`.
    ///
    /// Ogmios states prices as exact rationals rather than floats on purpose: a
    /// fee derived from a rounded price is a fee the node disagrees with. Keep
    /// the pair and let the caller do the arithmetic.
    fn parse_ratio(raw: &str) -> Option<(u64, u64)> {
        let (num, den) = raw.split_once('/')?;
        let num = num.trim().parse().ok()?;
        let den: u64 = den.trim().parse().ok()?;
        // A zero denominator is not a price, and it would divide by zero at the
        // point of use rather than here, where it is still obvious what went
        // wrong.
        (den != 0).then_some((num, den))
    }

    /// Build tx-builder parameters straight from a Koios/Ogmios protocol
    /// parameters response.
    ///
    /// Carries the LIVE Plutus cost models through. Without them a builder falls
    /// back to bundled constants, which have gone stale across protocol updates
    /// before and take every script spend down with `PPViewHashesDontMatch` —
    /// so this conversion is the difference between a jpg buy that submits and
    /// one that is rejected outright.
    impl From<&KoiosProtocolParams> for TxBuildParams {
        fn from(pp: &KoiosProtocolParams) -> Self {
            let models = pp.plutus_cost_models.as_ref();
            Self {
                min_fee_coefficient: pp.min_fee_coefficient,
                min_fee_constant: pp.min_fee_constant.ada.lovelace,
                coins_per_utxo_byte: pp.min_utxo_deposit_coefficient,
                max_tx_size: pp
                    .max_transaction_size
                    .as_ref()
                    .map(|s| s.bytes as u32)
                    .unwrap_or(16_384),
                max_value_size: pp.max_value_size.as_ref().map(|s| s.bytes).unwrap_or(5_000),
                // Live where available; Conway mainnet otherwise. Never a
                // permissive fallback — this is the ceiling that stops an
                // over-budget sweep reaching the node, and the evaluator does
                // not check it for us.
                max_tx_ex_units: pp
                    .max_execution_units_per_transaction
                    .as_ref()
                    .map(|eu| (eu.memory, eu.cpu))
                    .unwrap_or((16_500_000, 10_000_000_000)),
                price_mem: pp
                    .script_execution_prices
                    .as_ref()
                    .and_then(|p| parse_ratio(&p.memory)),
                price_step: pp
                    .script_execution_prices
                    .as_ref()
                    .and_then(|p| parse_ratio(&p.cpu)),
                // Ogmios reports this under `minFeeReferenceScripts`, which this
                // struct does not model yet; the mainnet value is stable.
                min_fee_ref_script_cost_per_byte: 15,
                ref_script_size: 0,
                cost_models: PlutusCostModels {
                    plutus_v1: models.and_then(|m| m.plutus_v1.clone()),
                    plutus_v2: models.and_then(|m| m.plutus_v2.clone()),
                    plutus_v3: models.and_then(|m| m.plutus_v3.clone()),
                },
            }
        }
    }
}

#[cfg(feature = "evaluator")]
mod evaluator_impl {
    use super::KoiosApi;
    use async_trait::async_trait;
    use cardano_tx::evaluate::{
        EvalError, EvalExUnits, PendingUtxo, RedeemerEvaluation, TxEvaluator,
    };

    #[async_trait(?Send)]
    impl TxEvaluator for KoiosApi {
        fn name(&self) -> &str {
            "koios"
        }

        async fn evaluate(&self, tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError> {
            self.evaluate_pending(tx_cbor_hex, &[]).await
        }

        /// Ogmios resolves a chained transaction's inputs against
        /// `additionalUtxo`, so this is the evaluator a chained plan needs.
        async fn evaluate_pending(
            &self,
            tx_cbor_hex: &str,
            pending: &[PendingUtxo],
        ) -> Result<Vec<RedeemerEvaluation>, EvalError> {
            let budgets = self
                .evaluate_transaction_with(tx_cbor_hex, pending)
                .await
                .map_err(|e| {
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
