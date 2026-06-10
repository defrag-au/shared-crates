//! `TxPlan` — the composable v2 transaction builder over pluggable coin
//! selection (`docs/design/CARDANO_TX_BUILDER_V2.md`). **This is the trusted
//! path for native-signature value transactions** (sends, refunds, sweeps,
//! splits, asset transfers); the legacy free functions in `builder::send` are
//! deprecated recipes over hand-rolled selection loops. (Plutus/script flows
//! use `builder::fluent::TxBuilder`; the deterministic mint recipe lives in
//! `builder::mint`.)
//!
//! Composes inputs (must-spend + a select-from pool with an exclude set),
//! pure-ADA and native-asset outputs, validity bounds, witness-aware fees, and
//! optional metadata in ONE place; auto-derives the selection target, runs
//! [`crate::select::select`], and emits the SAME [`UnsignedTx`] the existing
//! builders produce — so the `build_and_sign[_multi]_tracked` path gives
//! `(SignedTx, TxEffects)` unchanged (wallet-ledger chaining + 504-recovery
//! keep working with no new index arithmetic).
//!
//! Hardening invariants this module owns:
//! - **Value balance, always**: Σ inputs = Σ outputs + fee, for lovelace AND
//!   every native asset. Asset-bearing inputs have their residual assets
//!   re-output to the change address automatically — an input's assets can
//!   never be silently dropped into `ValueNotConservedUTxO`.
//! - **Witness-aware fees**: [`TxPlan::witnesses`] sizes the converged fee for
//!   the number of vkey signatures the caller will attach. A tx signed with
//!   more keys than the fee was sized for is rejected `FeeTooSmallUTxO`.
//! - **Bounded validity**: [`TxPlan::valid_until`] sets the TTL. A
//!   non-deterministic tx (one whose retry REBUILDS differently — e.g. a
//!   refund payout) MUST carry one, or "absent from chain past the grace
//!   window" never becomes a guarantee and a presumed-lost tx can land after
//!   its replacement paid (a double-pay). Deterministic txs (mints) stay
//!   unbounded on purpose — their identical rebuild hash is the safety.
//! - **No duplicate inputs**: a repeated must-spend ref fails at plan time
//!   ([`SelectError::DuplicateMustSpend`]), not at the ledger.

use cardano_assets::AssetId;
use pallas_addresses::Address;
use pallas_txbuilder::{Output, StagingTransaction};
use std::collections::{BTreeMap, HashSet};

use crate::builder::{converge_fee_with_witnesses, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::input::add_input_ref;
use crate::helpers::output::{add_assets_to_output, create_ada_output};
use crate::params::TxBuildParams;
use crate::select::{select, SelectError, Selectable, Selection, Strategy};
use crate::selection::{estimate_simple_fee, PER_INPUT_FEE_HEADROOM};

/// Lovelace headroom kept as change so a selected build never emits a
/// sub-min-UTxO change output; also absorbs per-input fee growth (the converge
/// pass computes the exact fee). Matches `build_send_many`.
const MIN_CHANGE_CUSHION: u64 = 1_500_000;

/// One requested native-asset transfer: `(asset, quantity)`.
pub type AssetAmount = (AssetId, u64);

/// A fluent plan for a value transaction with pluggable input selection.
pub struct TxPlan<'a, U: Selectable> {
    change_address: Address,
    network_id: u8,
    params: TxBuildParams,
    must_spend: Vec<&'a U>,
    pool: &'a [U],
    exclude: HashSet<(String, u32)>,
    strategy: Strategy,
    outputs: Vec<(Address, u64)>,
    /// Native-asset outputs: recipient + the assets to deliver. Lovelace is the
    /// computed min-UTxO for the asset bundle (never user-supplied — the floor
    /// is a ledger rule, not a knob).
    asset_outputs: Vec<(Address, Vec<AssetAmount>)>,
    metadata: Option<serde_json::Value>,
    sweep_to: Option<Address>,
    rehome_assets: bool,
    fold_change: bool,
    witnesses: u32,
    valid_from: Option<u64>,
    valid_until: Option<u64>,
}

impl<'a, U: Selectable> TxPlan<'a, U> {
    /// Start a plan. `change_address` receives the change (the operational `O`
    /// address in the engine). Default strategy is `ManualOnly` until `select_from`.
    pub fn new(change_address: Address, network_id: u8, params: TxBuildParams) -> Self {
        Self {
            change_address,
            network_id,
            params,
            must_spend: Vec::new(),
            pool: &[],
            exclude: HashSet::new(),
            strategy: Strategy::ManualOnly,
            outputs: Vec::new(),
            asset_outputs: Vec::new(),
            metadata: None,
            sweep_to: None,
            rehome_assets: false,
            fold_change: false,
            witnesses: 1,
            valid_from: None,
            valid_until: None,
        }
    }

    /// Inputs that are ALWAYS spent (a split source, an order's payment, parcels).
    pub fn must_spend(mut self, utxos: impl IntoIterator<Item = &'a U>) -> Self {
        self.must_spend.extend(utxos);
        self
    }

    /// The candidate pool + how to draw from it to cover the remaining target.
    pub fn select_from(mut self, pool: &'a [U], strategy: Strategy) -> Self {
        self.pool = pool;
        self.strategy = strategy;
        self
    }

    /// `(tx_hash, output_index)` pairs the pool selection must never touch
    /// (earmarked parcels) — first-class, replacing the `exclude_earmarked_parcels`
    /// pre-filter.
    pub fn exclude(mut self, ids: impl IntoIterator<Item = (String, u32)>) -> Self {
        self.exclude.extend(ids);
        self
    }

    /// Add one pure-ADA payout output.
    pub fn pay_to(mut self, addr: Address, lovelace: u64) -> Self {
        self.outputs.push((addr, lovelace));
        self
    }

    /// Add several pure-ADA payout outputs (refund payers, distribution payees).
    pub fn pay_many(mut self, outs: impl IntoIterator<Item = (Address, u64)>) -> Self {
        self.outputs.extend(outs);
        self
    }

    /// Deliver native assets to `addr` (an NFT transfer / FT distribution). The
    /// output's lovelace is the computed min-UTxO for the bundle. Holding UTxOs
    /// are taken from `must_spend` first, then AUTO-SELECTED from the pool by
    /// asset id (deterministic order); residual assets the chosen inputs carry
    /// beyond what's sent are re-output to the change address automatically.
    pub fn send_assets_to(
        mut self,
        addr: Address,
        assets: impl IntoIterator<Item = AssetAmount>,
    ) -> Self {
        self.asset_outputs
            .push((addr, assets.into_iter().collect()));
        self
    }

    /// Attach CIP-25/674 metadata (e.g. the `refund:<order_id>` lines).
    pub fn metadata(mut self, md: serde_json::Value) -> Self {
        self.metadata = Some(md);
        self
    }

    /// Size the converged fee for `n` vkey witnesses (floored to 1). REQUIRED
    /// whenever the caller signs with more than one key (e.g. the engine's
    /// Mode-B refund spends inputs at `D` and `O` and signs with both) — each
    /// extra witness is ~101 bytes the fee must cover, or the node rejects the
    /// tx `FeeTooSmallUTxO` after all the build work.
    pub fn witnesses(mut self, n: u32) -> Self {
        self.witnesses = n.max(1);
        self
    }

    /// Lower validity bound (tx valid from this slot).
    pub fn valid_from(mut self, slot: u64) -> Self {
        self.valid_from = Some(slot);
        self
    }

    /// Upper validity bound — the TTL (`invalid_hereafter`). After this slot the
    /// ledger can NEVER accept the tx, which is what makes "absent past the
    /// grace window" a sound failure verdict for a NON-deterministic tx (one
    /// whose retry rebuilds differently, e.g. a refund payout): without a TTL
    /// the presumed-lost original can land long after its replacement paid.
    /// Deterministic txs (mints) deliberately don't set one — their rebuild is
    /// byte-identical, so a late landing is the same tx, not a double-spend.
    pub fn valid_until(mut self, slot: u64) -> Self {
        self.valid_until = Some(slot);
        self
    }

    /// SWEEP mode: spend the whole `must_spend` set (caller-curated) and send
    /// the entire balance MINUS the fee to `address` as one output — no pool
    /// selection. The `build_send_max` shape, for dust consolidation (sweep to
    /// self) and withdraw (sweep to an external address). Asset-bearing inputs
    /// are SKIPPED unless [`TxPlan::rehome_assets`] is set. `pay_*`/`select_from`
    /// are ignored when set.
    pub fn sweep_to(mut self, address: Address) -> Self {
        self.sweep_to = Some(address);
        self
    }

    /// SWEEP modifier: also spend the asset-bearing `must_spend` inputs,
    /// re-outputting ALL their assets in one aggregated min-ADA output to the
    /// change address — so the ADA locked above the assets' minimum joins the
    /// sweep. Sweep-to-self + rehome = a full consolidation (assets packed into
    /// one output, every spare lovelace in another). Script-ref inputs are
    /// still never spent.
    pub fn rehome_assets(mut self) -> Self {
        self.rehome_assets = true;
        self
    }

    /// SELF-FUNDING mode (parcel split): the outputs are sized to consume the inputs
    /// almost exactly, leaving only ~the fee. So DON'T reserve a change cushion in
    /// the selection target, and when the post-output leftover is below the min-UTxO
    /// floor (can't form a valid change UTxO) ABSORB it into the fee rather than
    /// emit a sub-floor change — the source funds its own build with no operator
    /// float. A larger leftover (an intermediate split whose change funds the next
    /// batch) still emits a normal chain-link change. Without this, `build()` would
    /// reserve `MIN_CHANGE_CUSHION` and fail to fund a payment that's sized to its
    /// own parcels + fee. Pure-ADA only (asset outputs are rejected).
    pub fn fold_change(mut self) -> Self {
        self.fold_change = true;
        self
    }

    /// Net the target (Σ outputs + fee + change cushion), resolve the asset
    /// inputs, run selection, assemble the staging tx (inputs = must_spend ++
    /// auto asset picks ++ selected, outputs + asset outputs + asset change +
    /// pure change), and converge the fee for the configured witness count.
    /// Returns the standard [`UnsignedTx`].
    pub fn build(self) -> Result<UnsignedTx, TxBuildError> {
        if let Some(target) = self.sweep_to.clone() {
            return self.build_sweep(target);
        }
        if self.fold_change {
            if !self.asset_outputs.is_empty() {
                return Err(TxBuildError::BuildFailed(
                    "TxPlan: fold_change is pure-ADA only (no asset outputs)".into(),
                ));
            }
            return self.build_fold();
        }
        if self.rehome_assets {
            return Err(TxBuildError::BuildFailed(
                "TxPlan: rehome_assets is a sweep modifier — use sweep_to".into(),
            ));
        }
        if self.outputs.is_empty() && self.asset_outputs.is_empty() {
            return Err(TxBuildError::BuildFailed("TxPlan: no outputs".into()));
        }
        let min_pure_utxo = self.params.min_pure_utxo();
        for (i, (_, amt)) in self.outputs.iter().enumerate() {
            if *amt < min_pure_utxo {
                return Err(TxBuildError::BuildFailed(format!(
                    "TxPlan: outputs[{i}] = {amt} lovelace < min_pure_utxo {min_pure_utxo}"
                )));
            }
        }
        let metadata_bytes = encode_metadata(&self.metadata)?;

        // ── Asset planning ───────────────────────────────────────────────
        // Required quantity per asset id across every asset output.
        let mut required: BTreeMap<String, AssetAmount> = BTreeMap::new();
        for (_, assets) in &self.asset_outputs {
            for (id, qty) in assets {
                if *qty == 0 {
                    return Err(TxBuildError::BuildFailed(format!(
                        "TxPlan: zero-quantity asset {} in send_assets_to",
                        id.concatenated()
                    )));
                }
                let entry = required
                    .entry(id.concatenated())
                    .or_insert_with(|| (id.clone(), 0));
                entry.1 = entry.1.saturating_add(*qty);
            }
        }

        // Asset inputs: the caller's must_spend first; any still-uncovered
        // quantity is auto-picked from the pool by asset id, in deterministic
        // (tx_hash, index) order. An auto-picked UTxO joins must_spend — it is
        // ALWAYS spent, exactly as if the caller had named it.
        let mut must_spend: Vec<&'a U> = self.must_spend.clone();
        if !required.is_empty() {
            let have = aggregate_input_assets(&must_spend)?;
            let mut needed: BTreeMap<String, AssetAmount> = BTreeMap::new();
            for (key, (id, qty)) in &required {
                let held = have.get(key).map(|(_, h)| *h).unwrap_or(0);
                if *qty > held {
                    needed.insert(key.clone(), (id.clone(), qty - held));
                }
            }
            if !needed.is_empty() {
                let already: HashSet<(&str, u32)> = must_spend
                    .iter()
                    .map(|u| (u.tx_hash(), u.output_index()))
                    .collect();
                let mut candidates: Vec<&'a U> = self
                    .pool
                    .iter()
                    .filter(|u| u.has_assets() && !u.has_script_ref())
                    .filter(|u| {
                        !self
                            .exclude
                            .contains(&(u.tx_hash().to_string(), u.output_index()))
                            && !already.contains(&(u.tx_hash(), u.output_index()))
                    })
                    .collect();
                candidates.sort_by(|a, b| {
                    a.tx_hash()
                        .cmp(b.tx_hash())
                        .then(a.output_index().cmp(&b.output_index()))
                });
                for u in candidates {
                    if needed.is_empty() {
                        break;
                    }
                    // An opaque asset input (has_assets, no detail) reports no
                    // ids → never matches → never picked. Safe by construction.
                    let assets = u.assets();
                    if !assets
                        .iter()
                        .any(|a| needed.contains_key(&a.asset_id.concatenated()))
                    {
                        continue;
                    }
                    for a in &assets {
                        if let Some(entry) = needed.get_mut(&a.asset_id.concatenated()) {
                            entry.1 = entry.1.saturating_sub(a.quantity);
                        }
                    }
                    needed.retain(|_, (_, qty)| *qty > 0);
                    must_spend.push(u);
                }
                if let Some((key, (_, missing))) = needed.iter().next() {
                    return Err(TxBuildError::AssetNotFound(format!(
                        "{key} (short {missing} after must_spend + pool)"
                    )));
                }
            }
        }

        // Residual assets (inputs beyond what's sent) → an aggregated asset
        // change output to self. This is the value-balance guarantee: an
        // asset-bearing input can never have its assets silently dropped.
        let input_assets = aggregate_input_assets(&must_spend)?;
        let mut residual: Vec<AssetAmount> = Vec::new();
        for (key, (id, held)) in &input_assets {
            let sent = required.get(key).map(|(_, q)| *q).unwrap_or(0);
            if sent > *held {
                return Err(TxBuildError::AssetNotFound(format!(
                    "{key} (have {held}, sending {sent})"
                )));
            }
            if held - sent > 0 {
                residual.push((id.clone(), held - sent));
            }
        }

        // Concrete asset outputs (auto min-ADA per bundle) + the asset change.
        let mut asset_outs: Vec<(Output, u64)> = Vec::new();
        for (addr, assets) in &self.asset_outputs {
            let ids: Vec<AssetId> = assets.iter().map(|(id, _)| id.clone()).collect();
            let lovelace = min_ada_for_assets(&self.params, &ids);
            asset_outs.push((
                build_asset_output(addr.clone(), lovelace, assets)?,
                lovelace,
            ));
        }
        let asset_change: Option<(Output, u64)> = if residual.is_empty() {
            None
        } else {
            let ids: Vec<AssetId> = residual.iter().map(|(id, _)| id.clone()).collect();
            let lovelace = min_ada_for_assets(&self.params, &ids);
            Some((
                build_asset_output(self.change_address.clone(), lovelace, &residual)?,
                lovelace,
            ))
        };

        let total_pure_outputs: u64 = self.outputs.iter().map(|(_, l)| *l).sum();
        let total_asset_lovelace: u64 = asset_outs.iter().map(|(_, l)| *l).sum::<u64>()
            + asset_change.as_ref().map(|(_, l)| *l).unwrap_or(0);
        // Target estimate only (the converged fee is exact): base + metadata +
        // a rough per-asset-output weight + per-input headroom for the inputs
        // already committed.
        let fee_estimate = estimate_simple_fee(&self.params)
            + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64)
            + (asset_outs.len() as u64 + u64::from(asset_change.is_some())) * 5_000
            + must_spend.len() as u64 * PER_INPUT_FEE_HEADROOM;
        let target = total_pure_outputs
            .saturating_add(total_asset_lovelace)
            .saturating_add(fee_estimate)
            .saturating_add(MIN_CHANGE_CUSHION);

        let sel = Selection {
            must_spend,
            pool: self.pool,
            exclude: &self.exclude,
            strategy: self.strategy,
        };
        let chosen = select(&sel, target).map_err(map_select_err)?;
        let input_lovelace: u64 = chosen.iter().map(|u| u.lovelace()).sum();
        let input_refs: Vec<(String, u32)> = chosen
            .iter()
            .map(|u| (u.tx_hash().to_string(), u.output_index()))
            .collect();
        drop(sel); // release the &self.exclude borrow before moving fields below

        let outputs = self.outputs;
        let change_address = self.change_address;
        let network_id = self.network_id;
        let params = self.params;
        let (valid_from, valid_until) = (self.valid_from, self.valid_until);
        let total_committed = total_pure_outputs + total_asset_lovelace;

        converge_fee_with_witnesses(
            move |fee| {
                let mut tx = StagingTransaction::new();
                for (h, ix) in &input_refs {
                    tx = add_input_ref(tx, h, *ix)?;
                }
                for (addr, amount) in &outputs {
                    tx = tx.output(create_ada_output(addr.clone(), *amount));
                }
                for (out, _) in &asset_outs {
                    tx = tx.output(out.clone());
                }
                if let Some((out, _)) = &asset_change {
                    tx = tx.output(out.clone());
                }
                if let Some(bytes) = &metadata_bytes {
                    tx = tx.add_auxiliary_data(bytes.clone());
                }
                // Pure change back to self; converge balances the fee around it.
                let change = input_lovelace
                    .checked_sub(total_committed)
                    .and_then(|v| v.checked_sub(fee))
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: total_committed + fee,
                        available: input_lovelace,
                    })?;
                if change > 0 {
                    tx = tx.output(create_ada_output(change_address.clone(), change));
                }
                tx = apply_validity(tx, valid_from, valid_until);
                Ok(tx.fee(fee).network_id(network_id))
            },
            fee_estimate,
            &params,
            self.witnesses,
        )
    }

    /// SELF-FUNDING build (`fold_change`): the inputs are sized to their own outputs
    /// (a parcel split's source carved into parcels). Selection nets `outputs + fee`
    /// with NO change cushion; the fee converges around a chain-link change when the
    /// leftover clears the min-UTxO floor, otherwise the sub-floor leftover is folded
    /// into the fee (no change output) so the source funds its own build. Guards
    /// against an underpaying tx (the fragmented-source `fee=0` hazard).
    fn build_fold(self) -> Result<UnsignedTx, TxBuildError> {
        if self.outputs.is_empty() {
            return Err(TxBuildError::BuildFailed("TxPlan fold: no outputs".into()));
        }
        let min_pure_utxo = self.params.min_pure_utxo();
        for (i, (_, amt)) in self.outputs.iter().enumerate() {
            if *amt < min_pure_utxo {
                return Err(TxBuildError::BuildFailed(format!(
                    "TxPlan fold: outputs[{i}] = {amt} lovelace < min_pure_utxo {min_pure_utxo}"
                )));
            }
        }
        let metadata_bytes = encode_metadata(&self.metadata)?;
        let total_outputs: u64 = self.outputs.iter().map(|(_, l)| *l).sum();
        let base_fee = estimate_simple_fee(&self.params)
            + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64);

        // Net `outputs + fee` only — NO change cushion (the source is sized to its
        // own parcels + fee). `ManualOnly` (a paid split) errors here if the source
        // can't cover; a zero-cost split with a pool draws the shortfall.
        let sel = Selection {
            must_spend: self.must_spend,
            pool: self.pool,
            exclude: &self.exclude,
            strategy: self.strategy,
        };
        let chosen =
            select(&sel, total_outputs.saturating_add(base_fee)).map_err(map_select_err)?;
        let input_lovelace: u64 = chosen.iter().map(|u| u.lovelace()).sum();
        let input_refs: Vec<(String, u32)> = chosen
            .iter()
            .map(|u| (u.tx_hash().to_string(), u.output_index()))
            .collect();
        drop(sel);

        let outputs = self.outputs;
        let change_address = self.change_address;
        let network_id = self.network_id;
        let params = self.params;
        let (valid_from, valid_until) = (self.valid_from, self.valid_until);

        let final_fee_estimate = base_fee + (input_refs.len() as u64 * PER_INPUT_FEE_HEADROOM);
        let emit_change_floor = min_pure_utxo.saturating_add(200_000);
        let est_remainder = input_lovelace
            .saturating_sub(total_outputs)
            .saturating_sub(final_fee_estimate);

        if est_remainder >= emit_change_floor {
            // Chain-link change: converge the fee around a normal change output.
            converge_fee_with_witnesses(
                move |fee| {
                    let mut tx = StagingTransaction::new();
                    for (h, ix) in &input_refs {
                        tx = add_input_ref(tx, h, *ix)?;
                    }
                    for (addr, amount) in &outputs {
                        tx = tx.output(create_ada_output(addr.clone(), *amount));
                    }
                    if let Some(bytes) = &metadata_bytes {
                        tx = tx.add_auxiliary_data(bytes.clone());
                    }
                    let change = input_lovelace
                        .checked_sub(total_outputs)
                        .and_then(|v| v.checked_sub(fee))
                        .ok_or(TxBuildError::InsufficientFunds {
                            needed: total_outputs + fee,
                            available: input_lovelace,
                        })?;
                    if change > 0 {
                        tx = tx.output(create_ada_output(change_address.clone(), change));
                    }
                    tx = apply_validity(tx, valid_from, valid_until);
                    Ok(tx.fee(fee).network_id(network_id))
                },
                final_fee_estimate,
                &params,
                self.witnesses,
            )
        } else {
            // Fold: the sub-floor leftover is paid directly as the fee (no change).
            // GUARD: a source that can't cover outputs + the min fee fails cleanly
            // rather than flooring the fee below the minimum.
            if input_lovelace < total_outputs.saturating_add(base_fee) {
                return Err(TxBuildError::InsufficientFunds {
                    needed: total_outputs + base_fee,
                    available: input_lovelace,
                });
            }
            let fee = input_lovelace.saturating_sub(total_outputs);
            let mut tx = StagingTransaction::new();
            for (h, ix) in &input_refs {
                tx = add_input_ref(tx, h, *ix)?;
            }
            for (addr, amount) in &outputs {
                tx = tx.output(create_ada_output(addr.clone(), *amount));
            }
            if let Some(bytes) = &metadata_bytes {
                tx = tx.add_auxiliary_data(bytes.clone());
            }
            tx = apply_validity(tx, valid_from, valid_until);
            Ok(UnsignedTx {
                staging: tx.fee(fee).network_id(network_id),
                fee,
            })
        }
    }

    /// SWEEP build: spend the `must_spend` inputs and send the whole balance
    /// minus the converged fee to `target` as a single output. With
    /// [`TxPlan::rehome_assets`], asset-bearing inputs are spent too and their
    /// assets re-output (aggregated, min-ADA) to the change address — freeing
    /// the excess ADA locked above the assets' minimum into the sweep. Mirrors
    /// `build_send_max`/`build_consolidate` (sweep-to-self + rehome).
    fn build_sweep(self, target: Address) -> Result<UnsignedTx, TxBuildError> {
        check_no_duplicate_inputs(&self.must_spend)?;
        let pure: Vec<&'a U> = self
            .must_spend
            .iter()
            .copied()
            .filter(|u| !u.has_assets() && !u.has_script_ref())
            .collect();
        let asset_inputs: Vec<&'a U> = if self.rehome_assets {
            self.must_spend
                .iter()
                .copied()
                .filter(|u| u.has_assets() && !u.has_script_ref())
                .collect()
        } else {
            Vec::new()
        };
        if pure.is_empty() && asset_inputs.is_empty() {
            return Err(TxBuildError::BuildFailed(
                "TxPlan sweep: no spendable inputs to sweep".into(),
            ));
        }

        // Aggregated asset re-home output (errors on an opaque asset input —
        // we will not build a tx that drops assets).
        let rehomed = aggregate_input_assets(&asset_inputs)?;
        let asset_home: Option<(Output, u64)> = if rehomed.is_empty() {
            None
        } else {
            let bundle: Vec<AssetAmount> = rehomed.values().cloned().collect();
            let ids: Vec<AssetId> = bundle.iter().map(|(id, _)| id.clone()).collect();
            let lovelace = min_ada_for_assets(&self.params, &ids);
            Some((
                build_asset_output(self.change_address.clone(), lovelace, &bundle)?,
                lovelace,
            ))
        };
        let asset_home_lovelace = asset_home.as_ref().map(|(_, l)| *l).unwrap_or(0);

        let total: u64 = pure
            .iter()
            .chain(asset_inputs.iter())
            .map(|u| u.lovelace())
            .sum();
        let input_refs: Vec<(String, u32)> = pure
            .iter()
            .chain(asset_inputs.iter())
            .map(|u| (u.tx_hash().to_string(), u.output_index()))
            .collect();
        let metadata_bytes = encode_metadata(&self.metadata)?;
        let min_pure_utxo = self.params.min_pure_utxo();
        let fee_estimate = estimate_simple_fee(&self.params)
            + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64)
            + u64::from(asset_home.is_some()) * 5_000;
        let network_id = self.network_id;
        let params = self.params;
        let (valid_from, valid_until) = (self.valid_from, self.valid_until);

        converge_fee_with_witnesses(
            move |fee| {
                let mut tx = StagingTransaction::new();
                for (h, ix) in &input_refs {
                    tx = add_input_ref(tx, h, *ix)?;
                }
                let out = total
                    .checked_sub(asset_home_lovelace)
                    .and_then(|v| v.checked_sub(fee))
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: asset_home_lovelace + fee,
                        available: total,
                    })?;
                if out < min_pure_utxo {
                    return Err(TxBuildError::BuildFailed(format!(
                        "TxPlan sweep: output {out} < min_pure_utxo {min_pure_utxo} after fee"
                    )));
                }
                tx = tx.output(create_ada_output(target.clone(), out));
                if let Some((home, _)) = &asset_home {
                    tx = tx.output(home.clone());
                }
                if let Some(bytes) = &metadata_bytes {
                    tx = tx.add_auxiliary_data(bytes.clone());
                }
                tx = apply_validity(tx, valid_from, valid_until);
                Ok(tx.fee(fee).network_id(network_id))
            },
            fee_estimate,
            &params,
            self.witnesses,
        )
    }
}

// ── shared internals ─────────────────────────────────────────────────────

/// Map a selection failure to the builder error surface.
fn map_select_err(e: SelectError) -> TxBuildError {
    match e {
        SelectError::Insufficient { target, available } => TxBuildError::InsufficientFunds {
            needed: target,
            available,
        },
        SelectError::DuplicateMustSpend {
            tx_hash,
            output_index,
        } => TxBuildError::BuildFailed(format!(
            "duplicate must-spend input {tx_hash}#{output_index}"
        )),
    }
}

/// Duplicate-input guard for the paths that don't go through `select()`.
fn check_no_duplicate_inputs<U: Selectable>(inputs: &[&U]) -> Result<(), TxBuildError> {
    let mut seen: HashSet<(&str, u32)> = HashSet::with_capacity(inputs.len());
    for u in inputs {
        if !seen.insert((u.tx_hash(), u.output_index())) {
            return Err(TxBuildError::BuildFailed(format!(
                "duplicate must-spend input {}#{}",
                u.tx_hash(),
                u.output_index()
            )));
        }
    }
    Ok(())
}

/// Aggregate the native assets across `inputs`, keyed by concatenated asset id
/// (deterministic order). Errors when an input claims `has_assets` but its
/// [`Selectable`] impl provides no detail — such an input cannot be
/// value-balanced, and silently dropping its assets would surface as
/// `ValueNotConservedUTxO` at submit, after the build work.
fn aggregate_input_assets<U: Selectable>(
    inputs: &[&U],
) -> Result<BTreeMap<String, AssetAmount>, TxBuildError> {
    let mut out: BTreeMap<String, AssetAmount> = BTreeMap::new();
    for u in inputs {
        let assets = u.assets();
        if u.has_assets() && assets.is_empty() {
            return Err(TxBuildError::BuildFailed(format!(
                "input {}#{} is asset-bearing but provides no asset detail — cannot \
                 value-balance it (override Selectable::assets, or use a UtxoApi pool)",
                u.tx_hash(),
                u.output_index()
            )));
        }
        for a in assets {
            let entry = out
                .entry(a.asset_id.concatenated())
                .or_insert_with(|| (a.asset_id.clone(), 0));
            entry.1 = entry.1.saturating_add(a.quantity);
        }
    }
    Ok(out)
}

/// The min-UTxO lovelace for an output carrying `ids` (no datum).
fn min_ada_for_assets(params: &TxBuildParams, ids: &[AssetId]) -> u64 {
    crate::calculate_min_ada_with_params(
        &crate::builder::send::to_maestro_params(params),
        ids,
        &crate::OutputParams { datum_size: None },
    )
}

/// Build one native-asset output at `lovelace`.
fn build_asset_output(
    addr: Address,
    lovelace: u64,
    assets: &[AssetAmount],
) -> Result<Output, TxBuildError> {
    let triples: Vec<(&str, &str, u64)> = assets
        .iter()
        .map(|(id, qty)| (id.policy_id(), id.asset_name_hex(), *qty))
        .collect();
    add_assets_to_output(create_ada_output(addr, lovelace), &triples)
}

/// Encode the optional metadata once (the converge closure re-attaches bytes).
fn encode_metadata(md: &Option<serde_json::Value>) -> Result<Option<Vec<u8>>, TxBuildError> {
    match md {
        Some(v) => Ok(Some(
            crate::metadata::cip25::build_metadata_auxiliary_data(v)
                .map_err(|e| TxBuildError::BuildFailed(format!("metadata encoding failed: {e}")))?,
        )),
        None => Ok(None),
    }
}

/// Apply the validity interval to a staging tx.
fn apply_validity(
    mut tx: StagingTransaction,
    valid_from: Option<u64>,
    valid_until: Option<u64>,
) -> StagingTransaction {
    if let Some(slot) = valid_from {
        tx = tx.valid_from_slot(slot);
    }
    if let Some(slot) = valid_until {
        tx = tx.invalid_from_slot(slot);
    }
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::{AssetQuantity, UtxoApi};

    fn params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155_381,
            coins_per_utxo_byte: 4_310,
            max_tx_size: 16_384,
            max_value_size: 5_000,
            ..Default::default()
        }
    }

    fn addr() -> Address {
        Address::from_bech32(
            "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp",
        )
        .unwrap()
    }

    fn ada(h: &str, ix: u32, lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: h.repeat(64 / h.len().max(1)).chars().take(64).collect(),
            output_index: ix,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn nft(h: &str, ix: u32, lovelace: u64, policy: &str, name_hex: &str, qty: u64) -> UtxoApi {
        let mut u = ada(h, ix, lovelace);
        u.assets.push(AssetQuantity {
            asset_id: AssetId::new_unchecked(policy.repeat(56 / policy.len()), name_hex.into()),
            quantity: qty,
        });
        u
    }

    const POLICY_A: &str = "ab";
    const NAME_1: &str = "0a0b";

    /// Σ inputs (lovelace + per-asset) must equal Σ outputs + fee — THE invariant
    /// every TxPlan build mode must hold. Inputs are looked up from `world` by ref.
    fn assert_balanced(unsigned: &UnsignedTx, world: &[UtxoApi]) {
        let inputs: Vec<&UtxoApi> = unsigned
            .staging
            .inputs
            .iter()
            .flatten()
            .map(|i| {
                let h = hex::encode(i.tx_hash.0);
                world
                    .iter()
                    .find(|u| u.tx_hash == h && u.output_index as u64 == i.txo_index)
                    .expect("input not in world")
            })
            .collect();
        let in_lovelace: u64 = inputs.iter().map(|u| u.lovelace).sum();
        let out_lovelace: u64 = unsigned
            .staging
            .outputs
            .iter()
            .flatten()
            .map(|o| o.lovelace)
            .sum();
        assert_eq!(
            in_lovelace,
            out_lovelace + unsigned.fee,
            "lovelace imbalance: in={in_lovelace} out={out_lovelace} fee={}",
            unsigned.fee
        );

        // Per-asset balance.
        let mut in_assets: BTreeMap<String, u64> = BTreeMap::new();
        for u in &inputs {
            for a in &u.assets {
                *in_assets.entry(a.asset_id.concatenated()).or_default() += a.quantity;
            }
        }
        let mut out_assets: BTreeMap<String, u64> = BTreeMap::new();
        for o in unsigned.staging.outputs.iter().flatten() {
            if let Some(assets) = &o.assets {
                for (policy, names) in assets.iter() {
                    for (name, qty) in names {
                        let key = format!("{}{}", hex::encode(policy.0), hex::encode(&name.0));
                        *out_assets.entry(key).or_default() += qty;
                    }
                }
            }
        }
        assert_eq!(in_assets, out_assets, "asset imbalance");
    }

    #[test]
    fn pure_pay_balances() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let unsigned = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .pay_to(addr(), 5_000_000)
            .build()
            .unwrap();
        assert_balanced(&unsigned, &pool);
    }

    #[test]
    fn extra_witness_raises_fee() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let one = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .pay_to(addr(), 5_000_000)
            .build()
            .unwrap();
        let two = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .pay_to(addr(), 5_000_000)
            .witnesses(2)
            .build()
            .unwrap();
        let delta = two.fee - one.fee;
        assert!(
            (4_000..=5_000).contains(&delta),
            "second witness should add ~one vkey of fee, got {delta}"
        );
        assert_balanced(&two, &pool);
    }

    #[test]
    fn valid_until_sets_ttl() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let unsigned = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .pay_to(addr(), 5_000_000)
            .valid_until(123_456)
            .build()
            .unwrap();
        assert_eq!(unsigned.staging.invalid_from_slot, Some(123_456));
        let sweep = TxPlan::new(addr(), 0, params())
            .must_spend(pool.iter())
            .sweep_to(addr())
            .valid_until(99)
            .build()
            .unwrap();
        assert_eq!(sweep.staging.invalid_from_slot, Some(99));
    }

    #[test]
    fn duplicate_must_spend_rejected() {
        let u = ada("aa", 0, 50_000_000);
        let err = TxPlan::new(addr(), 0, params())
            .must_spend([&u, &u])
            .pay_to(addr(), 5_000_000)
            .build()
            .unwrap_err();
        assert!(format!("{err}").contains("duplicate"), "{err}");
        let err = TxPlan::new(addr(), 0, params())
            .must_spend([&u, &u])
            .sweep_to(addr())
            .build()
            .unwrap_err();
        assert!(format!("{err}").contains("duplicate"), "{err}");
    }

    #[test]
    fn asset_send_auto_selects_and_balances() {
        // Pool: a fee/float UTxO + an NFT holding (qty 3, sending 2 → residual 1).
        let world = vec![
            ada("aa", 0, 20_000_000),
            nft("bb", 1, 1_400_000, POLICY_A, NAME_1, 3),
        ];
        let id = AssetId::new_unchecked(POLICY_A.repeat(28), NAME_1.into());
        let unsigned = TxPlan::new(addr(), 0, params())
            .select_from(&world, Strategy::SmallestSufficient)
            .send_assets_to(addr(), [(id, 2)])
            .build()
            .unwrap();
        assert_balanced(&unsigned, &world);
        // Outputs: asset delivery + asset change (residual 1) + pure change.
        let n_asset_outputs = unsigned
            .staging
            .outputs
            .iter()
            .flatten()
            .filter(|o| o.assets.is_some())
            .count();
        assert_eq!(n_asset_outputs, 2, "delivery + residual change");
    }

    #[test]
    fn asset_send_insufficient_quantity_errors() {
        let world = vec![
            ada("aa", 0, 20_000_000),
            nft("bb", 1, 1_400_000, POLICY_A, NAME_1, 1),
        ];
        let id = AssetId::new_unchecked(POLICY_A.repeat(28), NAME_1.into());
        let err = TxPlan::new(addr(), 0, params())
            .select_from(&world, Strategy::SmallestSufficient)
            .send_assets_to(addr(), [(id, 5)])
            .build()
            .unwrap_err();
        assert!(matches!(err, TxBuildError::AssetNotFound(_)), "{err}");
    }

    #[test]
    fn sweep_rehome_keeps_assets_and_balances() {
        let world = vec![
            ada("aa", 0, 30_000_000),
            nft("bb", 1, 5_000_000, POLICY_A, NAME_1, 2),
        ];
        let unsigned = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .rehome_assets()
            .build()
            .unwrap();
        assert_balanced(&unsigned, &world);
        // The asset home output exists and the sweep recovered the NFT UTxO's
        // excess ADA (sweep output > the pure input alone − fee).
        let outs: Vec<_> = unsigned.staging.outputs.iter().flatten().collect();
        assert_eq!(outs.len(), 2);
        assert!(outs.iter().any(|o| o.assets.is_some()));
        let sweep_out = outs.iter().find(|o| o.assets.is_none()).unwrap();
        assert!(sweep_out.lovelace > 30_000_000 - unsigned.fee);
    }

    #[test]
    fn sweep_without_rehome_skips_asset_inputs() {
        let world = vec![
            ada("aa", 0, 30_000_000),
            nft("bb", 1, 5_000_000, POLICY_A, NAME_1, 2),
        ];
        let unsigned = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .build()
            .unwrap();
        // Only the pure input is spent; the NFT UTxO is untouched.
        assert_eq!(unsigned.staging.inputs.iter().flatten().count(), 1);
        assert_balanced(&unsigned, &world);
    }

    #[test]
    fn fold_change_rejects_asset_outputs() {
        let world = vec![ada("aa", 0, 30_000_000)];
        let id = AssetId::new_unchecked(POLICY_A.repeat(28), NAME_1.into());
        let err = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .send_assets_to(addr(), [(id, 1)])
            .fold_change()
            .build()
            .unwrap_err();
        assert!(format!("{err}").contains("pure-ADA only"), "{err}");
    }
}
