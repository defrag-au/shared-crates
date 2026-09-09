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

use cardano_assets::UtxoApi;
use cardano_assets::utxo::UtxoTag;
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{ExUnits, Input, Output, ScriptKind, StagingTransaction};
use std::collections::HashSet;

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

    /// Build with script evaluation via the injected [`TxEvaluator`](crate::evaluate::TxEvaluator).
    ///
    /// 1. Build with estimated ExUnits (from the `ScriptInput`/`MintEntry` values)
    /// 2. Evaluate the TX via the provider (Maestro, Koios/Ogmios, …) for real units
    /// 3. Patch the redeemers with actual ExUnits and rebuild
    ///
    /// This produces accurate fees that include the script execution cost.
    /// The provider is any [`TxEvaluator`](crate::evaluate::TxEvaluator); passing a
    /// `&maestro::MaestroApi` keeps the previous behaviour unchanged.
    pub async fn build_evaluated<E>(self, evaluator: &E) -> Result<UnsignedTx, TxBuildError>
    where
        E: crate::evaluate::TxEvaluator + ?Sized,
    {
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

        // Evaluate script execution costs via the injected provider (Maestro,
        // Koios/Ogmios, …) to get the real per-redeemer ExUnits.
        let eval_results = evaluator.evaluate(&tx_cbor_hex).await.map_err(|e| {
            TxBuildError::BuildFailed(format!("{} evaluate failed: {e}", evaluator.name()))
        })?;

        // Patch spend redeemer ExUnits (redeemer_tag = "spend").
        //
        // A spend redeemer's index is its input's position in the LEDGER's
        // sorted input list — not a running count of script inputs. Those
        // coincide only when every script input sorts ahead of every plain
        // one, which is a property of the tx hashes involved and therefore
        // luck. A 4-listing sweep whose funding UTxO happened to sort first
        // put the script inputs at 1..=4 while the counter looked for 0..=3:
        // one redeemer matched nothing and kept the default estimate, and the
        // other three were handed ANOTHER listing's units. Three of four
        // spends then failed on-chain for overspending their budget.
        //
        // Sort the same way the ledger does — by (tx hash, output index) — and
        // match on the real position.
        let all_refs: Vec<(Vec<u8>, u64)> = prepared
            .inputs
            .iter()
            .map(|(input, _)| (input.tx_hash.0.to_vec(), input.txo_index))
            .collect();

        for (input, script_ctx) in &mut prepared.inputs {
            let Some(ctx) = script_ctx else { continue };
            let key = (input.tx_hash.0.to_vec(), input.txo_index);
            let Some(position) = spend_redeemer_index(&all_refs, &key) else {
                continue;
            };
            if let Some(eval) = eval_results
                .iter()
                .find(|r| r.redeemer_tag == "spend" && r.redeemer_index == position)
            {
                ctx.ex_units = with_budget_margin(ExUnits {
                    mem: eval.ex_units.mem,
                    steps: eval.ex_units.steps,
                });
            }
        }

        // Patch mint redeemer ExUnits (redeemer_tag = "mint")
        for (mint_idx, mint_entry) in prepared.mints.iter_mut().enumerate() {
            if let Some(eval) = eval_results
                .iter()
                .find(|r| r.redeemer_tag == "mint" && r.redeemer_index == mint_idx as u64)
            {
                mint_entry.ex_units = ExUnits {
                    mem: eval.ex_units.mem,
                    steps: eval.ex_units.steps,
                };
            }
        }

        // The budget the whole transaction will be charged against, which is
        // the one thing the evaluator does NOT check: it reports per-redeemer
        // costs and never sums them against `maxTxExUnits`. Without this the
        // build succeeds, the caller sees "evaluated OK", and the node rejects
        // with `ExUnitsTooBigUTxO` — with no indication that the batch was
        // simply too large.
        let (mem_cap, steps_cap) = prepared.params.max_tx_ex_units;
        let mem: u64 = prepared
            .inputs
            .iter()
            .filter_map(|(_, s)| s.as_ref().map(|c| c.ex_units.mem))
            .chain(prepared.mints.iter().map(|m| m.ex_units.mem))
            .sum();
        let steps: u64 = prepared
            .inputs
            .iter()
            .filter_map(|(_, s)| s.as_ref().map(|c| c.ex_units.steps))
            .chain(prepared.mints.iter().map(|m| m.ex_units.steps))
            .sum();
        if mem > mem_cap || steps > steps_cap {
            return Err(TxBuildError::ExUnitsExceeded {
                mem,
                mem_cap,
                steps,
                steps_cap,
            });
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
        // The REAL pure-ADA floor (shared helper). This was `188 *` — below the
        // ledger's 228-based minimum, so a change output in the ~0.81–0.98 ADA
        // band passed the builder but was rejected `BabbageOutputTooSmallUTxO`.
        let min_change_lovelace = self.deps.params.min_pure_utxo();
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

            candidates.sort_by_key(|b| std::cmp::Reverse(b.lovelace));

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

    /// Track the highest Plutus version any script in this TX uses, which
    /// selects the cost model that goes into the language views.
    ///
    /// Both sources state their language; neither is inferred. A reference
    /// script used to default to V3 here on the grounds that reference scripts
    /// are "typically" V3 — but the language is part of the script-integrity
    /// hash, so a wrong guess makes the node compute a different hash and
    /// reject the transaction with `ScriptIntegrityHashMismatch`. Nothing
    /// catches it earlier: `evaluateTransaction` executes the scripts and does
    /// not check this field, so such a TX evaluates perfectly and then fails
    /// at submit. Every jpg.store buy went out that way — both jpg validators
    /// are plutusV2, and every buy spends them by reference.
    fn track_script_kind(&mut self, source: &ScriptSource) {
        let new_kind = match source {
            ScriptSource::Inline { language, .. } => *language,
            ScriptSource::Reference { language, .. } => *language,
        };
        self.max_script_kind = Some(match self.max_script_kind {
            None => new_kind,
            Some(existing) => higher_plutus_version(existing, new_kind),
        });
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
    ///
    /// Change handling is the VALUE-BALANCE guarantee: the leftover either
    /// clears the pure-ADA floor and becomes a change output, or it is FOLDED
    /// INTO THE FEE — never silently dropped (a dropped leftover is
    /// `ValueNotConservedUTxO` at the node, after the build + signing work).
    /// And the leftover is computed CHECKED: a converged fee above the rough
    /// selection estimate surfaces as `InsufficientFunds`, not as an
    /// unbalanced tx. The returned [`UnsignedTx::fee`] is the EFFECTIVE fee
    /// (the staged value, including any folded leftover).
    fn converge(&self) -> Result<UnsignedTx, TxBuildError> {
        let total_input = self.total_input;
        let output_lovelace = self.output_lovelace;
        let min_change_lovelace = self.min_change_lovelace;

        let mut unsigned = super::converge_fee(
            |fee| {
                let change = total_input
                    .checked_sub(output_lovelace)
                    .and_then(|v| v.checked_sub(fee))
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: output_lovelace + fee,
                        available: total_input,
                    })?;

                let mut all_outputs = self.outputs.clone();
                let effective_fee = if change >= min_change_lovelace {
                    all_outputs.push(create_ada_output(self.change_address.clone(), change));
                    fee
                } else {
                    // Sub-floor leftover can't form a valid change output —
                    // absorb it into the fee so the tx stays balanced. Bounded
                    // by the floor (~1 ADA); callers avoid the band by adding
                    // input headroom.
                    fee + change
                };

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
                    effective_fee,
                    &self.params.cost_models,
                )
            },
            300_000,
            &self.params,
        )?;
        // Report the staged (effective) fee — converge_fee returns its converged
        // base, which under-reports when a leftover was folded in above.
        if let Some(staged) = unsigned.staging.fee {
            unsigned.fee = staged;
        }
        Ok(unsigned)
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
    cost_models: &super::cost_models::PlutusCostModels,
) -> Result<StagingTransaction, TxBuildError> {
    let mut tx = StagingTransaction::new();
    let mut wanted_refs: Vec<Input> = Vec::new();

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

            // Script (inline or reference). Reference inputs are collected and
            // emitted once, below — see `wanted_refs`.
            match &ctx.script {
                ScriptSource::Inline { language, bytes } => {
                    tx = tx.script(*language, bytes.clone());
                }
                ScriptSource::Reference { utxo, .. } => {
                    wanted_refs.push(utxo.clone());
                }
            }

            // Datum witness (if not inline)
            if let Some(datum) = &ctx.datum_cbor {
                tx = tx.datum(datum.clone());
            }
        }
    }

    // 2. Reference inputs — DEDUPLICATED.
    //
    // Conway encodes reference inputs as a `set`, and the ledger rejects a set
    // containing the same entry twice ("final number of elements does not match
    // the total count that was decoded"). Two paths converge here: several
    // script inputs sharing one reference script (a sweep of listings at the
    // same contract), and a caller that also adds the reference explicitly.
    // Both are natural, so dedupe rather than making callers coordinate.
    wanted_refs.extend(reference_inputs.iter().cloned());
    let mut emitted: HashSet<([u8; 32], u64)> = HashSet::new();
    for ref_input in wanted_refs {
        if emitted.insert((ref_input.tx_hash.0, ref_input.txo_index)) {
            tx = tx.reference_input(ref_input);
        }
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
            ScriptSource::Reference { utxo, .. } => {
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
    //
    // Sourced from the live protocol parameters, falling back to the bundled
    // constants only when the caller had none. A stale or wrong-length cost
    // model produces a wrong script-integrity hash and the node rejects every
    // script spend with `PPViewHashesDontMatch` — the failure that took the
    // collection-offer cancel path down when these were hardcoded here.
    if let Some(kind) = max_script_kind {
        let cost_model = match kind {
            ScriptKind::PlutusV2 => cost_models.v2(),
            _ => cost_models.v3(),
        };
        tx = tx.add_language(kind, cost_model);
    }

    // 10. Fee + network
    tx = tx.fee(fee).network_id(network_id);

    Ok(tx)
}

/// Headroom added to every evaluated execution budget, in percent.
///
/// The evaluator reports what the scripts cost in the transaction it was GIVEN
/// — but the ScriptContext a validator sees includes the transaction's fee,
/// and the fee is not final at evaluation time: the second convergence round
/// changes it, which perturbs the context and moves the true cost slightly.
/// Booking the exact reported figure therefore fails on the node by a hair.
/// Observed on a real 4-listing V1 sweep: short by 2,206 memory units.
///
/// Execution units are only charged as FEE, and the fee is bounded by the
/// units booked, so the margin costs a few hundred lovelace. Failing costs the
/// whole transaction.
const EX_UNIT_MARGIN_PERCENT: u64 = 15;

/// Apply [`EX_UNIT_MARGIN_PERCENT`] to an evaluated budget.
fn with_budget_margin(units: ExUnits) -> ExUnits {
    let scale = |v: u64| v.saturating_mul(100 + EX_UNIT_MARGIN_PERCENT) / 100;
    ExUnits {
        mem: scale(units.mem),
        steps: scale(units.steps),
    }
}

/// A spend redeemer's index, given every input ref in the transaction.
///
/// Extracted so the mapping is testable without a live evaluator — it is the
/// step that silently mis-assigned execution budgets between listings.
fn spend_redeemer_index(all_refs: &[(Vec<u8>, u64)], script_ref: &(Vec<u8>, u64)) -> Option<u64> {
    let mut sorted = all_refs.to_vec();
    sorted.sort();
    sorted.iter().position(|r| r == script_ref).map(|p| p as u64)
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
    if rank(a) >= rank(b) { a } else { b }
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
                ..Default::default()
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

    /// Σ inputs must equal Σ outputs + staged fee — the balance invariant.
    fn assert_balanced(unsigned: &UnsignedTx, input_lovelace: u64) {
        let out: u64 = unsigned
            .staging
            .outputs
            .iter()
            .flatten()
            .map(|o| o.lovelace)
            .sum();
        let fee = unsigned.staging.fee.expect("fee staged");
        assert_eq!(
            input_lovelace,
            out + fee,
            "unbalanced: in={input_lovelace} out={out} fee={fee}"
        );
        assert_eq!(unsigned.fee, fee, "UnsignedTx.fee must be the staged fee");
    }

    /// A leftover below the pure-ADA floor must FOLD INTO THE FEE, not vanish
    /// (the old behavior dropped it → ValueNotConservedUTxO at the node).
    #[test]
    fn sub_floor_leftover_folds_into_fee() {
        let deps = test_deps();
        let to_addr = deps.from_address.clone();
        let input = deps.utxos[0].clone(); // 50 ADA
        let floor = deps.params.min_pure_utxo();

        // Pay out so the leftover after a ~0.17 ADA fee sits WELL below the
        // floor (~0.4 ADA leftover).
        let pay = 50_000_000 - 600_000;
        let unsigned = TxBuilder::new(deps)
            .input(&input)
            .unwrap()
            .pay_to(&to_addr, pay)
            .build()
            .unwrap();
        assert_balanced(&unsigned, 50_000_000);
        // No change output (only the payment), the leftover rode the fee.
        assert_eq!(unsigned.staging.outputs.iter().flatten().count(), 1);
        assert!(unsigned.fee < floor + 600_000);
    }

    /// A leftover in the old trap band (above the bogus 188-based threshold,
    /// below the real 228-based floor) must also fold — the old code emitted a
    /// sub-minimum change output here (BabbageOutputTooSmallUTxO).
    #[test]
    fn old_trap_band_leftover_folds_not_emitted() {
        let deps = test_deps();
        let to_addr = deps.from_address.clone();
        let input = deps.utxos[0].clone(); // 50 ADA
        let floor = deps.params.min_pure_utxo(); // 228-based, ~983k
        let bogus = 188 * deps.params.coins_per_utxo_byte; // ~810k
        assert!(bogus < floor);

        // Target a leftover-after-fee of ~900k — inside (bogus, floor).
        let pay = 50_000_000 - 900_000 - 170_000;
        let unsigned = TxBuilder::new(deps)
            .input(&input)
            .unwrap()
            .pay_to(&to_addr, pay)
            .build()
            .unwrap();
        assert_balanced(&unsigned, 50_000_000);
        for o in unsigned.staging.outputs.iter().flatten() {
            assert!(
                o.lovelace == pay || o.lovelace >= floor,
                "no output may sit below the pure-ADA floor: {}",
                o.lovelace
            );
        }
    }

    /// A converged fee that exceeds the inputs must fail CLEANLY — the old
    /// saturating math sent an unbalanced tx instead.
    #[test]
    fn insufficient_after_convergence_is_clean_error() {
        let mut deps = test_deps();
        deps.utxos[0].lovelace = 2_050_000; // barely above the payment
        let to_addr = deps.from_address.clone();
        let input = deps.utxos[0].clone();
        let result = TxBuilder::new(deps)
            .input(&input)
            .unwrap()
            .pay_to(&to_addr, 2_000_000)
            .build();
        assert!(
            matches!(result, Err(TxBuildError::InsufficientFunds { .. })),
            "expected InsufficientFunds, got {result:?}"
        );
    }

    /// Several script inputs sharing ONE reference script must emit that
    /// reference input exactly once.
    ///
    /// Conway encodes reference inputs as a `set`, and the node rejects a set
    /// containing the same entry twice — "final number of elements: 1 does not
    /// match the total count that was decoded: 2". The failure is invisible
    /// locally: the tx builds and serialises fine, and is only rejected at
    /// evaluation/submission with an error that blames CBOR rather than the
    /// duplicate. Found while sweeping two jpg.store listings at one contract.
    /// The real 4-listing V1 sweep that failed on-chain: a funding UTxO whose
    /// hash sorts FIRST, then four script inputs. A running count of script
    /// inputs would say 0,1,2,3; the ledger says 1,2,3,4. Getting this wrong
    /// does not error — it silently hands each listing another listing's
    /// execution budget, and three of four spends died overspending.
    #[test]
    fn spend_redeemer_index_is_the_ledgers_sorted_position() {
        // `2f29…` sorts before `e555…`, so the funding input is index 0.
        let funding = (vec![0x2f, 0x29], 1u64);
        let refs = vec![
            funding.clone(),
            (vec![0xe5, 0x55], 0),
            (vec![0xe5, 0x55], 1),
            (vec![0xe5, 0x55], 2),
            (vec![0xe5, 0x55], 6),
        ];

        assert_eq!(spend_redeemer_index(&refs, &funding), Some(0));
        assert_eq!(spend_redeemer_index(&refs, &(vec![0xe5, 0x55], 0)), Some(1));
        assert_eq!(spend_redeemer_index(&refs, &(vec![0xe5, 0x55], 1)), Some(2));
        assert_eq!(spend_redeemer_index(&refs, &(vec![0xe5, 0x55], 2)), Some(3));
        assert_eq!(spend_redeemer_index(&refs, &(vec![0xe5, 0x55], 6)), Some(4));
    }

    /// Insertion order must not matter — only the sorted position does.
    #[test]
    fn spend_redeemer_index_ignores_insertion_order() {
        let a = (vec![0xaa], 0u64);
        let b = (vec![0xbb], 0u64);
        let inserted_backwards = vec![b.clone(), a.clone()];
        assert_eq!(spend_redeemer_index(&inserted_backwards, &a), Some(0));
        assert_eq!(spend_redeemer_index(&inserted_backwards, &b), Some(1));
    }

    /// The evaluated budget is for the transaction as EVALUATED; the fee moves
    /// afterwards and the fee is inside the ScriptContext, so the true cost
    /// shifts. Booking the exact figure failed by 2,206 memory units on a real
    /// sweep.
    #[test]
    fn evaluated_budgets_get_headroom() {
        let evaluated = ExUnits {
            mem: 1_000_000,
            steps: 400_000_000,
        };
        let booked = with_budget_margin(ExUnits {
            mem: evaluated.mem,
            steps: evaluated.steps,
        });
        assert!(booked.mem > evaluated.mem, "memory must gain headroom");
        assert!(booked.steps > evaluated.steps, "steps must gain headroom");
        assert_eq!(booked.mem, 1_150_000);
        assert_eq!(booked.steps, 460_000_000);
    }

    #[test]
    fn shared_reference_script_is_emitted_once() {
        use pallas_txbuilder::BuildConway;

        let script_ref = Input::new(Hash::from([0xab; 32]), 0);
        let script_input = || ScriptInput {
            script: ScriptSource::Reference {
                utxo: script_ref.clone(),
                language: ScriptKind::PlutusV2,
            },
            datum_cbor: None,
            redeemer_cbor: vec![0xd8, 0x79, 0x80],
            ex_units: ExUnits {
                mem: 1_000,
                steps: 1_000,
            },
        };

        let inputs = vec![
            (Input::new(Hash::from([0x01; 32]), 0), Some(script_input())),
            (Input::new(Hash::from([0x02; 32]), 0), Some(script_input())),
        ];

        let tx = assemble_tx(
            &inputs,
            // The caller ALSO passes it explicitly — the other way a duplicate
            // arises, and equally natural.
            std::slice::from_ref(&script_ref),
            &[],
            &[],
            &[],
            &ValidityInterval::default(),
            &None,
            &None,
            Some(ScriptKind::PlutusV2),
            1,
            200_000,
            &crate::builder::cost_models::PlutusCostModels::EMPTY,
        )
        .expect("assembles");

        let built = tx.build_conway_raw().expect("serialises");
        let refs = count_reference_inputs(&built.tx_bytes.0);
        assert_eq!(
            refs, 1,
            "three requests for one reference script must collapse to a single \
             reference input; got {refs}, which the ledger rejects as a duplicate set entry"
        );
    }

    /// Count reference inputs (body key 18) in a serialised tx.
    fn count_reference_inputs(cbor: &[u8]) -> usize {
        use pallas_traverse::MultiEraTx;
        let tx = MultiEraTx::decode(cbor).expect("tx decodes");
        tx.reference_inputs().len()
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
