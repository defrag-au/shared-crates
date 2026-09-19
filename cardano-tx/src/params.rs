//! Protocol parameters abstraction for transaction building
//!
//! [`TxBuildParams`] captures the minimum protocol parameters needed to build
//! transactions. Consumers convert from their source-specific types (Maestro,
//! Blockfrost, Koios, etc.) into this common representation.

use crate::builder::cost_models::PlutusCostModels;
use pallas_txbuilder::ExUnits;

/// Conway's `minFeeRefScriptCoinsPerByte`, in lovelace, as it currently stands
/// on mainnet and preprod.
///
/// Stated here rather than read from the upstream parameter payload because
/// none of our providers surfaces it. That makes it a **pin, not a fact**: if
/// the protocol parameter is ever changed by governance, transactions that use
/// reference scripts start being rejected `FeeTooSmallUTxO` with no other
/// symptom, and this is the line to change.
///
/// Note that [`TxBuildParams::default()`] leaves this **zero**, because
/// `Default` derives field-by-field. A test built on `..Default::default()`
/// therefore prices reference scripts at nothing and will happily pass while
/// proving the opposite of what it claims. Set it explicitly in any test that
/// means to exercise the reference-script fee.
pub const CONWAY_MIN_FEE_REF_SCRIPT_COST_PER_BYTE: u64 = 15;

/// Conway's `maxTxSize` in bytes, as it currently stands on mainnet and
/// preprod, and the value [`TxBuildParams::default()`] applies.
///
/// A pin, like [`CONWAY_MIN_FEE_REF_SCRIPT_COST_PER_BYTE`] — if governance
/// raises the limit, this is the line to change.
///
/// **This default is deliberately bounded.** The size limit is the one a
/// builder is most likely to breach without noticing, because nothing
/// downstream reports it: `evaluateTransaction` runs the scripts and never
/// looks at size, so an oversized transaction is reported as evaluating
/// perfectly and is then rejected by the node with `MaxTxSizeUTxO`. Defaulting
/// to zero would make the unguarded case the easy one to reach by accident.
/// A caller that genuinely wants no ceiling must set `max_tx_size: 0` and say
/// so.
pub const CONWAY_MAX_TX_SIZE: u32 = 16_384;

/// Minimum protocol parameters needed for transaction building.
///
/// These values come from the Cardano node's protocol parameters and are
/// used for fee calculation, min UTxO computation, and size validation.
///
/// `Default` is hand-written rather than derived so that `max_tx_size` can
/// carry [`CONWAY_MAX_TX_SIZE`] instead of zero — see that constant for why
/// the size ceiling in particular must not default to unbounded. Every other
/// field still defaults to its zero value.
/// Serialisable so a host that does NOT read protocol parameters itself can be
/// handed them. That is the browser case: it builds transactions but has no
/// indexer, so a worker sends it exactly this — already normalised, rather
/// than an indexer's raw row that each consumer would re-derive differently.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TxBuildParams {
    /// Per-byte fee multiplier (Cardano parameter `a`)
    pub min_fee_coefficient: u64,
    /// Fixed fee constant (Cardano parameter `b`) in lovelace
    pub min_fee_constant: u64,
    /// Coins per UTxO byte (Babbage/Conway parameter `coinsPerUTxOByte`)
    pub coins_per_utxo_byte: u64,
    /// Maximum transaction size in bytes
    pub max_tx_size: u32,
    /// Per-transaction execution-unit ceiling (`maxTxExUnits`), memory then
    /// steps.
    ///
    /// Held here because NOTHING else checks it before submit:
    /// `evaluateTransaction` returns PER-REDEEMER budgets and never sums them
    /// against this, so a sweep evaluates perfectly and the node rejects it
    /// with `ExUnitsTooBigUTxO`. Same blind spot as fees and the
    /// script-integrity hash.
    ///
    /// It binds sooner than intuition suggests for marketplace sweeps: a jpg
    /// V1 buy costs O(n²) across a sweep — each validator scans the output
    /// list for its own payouts, and the list grows with the sweep — measured
    /// at 2.85M / 7.18M / 12.25M / 18.10M memory for 1 / 2 / 3 / 4 listings.
    /// Four does not fit; three sits at 74%.
    /// NAMED, not a `(u64, u64)`. The two figures differ by two orders of
    /// magnitude, so a transposed pair does not look wrong to a reader or to
    /// the compiler — and this crate hands the pair to consumers that order it
    /// the OTHER way round (`tx-eval` takes `(steps, mem)`). One such
    /// transposition silently rejected every packing in a cart search and
    /// reported a perfectly buildable cart as impossible.
    pub max_tx_ex_units: ExUnits,
    /// Maximum serialised size of the *value* portion of a single UTxO output
    /// (Cardano Conway parameter `maxValueSize`). Outputs whose value exceeds
    /// this limit are rejected by the ledger (`OutputTooBigUTxO`).
    pub max_value_size: u64,
    /// Script execution memory price as (numerator, denominator).
    /// Fee contribution per redeemer = mem_units × numerator / denominator.
    /// `None` for callers that don't need Plutus fee calculation.
    pub price_mem: Option<(u64, u64)>,
    /// Script execution CPU step price as (numerator, denominator).
    /// Fee contribution per redeemer = step_units × numerator / denominator.
    pub price_step: Option<(u64, u64)>,
    /// Cost per byte of reference scripts (Conway parameter `minFeeRefScriptCostPerByte`).
    /// Added to the fee for each byte of script referenced via CIP-33 reference inputs.
    /// Mainnet default: 15 lovelace/byte.
    pub min_fee_ref_script_cost_per_byte: u64,
    /// Total size in bytes of all scripts in reference inputs.
    /// Set by the caller when building Plutus TXs with reference scripts.
    pub ref_script_size: u64,
    /// Per-language Plutus cost models from the node's current protocol
    /// parameters. Folded into the script-integrity hash by script-spending
    /// builders; MUST match the node or the tx is rejected with
    /// `PPViewHashesDontMatch`. Empty by default — resolvers fall back to the
    /// bundled [`crate::builder::cost_models`] constants when a language is
    /// absent.
    pub cost_models: PlutusCostModels,
}

impl Default for TxBuildParams {
    fn default() -> Self {
        Self {
            // The one field that does NOT default to zero. An unbounded size
            // ceiling is silently wrong — see `CONWAY_MAX_TX_SIZE`.
            max_tx_size: CONWAY_MAX_TX_SIZE,
            min_fee_coefficient: 0,
            min_fee_constant: 0,
            coins_per_utxo_byte: 0,
            max_tx_ex_units: ExUnits { mem: 0, steps: 0 },
            max_value_size: 0,
            price_mem: None,
            price_step: None,
            min_fee_ref_script_cost_per_byte: 0,
            ref_script_size: 0,
            cost_models: PlutusCostModels::default(),
        }
    }
}

/// Charged size (bytes) of a worst-case PURE-ADA output under the Babbage
/// `coinsPerUTxOByte` min-UTxO formula: the ledger's fixed 160-byte UTxO
/// overhead + ~68 serialized output bytes (Shelley base address + max-width
/// coin). `min lovelace = PURE_ADA_OUTPUT_CHARGED_BYTES × coinsPerUTxOByte`.
/// ONE definition — this was a hand-copied `228` at every call site, where a
/// single drifted copy (one site had `188`) meant sub-minimum outputs rejected
/// `BabbageOutputTooSmallUTxO` at submit, after the build work.
pub const PURE_ADA_OUTPUT_CHARGED_BYTES: u64 = 228;

impl TxBuildParams {
    /// The pure-ADA minimum UTxO value under these params — the floor every
    /// asset-less output must clear or the ledger rejects the tx
    /// (`BabbageOutputTooSmallUTxO`). See [`PURE_ADA_OUTPUT_CHARGED_BYTES`].
    pub fn min_pure_utxo(&self) -> u64 {
        PURE_ADA_OUTPUT_CHARGED_BYTES * self.coins_per_utxo_byte
    }

    /// The minimum UTxO value for an output carrying `assets`.
    ///
    /// Delegates to the shared size calculation so the quantity's CBOR width is
    /// accounted for — a fixed per-asset estimate under-counts large quantities
    /// and produces `BabbageOutputTooSmallUTxO` at submit.
    ///
    /// Exists so builders can size an asset-bearing output from `TxBuildParams`
    /// alone, without reaching for an indexer's `ProtocolParameters` type.
    pub fn min_utxo_for_assets(&self, assets: &[crate::utxo::AssetAmount]) -> u64 {
        crate::utxo::min_ada_for_assets(self.coins_per_utxo_byte, assets)
    }
}

#[cfg(feature = "maestro")]
impl From<&maestro::ProtocolParameters> for TxBuildParams {
    fn from(pp: &maestro::ProtocolParameters) -> Self {
        let (price_mem, price_step) = pp
            .script_execution_prices
            .as_ref()
            .map(|ep| (ep.parse_memory(), ep.parse_cpu()))
            .unwrap_or((None, None));

        Self {
            min_fee_coefficient: pp.min_fee_coefficient,
            min_fee_constant: pp.min_fee_constant.ada.lovelace,
            coins_per_utxo_byte: pp.min_utxo_deposit_coefficient,
            // Maestro doesn't expose max_tx_size directly; use Cardano mainnet default
            max_tx_size: 16384,
            max_value_size: 5000,
            // From the live params when present, else Conway mainnet's values.
            // A too-LARGE fallback would let an over-budget sweep through to
            // the node, which is the failure this field exists to prevent, so
            // the default is the real protocol figure rather than something
            // permissive.
            max_tx_ex_units: pp
                .max_execution_units_per_transaction
                .as_ref()
                .map(|eu| ExUnits {
                    mem: eu.memory,
                    steps: eu.cpu,
                })
                .unwrap_or(ExUnits {
                    mem: 16_500_000,
                    steps: 10_000_000_000,
                }),
            price_mem,
            price_step,
            min_fee_ref_script_cost_per_byte: CONWAY_MIN_FEE_REF_SCRIPT_COST_PER_BYTE,
            // Nothing is known about reference scripts at this point; whoever
            // builds a transaction that USES one must declare its size.
            ref_script_size: 0,
            cost_models: PlutusCostModels::from(pp),
        }
    }
}

#[cfg(feature = "maestro")]
impl From<&maestro::ProtocolParameters> for PlutusCostModels {
    fn from(pp: &maestro::ProtocolParameters) -> Self {
        pp.plutus_cost_models
            .as_ref()
            .map(|cm| Self {
                plutus_v1: cm.plutus_v1.clone(),
                plutus_v2: cm.plutus_v2.clone(),
                plutus_v3: cm.plutus_v3.clone(),
            })
            .unwrap_or_default()
    }
}

// Only the Maestro adapter is exercised here, so the whole module goes with
// the feature — gating the single test would leave `use super::*` unused.
#[cfg(all(test, feature = "maestro"))]
mod tests {
    use super::*;

    #[test]
    fn test_from_maestro_params() {
        let maestro_params = maestro::ProtocolParameters {
            min_fee_coefficient: 44,
            min_fee_constant: maestro::AdaLovelace {
                ada: maestro::AdaAmount { lovelace: 155381 },
            },
            min_utxo_deposit_coefficient: 4310,
            script_execution_prices: Some(maestro::ExecutionPrices {
                memory: "577/10000".to_string(),
                cpu: "721/10000000".to_string(),
            }),
            max_execution_units_per_transaction: None,
            max_transaction_size: None,
            plutus_cost_models: None,
        };

        let params = TxBuildParams::from(&maestro_params);
        assert_eq!(params.min_fee_coefficient, 44);
        assert_eq!(params.min_fee_constant, 155381);
        assert_eq!(params.coins_per_utxo_byte, 4310);
        assert_eq!(params.max_tx_size, 16384);
        assert_eq!(params.max_value_size, 5000);
        assert_eq!(params.price_mem, Some((577, 10000)));
        assert_eq!(params.price_step, Some((721, 10000000)));
    }
}
