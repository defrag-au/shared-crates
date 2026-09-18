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
#[cfg(feature = "maestro")]
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

/// Try each evaluator in order, taking the first real answer.
///
/// The ordering the [`EvalError`] split was written for: a provider that is
/// merely *unreachable* says nothing about the transaction, so the next one
/// gets a turn; a provider that says the transaction *cannot evaluate* has
/// answered, and asking again elsewhere would only produce the same refusal
/// more slowly.
///
/// The intended shape is local-first: an in-process evaluator is exact and
/// costs milliseconds, so it should answer whenever it can, with a remote
/// provider behind it for the transactions it cannot account for.
/// An evaluator that always declines, for a host that has no remote provider
/// at all — a browser, typically.
///
/// It reports [`EvalError::Unavailable`], not `Failed`: nothing is wrong with
/// the transaction, there is simply nobody here to evaluate it. That
/// distinction is load-bearing — `Failed` stops a [`FirstAvailable`] chain,
/// which would turn "this host has no remote" into "this transaction is
/// invalid".
///
/// Use this rather than `FirstAvailable::new(vec![])`, which panics by design.
/// Callers reach for the empty list because it reads as "no evaluators", but
/// the two mean opposite things: an empty chain can never answer anything,
/// whereas this answers "not me" and lets the caller fall back to its
/// conservative placeholder ex-units.
pub struct NoEvaluator;

#[async_trait(?Send)]
impl TxEvaluator for NoEvaluator {
    fn name(&self) -> &str {
        "none"
    }

    async fn evaluate(&self, _tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError> {
        Err(EvalError::Unavailable(
            "this host has no remote evaluator".into(),
        ))
    }
}

pub struct FirstAvailable<'a> {
    evaluators: Vec<&'a dyn TxEvaluator>,
}

impl<'a> FirstAvailable<'a> {
    /// Panics on an empty list: an evaluator that can never answer is a
    /// wiring mistake, and failing at construction beats failing on the first
    /// transaction a user tries to build.
    pub fn new(evaluators: Vec<&'a dyn TxEvaluator>) -> Self {
        assert!(
            !evaluators.is_empty(),
            "FirstAvailable needs at least one evaluator"
        );
        Self { evaluators }
    }
}

#[async_trait(?Send)]
impl TxEvaluator for FirstAvailable<'_> {
    fn name(&self) -> &str {
        "first-available"
    }

    async fn evaluate(&self, tx_cbor_hex: &str) -> Result<Vec<RedeemerEvaluation>, EvalError> {
        let mut last = None;
        for e in &self.evaluators {
            match e.evaluate(tx_cbor_hex).await {
                Ok(v) => return Ok(v),
                Err(err) if err.is_unavailable() => {
                    tracing::info!(evaluator = e.name(), %err, "evaluator unavailable; trying the next");
                    last = Some(err);
                }
                // A verdict, not an outage. Stop.
                Err(err) => return Err(err),
            }
        }
        Err(last.unwrap_or_else(|| EvalError::Unavailable("no evaluator answered".into())))
    }

    async fn evaluate_pending(
        &self,
        tx_cbor_hex: &str,
        pending: &[PendingUtxo],
    ) -> Result<Vec<RedeemerEvaluation>, EvalError> {
        let mut last = None;
        for e in &self.evaluators {
            match e.evaluate_pending(tx_cbor_hex, pending).await {
                Ok(v) => return Ok(v),
                Err(err) if err.is_unavailable() => {
                    tracing::info!(evaluator = e.name(), %err, "evaluator unavailable; trying the next");
                    last = Some(err);
                }
                Err(err) => return Err(err),
            }
        }
        Err(last.unwrap_or_else(|| EvalError::Unavailable("no evaluator answered".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Unavailable`, never `Failed`. A `Failed` would STOP a `FirstAvailable`
    /// chain and report a perfectly good transaction as unevaluatable, when
    /// all that happened is this host has no remote provider.
    #[tokio::test]
    async fn no_evaluator_declines_as_unavailable() {
        let err = NoEvaluator.evaluate("00").await.unwrap_err();
        assert!(
            err.is_unavailable(),
            "a host with no remote must not condemn the transaction: {err}"
        );
    }

    /// The reason `NoEvaluator` exists rather than an empty vec: it can sit in
    /// a chain and be fallen through, where `FirstAvailable::new(vec![])`
    /// panics at construction.
    #[tokio::test]
    async fn first_available_falls_through_a_declining_evaluator() {
        let none = NoEvaluator;
        let chain = FirstAvailable::new(vec![&none as &dyn TxEvaluator]);
        let err = chain.evaluate("00").await.unwrap_err();
        assert!(err.is_unavailable());
    }
}
