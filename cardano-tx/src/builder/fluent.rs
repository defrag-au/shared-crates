//! Fluent transaction builder — a higher-level API on top of pallas `StagingTransaction`.
//!
//! Wraps the low-level pallas TX builder with a chainable API that handles:
//! - Plutus cost model injection (auto-detected from script kinds used)
//! - Collateral selection (auto or manual)
//! - Fee convergence via two-round build
//! - Inline vs reference script support
//!
//! # Example
//! ```ignore
//! let unsigned = TxBuilder::new(deps)
//!     .input(&utxo)
//!     .pay_to(&address, 2_000_000)
//!     .with_signer(pkh)
//!     .build()?;
//! ```

use cardano_assets::utxo::UtxoTag;
use cardano_assets::UtxoApi;
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{ExUnits, Input, Output, ScriptKind, StagingTransaction};
use std::collections::HashSet;

use super::cost_models::{PLUTUS_V2_COST_MODEL, PLUTUS_V3_COST_MODEL};
use super::script::{CollateralConfig, MintEntry, ScriptInput, ScriptSource, ValidityInterval};
use super::{TxDeps, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::decode::decode_tx_hash;
use crate::helpers::output::{add_assets_to_output, create_ada_output};
use crate::metadata::cip25::build_cip25_auxiliary_data;
use crate::selection::{estimate_simple_fee, select_collateral};

// ============================================================================
// TxBuilder
// ============================================================================

/// Fluent transaction builder.
///
/// Consuming `self` pattern (same as pallas `StagingTransaction`) — each method
/// takes ownership and returns the modified builder.
pub struct TxBuilder {
    deps: TxDeps,
    inputs: Vec<(Input, Option<ScriptInput>)>,
    reference_inputs: Vec<Input>,
    outputs: Vec<Output>,
    mints: Vec<MintEntry>,
    required_signers: Vec<Hash<28>>,
    validity: ValidityInterval,
    auxiliary_data: Option<Vec<u8>>,
    collateral: Option<CollateralConfig>,
    /// Track highest Plutus version used (for cost model selection)
    max_script_kind: Option<ScriptKind>,
    /// Track UTxO refs already added as inputs (tx_hash, output_index) to avoid double-spend.
    used_input_refs: HashSet<(String, u32)>,
    /// Sum of lovelace from explicitly added inputs (for coin selection).
    input_lovelace: u64,
}

impl TxBuilder {
    /// Create a new builder with the given dependencies.
    pub fn new(deps: TxDeps) -> Self {
        Self {
            deps,
            inputs: Vec::new(),
            reference_inputs: Vec::new(),
            outputs: Vec::new(),
            mints: Vec::new(),
            required_signers: Vec::new(),
            validity: ValidityInterval::default(),
            auxiliary_data: None,
            collateral: None,
            max_script_kind: None,
            used_input_refs: HashSet::new(),
            input_lovelace: 0,
        }
    }

    // --- Inputs ---

    /// Add a plain (non-script) UTxO input.
    pub fn input(mut self, utxo: &UtxoApi) -> Result<Self, TxBuildError> {
        let tx_hash = decode_tx_hash(&utxo.tx_hash)?;
        let input = Input::new(Hash::from(tx_hash), utxo.output_index as u64);
        self.used_input_refs
            .insert((utxo.tx_hash.clone(), utxo.output_index));
        self.input_lovelace += utxo.lovelace;
        self.inputs.push((input, None));
        Ok(self)
    }

    /// Add a Plutus script UTxO spend.
    pub fn spend_script_utxo(
        mut self,
        utxo: &UtxoApi,
        script_input: ScriptInput,
    ) -> Result<Self, TxBuildError> {
        let tx_hash = decode_tx_hash(&utxo.tx_hash)?;
        let input = Input::new(Hash::from(tx_hash), utxo.output_index as u64);
        self.used_input_refs
            .insert((utxo.tx_hash.clone(), utxo.output_index));
        self.input_lovelace += utxo.lovelace;
        self.track_script_kind(&script_input.script);
        self.inputs.push((input, Some(script_input)));
        Ok(self)
    }

    /// Add a reference input (CIP-31 — read-only, not consumed).
    pub fn reference_input(mut self, tx_hash_hex: &str, index: u32) -> Result<Self, TxBuildError> {
        let tx_hash = decode_tx_hash(tx_hash_hex)?;
        self.reference_inputs
            .push(Input::new(Hash::from(tx_hash), index as u64));
        Ok(self)
    }

    // --- Outputs ---

    /// Add a simple ADA-only output.
    pub fn pay_to(mut self, address: &Address, lovelace: u64) -> Self {
        self.outputs
            .push(create_ada_output(address.clone(), lovelace));
        self
    }

    /// Add an output with inline datum (CBOR bytes).
    pub fn pay_to_with_datum(
        mut self,
        address: &Address,
        lovelace: u64,
        datum_cbor: Vec<u8>,
    ) -> Self {
        let output = create_ada_output(address.clone(), lovelace).set_inline_datum(datum_cbor);
        self.outputs.push(output);
        self
    }

    /// Add an output with native assets.
    ///
    /// Assets are `(policy_hex, asset_name_hex, quantity)` tuples.
    pub fn pay_to_with_assets(
        mut self,
        address: &Address,
        lovelace: u64,
        assets: &[(&str, &str, u64)],
    ) -> Result<Self, TxBuildError> {
        let output = create_ada_output(address.clone(), lovelace);
        let output = add_assets_to_output(output, assets)?;
        self.outputs.push(output);
        Ok(self)
    }

    /// Add a pre-built output (escape hatch).
    pub fn output(mut self, output: Output) -> Self {
        self.outputs.push(output);
        self
    }

    // --- Minting ---

    /// Add a minting operation.
    pub fn mint(mut self, entry: MintEntry) -> Self {
        self.track_script_kind(&entry.script);
        self.mints.push(entry);
        self
    }

    // --- Signing & Validity ---

    /// Require a specific signer (disclosed signer / required signer).
    pub fn with_signer(mut self, pkh: Hash<28>) -> Self {
        self.required_signers.push(pkh);
        self
    }

    /// Set the lower validity bound (transaction valid from this slot).
    pub fn valid_from(mut self, slot: u64) -> Self {
        self.validity.valid_from = Some(slot);
        self
    }

    /// Set the upper validity bound (TTL — transaction invalid after this slot).
    pub fn valid_to(mut self, slot: u64) -> Self {
        self.validity.invalid_after = Some(slot);
        self
    }

    // --- Metadata ---

    /// Attach CIP-25 metadata (for minting with on-chain metadata).
    pub fn with_cip25_metadata(
        mut self,
        metadata_json: &serde_json::Value,
    ) -> Result<Self, TxBuildError> {
        let aux_bytes = build_cip25_auxiliary_data(metadata_json)
            .map_err(|e| TxBuildError::BuildFailed(format!("CIP-25 metadata error: {e}")))?;
        self.auxiliary_data = Some(aux_bytes);
        Ok(self)
    }

    // --- Collateral ---

    /// Configure collateral for Plutus transactions.
    pub fn with_collateral(mut self, config: CollateralConfig) -> Self {
        self.collateral = Some(config);
        self
    }

    // --- Build ---

    /// Build the transaction, performing fee convergence.
    ///
    /// Internally:
    /// 1. Auto-selects collateral if needed and configured as `Auto`
    /// 2. Detects the highest Plutus version and sets the appropriate cost model
    /// 3. Runs two-round fee convergence
    /// 4. Returns `UnsignedTx` ready for signing
    pub fn build(self) -> Result<UnsignedTx, TxBuildError> {
        let prepared = self.prepare()?;
        prepared.converge()
    }

    /// Build with script evaluation via Maestro.
    ///
    /// 1. Build with estimated ExUnits (from the `ScriptInput`/`MintEntry` values)
    /// 2. Evaluate the TX via Maestro to get real execution units
    /// 3. Patch the redeemers with actual ExUnits and rebuild
    ///
    /// This produces accurate fees that include the script execution cost.
    pub async fn build_evaluated(
        self,
        maestro: &maestro::MaestroApi,
    ) -> Result<UnsignedTx, TxBuildError> {
        let mut prepared = self.prepare()?;

        // Round 1: build with estimated ExUnits
        let initial = prepared.converge()?;

        // Serialize to CBOR for evaluation
        use pallas_txbuilder::BuildConway;
        let built = initial
            .staging
            .build_conway_raw()
            .map_err(|e| TxBuildError::BuildFailed(format!("build_conway_raw failed: {e}")))?;
        let tx_cbor_hex = hex::encode(&built.tx_bytes.0);

        // Evaluate via Maestro
        let eval_results = maestro
            .evaluate_transaction(&tx_cbor_hex, None::<&[maestro::AdditionalUtxo]>)
            .await
            .map_err(|e| TxBuildError::BuildFailed(format!("Maestro evaluate failed: {e}")))?;

        // Patch spend redeemer ExUnits (redeemer_tag = "spend")
        // Maestro returns redeemer_index which maps to sorted input order.
        // Our inputs list is in insertion order, which should match the sorted
        // order after assemble_tx. We match by index.
        let mut spend_idx = 0u64;
        for (_input, script_ctx) in &mut prepared.inputs {
            if let Some(ctx) = script_ctx {
                // Find matching evaluation result
                if let Some(eval) = eval_results.iter().find(|r| {
                    r.redeemer_tag == "spend" && r.redeemer_index == spend_idx
                }) {
                    ctx.ex_units = ExUnits {
                        mem: eval.ex_units.mem,
                        steps: eval.ex_units.steps,
                    };
                }
                spend_idx += 1;
            }
        }

        // Patch mint redeemer ExUnits (redeemer_tag = "mint")
        for (mint_idx, mint_entry) in prepared.mints.iter_mut().enumerate() {
            if let Some(eval) = eval_results.iter().find(|r| {
                r.redeemer_tag == "mint" && r.redeemer_index == mint_idx as u64
            }) {
                mint_entry.ex_units = ExUnits {
                    mem: eval.ex_units.mem,
                    steps: eval.ex_units.steps,
                };
            }
        }

        // Round 2: rebuild with real ExUnits — fee now includes execution cost
        prepared.converge()
    }

    /// Resolve collateral + coin selection, returning a `PreparedTx` ready
    /// for fee convergence. Shared by `build()` and `build_evaluated()`.
    fn prepare(self) -> Result<PreparedTx, TxBuildError> {
        let has_scripts = self.max_script_kind.is_some();

        // Resolve collateral
        let collateral_input = if has_scripts {
            match &self.collateral {
                Some(CollateralConfig::Manual(input)) => Some(input.clone()),
                Some(CollateralConfig::Auto) | None => {
                    let collateral_utxo = select_collateral(&self.deps.utxos).ok_or_else(|| {
                        TxBuildError::BuildFailed(
                            "No suitable collateral UTxO found (need pure ADA >= 5 ADA)"
                                .to_string(),
                        )
                    })?;
                    let tx_hash = decode_tx_hash(&collateral_utxo.tx_hash)?;
                    Some(Input::new(
                        Hash::from(tx_hash),
                        collateral_utxo.output_index as u64,
                    ))
                }
            }
        } else {
            None
        };

        // ── Coin selection ───────────────────────────────────────────────
        let output_lovelace: u64 = self.outputs.iter().map(|o| o.lovelace).sum();
        let estimated_fee = estimate_simple_fee(&self.deps.params);
        let min_change_lovelace = 188 * self.deps.params.coins_per_utxo_byte;
        let required = output_lovelace + estimated_fee + min_change_lovelace;

        let mut inputs = self.inputs;
        let mut total_input = self.input_lovelace;

        if total_input < required {
            let mut candidates: Vec<&UtxoApi> = self
                .deps
                .utxos
                .iter()
                .filter(|u| {
                    !self
                        .used_input_refs
                        .contains(&(u.tx_hash.clone(), u.output_index))
                    && u.assets.is_empty()
                    && !u.tags.contains(&UtxoTag::HasDatum)
                    && !u.tags.contains(&UtxoTag::HasScriptRef)
                    && !u.tags.contains(&UtxoTag::ScriptAddress)
                })
                .collect();

            candidates.sort_by(|a, b| b.lovelace.cmp(&a.lovelace));

            for utxo in candidates {
                if total_input >= required {
                    break;
                }
                let tx_hash = decode_tx_hash(&utxo.tx_hash)?;
                let input = Input::new(Hash::from(tx_hash), utxo.output_index as u64);
                inputs.push((input, None));
                total_input += utxo.lovelace;
            }

            if total_input < output_lovelace + estimated_fee {
                return Err(TxBuildError::InsufficientFunds {
                    needed: required,
                    available: total_input,
                });
            }
        }

        Ok(PreparedTx {
            inputs,
            reference_inputs: self.reference_inputs,
            outputs: self.outputs,
            mints: self.mints,
            required_signers: self.required_signers,
            validity: self.validity,
            auxiliary_data: self.auxiliary_data,
            collateral_input,
            max_script_kind: self.max_script_kind,
            network_id: self.deps.network_id,
            change_address: self.deps.from_address,
            params: self.deps.params,
            total_input,
            output_lovelace,
            min_change_lovelace,
        })
    }

    // --- Private helpers ---

    fn track_script_kind(&mut self, source: &ScriptSource) {
        if let ScriptSource::Inline { language, .. } = source {
            let new_kind = *language;
            self.max_script_kind = Some(match self.max_script_kind {
                None => new_kind,
                Some(existing) => higher_plutus_version(existing, new_kind),
            });
        } else {
            // Reference scripts — we still need a cost model. Default to V3 if not set,
            // since reference scripts are typically used with V3.
            if self.max_script_kind.is_none() {
                self.max_script_kind = Some(ScriptKind::PlutusV3);
            }
        }
    }
}

// ============================================================================
// PreparedTx — intermediate state after coin selection, before fee convergence
// ============================================================================

/// A transaction with collateral + coin selection resolved, ready for fee
/// convergence. Created by `TxBuilder::prepare()`, used by both `build()`
/// and `build_evaluated()`.
struct PreparedTx {
    inputs: Vec<(Input, Option<ScriptInput>)>,
    reference_inputs: Vec<Input>,
    outputs: Vec<Output>,
    mints: Vec<MintEntry>,
    required_signers: Vec<Hash<28>>,
    validity: ValidityInterval,
    auxiliary_data: Option<Vec<u8>>,
    collateral_input: Option<Input>,
    max_script_kind: Option<ScriptKind>,
    network_id: u8,
    change_address: Address,
    params: crate::params::TxBuildParams,
    total_input: u64,
    output_lovelace: u64,
    min_change_lovelace: u64,
}

impl PreparedTx {
    /// Run two-round fee convergence and produce the final unsigned TX.
    fn converge(&self) -> Result<UnsignedTx, TxBuildError> {
        let total_input = self.total_input;
        let output_lovelace = self.output_lovelace;
        let min_change_lovelace = self.min_change_lovelace;

        super::converge_fee(
            |fee| {
                let change = total_input.saturating_sub(output_lovelace + fee);

                let mut all_outputs = self.outputs.clone();
                if change >= min_change_lovelace {
                    all_outputs.push(create_ada_output(self.change_address.clone(), change));
                }

                assemble_tx(
                    &self.inputs,
                    &self.reference_inputs,
                    &all_outputs,
                    &self.mints,
                    &self.required_signers,
                    &self.validity,
                    &self.auxiliary_data,
                    &self.collateral_input,
                    self.max_script_kind,
                    self.network_id,
                    fee,
                )
            },
            300_000,
            &self.params,
        )
    }
}

// ============================================================================
// Assembly (stateless — called from converge_fee closure)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn assemble_tx(
    inputs: &[(Input, Option<ScriptInput>)],
    reference_inputs: &[Input],
    outputs: &[Output],
    mints: &[MintEntry],
    required_signers: &[Hash<28>],
    validity: &ValidityInterval,
    auxiliary_data: &Option<Vec<u8>>,
    collateral_input: &Option<Input>,
    max_script_kind: Option<ScriptKind>,
    network_id: u8,
    fee: u64,
) -> Result<StagingTransaction, TxBuildError> {
    let mut tx = StagingTransaction::new();

    // 1. Inputs + script context
    for (input, script_ctx) in inputs {
        tx = tx.input(input.clone());

        if let Some(ctx) = script_ctx {
            // Redeemer
            tx = tx.add_spend_redeemer(
                input.clone(),
                ctx.redeemer_cbor.clone(),
                Some(ExUnits {
                    mem: ctx.ex_units.mem,
                    steps: ctx.ex_units.steps,
                }),
            );

            // Script (inline or reference)
            match &ctx.script {
                ScriptSource::Inline { language, bytes } => {
                    tx = tx.script(*language, bytes.clone());
                }
                ScriptSource::Reference { utxo } => {
                    tx = tx.reference_input(utxo.clone());
                }
            }

            // Datum witness (if not inline)
            if let Some(datum) = &ctx.datum_cbor {
                tx = tx.datum(datum.clone());
            }
        }
    }

    // 2. Reference inputs
    for ref_input in reference_inputs {
        tx = tx.reference_input(ref_input.clone());
    }

    // 3. Outputs
    for output in outputs {
        tx = tx.output(output.clone());
    }

    // 4. Minting
    for mint_entry in mints {
        for (asset_name, quantity) in &mint_entry.assets {
            tx = tx
                .mint_asset(mint_entry.policy, asset_name.clone(), *quantity)
                .map_err(|e| TxBuildError::BuildFailed(format!("mint_asset failed: {e}")))?;
        }

        tx = tx.add_mint_redeemer(
            mint_entry.policy,
            mint_entry.redeemer_cbor.clone(),
            Some(ExUnits {
                mem: mint_entry.ex_units.mem,
                steps: mint_entry.ex_units.steps,
            }),
        );

        match &mint_entry.script {
            ScriptSource::Inline { language, bytes } => {
                tx = tx.script(*language, bytes.clone());
            }
            ScriptSource::Reference { utxo } => {
                tx = tx.reference_input(utxo.clone());
            }
        }
    }

    // 5. Required signers
    for pkh in required_signers {
        tx = tx.disclosed_signer(*pkh);
    }

    // 6. Validity interval
    if let Some(slot) = validity.valid_from {
        tx = tx.valid_from_slot(slot);
    }
    if let Some(slot) = validity.invalid_after {
        tx = tx.invalid_from_slot(slot);
    }

    // 7. Auxiliary data (metadata)
    if let Some(aux) = auxiliary_data {
        tx = tx.add_auxiliary_data(aux.clone());
    }

    // 8. Collateral
    if let Some(col) = collateral_input {
        tx = tx.collateral_input(col.clone());
    }

    // 9. Language view (cost model)
    if let Some(kind) = max_script_kind {
        let cost_model = match kind {
            ScriptKind::PlutusV2 => PLUTUS_V2_COST_MODEL.to_vec(),
            ScriptKind::PlutusV3 => PLUTUS_V3_COST_MODEL.to_vec(),
            _ => PLUTUS_V3_COST_MODEL.to_vec(),
        };
        tx = tx.language_view(kind, cost_model);
    }

    // 10. Fee + network
    tx = tx.fee(fee).network_id(network_id);

    Ok(tx)
}

/// Return the "higher" Plutus version (V3 > V2 > V1).
/// When a TX uses both V2 and V3 scripts, we need the V3 cost model.
fn higher_plutus_version(a: ScriptKind, b: ScriptKind) -> ScriptKind {
    fn rank(k: ScriptKind) -> u8 {
        match k {
            ScriptKind::PlutusV1 => 1,
            ScriptKind::PlutusV2 => 2,
            ScriptKind::PlutusV3 => 3,
            _ => 0,
        }
    }
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::TxBuildParams;

    fn test_deps() -> TxDeps {
        let addr = Address::from_bech32(
            "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp"
        ).unwrap();
        TxDeps {
            utxos: vec![UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 50_000_000,
                assets: vec![],
                tags: vec![],
            }],
            params: TxBuildParams {
                min_fee_coefficient: 44,
                min_fee_constant: 155381,
                coins_per_utxo_byte: 4310,
                max_tx_size: 16384,
                max_value_size: 5000,
                price_mem: None,
                price_step: None,
            },
            from_address: addr,
            network_id: 0,
        }
    }

    #[test]
    fn test_simple_payment_build() {
        let deps = test_deps();
        let to_addr = deps.from_address.clone();
        let input_utxo = deps.utxos[0].clone();

        let result = TxBuilder::new(deps)
            .input(&input_utxo)
            .unwrap()
            .pay_to(&to_addr, 2_000_000)
            .build();

        assert!(result.is_ok(), "build failed: {result:?}");
        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
        assert!(unsigned.fee < 1_000_000);
    }

    #[test]
    fn test_higher_plutus_version() {
        assert!(matches!(
            higher_plutus_version(ScriptKind::PlutusV2, ScriptKind::PlutusV3),
            ScriptKind::PlutusV3
        ));
        assert!(matches!(
            higher_plutus_version(ScriptKind::PlutusV3, ScriptKind::PlutusV2),
            ScriptKind::PlutusV3
        ));
        assert!(matches!(
            higher_plutus_version(ScriptKind::PlutusV2, ScriptKind::PlutusV2),
            ScriptKind::PlutusV2
        ));
    }
}
