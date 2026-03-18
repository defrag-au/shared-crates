use cardano_assets::utxo::{AssetQuantity, UtxoTag};
use serde::{Deserialize, Serialize};

/// A complete optimization plan: one or more transaction steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationPlan {
    /// Ordered sequence of transactions to execute.
    pub steps: Vec<OptimizationStep>,
    /// Summary statistics.
    pub summary: PlanSummary,
}

/// Summary of the entire optimization plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSummary {
    /// Number of UTxOs before optimization.
    pub utxos_before: usize,
    /// Number of UTxOs after all steps complete.
    pub utxos_after: usize,
    /// Total estimated fees across all steps (lovelace).
    pub total_fees: u64,
    /// Number of transaction steps required.
    pub num_steps: usize,
    /// Estimated ADA freed by reducing locked min-UTxO (lovelace).
    pub ada_freed: u64,
}

/// A single transaction step in the optimization plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationStep {
    /// Which step this is (0-indexed).
    pub step_index: usize,
    /// Input UTxO references consumed by this step.
    pub inputs: Vec<InputRef>,
    /// Ideal outputs produced by this step.
    pub outputs: Vec<PlannedOutput>,
    /// Estimated transaction size in bytes.
    pub estimated_size: u64,
    /// Estimated fee in lovelace.
    pub estimated_fee: u64,
    /// The UTxO set state AFTER this step executes.
    /// (Remaining untouched UTxOs + new outputs from this step.)
    pub resulting_utxos: Vec<UtxoSnapshot>,
}

/// Reference to an input UTxO consumed by a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRef {
    /// Full UTxO reference "tx_hash#index".
    pub utxo_ref: String,
    /// Index into the original wallet UTxO list (for the current step's input set).
    pub original_index: usize,
}

/// A planned output to be created by a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedOutput {
    /// Unique ID for animation tracking (e.g., "step0_out3").
    pub output_id: String,
    /// Lovelace in this output.
    pub lovelace: u64,
    /// Assets in this output.
    pub assets: Vec<AssetQuantity>,
    /// Which input UTxO refs contributed tokens to this output.
    /// Used for merge/split animation provenance.
    pub source_utxo_refs: Vec<String>,
    /// Classification tag for the output type.
    pub output_kind: OutputKind,
}

/// What kind of output this is (for animation and display).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    /// Consolidated same-policy tokens.
    PolicyBundle,
    /// Isolated fungible token.
    FungibleIsolate,
    /// Isolated non-fungible token.
    NonfungibleIsolate,
    /// Rolled-up ADA.
    AdaRollup,
    /// Split ADA portion (50/15/10/10/5/5/5%).
    AdaSplit,
    /// Backfill output (tokens that came along with selected inputs).
    Backfill,
    /// Change output with remaining ADA.
    Change,
}

/// Snapshot of a UTxO at a point in time (for before/after display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoSnapshot {
    /// UTxO reference "tx_hash#index".
    pub utxo_ref: String,
    /// Total lovelace.
    pub lovelace: u64,
    /// Assets carried.
    pub assets: Vec<AssetQuantity>,
    /// Tags from the original UTxO.
    pub tags: Vec<UtxoTag>,
    /// If this UTxO was produced by a step, which step.
    pub produced_by_step: Option<usize>,
    /// If this UTxO will be consumed by a later step, which step.
    pub consumed_by_step: Option<usize>,
}
