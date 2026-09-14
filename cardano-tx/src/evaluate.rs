//! Provider-agnostic script evaluation — a [`TxEvaluator`] trait that turns an
//! unsigned transaction's CBOR into the real per-redeemer execution-unit budget.
//!
//! This is the read-side sibling of [`crate::submit::SubmitProvider`]: it lets a
//! tx builder ([`crate::builder::UnsignedTxBuilder::build_evaluated`]) source
//! ExUnits from whichever backend is available — Maestro's `/transactions/evaluate`,
//! Koios's Ogmios `evaluateTransaction`, or (eventually) a local Plutus VM — without
//! the builder knowing which. Dolos/mitos submit but do not run phase-2, so
//! evaluation is the one write-path capability that still needs an external (or
//! embedded-VM) provider.

use async_trait::async_trait;

/// Execution-unit budget for one redeemer: `mem` = memory units, `steps` = CPU
/// units. Field names mirror the ledger's `ExUnits` so builders can copy across
/// verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalExUnits {
    pub mem: u64,
    pub steps: u64,
}

/// The real execution cost of a single redeemer, tagged by its purpose
/// (`"spend"`, `"mint"`, `"withdraw"`, …) and its index within that purpose
/// group — the coordinates a builder uses to patch the estimate it built with.
#[derive(Debug, Clone)]
pub struct RedeemerEvaluation {
    pub redeemer_tag: String,
    pub redeemer_index: u64,
    pub ex_units: EvalExUnits,
}

/// Why evaluation didn't return usable budgets — mirrors [`crate::submit::SubmitError`]
/// so an ordered evaluate-fallback can make the same "try the next provider vs.
/// stop" decision.
#[derive(Debug, Clone)]
pub enum EvalError {
    /// The provider couldn't be reached or is degraded (connect failure, timeout,
    /// 5xx). Evaluation AT THIS PROVIDER is unknown, so a fallback may try the next.
    Unavailable(String),
    /// The transaction itself couldn't be evaluated (malformed CBOR, a script that
    /// fails phase-2, unresolved inputs). Invalid everywhere → a fallback STOPS.
    Failed(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unavailable(m) => write!(f, "evaluator unavailable: {m}"),
            EvalError::Failed(m) => write!(f, "evaluation failed: {m}"),
        }
    }
}

impl std::error::Error for EvalError {}

impl EvalError {
    /// True if a fallback should try the NEXT evaluator (downtime), false if it
    /// should stop (the tx is unevaluatable everywhere).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, EvalError::Unavailable(_))
    }
}

/// A UTxO the ledger does not know about yet.
///
/// The output of an earlier transaction in a CHAINED plan — built and about to
/// be signed, but not submitted, so no indexer can resolve it. An evaluator
/// needs it to run phase-2 on the transaction that spends it.
///
/// Provider-neutral on purpose: Ogmios takes this as JSON `additionalUtxo`,
/// Maestro wants a CBOR-encoded output, and neither shape belongs in a
/// builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUtxo {
    pub tx_hash: String,
    pub index: u32,
    /// Bech32.
    pub address: String,
    pub lovelace: u64,
    /// `(policy_hex, asset_name_hex, quantity)`.
    pub assets: Vec<(String, String, u64)>,
}

/// A script execution-cost evaluator — one route from unsigned-tx CBOR to real
/// per-redeemer ExUnits. Implementors own their transport and MUST classify
/// "I couldn't reach the evaluator" as [`EvalError::Unavailable`] and "the tx
/// won't evaluate" as [`EvalError::Failed`]. Object-safe (`?Send`, wasm-safe) to
/// match [`crate::submit::SubmitProvider`].
#[async_trait(?Send)]
pub trait TxEvaluator {
    /// Stable identifier for logs / metrics (e.g. `"maestro"`, `"koios"`).
    fn name(&self) -> &str;

    /// Evaluate the scripts in `tx_cbor_hex` (an unsigned tx built with estimated
    /// ExUnits) and return the real budget for every redeemer.
    async fn evaluate(&self, tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError>;

    /// Evaluate a transaction that spends UTxOs which do not exist on chain
    /// yet — the outputs of an earlier, unsubmitted transaction in the same
    /// chained plan.
    ///
    /// The default REFUSES rather than quietly dropping `pending`. An
    /// evaluator that ignored it would report an unresolved input and blame
    /// the transaction, when the real answer is "this provider cannot
    /// evaluate a chain".
    async fn evaluate_pending(
        &self,
        tx_cbor_hex: &str,
        pending: &[PendingUtxo],
    ) -> Result<Vec<RedeemerEvaluation>, EvalError> {
        if pending.is_empty() {
            return self.evaluate(tx_cbor_hex).await;
        }
        Err(EvalError::Failed(format!(
            "{} cannot evaluate a chained transaction: it has no way to be told \
             about the {} UTxO(s) an earlier, unsubmitted transaction will create",
            self.name(),
            pending.len()
        )))
    }
}

/// Maestro's evaluator. Kept working for the unchained path; deliberately NOT
/// taught to evaluate a chain.
///
/// Maestro's `AdditionalUtxo` takes a CBOR-encoded output rather than Ogmios's
/// JSON, so supporting it means a second encoding — and the Maestro API is
/// retired on 2026-09-18, with every write path already on Koios. The
/// inherited default refuses with a message that says so, which is a better
/// outcome than a mapping nobody will maintain.
#[async_trait(?Send)]
impl TxEvaluator for maestro::MaestroApi {
    fn name(&self) -> &str {
        "maestro"
    }

    async fn evaluate(&self, tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError> {
        let results = self
            .evaluate_transaction(tx_cbor_hex, None::<&[maestro::AdditionalUtxo]>)
            .await
            .map_err(|e| EvalError::Failed(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| RedeemerEvaluation {
                redeemer_tag: r.redeemer_tag,
                redeemer_index: r.redeemer_index,
                ex_units: EvalExUnits {
                    mem: r.ex_units.mem,
                    steps: r.ex_units.steps,
                },
            })
            .collect())
    }
}
