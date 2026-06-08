//! `TxPlan` — a composable v2 transaction builder over pluggable coin selection
//! (`docs/design/CARDANO_TX_BUILDER_V2.md`).
//!
//! Composes inputs (must-spend + a select-from pool with an exclude set), pure-ADA
//! payout outputs, and optional metadata in ONE place, auto-derives the selection
//! target, runs [`crate::select::select`], and emits the SAME [`UnsignedTx`] the
//! existing builders produce — so the existing `build_and_sign[_multi]_tracked`
//! path gives `(SignedTx, TxEffects)` unchanged (the wallet-ledger chaining +
//! 504-recovery keep working with no new index arithmetic).
//!
//! v1 scope = the send / refund family (pure-ADA outputs). Mints + multi-asset
//! selection are later phases; this leaves every current `builder::*` intact.

use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;
use std::collections::HashSet;

use crate::builder::{converge_fee, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::input::add_input_ref;
use crate::helpers::output::create_ada_output;
use crate::params::TxBuildParams;
use crate::select::{select, Selectable, SelectError, Selection, Strategy};
use crate::selection::estimate_simple_fee;

/// Lovelace headroom kept as change so a selected build never emits a sub-min-UTxO
/// change output; also absorbs per-input fee growth (the converge pass computes the
/// exact fee). Matches `build_send_many`.
const MIN_CHANGE_CUSHION: u64 = 1_500_000;

/// A fluent plan for a pure-ADA payout transaction with pluggable input selection.
pub struct TxPlan<'a, U: Selectable> {
    change_address: Address,
    network_id: u8,
    params: TxBuildParams,
    must_spend: Vec<&'a U>,
    pool: &'a [U],
    exclude: HashSet<(String, u32)>,
    strategy: Strategy,
    outputs: Vec<(Address, u64)>,
    metadata: Option<serde_json::Value>,
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
            metadata: None,
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

    /// Attach CIP-25/674 metadata (e.g. the `refund:<order_id>` lines).
    pub fn metadata(mut self, md: serde_json::Value) -> Self {
        self.metadata = Some(md);
        self
    }

    /// Net the target (Σ outputs + fee + change cushion), run selection, assemble
    /// the staging tx (inputs = must_spend ++ selected, outputs, metadata, change →
    /// change_address) and converge the fee. Returns the standard [`UnsignedTx`].
    pub fn build(self) -> Result<UnsignedTx, TxBuildError> {
        if self.outputs.is_empty() {
            return Err(TxBuildError::BuildFailed("TxPlan: no outputs".into()));
        }
        let min_pure_utxo = 228 * self.params.coins_per_utxo_byte;
        for (i, (_, amt)) in self.outputs.iter().enumerate() {
            if *amt < min_pure_utxo {
                return Err(TxBuildError::BuildFailed(format!(
                    "TxPlan: outputs[{i}] = {amt} lovelace < min_pure_utxo {min_pure_utxo}"
                )));
            }
        }
        let metadata_bytes = match &self.metadata {
            Some(v) => Some(
                crate::metadata::cip25::build_metadata_auxiliary_data(v).map_err(|e| {
                    TxBuildError::BuildFailed(format!("metadata encoding failed: {e}"))
                })?,
            ),
            None => None,
        };

        let total_outputs: u64 = self.outputs.iter().map(|(_, l)| *l).sum();
        let fee_estimate = estimate_simple_fee(&self.params)
            + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64);
        let target = total_outputs
            .saturating_add(fee_estimate)
            .saturating_add(MIN_CHANGE_CUSHION);

        let sel = Selection {
            must_spend: self.must_spend,
            pool: self.pool,
            exclude: &self.exclude,
            strategy: self.strategy,
        };
        let chosen = select(&sel, target).map_err(|e| match e {
            SelectError::Insufficient { target, available } => {
                TxBuildError::InsufficientFunds { needed: target, available }
            }
        })?;
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

        converge_fee(
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
                // Change back to self; converge balances the fee around it.
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
                Ok(tx.fee(fee).network_id(network_id))
            },
            fee_estimate,
            &params,
        )
    }
}
