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

use pallas_addresses::Address;
use pallas_txbuilder::{Output, ScriptKind, StagingTransaction};
use std::collections::{BTreeMap, HashSet};

use crate::builder::{UnsignedTx, converge_fee_with_witnesses};
use crate::error::TxBuildError;
use crate::helpers::input::add_input_ref;
use crate::helpers::output::{add_assets_to_output, create_ada_output};
use crate::params::TxBuildParams;
use crate::select::{SelectError, Selectable, Selection, Strategy, select};
use crate::selection::{PER_INPUT_FEE_HEADROOM, estimate_simple_fee};

/// Lovelace headroom kept as change so a selected build never emits a
/// sub-min-UTxO change output; also absorbs per-input fee growth (the converge
/// pass computes the exact fee). Matches `build_send_many`.
const MIN_CHANGE_CUSHION: u64 = 1_500_000;

/// One requested native-asset transfer: `(asset, quantity)`.
///
/// Same type the min-UTxO calculation takes — the quantity travels with the
/// asset all the way to the size estimate, because it is encoded in the output.
pub use crate::utxo::AssetAmount;

/// One CIP-33 reference-script output being created.
struct ScriptOutput {
    address: Address,
    language: ScriptKind,
    script_bytes: Vec<u8>,
    /// An optional inline datum, so the UTxO can say what it holds. The script
    /// hash alone cannot; a depot is the thing you come back to much later.
    datum: Option<Vec<u8>>,
}

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
    /// CIP-33 reference-script outputs. Lovelace is the computed min-UTxO for
    /// the script (and label, when present), like `asset_outputs`.
    script_outputs: Vec<ScriptOutput>,
    metadata: Option<serde_json::Value>,
    /// A native script attached to the witness set, satisfying inputs that sit
    /// at its address (a script depot being retired).
    native_script: Option<Vec<u8>>,
    sweep_to: Option<Address>,
    rehome_assets: bool,
    spend_script_refs: bool,
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
            script_outputs: Vec::new(),
            metadata: None,
            native_script: None,
            sweep_to: None,
            rehome_assets: false,
            spend_script_refs: false,
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

    /// Park a validator on chain as a CIP-33 reference script in an output at
    /// `addr`, so later transactions can reference it instead of carrying the
    /// script bytes in their witness set. The output's lovelace is the
    /// computed min-UTxO for the script's size (a ledger rule, not a knob —
    /// about 8 ADA for a 1.5 KB Plutus V2 validator).
    ///
    /// Put it at the validator's OWN address with no datum and it can never
    /// be spent: a spend would need a datum the output does not have. That is
    /// what a permanent reference wants, and it is how jpg.store parks theirs.
    pub fn deploy_script_to(
        self,
        addr: Address,
        language: ScriptKind,
        script_bytes: Vec<u8>,
    ) -> Self {
        self.deploy_labelled_script_to(addr, language, script_bytes, None)
    }

    /// [`TxPlan::deploy_script_to`] with an optional inline datum describing
    /// what the script IS.
    ///
    /// A script hash cannot tell you which of your validators it is, and a
    /// depot of reference scripts is something you come back to months later.
    /// The datum rides in the output bytes, so it survives in any UTxO query;
    /// transaction metadata would not, being attached to the transaction
    /// rather than the output. It costs a little min-UTxO, which the
    /// calculation below accounts for.
    ///
    /// Only ever park a datum at an address whose script IGNORES datums (a
    /// native script) or at one you never intend to spend. At a Plutus
    /// validator's own address a datum is what makes the output spendable at
    /// all, so adding one there would undo the permanence that parking it
    /// there was for.
    pub fn deploy_labelled_script_to(
        mut self,
        addr: Address,
        language: ScriptKind,
        script_bytes: Vec<u8>,
        datum: Option<Vec<u8>>,
    ) -> Self {
        self.script_outputs.push(ScriptOutput {
            address: addr,
            language,
            script_bytes,
            datum,
        });
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

    /// Attach a NATIVE script to the witness set, so inputs sitting at that
    /// script's address can be spent. Native scripts take no datum, no
    /// redeemer and no execution budget — the ledger evaluates the script
    /// directly — so this plus the required signatures is the whole witness.
    ///
    /// Used to retire a script depot ([`crate::depot`]): the reference-script
    /// UTxOs sit at a native-script address, and spending them reclaims the
    /// ADA they lock. Pair it with [`TxPlan::spend_script_refs`], because the
    /// sweep path skips reference-script inputs by default.
    pub fn native_script(mut self, script_bytes: Vec<u8>) -> Self {
        self.native_script = Some(script_bytes);
        self
    }

    /// SWEEP modifier: also spend the `must_spend` inputs that CARRY a
    /// reference script — the one case where destroying a reference is the
    /// point rather than an accident.
    ///
    /// Every other path refuses these deliberately: spending a reference UTxO
    /// breaks every transaction built to reference it, so it must be asked for
    /// by name and never fall out of ordinary coin selection. Retire the
    /// deployment record BEFORE the UTxO, or transactions in flight fail with
    /// no diagnosis.
    ///
    /// # The size is not optional
    ///
    /// `total_ref_script_bytes` is the summed serialised size of every
    /// reference script among those inputs, as the CHAIN reports it (Koios's
    /// `reference_script.size`). Conway charges
    /// `minFeeRefScriptCoinsPerByte` for reference scripts a transaction makes
    /// available, and **a spent input's script counts** just as a reference
    /// input's does.
    ///
    /// It is a parameter rather than a separate setter because omitting it is
    /// not a smaller mistake — it is `FeeTooSmallUTxO` at submit, after every
    /// other check has passed. `evaluateTransaction` does not look at this, so
    /// no dry run catches it. Measured on preprod: retiring one 1534-byte
    /// validator was rejected for underpaying by 22,965 lovelace.
    ///
    /// Passing the chain's figure slightly OVER-pays, because the ledger counts
    /// the unwrapped program while the chain reports the CBOR-wrapped bytes
    /// (three bytes here, 45 lovelace). Over-paying is always accepted;
    /// under-paying never is. The buy path makes the same trade.
    pub fn spend_script_refs(mut self, total_ref_script_bytes: u64) -> Self {
        self.spend_script_refs = true;
        // Added, not assigned: a caller may already have declared reference
        // INPUTS in `params`, and both kinds are charged.
        self.params.ref_script_size = self
            .params
            .ref_script_size
            .saturating_add(total_ref_script_bytes);
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
        // Sweep-only modifiers are checked BEFORE the fold branch: a modifier
        // a build mode ignores must fail loudly, never be silently dropped.
        if self.rehome_assets {
            return Err(TxBuildError::BuildFailed(
                "TxPlan: rehome_assets is a sweep modifier — use sweep_to".into(),
            ));
        }
        if self.spend_script_refs {
            return Err(TxBuildError::BuildFailed(
                "TxPlan: spend_script_refs is a sweep modifier — use sweep_to".into(),
            ));
        }
        if self.fold_change {
            if !self.asset_outputs.is_empty() || !self.script_outputs.is_empty() {
                return Err(TxBuildError::BuildFailed(
                    "TxPlan: fold_change is pure-ADA only (no asset or script outputs)".into(),
                ));
            }
            if self.native_script.is_some() {
                return Err(TxBuildError::BuildFailed(
                    "TxPlan: fold_change does not carry a native script witness".into(),
                ));
            }
            return self.build_fold();
        }
        if self.outputs.is_empty()
            && self.asset_outputs.is_empty()
            && self.script_outputs.is_empty()
        {
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
        // Each recipient's assets may need SEVERAL outputs: the ledger caps an
        // output's value at `maxValueSize`, so a wallet holding hundreds of
        // assets cannot be emptied into one.
        let mut asset_outs: Vec<(Output, u64)> = Vec::new();
        for (addr, assets) in &self.asset_outputs {
            for bundle in split_for_outputs(&self.params, assets) {
                let lovelace = min_ada_for_assets(&self.params, &bundle);
                asset_outs.push((
                    build_asset_output(addr.clone(), lovelace, &bundle)?,
                    lovelace,
                ));
            }
        }
        let mut asset_change: Vec<(Output, u64)> = Vec::new();
        for bundle in split_for_outputs(&self.params, &residual) {
            let lovelace = min_ada_for_assets(&self.params, &bundle);
            asset_change.push((
                build_asset_output(self.change_address.clone(), lovelace, &bundle)?,
                lovelace,
            ));
        }

        // Reference-script outputs, min-ADA sized for the script bytes. The
        // bytes also ride in the tx body, so they count toward the fee below.
        let mut script_outs: Vec<(Output, u64)> = Vec::new();
        let mut script_bytes_total: u64 = 0;
        for out in &self.script_outputs {
            // The label is part of the output, so it is part of the floor.
            let lovelace = crate::utxo::min_ada_with_coefficient(
                self.params.coins_per_utxo_byte,
                &[],
                &crate::OutputParams {
                    datum_size: out.datum.as_ref().map(|d| d.len()),
                    script_ref_size: Some(out.script_bytes.len()),
                },
            );
            script_bytes_total +=
                out.script_bytes.len() as u64 + out.datum.as_ref().map_or(0, |d| d.len() as u64);
            let mut output = create_ada_output(out.address.clone(), lovelace)
                .set_inline_script(out.language, out.script_bytes.clone());
            if let Some(datum) = &out.datum {
                output = output.set_inline_datum(datum.clone());
            }
            script_outs.push((output, lovelace));
        }

        let total_pure_outputs: u64 = self.outputs.iter().map(|(_, l)| *l).sum();
        let total_asset_lovelace: u64 = asset_outs.iter().map(|(_, l)| *l).sum::<u64>()
            + asset_change.iter().map(|(_, l)| *l).sum::<u64>()
            + script_outs.iter().map(|(_, l)| *l).sum::<u64>();
        // Target estimate only (the converged fee is exact): base + metadata +
        // script bytes + a rough per-asset-output weight + per-input headroom
        // for the inputs already committed.
        let fee_estimate = estimate_simple_fee(&self.params)
            + metadata_bytes.as_ref().map_or(0, |b| b.len() as u64)
            + script_bytes_total
            + self.native_script.as_ref().map_or(0, |s| s.len() as u64)
            + (asset_outs.len() + asset_change.len() + script_outs.len()) as u64 * 5_000
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
        let native_script = self.native_script;
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
                for (out, _) in asset_outs.iter().chain(&asset_change).chain(&script_outs) {
                    tx = tx.output(out.clone());
                }
                if let Some(bytes) = &metadata_bytes {
                    tx = tx.add_auxiliary_data(bytes.clone());
                }
                if let Some(script) = &native_script {
                    tx = tx.script(ScriptKind::Native, script.clone());
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
        if !self.script_outputs.is_empty() {
            return Err(TxBuildError::BuildFailed(
                "TxPlan sweep: deploy_script_to is not a sweep modifier — build a plain plan"
                    .into(),
            ));
        }
        check_no_duplicate_inputs(&self.must_spend)?;
        // A reference-script input is skipped unless it was asked for by name:
        // spending one breaks every transaction built to reference it, so it
        // must never fall out of ordinary sweeping.
        let spend_refs = self.spend_script_refs;
        let eligible = |u: &&'a U| spend_refs || !u.has_script_ref();
        let pure: Vec<&'a U> = self
            .must_spend
            .iter()
            .copied()
            .filter(|u| !u.has_assets())
            .filter(eligible)
            .collect();
        let asset_inputs: Vec<&'a U> = if self.rehome_assets {
            self.must_spend
                .iter()
                .copied()
                .filter(|u| u.has_assets())
                .filter(eligible)
                .collect()
        } else {
            Vec::new()
        };
        if pure.is_empty() && asset_inputs.is_empty() {
            return Err(TxBuildError::BuildFailed(
                "TxPlan sweep: no spendable inputs to sweep".into(),
            ));
        }

        // Asset re-home outputs (errors on an opaque asset input — we will not
        // build a tx that drops assets). Split across as many outputs as the
        // `maxValueSize` cap requires: a wallet with hundreds of assets has more
        // value than one output can legally carry.
        let rehomed = aggregate_input_assets(&asset_inputs)?;
        let bundle: Vec<AssetAmount> = rehomed.values().cloned().collect();
        let mut asset_home: Vec<(Output, u64)> = Vec::new();
        for chunk in split_for_outputs(&self.params, &bundle) {
            let lovelace = min_ada_for_assets(&self.params, &chunk);
            asset_home.push((
                build_asset_output(self.change_address.clone(), lovelace, &chunk)?,
                lovelace,
            ));
        }
        let asset_home_lovelace: u64 = asset_home.iter().map(|(_, l)| *l).sum();

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
            + self.native_script.as_ref().map_or(0, |s| s.len() as u64)
            + asset_home.len() as u64 * 5_000;
        let network_id = self.network_id;
        let params = self.params;
        let (valid_from, valid_until) = (self.valid_from, self.valid_until);
        let native_script = self.native_script;

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
                for (home, _) in &asset_home {
                    tx = tx.output(home.clone());
                }
                if let Some(bytes) = &metadata_bytes {
                    tx = tx.add_auxiliary_data(bytes.clone());
                }
                if let Some(script) = &native_script {
                    tx = tx.script(ScriptKind::Native, script.clone());
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

/// Split `assets` into per-output bundles that respect `maxValueSize`.
///
/// One output can only legally carry ~5000 bytes of value, so emptying a wallet
/// with hundreds of assets needs several. Returns an empty vec for no assets, so
/// callers can loop unconditionally.
fn split_for_outputs(params: &TxBuildParams, assets: &[AssetAmount]) -> Vec<Vec<AssetAmount>> {
    crate::utxo::split_by_value_size(assets, params.max_value_size)
}

/// The min-UTxO lovelace for an output carrying `assets` (no datum).
///
/// Takes the quantities, not just the ids: they are encoded inline in the output
/// and a fungible balance can be several bytes wider than an NFT's `01`.
fn min_ada_for_assets(params: &TxBuildParams, assets: &[AssetAmount]) -> u64 {
    crate::calculate_min_ada_with_params(
        &crate::builder::send::to_maestro_params(params),
        assets,
        &crate::OutputParams::default(),
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
    use cardano_assets::{AssetId, AssetQuantity, UtxoApi};

    fn params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155_381,
            coins_per_utxo_byte: 4_310,
            max_tx_size: 16_384,
            max_value_size: 5_000,
            // Stated, not defaulted: `Default` leaves this ZERO, which prices
            // every reference script at nothing and would let a test claiming
            // to exercise the reference-script fee pass while proving nothing.
            min_fee_ref_script_cost_per_byte:
                crate::params::CONWAY_MIN_FEE_REF_SCRIPT_COST_PER_BYTE,
            ..Default::default()
        }
    }

    /// One real observation, kept only as the origin of the rule below.
    ///
    /// Retiring the abandonware `ask.spend` validator out of a preprod depot on
    /// 2026-09-12 was rejected `FeeTooSmallUTxO`: supplied 167,086, expected
    /// 190,051. The chain reported that script as 1,534 bytes, and the missing
    /// 22,965 lovelace is the unwrapped program — the wrapped size less its
    /// three-byte CBOR header — at 15 lovelace each.
    ///
    /// Nothing in the system is 1,534 bytes long. Another validator is another
    /// size, so the tests below exercise the RELATIONSHIP across sizes rather
    /// than this figure, and production reads each script's size from the
    /// chain.
    const OBSERVED: (u64, u64) = (1_534, 22_965);

    /// An arbitrary plausible script size, for the tests that only need SOME
    /// reference script to be in play. Any value would do; nothing asserts
    /// against it.
    const SOME_SCRIPT_BYTES: u64 = 1_200;

    /// A REAL depot native script, not a hand-written stub.
    ///
    /// This matters more than it looks. A malformed script makes the staging
    /// transaction fail to serialise, and the fee calculation then falls back
    /// to a size estimate — so a test built on a stub silently measures the
    /// fallback path instead of the real one, and the fee assertions it makes
    /// are about nothing. That is exactly how the first version of the
    /// reference-script test passed its rate check while reporting a zero
    /// charge.
    fn depot_script() -> Vec<u8> {
        crate::depot::Depot::from_signers(&[
            "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29",
        ])
        .expect("a one-signer depot")
        .script_bytes()
        .to_vec()
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

    /// A reference-script deployment: the script output carries the bytes,
    /// is sized for them (the script dominates its min-UTxO), and the build
    /// still balances. A 1.5 KB validator wants roughly 8 ADA at 4310/byte.
    #[test]
    fn deploy_script_output_carries_script_and_balances() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let script_bytes = vec![0x59, 0x05, 0xfb]
            .into_iter()
            .chain(std::iter::repeat_n(0xabu8, 1531))
            .collect::<Vec<u8>>();
        let unsigned = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .deploy_script_to(addr(), ScriptKind::PlutusV2, script_bytes.clone())
            .build()
            .unwrap();
        assert_balanced(&unsigned, &pool);

        let outputs: Vec<_> = unsigned.staging.outputs.iter().flatten().collect();
        let script_out = outputs
            .iter()
            .find(|o| o.script.is_some())
            .expect("one output carries the script");
        assert_eq!(
            script_out.script.as_ref().unwrap().bytes.as_ref() as &[u8],
            &script_bytes[..],
            "the output carries exactly the bytes handed in"
        );
        let expected_min = crate::utxo::min_ada_with_coefficient(
            4_310,
            &[],
            &crate::OutputParams::with_script_ref(&script_bytes),
        );
        assert_eq!(
            script_out.lovelace, expected_min,
            "sized by the script's min-UTxO"
        );
        assert!(
            (7_000_000..=9_000_000).contains(&script_out.lovelace),
            "a 1.5 KB script locks ~8 ADA, got {}",
            script_out.lovelace
        );
        // The script bytes ride in the body, so the fee reflects them.
        assert!(
            unsigned.fee > 155_381 + 44 * 1_534,
            "fee must cover the script bytes, got {}",
            unsigned.fee
        );
    }

    /// Reference scripts are only a plain-build feature; the other modes say so.
    #[test]
    fn deploy_script_is_refused_by_sweep_and_fold() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let sweep = TxPlan::new(addr(), 0, params())
            .must_spend(&pool)
            .deploy_script_to(addr(), ScriptKind::PlutusV2, vec![0u8; 100])
            .sweep_to(addr())
            .build();
        assert!(sweep.is_err(), "sweep must refuse a script output");
        let fold = TxPlan::new(addr(), 0, params())
            .must_spend(&pool)
            .pay_to(addr(), 5_000_000)
            .deploy_script_to(addr(), ScriptKind::PlutusV2, vec![0u8; 100])
            .fold_change()
            .build();
        assert!(fold.is_err(), "fold must refuse a script output");
    }

    /// A UTxO carrying a reference script.
    fn script_ref_utxo(h: &str, ix: u32, lovelace: u64) -> UtxoApi {
        let mut u = ada(h, ix, lovelace);
        u.tags.push(cardano_assets::UtxoTag::HasScriptRef);
        u
    }

    /// RETIRING A DEPOT: sweeping reference-script UTxOs back to a wallet,
    /// with the depot's native script as the witness. This is the only path
    /// that may destroy a reference script, and it must be asked for by name.
    #[test]
    fn sweep_retires_script_refs_only_when_asked() {
        let world = vec![script_ref_utxo("aa", 0, 8_000_000)];

        // Default: the reference UTxO is skipped, so there is nothing to sweep.
        let err = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .build()
            .unwrap_err();
        assert!(
            format!("{err}").contains("no spendable inputs"),
            "a reference script must never be swept by accident, got {err}"
        );

        // Asked for by name, with the depot's native script attached.
        let native = vec![0x82, 0x00, 0x58, 0x1c];
        let unsigned = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .spend_script_refs(SOME_SCRIPT_BYTES)
            .native_script(native.clone())
            .build()
            .expect("retire must build");
        assert_balanced(&unsigned, &world);
        assert_eq!(unsigned.staging.inputs.iter().flatten().count(), 1);

        // The native script rides in the witness set, keyed by its 0x00-tagged
        // hash — without it the ledger cannot check who may spend the depot.
        let scripts = unsigned.staging.scripts.as_ref().expect("witness scripts");
        assert_eq!(scripts.len(), 1);
        let entry = scripts.values().next().unwrap();
        assert!(matches!(entry.kind, ScriptKind::Native));
        assert_eq!(entry.bytes.as_ref() as &[u8], &native[..]);
    }

    /// The ADA a depot locks comes back, less the fee. That recovery is the
    /// whole reason a depot is spendable rather than parked permanently.
    #[test]
    fn retiring_several_depot_utxos_returns_their_ada() {
        let world = vec![
            script_ref_utxo("aa", 0, 8_000_000),
            script_ref_utxo("bb", 1, 9_500_000),
        ];
        let unsigned = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .spend_script_refs(SOME_SCRIPT_BYTES * 2)
            .native_script(depot_script())
            .build()
            .unwrap();
        assert_balanced(&unsigned, &world);
        let outs: Vec<_> = unsigned.staging.outputs.iter().flatten().collect();
        assert_eq!(outs.len(), 1, "one output: everything back to the wallet");
        assert_eq!(outs[0].lovelace, 17_500_000 - unsigned.fee);
    }

    /// REGRESSION, from a real preprod rejection. Conway charges
    /// `minFeeRefScriptCoinsPerByte` for reference scripts a transaction makes
    /// available, and a SPENT input's script counts — not just a reference
    /// input's. Retiring one 1534-byte validator was rejected for underpaying
    /// by exactly 22,965 lovelace, which is 1531 × 15.
    ///
    /// Nothing catches this before submit: `evaluateTransaction` does not look
    /// at the reference-script fee, so the build, the review and every dry run
    /// all pass and the node rejects `FeeTooSmallUTxO`.
    #[test]
    fn retiring_pays_conways_reference_script_fee() {
        let world = vec![script_ref_utxo("aa", 0, 8_000_000)];
        let rate = params().min_fee_ref_script_cost_per_byte;
        assert!(rate > 0, "a zero rate would make this test prove nothing");

        let fee_for = |declared: u64| {
            let tx = TxPlan::new(addr(), 0, params())
                .must_spend(world.iter())
                .sweep_to(addr())
                .spend_script_refs(declared)
                .native_script(depot_script())
                .build()
                .unwrap();
            assert_balanced(&tx, &world);
            tx.fee
        };

        // THE RULE: whatever size is declared, the fee rises by exactly that
        // many bytes at the protocol's rate. Checked across a spread so this
        // pins a relationship rather than one validator's dimensions.
        let baseline = fee_for(0);
        for bytes in [1, 500, 1_534, 4_000, 25_600] {
            assert_eq!(
                fee_for(bytes) - baseline,
                bytes * rate,
                "declaring {bytes} reference-script bytes must add {bytes} × {rate}"
            );
        }

        // And the rule, applied to the one case measured on chain, covers what
        // the node actually asked for. Over by the CBOR header is what passing
        // the chain's reported size costs; under is the rejection.
        let (observed_bytes, observed_shortfall) = OBSERVED;
        assert!(
            fee_for(observed_bytes) - baseline >= observed_shortfall,
            "the rule must cover the observed preprod shortfall"
        );
    }

    /// A modifier a build mode ignores must fail, not be silently dropped —
    /// a retire that quietly became an ordinary build would leave the depot
    /// untouched while reporting success.
    #[test]
    fn script_ref_modifiers_are_refused_outside_sweep() {
        let pool = vec![ada("aa", 0, 50_000_000)];
        let plain = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .pay_to(addr(), 5_000_000)
            .spend_script_refs(SOME_SCRIPT_BYTES)
            .build();
        assert!(plain.is_err(), "plain build must refuse spend_script_refs");

        let fold = TxPlan::new(addr(), 0, params())
            .must_spend(pool.iter())
            .pay_to(addr(), 5_000_000)
            .spend_script_refs(SOME_SCRIPT_BYTES)
            .fold_change()
            .build();
        assert!(fold.is_err(), "fold must refuse spend_script_refs");

        let fold_script = TxPlan::new(addr(), 0, params())
            .must_spend(pool.iter())
            .pay_to(addr(), 5_000_000)
            .native_script(depot_script())
            .fold_change()
            .build();
        assert!(
            fold_script.is_err(),
            "fold must refuse a native script it would drop"
        );
    }

    /// Deploying to a depot is an ordinary build: several scripts in ONE
    /// transaction, each its own output, each sized for its own bytes.
    #[test]
    fn a_deployment_set_parks_every_script_in_one_transaction() {
        let pool = vec![ada("aa", 0, 100_000_000)];
        let small = vec![0xabu8; 400];
        let large = vec![0xcdu8; 1_600];
        let unsigned = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .deploy_script_to(addr(), ScriptKind::PlutusV3, small.clone())
            .deploy_script_to(addr(), ScriptKind::PlutusV3, large.clone())
            .build()
            .expect("a multi-script deployment must build");
        assert_balanced(&unsigned, &pool);

        let script_outs: Vec<_> = unsigned
            .staging
            .outputs
            .iter()
            .flatten()
            .filter(|o| o.script.is_some())
            .collect();
        assert_eq!(script_outs.len(), 2, "one output per distinct script");
        assert!(
            script_outs[1].lovelace > script_outs[0].lovelace,
            "each output is sized for its own script, not a shared figure"
        );
    }

    /// A labelled deployment carries its note as an inline datum, and the
    /// output's min-UTxO GROWS to pay for it. Sizing the output as if the
    /// label were free would emit a sub-minimum output the ledger rejects.
    #[test]
    fn a_label_rides_in_the_output_and_is_paid_for() {
        let pool = vec![ada("aa", 0, 100_000_000)];
        let script = vec![0xabu8; 800];
        let label = vec![0x9fu8; 64];

        let bare = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .deploy_script_to(addr(), ScriptKind::PlutusV3, script.clone())
            .build()
            .unwrap();
        let labelled = TxPlan::new(addr(), 0, params())
            .select_from(&pool, Strategy::SmallestSufficient)
            .deploy_labelled_script_to(
                addr(),
                ScriptKind::PlutusV3,
                script.clone(),
                Some(label.clone()),
            )
            .build()
            .unwrap();
        assert_balanced(&labelled, &pool);

        let script_out = |tx: &UnsignedTx| {
            tx.staging
                .outputs
                .iter()
                .flatten()
                .find(|o| o.script.is_some())
                .cloned()
                .expect("script output")
        };
        let bare_out = script_out(&bare);
        let labelled_out = script_out(&labelled);

        assert!(
            bare_out.datum.is_none(),
            "an unlabelled deployment carries no datum"
        );
        assert!(
            labelled_out.datum.is_some(),
            "the label must reach the output, or it is not on chain at all"
        );
        assert!(
            labelled_out.lovelace > bare_out.lovelace,
            "the label is part of the output, so it raises the min-UTxO floor: \
             bare={} labelled={}",
            bare_out.lovelace,
            labelled_out.lovelace
        );
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

    /// A wallet holding more assets than fit in one output must sweep into
    /// SEVERAL asset outputs. Packing them into one is `OutputTooBigUTxO` — a
    /// permanent rejection that strands the wallet.
    #[test]
    fn sweep_splits_oversized_asset_holdings_across_outputs() {
        // 240 NFTs with 32-byte names across 4 policies: well past 5000 bytes.
        let mut world = vec![ada("aa", 0, 400_000_000)];
        for p in 0..4u32 {
            let policy = format!("{:02x}", 0xa0 + p).repeat(28);
            for i in 0..60u32 {
                let mut u = ada("bb", p * 60 + i + 1, 2_000_000);
                u.assets.push(AssetQuantity {
                    asset_id: AssetId::new_unchecked(policy.clone(), format!("{i:064x}")),
                    quantity: 1,
                });
                world.push(u);
            }
        }

        let unsigned = TxPlan::new(addr(), 0, params())
            .must_spend(world.iter())
            .sweep_to(addr())
            .rehome_assets()
            .build()
            .expect("sweep must build");

        let asset_outputs: Vec<_> = unsigned
            .staging
            .outputs
            .as_ref()
            .expect("outputs")
            .iter()
            .filter(|o| o.assets.as_ref().is_some_and(|a| !a.is_empty()))
            .collect();

        assert!(
            asset_outputs.len() > 1,
            "240 assets must span several outputs, got {}",
            asset_outputs.len()
        );

        // Nothing may be dropped on the way out — this is a wallet being emptied.
        let total_assets: usize = asset_outputs
            .iter()
            .map(|o| {
                o.assets
                    .as_ref()
                    .map(|policies| policies.values().map(|names| names.len()).sum::<usize>())
                    .unwrap_or(0)
            })
            .sum();
        assert_eq!(total_assets, 240, "every asset must reach an output");
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
        let world = [ada("aa", 0, 30_000_000)];
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
