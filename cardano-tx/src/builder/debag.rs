//! Debag — split multi-policy "bag" UTxOs into clean per-policy UTxOs.
//!
//! The defrackit planner + builder. A cluttered wallet packs assets from many
//! policies into a few fat UTxOs; any tx that spends one drags every unrelated
//! asset through as change (and, in a co-signed swap, parades them across the
//! counterparty's hardware wallet). `plan_debag` partitions the wallet's
//! Cluttered/Bloated UTxOs into a short sequence of single-party self-send
//! transactions, and `build_debag` turns one plan item into an unsigned tx.
//!
//! `build_debag` is pool-agnostic: the caller decides whether the pool holds
//! only confirmed UTxOs or also *predicted* change from earlier txs in the
//! sequence (chained UTxO accounting — the body hash is stable under signing,
//! so a change ref is exact before any signature exists; proven in the minting
//! engine). The defrackit worker chains: each built tx's predicted pure-ADA
//! change re-enters the pool for the next build, so one Liquid UTxO can fund a
//! whole sequence. Chained txs must then be signed + submitted strictly in
//! build order.
//!
//! Target shape is per-policy ("promote everything to Clean" — see
//! `cardano_assets::utxo_health`, the same classification the `utxo_shelf`
//! widget renders, so diagnosis and plan agree by construction). A policy whose
//! assets would breach `max_value_size` is chunked across several outputs.

use crate::builder::swap::{estimate_value_size_from_map, min_utxo_for_assets};
use crate::builder::{TxBuildError, UnsignedTx, converge_fee};
use crate::helpers::input::add_utxo_inputs;
use crate::helpers::output::{add_assets_from_map, create_ada_output};
use crate::params::TxBuildParams;
use crate::select::{SelectError, Selection, Strategy, select};
use cardano_assets::AssetId;
use cardano_assets::utxo::UtxoApi;
use cardano_assets::utxo_health::{DUST_THRESHOLD, UtxoTier, classify_utxo, classify_wallet};
use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Flat selection headroom while picking ADA top-up inputs; `converge_fee`
/// computes the exact fee and any surplus returns as change.
const SELECTION_FEE_ESTIMATE: u64 = 1_000_000;

/// Keep planned per-tx output value bytes under this fraction of `max_tx_size`
/// (percent) — leaves room for inputs, witnesses, and body overhead.
const TX_VALUE_BUDGET_PCT: u64 = 60;

/// Use at most this fraction of `max_value_size` per output (percent) — same
/// margin the swap change-splitter uses.
const VALUE_SIZE_MARGIN_PCT: u64 = 90;

/// Cap on bag inputs merged into one planned tx. Inputs are cheap (~40 bytes
/// each); the output value-size budget is the real constraint, so this is a
/// generous backstop — a low cap fragments dust consolidation across txs.
const MAX_BAGS_PER_TX: usize = 40;

/// Lovelace per created collateral UTxO — middle of the Collateral band.
const COLLATERAL_SIZE: u64 = 5_000_000;

/// CIP-674 (`674.msg`) tag stamped on every debag tx. A mitos `tagged-tx`
/// companion holds ONE permanent subscription on this constant string
/// (fee-/address-independent), so the worker learns when each debag tx
/// confirms on-chain without watching ephemeral wallet addresses. Kept in
/// the builder so the worker's companion can subscribe the same value.
pub const DEFRACKIT_TAG: &str = "defrackit/v1";

/// Input-ref marker for pass-2 merge items: `("plan:{item_idx}", group_idx)`
/// refers to pass-1 item `item_idx`'s output `group_idx` (asset groups are a
/// built tx's first outputs, in group order). The caller resolves these to
/// real `(tx_hash, index)` refs after building the parents — body hashes are
/// stable under signing, so the refs are exact before submission.
pub const PLAN_REF_PREFIX: &str = "plan:";

/// How re-emitted assets are distributed across the new outputs — the trade
/// between trade-cleanliness (isolation) and locked-ADA / output count
/// (consolidation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtxoDistributionStrategy {
    /// One output per policy — even a single one-off asset gets its own UTxO.
    /// Spending any policy never drags another; locks the most ADA.
    PolicyIsolation,
    /// Policies holding at least `major_threshold` assets get their own
    /// output(s); smaller holdings gather into shared mixed "drawer" outputs.
    /// Fewer UTxOs and less locked ADA; spending a one-off drags its
    /// drawer-mates through the tx.
    MajorPolicies,
    /// As few outputs as possible, ignoring policy boundaries — minimum locked
    /// ADA. Also consolidates existing single-policy (Clean) UTxOs.
    Compact,
}

/// How a wallet should be optimised — the planner's user-facing knobs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebagOptions {
    /// Ensure the wallet holds this many pure-ADA collateral UTxOs (5 ADA
    /// each). The deficit is created as extra outputs on the first planned tx
    /// (or a standalone tx when the wallet needs no debagging).
    pub collateral_target: u32,
    /// Max assets per output. 0 = unlimited (bounded only by value size);
    /// N > 0 additionally splits any output at N assets.
    pub group_size: u32,
    /// Also sweep Dust-tier UTxOs (asset-bearing, near min-UTxO) into the
    /// plan: their assets consolidate into the new outputs, freeing the
    /// locked ADA — the "consolidate" direction.
    pub sweep_dust: bool,
    /// How the re-emitted assets are laid out across outputs.
    pub strategy: UtxoDistributionStrategy,
    /// [`UtxoDistributionStrategy::MajorPolicies`]: a policy with at least
    /// this many assets earns its own output; smaller holdings share drawers.
    pub major_threshold: u32,
}

impl Default for DebagOptions {
    fn default() -> Self {
        Self {
            collateral_target: 0,
            group_size: 0,
            sweep_dust: false,
            strategy: UtxoDistributionStrategy::PolicyIsolation,
            major_threshold: 4,
        }
    }
}

/// One future output: assets (all one policy unless a huge policy was chunked)
/// plus the estimated min-UTxO lovelace that will ride with them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputGroup {
    pub policy_id: String,
    /// `(asset_id, quantity)` — deterministic order (sorted by asset id).
    pub assets: Vec<(AssetId, u64)>,
    /// Estimated min-UTxO for this output (exact figure recomputed at build).
    pub min_lovelace: u64,
}

/// One planned transaction: spend these bag UTxOs, emit these grouped outputs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebagItem {
    /// Bag UTxO refs this tx spends (confirmed bags; the ADA top-up chosen at
    /// build time may additionally chain an earlier tx's predicted change).
    pub inputs: Vec<(String, u32)>,
    /// Total lovelace riding in those bags.
    pub input_lovelace: u64,
    pub output_groups: Vec<OutputGroup>,
    /// Rough network fee estimate for display (exact fee converges at build).
    pub est_fee: u64,
    /// Estimated min-UTxO locked by the bags today.
    pub locked_before: u64,
    /// Estimated min-UTxO locked by the new outputs.
    pub locked_after: u64,
    /// ADA this tx needs beyond the bags' own lovelace (topped up from Liquid
    /// UTxOs at build time). 0 when the bags self-fund.
    pub ada_shortfall: u64,
    /// Pure-ADA collateral UTxOs (5 ADA each) this tx additionally creates.
    #[serde(default)]
    pub collateral_outputs: u32,
    /// Pass-2 merge tx: spends pass-1 planned outputs (see [`PLAN_REF_PREFIX`]
    /// inputs) to fuse a policy that budget/input caps split across packs —
    /// without this, those outputs would be each other's merge partners and an
    /// immediate re-plan would find work again.
    #[serde(default)]
    pub merge_pass: bool,
}

/// Whole-wallet plan summary for display.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebagSummary {
    pub bag_count: usize,
    pub tx_count: usize,
    pub output_count: usize,
    pub est_total_fee: u64,
    /// Additional min-UTxO ADA locked after the split (recoverable — it sits in
    /// the owner's own wallet).
    pub locked_delta: i64,
    /// Collateral UTxOs the plan creates (0 when the target is already met).
    #[serde(default)]
    pub collateral_created: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebagPlan {
    pub items: Vec<DebagItem>,
    pub summary: DebagSummary,
}

/// Build result: the unsigned tx plus the pure-ADA change it returns (0 when
/// the remainder was folded into the final asset output).
pub struct DebagBuildResult {
    pub unsigned: UnsignedTx,
    pub change_lovelace: u64,
    /// Actual lovelace of each asset-group output, in group order (exact mins
    /// plus any sub-min remainder folded into the last group). Needed by
    /// callers that chain pass-2 merge txs off these predicted outputs.
    pub group_lovelaces: Vec<u64>,
}

/// Partition a wallet's Cluttered/Bloated UTxOs into planned debag txs,
/// shaped by [`DebagOptions`] (dust sweep, group size, collateral target).
///
/// Pure and deterministic (stable sort + explicit tiebreaks — no map-iteration
/// order leaking into the result). Collateral, ScriptLocked and Liquid tiers
/// are never spent. Dust joins the targets only when `opts.sweep_dust` — and
/// only *mergeable* dust (see `classify_wallet`); a swept dust UTxO whose only
/// merge partner is a Clean UTxO pulls that UTxO in as the merge host.
///
/// Packing is POLICY-AFFINE: bags sharing policies land in the same tx so their
/// assets actually merge into one output, instead of the same policy being
/// re-emitted from several txs. Leftover budget is filled with unrelated bags
/// for fee sharing, and any pack that would neither split a multi-policy bag
/// nor merge assets across inputs is dropped as a no-op.
pub fn plan_debag(utxos: &[UtxoApi], params: &TxBuildParams, opts: &DebagOptions) -> DebagPlan {
    let tiers = classify_wallet(utxos);

    // Primary targets: multi-policy bags, plus (per strategy) dust and Clean
    // UTxOs. Under Compact/MajorPolicies, sweeping also takes near-min UTxOs
    // that classify_wallet demoted to Clean (no same-policy partner) — they
    // ARE mergeable there, into cross-policy drawers.
    let mut target_idx: Vec<usize> = (0..utxos.len())
        .filter(|&i| match tiers[i] {
            UtxoTier::Cluttered | UtxoTier::Bloated => true,
            UtxoTier::Dust => opts.sweep_dust,
            UtxoTier::Clean => match opts.strategy {
                // Full consolidation: every asset-bearing UTxO is in scope.
                UtxoDistributionStrategy::Compact => !utxos[i].assets.is_empty(),
                // Drawers make demoted one-off dust mergeable again.
                UtxoDistributionStrategy::MajorPolicies => {
                    opts.sweep_dust && classify_utxo(&utxos[i]) == UtxoTier::Dust
                }
                UtxoDistributionStrategy::PolicyIsolation => false,
            },
            _ => false,
        })
        .collect();

    // Merge hosts (isolation only — drawers/compact merge cross-policy): a
    // swept dust UTxO only helps if its assets join another spent UTxO. When a
    // dust policy appears in no other target, pull in the smallest Clean UTxO
    // holding that policy as the merge host.
    if opts.sweep_dust && opts.strategy == UtxoDistributionStrategy::PolicyIsolation {
        let mut in_targets: HashSet<usize> = target_idx.iter().copied().collect();
        let mut policy_targets: HashMap<&str, u32> = HashMap::new();
        for &i in &target_idx {
            let mut seen: HashSet<&str> = HashSet::new();
            for a in &utxos[i].assets {
                if seen.insert(a.asset_id.policy_id.as_str()) {
                    *policy_targets
                        .entry(a.asset_id.policy_id.as_str())
                        .or_default() += 1;
                }
            }
        }
        let dust_targets: Vec<usize> = target_idx
            .iter()
            .copied()
            .filter(|&i| tiers[i] == UtxoTier::Dust)
            .collect();
        for i in dust_targets {
            for a in &utxos[i].assets {
                let p = a.asset_id.policy_id.as_str();
                if policy_targets.get(p).copied().unwrap_or(0) > 1 {
                    continue;
                }
                let host = (0..utxos.len())
                    .filter(|&j| {
                        !in_targets.contains(&j)
                            && tiers[j] == UtxoTier::Clean
                            && utxos[j]
                                .assets
                                .iter()
                                .any(|x| x.asset_id.policy_id.as_str() == p)
                    })
                    .min_by(|&x, &y| {
                        utxos[x]
                            .lovelace
                            .cmp(&utxos[y].lovelace)
                            .then_with(|| utxos[x].tx_hash.cmp(&utxos[y].tx_hash))
                            .then_with(|| utxos[x].output_index.cmp(&utxos[y].output_index))
                    });
                if let Some(h) = host {
                    in_targets.insert(h);
                    target_idx.push(h);
                    let mut seen: HashSet<&str> = HashSet::new();
                    for x in &utxos[h].assets {
                        if seen.insert(x.asset_id.policy_id.as_str()) {
                            *policy_targets
                                .entry(x.asset_id.policy_id.as_str())
                                .or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    let mut bags: Vec<&UtxoApi> = target_idx.iter().map(|&i| &utxos[i]).collect();
    // Fattest first (most policies, then most assets) — the worst offenders
    // seed the packs; deterministic tiebreak on ref.
    bags.sort_by(|a, b| {
        policy_count_of(b)
            .cmp(&policy_count_of(a))
            .then_with(|| b.assets.len().cmp(&a.assets.len()))
            .then_with(|| a.tx_hash.cmp(&b.tx_hash))
            .then_with(|| a.output_index.cmp(&b.output_index))
    });

    let value_budget = u64::from(params.max_tx_size) * TX_VALUE_BUDGET_PCT / 100;

    // Policy-affinity packing: seed a pack with the fattest unassigned bag,
    // grow it with the bag sharing the most policies (so same-policy assets
    // merge into one output), then fill any remaining budget with unrelated
    // bags for fee sharing.
    let bag_policies: Vec<HashSet<&str>> = bags
        .iter()
        .map(|b| {
            b.assets
                .iter()
                .map(|a| a.asset_id.policy_id.as_str())
                .collect()
        })
        .collect();
    let bag_bytes: Vec<u64> = bags
        .iter()
        .map(|b| estimate_value_size_from_map(&asset_map_of(b)))
        .collect();

    let mut assigned = vec![false; bags.len()];
    let mut packs: Vec<Vec<&UtxoApi>> = Vec::new();
    for seed in 0..bags.len() {
        if assigned[seed] {
            continue;
        }
        assigned[seed] = true;
        let mut pack = vec![seed];
        let mut pack_policies: HashSet<&str> = bag_policies[seed].clone();
        let mut bytes = bag_bytes[seed];

        // Affinity phase: repeatedly add the unassigned bag sharing the most
        // policies with the pack (first-best on the sorted order = deterministic).
        loop {
            if pack.len() >= MAX_BAGS_PER_TX {
                break;
            }
            let mut best: Option<usize> = None;
            let mut best_shared = 0usize;
            for (i, policies) in bag_policies.iter().enumerate() {
                if assigned[i] || bytes + bag_bytes[i] > value_budget {
                    continue;
                }
                let shared = policies.intersection(&pack_policies).count();
                if shared > best_shared {
                    best_shared = shared;
                    best = Some(i);
                }
            }
            let Some(i) = best else { break };
            assigned[i] = true;
            pack_policies.extend(bag_policies[i].iter().copied());
            bytes += bag_bytes[i];
            pack.push(i);
        }

        // Fill phase: unrelated bags share the fee while budget remains.
        for i in 0..bags.len() {
            if pack.len() >= MAX_BAGS_PER_TX {
                break;
            }
            if assigned[i] || bytes + bag_bytes[i] > value_budget {
                continue;
            }
            assigned[i] = true;
            pack_policies.extend(bag_policies[i].iter().copied());
            bytes += bag_bytes[i];
            pack.push(i);
        }

        packs.push(pack.into_iter().map(|i| bags[i]).collect());
    }

    let mut items = Vec::with_capacity(packs.len());
    for pack in packs {
        let mut combined: HashMap<AssetId, u64> = HashMap::new();
        let mut input_lovelace = 0u64;
        let mut locked_before = 0u64;
        // Per policy: how many of the pack's inputs hold it (merge detection).
        let mut policy_inputs: HashMap<&str, u32> = HashMap::new();
        for bag in &pack {
            input_lovelace += bag.lovelace;
            let ids: Vec<AssetId> = bag.assets.iter().map(|a| a.asset_id.clone()).collect();
            locked_before += min_utxo_for_assets(params, &ids);
            let mut seen: HashSet<&str> = HashSet::new();
            for aq in &bag.assets {
                *combined.entry(aq.asset_id.clone()).or_default() += aq.quantity;
                if seen.insert(aq.asset_id.policy_id.as_str()) {
                    *policy_inputs
                        .entry(aq.asset_id.policy_id.as_str())
                        .or_default() += 1;
                }
            }
        }

        let output_groups = group_assets(&combined, params, opts);

        // Identity guard: if the outputs exactly reproduce the inputs' asset
        // bundles, the tx changes nothing — this is what makes re-running the
        // same strategy idempotent (a drawer regrouped is the same drawer).
        let mut input_sets: Vec<Vec<(String, String, u64)>> = pack
            .iter()
            .map(|b| {
                let mut v: Vec<(String, String, u64)> = b
                    .assets
                    .iter()
                    .map(|a| {
                        (
                            a.asset_id.policy_id.to_string(),
                            a.asset_id.asset_name_hex.clone(),
                            a.quantity,
                        )
                    })
                    .collect();
                v.sort();
                v
            })
            .collect();
        input_sets.sort();
        let mut output_sets: Vec<Vec<(String, String, u64)>> = output_groups
            .iter()
            .map(|g| {
                let mut v: Vec<(String, String, u64)> = g
                    .assets
                    .iter()
                    .map(|(id, qty)| (id.policy_id.to_string(), id.asset_name_hex.clone(), *qty))
                    .collect();
                v.sort();
                v
            })
            .collect();
        output_sets.sort();
        if input_sets == output_sets {
            continue;
        }

        // Usefulness guard: the tx must split a multi-policy bag, merge a
        // policy spread across inputs, or reduce the UTxO count — otherwise it
        // just re-shuffles at a fee (the "dust shuffle" failure mode).
        let split_benefit = pack.iter().any(|b| policy_count_of(b) > 1);
        let merge_benefit = policy_inputs.values().any(|&n| n > 1);
        let count_benefit = output_groups.len() < pack.len();
        if !split_benefit && !merge_benefit && !count_benefit {
            continue;
        }
        let locked_after: u64 = output_groups.iter().map(|g| g.min_lovelace).sum();

        // Rough fee: base + coefficient × (inputs + output value bytes + body overhead).
        let est_value_bytes: u64 = output_groups
            .iter()
            .map(|g| {
                let map: HashMap<AssetId, u64> = g.assets.iter().cloned().collect();
                estimate_value_size_from_map(&map)
            })
            .sum();
        let est_size =
            300 + pack.len() as u64 * 70 + est_value_bytes + output_groups.len() as u64 * 70;
        let est_fee = params.min_fee_constant + params.min_fee_coefficient * est_size;

        let outflow = locked_after + est_fee;
        let ada_shortfall = outflow.saturating_sub(input_lovelace);

        items.push(DebagItem {
            inputs: pack
                .iter()
                .map(|u| (u.tx_hash.clone(), u.output_index))
                .collect(),
            input_lovelace,
            output_groups,
            est_fee,
            locked_before,
            locked_after,
            ada_shortfall,
            collateral_outputs: 0,
            merge_pass: false,
        });
    }

    // ── Pass 2: chained same-policy merges ───────────────────────────────
    // The value budget / input cap can force one policy to be re-emitted from
    // several pass-1 packs. Each such output would be the others' merge
    // partner, so the plan would leave mergeable "twin dust" behind and an
    // immediate re-plan would find work again. Fuse those planned outputs in
    // follow-up txs that spend them via UTxO chaining. A merge whose single
    // result would still sit at the dust threshold pulls in the smallest
    // untouched Clean UTxOs of the policy until no merge partner remains —
    // the terminal state re-plans to an empty plan.
    {
        // policy -> pass-1 (item, group) sources, deterministic order.
        let by_policy: Vec<(String, Vec<(usize, usize)>)> = {
            let mut m: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
            for (i, item) in items.iter().enumerate() {
                for (g, grp) in item.output_groups.iter().enumerate() {
                    // Skip "(mixed: …)" drawer labels — they are not policies.
                    if !grp.policy_id.starts_with('(') {
                        m.entry(grp.policy_id.as_str()).or_default().push((i, g));
                    }
                }
            }
            let mut v: Vec<(String, Vec<(usize, usize)>)> = m
                .into_iter()
                .map(|(p, refs)| (p.to_string(), refs))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        let spent_refs: HashSet<(&str, u32)> = items
            .iter()
            .flat_map(|it| it.inputs.iter().map(|(h, x)| (h.as_str(), *x)))
            .collect();

        struct MergeCand {
            inputs: Vec<(String, u32)>,
            input_lovelace: u64,
            locked_before: u64,
            chunks: Vec<OutputGroup>,
            value_bytes: u64,
        }
        let mut used_hosts: HashSet<usize> = HashSet::new();
        let mut cands: Vec<MergeCand> = Vec::new();
        for (policy, srcs) in &by_policy {
            let mut combined: HashMap<AssetId, u64> = HashMap::new();
            let mut input_lovelace = 0u64;
            for &(i, g) in srcs {
                let grp = &items[i].output_groups[g];
                for (id, qty) in &grp.assets {
                    *combined.entry(id.clone()).or_default() += qty;
                }
                input_lovelace += grp.min_lovelace;
            }
            let mut locked_before = input_lovelace;
            let mut inputs: Vec<(String, u32)> = srcs
                .iter()
                .map(|&(i, g)| (format!("{PLAN_REF_PREFIX}{i}"), g as u32))
                .collect();
            let mut chunks = group_by_policy(&combined, params, opts.group_size);

            // Residual-dust host pull: a single merged output at/below the
            // dust threshold still classifies Dust while untouched same-policy
            // UTxOs remain — keep pulling the smallest Clean one in until no
            // partner is left (or the output outgrows the threshold).
            while chunks.len() == 1 && chunks[0].min_lovelace <= DUST_THRESHOLD {
                let host = (0..utxos.len())
                    .filter(|&j| {
                        !used_hosts.contains(&j)
                            && tiers[j] == UtxoTier::Clean
                            && !spent_refs
                                .contains(&(utxos[j].tx_hash.as_str(), utxos[j].output_index))
                            && utxos[j]
                                .assets
                                .iter()
                                .any(|x| x.asset_id.policy_id.as_str() == policy)
                    })
                    .min_by(|&x, &y| {
                        utxos[x]
                            .lovelace
                            .cmp(&utxos[y].lovelace)
                            .then_with(|| utxos[x].tx_hash.cmp(&utxos[y].tx_hash))
                            .then_with(|| utxos[x].output_index.cmp(&utxos[y].output_index))
                    });
                let Some(h) = host else { break };
                used_hosts.insert(h);
                for aq in &utxos[h].assets {
                    *combined.entry(aq.asset_id.clone()).or_default() += aq.quantity;
                }
                input_lovelace += utxos[h].lovelace;
                let ids: Vec<AssetId> =
                    utxos[h].assets.iter().map(|a| a.asset_id.clone()).collect();
                locked_before += min_utxo_for_assets(params, &ids);
                inputs.push((utxos[h].tx_hash.clone(), utxos[h].output_index));
                chunks = group_by_policy(&combined, params, opts.group_size);
            }

            // Only merge when it reduces the output count — a cap-split policy
            // already has its final shape (81/81/81 stays 81/81/81).
            if chunks.len() >= inputs.len() {
                continue;
            }
            let value_bytes = chunks
                .iter()
                .map(|g| {
                    let map: HashMap<AssetId, u64> = g.assets.iter().cloned().collect();
                    estimate_value_size_from_map(&map)
                })
                .sum();
            cands.push(MergeCand {
                inputs,
                input_lovelace,
                locked_before,
                chunks,
                value_bytes,
            });
        }

        // Pack merge candidates into as few chained txs as the caps allow.
        let make_item = |inputs: Vec<(String, u32)>,
                         groups: Vec<OutputGroup>,
                         input_lovelace: u64,
                         locked_before: u64,
                         value_bytes: u64| {
            let locked_after: u64 = groups.iter().map(|g| g.min_lovelace).sum();
            let est_size = 300 + inputs.len() as u64 * 70 + value_bytes + groups.len() as u64 * 70;
            let est_fee = params.min_fee_constant + params.min_fee_coefficient * est_size;
            let ada_shortfall = (locked_after + est_fee).saturating_sub(input_lovelace);
            DebagItem {
                inputs,
                input_lovelace,
                output_groups: groups,
                est_fee,
                locked_before,
                locked_after,
                ada_shortfall,
                collateral_outputs: 0,
                merge_pass: true,
            }
        };
        let mut cur_inputs: Vec<(String, u32)> = Vec::new();
        let mut cur_groups: Vec<OutputGroup> = Vec::new();
        let mut cur_lovelace = 0u64;
        let mut cur_locked = 0u64;
        let mut cur_bytes = 0u64;
        let mut pass2: Vec<DebagItem> = Vec::new();
        for c in cands {
            let overflows = !cur_inputs.is_empty()
                && (cur_inputs.len() + c.inputs.len() > MAX_BAGS_PER_TX
                    || cur_bytes + c.value_bytes > value_budget);
            if overflows {
                pass2.push(make_item(
                    std::mem::take(&mut cur_inputs),
                    std::mem::take(&mut cur_groups),
                    std::mem::take(&mut cur_lovelace),
                    std::mem::take(&mut cur_locked),
                    std::mem::take(&mut cur_bytes),
                ));
            }
            cur_inputs.extend(c.inputs);
            cur_groups.extend(c.chunks);
            cur_lovelace += c.input_lovelace;
            cur_locked += c.locked_before;
            cur_bytes += c.value_bytes;
        }
        if !cur_inputs.is_empty() {
            pass2.push(make_item(
                cur_inputs,
                cur_groups,
                cur_lovelace,
                cur_locked,
                cur_bytes,
            ));
        }
        items.extend(pass2);
    }

    // Collateral deficit: create the missing 5-ADA UTxOs on the first tx, or
    // as a standalone tx when the wallet needs no debagging (funded from the
    // Liquid pool at build time).
    let existing_collateral = tiers.iter().filter(|t| **t == UtxoTier::Collateral).count() as u32;
    let collateral_deficit = opts.collateral_target.saturating_sub(existing_collateral);
    if collateral_deficit > 0 {
        let extra = u64::from(collateral_deficit) * COLLATERAL_SIZE;
        if let Some(first) = items.first_mut() {
            first.collateral_outputs = collateral_deficit;
            let outflow = first.locked_after + first.est_fee + extra;
            first.ada_shortfall = outflow.saturating_sub(first.input_lovelace);
        } else {
            let est_fee = params.min_fee_constant + params.min_fee_coefficient * 400;
            items.push(DebagItem {
                inputs: Vec::new(),
                input_lovelace: 0,
                output_groups: Vec::new(),
                est_fee,
                locked_before: 0,
                locked_after: 0,
                ada_shortfall: extra + est_fee,
                collateral_outputs: collateral_deficit,
                merge_pass: false,
            });
        }
    }

    let summary = DebagSummary {
        bag_count: items.iter().map(|i| i.inputs.len()).sum(),
        tx_count: items.len(),
        output_count: items.iter().map(|i| i.output_groups.len()).sum(),
        est_total_fee: items.iter().map(|i| i.est_fee).sum(),
        locked_delta: items
            .iter()
            .map(|i| i.locked_after as i64 - i.locked_before as i64)
            .sum(),
        collateral_created: collateral_deficit,
    };

    DebagPlan { items, summary }
}

/// Build one planned debag tx: spend the item's bags (+ ADA top-up from the
/// wallet's pure-ADA pool if the bags don't self-fund), emit one output per
/// group back to `owner`, and return any remaining ADA as pure change.
///
/// Single-party self-send → 1 witness. All outputs return to the signer's own
/// keys, so a hardware wallet shows a clean, low-click tx.
pub fn build_debag(
    wallet_utxos: &[UtxoApi],
    item: &DebagItem,
    owner: &Address,
    params: &TxBuildParams,
    network_id: u8,
) -> Result<DebagBuildResult, TxBuildError> {
    // Resolve planned inputs against the live pool — a missing ref means the
    // plan is stale (UTxO spent since planning); surface as NoSuitableUtxo so
    // the caller re-plans.
    let by_ref: HashMap<(&str, u32), &UtxoApi> = wallet_utxos
        .iter()
        .map(|u| ((u.tx_hash.as_str(), u.output_index), u))
        .collect();
    let mut bags: Vec<&UtxoApi> = Vec::with_capacity(item.inputs.len());
    for (hash, idx) in &item.inputs {
        match by_ref.get(&(hash.as_str(), *idx)) {
            Some(u) => bags.push(u),
            None => return Err(TxBuildError::NoSuitableUtxo),
        }
    }

    // Conservation check: the plan's output groups must exactly re-emit the
    // bags' assets. A mismatch means plan and wallet state have diverged.
    let mut in_assets: HashMap<AssetId, u64> = HashMap::new();
    for bag in &bags {
        for aq in &bag.assets {
            *in_assets.entry(aq.asset_id.clone()).or_default() += aq.quantity;
        }
    }
    let mut out_assets: HashMap<AssetId, u64> = HashMap::new();
    for group in &item.output_groups {
        for (id, qty) in &group.assets {
            *out_assets.entry(id.clone()).or_default() += qty;
        }
    }
    if in_assets != out_assets {
        return Err(TxBuildError::BuildFailed(
            "debag plan does not conserve the bag assets — re-plan against fresh wallet state"
                .to_string(),
        ));
    }

    // Exact min-UTxO per output (plan figures were estimates).
    let group_mins: Vec<u64> = item
        .output_groups
        .iter()
        .map(|g| {
            let ids: Vec<AssetId> = g.assets.iter().map(|(id, _)| id.clone()).collect();
            min_utxo_for_assets(params, &ids)
        })
        .collect();
    let mins_total: u64 = group_mins.iter().sum();

    // ADA top-up from the wallet's pure-ADA pool when the bags don't self-fund.
    // The selector's pool filter is pure-ADA/no-script-ref only, so a top-up can
    // never drag new assets into this tx.
    static EMPTY_EXCLUDE: std::sync::OnceLock<HashSet<(String, u32)>> = std::sync::OnceLock::new();
    let exclude = EMPTY_EXCLUDE.get_or_init(HashSet::new);
    let sel = Selection {
        must_spend: bags,
        pool: wallet_utxos,
        exclude,
        strategy: Strategy::SmallestSufficient,
    };
    let collateral_total = u64::from(item.collateral_outputs) * COLLATERAL_SIZE;
    let selected = select(&sel, mins_total + collateral_total + SELECTION_FEE_ESTIMATE).map_err(
        |e| match e {
            SelectError::Insufficient { target, available } => TxBuildError::InsufficientFunds {
                needed: target,
                available,
            },
            SelectError::DuplicateMustSpend { .. } => TxBuildError::NoSuitableUtxo,
        },
    )?;
    let total_in: u64 = selected.iter().map(|u| u.lovelace).sum();

    let min_pure = params.min_pure_utxo();
    let owner = owner.clone();
    let groups: Vec<(HashMap<AssetId, u64>, u64)> = item
        .output_groups
        .iter()
        .zip(group_mins.iter())
        .map(|(g, min)| (g.assets.iter().cloned().collect(), *min))
        .collect();
    let selected_owned: Vec<UtxoApi> = selected.iter().map(|u| (*u).clone()).collect();
    let collateral_count = item.collateral_outputs;

    // CIP-674 tag so a mitos `tagged-tx` companion can watch every debag tx
    // (permanent subscription on this constant, not on ephemeral addresses).
    // Pre-encode once; re-attach each convergence round so the fee accounts
    // for the aux-data weight. Aux-data hash rides in the body → the tx hash
    // is still stable pre-signature (wave chaining depends on that).
    let metadata_bytes =
        crate::metadata::cip20::build_cip20_auxiliary_data(&[DEFRACKIT_TAG.to_string()])
            .map_err(|e| TxBuildError::BuildFailed(format!("metadata: {e}")))?;

    let unsigned = converge_fee(
        move |fee| {
            let outflow = mins_total + collateral_total + fee;
            let mut change =
                total_in
                    .checked_sub(outflow)
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: outflow,
                        available: total_in,
                    })?;

            let refs: Vec<&UtxoApi> = selected_owned.iter().collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &refs)?;

            // Sub-min change can't stand alone; fold it into the last asset
            // group, else the last collateral output (5 ADA + <1 ADA stays
            // inside the collateral band).
            let fold_into_group = !groups.is_empty() && change > 0 && change < min_pure;
            let fold_into_collateral =
                groups.is_empty() && collateral_count > 0 && change > 0 && change < min_pure;

            // One output per asset group.
            for (i, (assets, min)) in groups.iter().enumerate() {
                let mut lovelace = *min;
                if fold_into_group && i + 1 == groups.len() {
                    lovelace += change;
                    change = 0;
                }
                let output = create_ada_output(owner.clone(), lovelace);
                let output = add_assets_from_map(output, assets)?;
                tx = tx.output(output);
            }

            // Requested collateral UTxOs (pure ADA, 5 ADA each).
            for i in 0..collateral_count {
                let mut lovelace = COLLATERAL_SIZE;
                if fold_into_collateral && i + 1 == collateral_count {
                    lovelace += change;
                    change = 0;
                }
                tx = tx.output(create_ada_output(owner.clone(), lovelace));
            }

            if change > 0 {
                tx = tx.output(create_ada_output(owner.clone(), change));
            }

            tx = tx.add_auxiliary_data(metadata_bytes.clone());

            Ok(tx.fee(fee).network_id(network_id))
        },
        200_000,
        params,
    )?;

    let change_lovelace = total_in
        .saturating_sub(mins_total)
        .saturating_sub(collateral_total)
        .saturating_sub(unsigned.fee);

    // Mirror the closure's fold: a sub-min remainder rode out on the last
    // asset group, so that output's actual value exceeds its min.
    let mut group_lovelaces = group_mins.clone();
    if change_lovelace > 0
        && change_lovelace < min_pure
        && let Some(last) = group_lovelaces.last_mut()
    {
        *last += change_lovelace;
    }

    Ok(DebagBuildResult {
        unsigned,
        change_lovelace,
        group_lovelaces,
    })
}

// ── helpers ──────────────────────────────────────────────────────────────

fn policy_count_of(u: &UtxoApi) -> usize {
    cardano_assets::utxo_health::policy_count(u)
}

fn asset_map_of(u: &UtxoApi) -> HashMap<AssetId, u64> {
    let mut map: HashMap<AssetId, u64> = HashMap::new();
    for aq in &u.assets {
        *map.entry(aq.asset_id.clone()).or_default() += aq.quantity;
    }
    map
}

/// Group a combined asset map by policy, chunking any policy whose assets would
/// breach the per-output value-size margin — or, when `group_size > 0`, exceed
/// that many assets per output. Deterministic: policies and assets sorted.
fn group_by_policy(
    combined: &HashMap<AssetId, u64>,
    params: &TxBuildParams,
    group_size: u32,
) -> Vec<OutputGroup> {
    let mut by_policy: HashMap<&str, Vec<(&AssetId, u64)>> = HashMap::new();
    for (id, qty) in combined {
        by_policy
            .entry(id.policy_id.as_str())
            .or_default()
            .push((id, *qty));
    }
    let mut policies: Vec<&str> = by_policy.keys().copied().collect();
    policies.sort_unstable();

    let threshold = params.max_value_size * VALUE_SIZE_MARGIN_PCT / 100;
    let mut groups = Vec::new();
    for policy in policies {
        let mut assets = by_policy.remove(policy).unwrap_or_default();
        assets.sort_by(|a, b| a.0.asset_name_hex.cmp(&b.0.asset_name_hex));

        // Greedy chunking under the size margin (and the asset-count cap,
        // balanced so this policy's chunks come out near-equal).
        let cap = balanced_cap(assets.len(), group_size);
        let mut chunk: HashMap<AssetId, u64> = HashMap::new();
        for (id, qty) in assets {
            if cap > 0 && chunk.len() as u32 >= cap {
                groups.push(make_group(policy, &chunk, params));
                chunk.clear();
            }
            chunk.insert(id.clone(), qty);
            if estimate_value_size_from_map(&chunk) > threshold {
                // Split off the overflow asset into the next chunk.
                chunk.remove(id);
                if !chunk.is_empty() {
                    groups.push(make_group(policy, &chunk, params));
                    chunk.clear();
                }
                chunk.insert(id.clone(), qty);
            }
        }
        if !chunk.is_empty() {
            groups.push(make_group(policy, &chunk, params));
        }
    }
    groups
}

/// Balance a count cap over `n` assets so chunks come out near-equal instead
/// of leaving a tiny remainder (243 @ cap 120 → 81/81/81, not 120/120/3 — a
/// 3-asset tail would just be new dust). Returns `group_size` unchanged when
/// no split is needed (0 = uncapped).
fn balanced_cap(n: usize, group_size: u32) -> u32 {
    let n = n as u32;
    if group_size == 0 || n <= group_size {
        return group_size;
    }
    let chunks = n.div_ceil(group_size);
    n.div_ceil(chunks)
}

fn make_group(policy: &str, assets: &HashMap<AssetId, u64>, params: &TxBuildParams) -> OutputGroup {
    let ids: Vec<AssetId> = assets.keys().cloned().collect();
    let mut sorted: Vec<(AssetId, u64)> = assets.iter().map(|(k, v)| (k.clone(), *v)).collect();
    sorted.sort_by(|a, b| a.0.asset_name_hex.cmp(&b.0.asset_name_hex));
    OutputGroup {
        policy_id: policy.to_string(),
        assets: sorted,
        min_lovelace: min_utxo_for_assets(params, &ids),
    }
}

/// Dispatch a pack's combined assets to the strategy's grouping.
fn group_assets(
    combined: &HashMap<AssetId, u64>,
    params: &TxBuildParams,
    opts: &DebagOptions,
) -> Vec<OutputGroup> {
    match opts.strategy {
        UtxoDistributionStrategy::PolicyIsolation => {
            group_by_policy(combined, params, opts.group_size)
        }
        UtxoDistributionStrategy::MajorPolicies => {
            // Split by per-policy holding size: majors isolate, minors share
            // "drawer" outputs.
            let mut policy_counts: HashMap<&str, u32> = HashMap::new();
            for id in combined.keys() {
                *policy_counts.entry(id.policy_id.as_str()).or_default() += 1;
            }
            let threshold = opts.major_threshold.max(1);
            let mut major: HashMap<AssetId, u64> = HashMap::new();
            let mut minor: HashMap<AssetId, u64> = HashMap::new();
            for (id, qty) in combined {
                if policy_counts[id.policy_id.as_str()] >= threshold {
                    major.insert(id.clone(), *qty);
                } else {
                    minor.insert(id.clone(), *qty);
                }
            }
            let mut groups = group_by_policy(&major, params, opts.group_size);
            groups.extend(chunk_mixed(&minor, params, opts.group_size));
            groups
        }
        UtxoDistributionStrategy::Compact => chunk_mixed(combined, params, opts.group_size),
    }
}

/// Chunk an asset map into as few outputs as possible, ignoring policy
/// boundaries (assets sorted by policy then name so policies stay adjacent).
/// A chunk that ends up single-policy is labelled with that policy; otherwise
/// `(mixed)`.
fn chunk_mixed(
    combined: &HashMap<AssetId, u64>,
    params: &TxBuildParams,
    group_size: u32,
) -> Vec<OutputGroup> {
    if combined.is_empty() {
        return Vec::new();
    }
    let mut assets: Vec<(&AssetId, u64)> = combined.iter().map(|(id, qty)| (id, *qty)).collect();
    assets.sort_by(|a, b| {
        a.0.policy_id
            .cmp(&b.0.policy_id)
            .then_with(|| a.0.asset_name_hex.cmp(&b.0.asset_name_hex))
    });

    let threshold = params.max_value_size * VALUE_SIZE_MARGIN_PCT / 100;
    let cap = balanced_cap(assets.len(), group_size);
    let mut groups = Vec::new();
    let mut chunk: HashMap<AssetId, u64> = HashMap::new();
    for (id, qty) in assets {
        if cap > 0 && chunk.len() as u32 >= cap {
            groups.push(make_mixed_group(&chunk, params));
            chunk.clear();
        }
        chunk.insert(id.clone(), qty);
        if estimate_value_size_from_map(&chunk) > threshold {
            chunk.remove(id);
            if !chunk.is_empty() {
                groups.push(make_mixed_group(&chunk, params));
                chunk.clear();
            }
            chunk.insert(id.clone(), qty);
        }
    }
    if !chunk.is_empty() {
        groups.push(make_mixed_group(&chunk, params));
    }
    groups
}

fn make_mixed_group(assets: &HashMap<AssetId, u64>, params: &TxBuildParams) -> OutputGroup {
    let mut policies: Vec<&str> = assets.keys().map(|id| id.policy_id.as_str()).collect();
    policies.sort_unstable();
    policies.dedup();
    let label = if policies.len() == 1 {
        policies[0].to_string()
    } else {
        format!("(mixed: {} policies)", policies.len())
    };
    make_group(&label, assets, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::utxo::AssetQuantity;

    const TEST_ADDR: &str = "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp";

    fn asset_id(policy: char, name: &str) -> AssetId {
        AssetId::new_unchecked(policy.to_string().repeat(56), hex::encode(name))
    }

    fn utxo(tx_hash: &str, lovelace: u64, assets: Vec<(AssetId, u64)>) -> UtxoApi {
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets: assets
                .into_iter()
                .map(|(asset_id, quantity)| AssetQuantity { asset_id, quantity })
                .collect(),
            tags: vec![],
        }
    }

    fn params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
            max_value_size: 5000,
            ..Default::default()
        }
    }

    /// A built debag tx carries the CIP-674 tag AND its body hash is stable
    /// (wave chaining spends predicted refs before signing — the aux-data
    /// hash rides in the body, but re-attaching identical bytes each round
    /// must not perturb the hash across two independent builds).
    #[test]
    fn build_debag_stamps_tag_and_is_deterministic() {
        use pallas_addresses::Address;
        use pallas_txbuilder::BuildConway;
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                10_000_000,
                vec![(asset_id('a', "one"), 1), (asset_id('b', "two"), 1)],
            ),
            utxo(&"c".repeat(64), 50_000_000, vec![]), // Liquid ADA top-up
            utxo(&"d".repeat(64), 5_000_000, vec![]),  // collateral candidate
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        let item = plan.items.first().expect("a debag item");
        let owner = Address::from_bech32(TEST_ADDR).unwrap();

        let build = || {
            let r = build_debag(&wallet, item, &owner, &params(), 0).expect("build");
            r.unsigned.staging.build_conway_raw().unwrap().tx_bytes
        };
        let a = build();
        let b = build();
        assert_eq!(a.as_ref(), b.as_ref(), "tx bytes must be deterministic");

        // The CBOR must contain the tag string (674.msg = "defrackit/v1").
        let hex = hex::encode(a.as_ref());
        assert!(
            hex.contains(&hex::encode(DEFRACKIT_TAG)),
            "built tx must carry the CIP-674 tag"
        );
    }

    /// A 3-policy bag: 3 per-policy groups; Clean/Liquid/Collateral untouched.
    #[test]
    fn plan_promotes_bag_to_clean() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                10_000_000,
                vec![
                    (asset_id('a', "one"), 1),
                    (asset_id('b', "two"), 1),
                    (asset_id('c', "three"), 1),
                    (asset_id('c', "four"), 1),
                ],
            ),
            utxo(&"b".repeat(64), 3_000_000, vec![(asset_id('d', "solo"), 1)]), // Clean
            utxo(&"c".repeat(64), 50_000_000, vec![]),                          // Liquid
            utxo(&"d".repeat(64), 5_000_000, vec![]),                           // Collateral
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        assert_eq!(plan.items.len(), 1);
        let item = &plan.items[0];
        assert_eq!(item.inputs, vec![("a".repeat(64), 0)]);
        assert_eq!(item.output_groups.len(), 3, "one output per policy");
        assert!(
            plan.summary.locked_delta > 0,
            "splitting locks extra min-UTxO"
        );
        assert_eq!(
            item.ada_shortfall, 0,
            "10 ADA bag self-funds 3 small outputs"
        );
    }

    #[test]
    fn plan_empty_for_clean_wallet() {
        let wallet = vec![
            utxo(&"a".repeat(64), 3_000_000, vec![(asset_id('a', "x"), 1)]),
            utxo(&"b".repeat(64), 50_000_000, vec![]),
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        assert!(plan.items.is_empty());
        assert_eq!(plan.summary.tx_count, 0);
    }

    /// Two small bags pack into ONE tx; same-policy assets merge into one group.
    #[test]
    fn plan_packs_small_bags_and_merges_policies() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                4_000_000,
                vec![(asset_id('a', "x"), 1), (asset_id('b', "y"), 1)],
            ),
            utxo(
                &"b".repeat(64),
                4_000_000,
                vec![(asset_id('a', "z"), 1), (asset_id('c', "w"), 1)],
            ),
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        assert_eq!(plan.items.len(), 1, "small bags share one tx");
        let item = &plan.items[0];
        assert_eq!(item.inputs.len(), 2);
        // Policies a, b, c — policy `a` assets from BOTH bags share one group.
        assert_eq!(item.output_groups.len(), 3);
        let a_group = item
            .output_groups
            .iter()
            .find(|g| g.policy_id == "a".repeat(56))
            .unwrap();
        assert_eq!(a_group.assets.len(), 2);
    }

    /// Build: per-policy outputs + pure-ADA change, assets conserved, value
    /// balanced (inputs == outputs + fee).
    #[test]
    fn build_emits_groups_plus_change() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                10_000_000,
                vec![(asset_id('a', "one"), 1), (asset_id('b', "two"), 1)],
            ),
            utxo(&"c".repeat(64), 50_000_000, vec![]),
        ];
        let p = params();
        let plan = plan_debag(&wallet, &p, &DebagOptions::default());
        let item = &plan.items[0];
        let owner = Address::from_bech32(TEST_ADDR).unwrap();
        let built = build_debag(&wallet, item, &owner, &p, 0).unwrap();

        let outputs = built.unsigned.staging.outputs.as_ref().unwrap();
        assert_eq!(outputs.len(), 3, "2 policy groups + 1 pure change");
        assert!(built.change_lovelace > 0);

        // Value balance: bag input funds outputs + fee (no top-up needed).
        let out_lovelace: u64 = outputs.iter().map(|o| o.lovelace).sum();
        assert_eq!(out_lovelace + built.unsigned.fee, 10_000_000);

        // The Liquid UTxO was NOT dragged in (bag self-funds).
        let inputs = built.unsigned.staging.inputs.as_ref().unwrap();
        assert_eq!(inputs.len(), 1);
    }

    /// A min-ADA bag can't fund its own split — top-up comes from the Liquid
    /// pool (never from asset UTxOs), and change returns.
    #[test]
    fn build_tops_up_from_liquid_pool() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                1_600_000,
                vec![(asset_id('a', "x"), 1), (asset_id('b', "y"), 1)],
            ),
            utxo(
                &"c".repeat(64),
                5_000_000,
                vec![(asset_id('d', "decoy"), 1)],
            ), // asset UTxO: must not fund
            utxo(&"d".repeat(64), 20_000_000, vec![]), // Liquid
        ];
        let p = params();
        let plan = plan_debag(&wallet, &p, &DebagOptions::default());
        let item = &plan.items[0];
        assert!(
            item.ada_shortfall > 0,
            "1.6 ADA can't fund two asset outputs"
        );
        let owner = Address::from_bech32(TEST_ADDR).unwrap();
        let built = build_debag(&wallet, item, &owner, &p, 0).unwrap();

        let inputs = built.unsigned.staging.inputs.as_ref().unwrap();
        assert_eq!(inputs.len(), 2, "bag + one Liquid top-up");
        // Decoy asset UTxO untouched; conservation guard would have caught its
        // assets appearing in outputs.
        let outputs = built.unsigned.staging.outputs.as_ref().unwrap();
        let total_out_assets: usize = outputs
            .iter()
            .map(|o| o.assets.as_ref().map(|a| a.len()).unwrap_or(0))
            .sum();
        assert_eq!(
            total_out_assets, 2,
            "exactly the bag's 2 policies re-emitted"
        );
    }

    /// `group_size` splits a single policy's assets into capped groups.
    #[test]
    fn plan_group_size_caps_assets_per_output() {
        // A Cluttered bag (2 policies): policy `a` holds 5 assets, `b` holds 1.
        let mut assets = vec![(asset_id('b', "solo"), 1)];
        for i in 0..5u32 {
            assets.push((asset_id('a', &format!("n{i}")), 1));
        }
        let wallet = vec![utxo(&"a".repeat(64), 20_000_000, assets)];
        let opts = DebagOptions {
            group_size: 2,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        let item = &plan.items[0];
        // Policy a: ceil(5/2) = 3 groups; policy b: 1 group.
        assert_eq!(item.output_groups.len(), 4);
        assert!(item.output_groups.iter().all(|g| g.assets.len() <= 2));
    }

    /// The count cap is balanced across a policy's chunks: 25 assets at cap 10
    /// come out 9/9/7, not 10/10/5 — a lopsided tail would just be new dust.
    #[test]
    fn plan_group_size_chunks_are_balanced() {
        // A Cluttered bag: 25 assets of policy `a` plus a `c` one-off, so the
        // bag is a genuine split target under policy isolation.
        let mut assets: Vec<(AssetId, u64)> = (0..25u32)
            .map(|i| (asset_id('a', &format!("n{i:02}")), 1))
            .collect();
        assets.push((asset_id('c', "solo"), 1));
        let wallet = vec![utxo(&"a".repeat(64), 40_000_000, assets)];
        let opts = DebagOptions {
            group_size: 10,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        let mut a_sizes: Vec<usize> = plan
            .items
            .iter()
            .flat_map(|i| &i.output_groups)
            .filter(|g| g.policy_id.starts_with('a'))
            .map(|g| g.assets.len())
            .collect();
        a_sizes.sort_unstable();
        // 25 policy-a assets, cap 10 → 3 chunks at balanced cap ceil(25/3)=9.
        assert_eq!(a_sizes, vec![7, 9, 9]);
    }

    /// Numeric policy ids for tests needing more than 26 distinct policies.
    fn pid_asset(policy_num: u32, name: &str) -> AssetId {
        AssetId::new_unchecked(format!("{policy_num:056x}"), hex::encode(name))
    }

    /// A policy forced across two packs by the input cap gets fused by a
    /// pass-2 merge tx that spends the pass-1 planned outputs (chained).
    #[test]
    fn plan_merge_fuses_policy_split_across_packs() {
        // 41 Cluttered bags (cap is 40/tx): each holds one asset of a SHARED
        // policy plus one asset of a unique policy → two packs, each emitting
        // a shared-policy group.
        const SHARED: u32 = 999_999;
        let wallet: Vec<UtxoApi> = (0..41u32)
            .map(|i| {
                utxo(
                    &format!("{i:064x}"),
                    5_000_000,
                    vec![
                        (pid_asset(SHARED, &format!("s{i}")), 1),
                        (pid_asset(i, "solo"), 1),
                    ],
                )
            })
            .collect();
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());

        let merges: Vec<&DebagItem> = plan.items.iter().filter(|i| i.merge_pass).collect();
        assert_eq!(merges.len(), 1, "expected exactly one merge tx");
        let merge = merges[0];
        // Spends the two pass-1 shared-policy outputs via placeholder refs.
        assert_eq!(merge.inputs.len(), 2);
        assert!(
            merge
                .inputs
                .iter()
                .all(|(h, _)| h.starts_with(PLAN_REF_PREFIX))
        );
        // One fused output holding all 41 shared-policy assets.
        assert_eq!(merge.output_groups.len(), 1);
        assert_eq!(merge.output_groups[0].assets.len(), 41);
        let shared_policy = format!("{SHARED:056x}");
        assert_eq!(merge.output_groups[0].policy_id, shared_policy);
        // The placeholder refs point at real shared-policy groups.
        for (h, g) in &merge.inputs {
            let item_idx: usize = h[PLAN_REF_PREFIX.len()..].parse().unwrap();
            let grp = &plan.items[item_idx].output_groups[*g as usize];
            assert_eq!(grp.policy_id, shared_policy);
        }
    }

    /// A planned output that would stay at the dust threshold with untouched
    /// same-policy Clean UTxOs around pulls them in until no merge partner
    /// remains.
    #[test]
    fn plan_merge_pulls_hosts_until_terminal() {
        let wallet = vec![
            // Cluttered bag → splits into two near-min singleton outputs.
            utxo(
                &"a".repeat(64),
                5_000_000,
                vec![(asset_id('a', "one"), 1), (asset_id('b', "solo"), 1)],
            ),
            // Two untouched Clean singletons of policy `a` — each would be a
            // merge partner for the planned `a` output.
            utxo(&"b".repeat(64), 2_000_000, vec![(asset_id('a', "two"), 1)]),
            utxo(
                &"c".repeat(64),
                2_000_000,
                vec![(asset_id('a', "three"), 1)],
            ),
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());

        let merges: Vec<&DebagItem> = plan.items.iter().filter(|i| i.merge_pass).collect();
        assert_eq!(merges.len(), 1);
        let merge = merges[0];
        // 1 placeholder (the planned `a` singleton) + both hosts.
        assert_eq!(merge.inputs.len(), 3);
        assert_eq!(
            merge
                .inputs
                .iter()
                .filter(|(h, _)| h.starts_with(PLAN_REF_PREFIX))
                .count(),
            1
        );
        assert_eq!(merge.output_groups.len(), 1);
        assert_eq!(merge.output_groups[0].assets.len(), 3);
        // Policy `b` has no partner anywhere → its planned singleton demotes
        // to Clean at re-plan; no merge for it.
        assert!(!merge.output_groups[0].policy_id.starts_with('b'));
    }

    /// THE terminal-state guarantee: applying the plan's predicted outputs and
    /// re-planning with the same options finds nothing to do.
    #[test]
    fn plan_is_terminal_after_merge_pass() {
        const SHARED: u32 = 999_999;
        let mut wallet: Vec<UtxoApi> = (0..41u32)
            .map(|i| {
                utxo(
                    &format!("{i:064x}"),
                    5_000_000,
                    vec![
                        (pid_asset(SHARED, &format!("s{i}")), 1),
                        (pid_asset(i, "solo"), 1),
                    ],
                )
            })
            .collect();
        wallet.push(utxo(&"f".repeat(64), 100_000_000, vec![])); // Liquid
        let opts = DebagOptions {
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert!(plan.items.iter().any(|i| i.merge_pass));

        // Predicted post-state: untouched UTxOs + every planned output that
        // is NOT consumed by a merge tx (+ the merge outputs themselves), at
        // their min-UTxO values, plus a change stub.
        let spent: HashSet<(String, u32)> = plan
            .items
            .iter()
            .flat_map(|it| it.inputs.iter().cloned())
            .collect();
        let consumed_planned: HashSet<(usize, u32)> = plan
            .items
            .iter()
            .flat_map(|it| it.inputs.iter())
            .filter_map(|(h, g)| {
                h.strip_prefix(PLAN_REF_PREFIX)
                    .and_then(|n| n.parse().ok())
                    .map(|i: usize| (i, *g))
            })
            .collect();
        let mut post: Vec<UtxoApi> = wallet
            .iter()
            .filter(|u| !spent.contains(&(u.tx_hash.clone(), u.output_index)))
            .cloned()
            .collect();
        for (i, item) in plan.items.iter().enumerate() {
            for (g, grp) in item.output_groups.iter().enumerate() {
                if consumed_planned.contains(&(i, g as u32)) {
                    continue;
                }
                let mut u = utxo(
                    &format!("{i:032x}{g:032x}"),
                    grp.min_lovelace,
                    grp.assets.clone(),
                );
                u.output_index = g as u32;
                post.push(u);
            }
        }
        post.push(utxo(&"e".repeat(64), 50_000_000, vec![])); // change stub

        let replan = plan_debag(&post, &params(), &opts);
        assert!(
            replan.items.is_empty(),
            "re-plan after applying the plan should be empty, got {} items",
            replan.items.len()
        );
    }

    /// `sweep_dust` folds Dust-tier UTxOs into the per-policy outputs.
    #[test]
    fn plan_sweep_dust_consolidates() {
        let wallet = vec![
            // A Cluttered bag.
            utxo(
                &"a".repeat(64),
                4_000_000,
                vec![(asset_id('a', "x"), 1), (asset_id('b', "y"), 1)],
            ),
            // Dust holding another asset of policy `a`.
            utxo(&"b".repeat(64), 1_300_000, vec![(asset_id('a', "z"), 1)]),
        ];
        let default_plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        assert_eq!(
            default_plan.summary.bag_count, 1,
            "dust untouched by default"
        );

        let opts = DebagOptions {
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        let item = &plan.items[0];
        assert_eq!(item.inputs.len(), 2, "dust UTxO joins the tx");
        let a_group = item
            .output_groups
            .iter()
            .find(|g| g.policy_id == "a".repeat(56))
            .unwrap();
        assert_eq!(
            a_group.assets.len(),
            2,
            "dust asset merged into policy group"
        );
    }

    /// `collateral_target` creates the deficit on the first tx; build emits the
    /// 5-ADA outputs and stays value-balanced.
    #[test]
    fn collateral_created_and_built() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                30_000_000,
                vec![(asset_id('a', "x"), 1), (asset_id('b', "y"), 1)],
            ),
            utxo(&"c".repeat(64), 50_000_000, vec![]), // Liquid (not collateral band)
        ];
        let p = params();
        let opts = DebagOptions {
            collateral_target: 2,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &p, &opts);
        assert_eq!(plan.summary.collateral_created, 2);
        let item = &plan.items[0];
        assert_eq!(item.collateral_outputs, 2);

        let owner = Address::from_bech32(TEST_ADDR).unwrap();
        let built = build_debag(&wallet, item, &owner, &p, 0).unwrap();
        let outputs = built.unsigned.staging.outputs.as_ref().unwrap();
        // 2 policy groups + 2 collateral + 1 change.
        assert_eq!(outputs.len(), 5);
        let collateral_count = outputs
            .iter()
            .filter(|o| o.lovelace == 5_000_000 && o.assets.is_none())
            .count();
        assert_eq!(collateral_count, 2);
        // Value balance holds (bag self-funds: 30 ADA covers mins + 10 + fee).
        let out_lovelace: u64 = outputs.iter().map(|o| o.lovelace).sum();
        assert_eq!(out_lovelace + built.unsigned.fee, 30_000_000);
    }

    /// Collateral-only plan (clean wallet, deficit set): a standalone tx funded
    /// from the Liquid pool.
    #[test]
    fn collateral_only_plan_builds() {
        let wallet = vec![utxo(&"c".repeat(64), 50_000_000, vec![])];
        let p = params();
        let opts = DebagOptions {
            collateral_target: 2,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &p, &opts);
        assert_eq!(plan.items.len(), 1);
        let item = &plan.items[0];
        assert!(item.inputs.is_empty());
        assert_eq!(item.collateral_outputs, 2);

        let owner = Address::from_bech32(TEST_ADDR).unwrap();
        let built = build_debag(&wallet, item, &owner, &p, 0).unwrap();
        let outputs = built.unsigned.staging.outputs.as_ref().unwrap();
        // 2 collateral + change.
        assert_eq!(outputs.len(), 3);
        let out_lovelace: u64 = outputs.iter().map(|o| o.lovelace).sum();
        assert_eq!(out_lovelace + built.unsigned.fee, 50_000_000);
    }

    /// Affinity packing clusters same-policy dust into ONE tx and merges it
    /// into one output (the old blind pack scattered it 1:1 across txs).
    #[test]
    fn plan_affinity_merges_same_policy_dust() {
        let mut wallet = vec![utxo(&"f".repeat(64), 50_000_000, vec![])]; // Liquid
        for i in 0..5u32 {
            wallet.push(utxo(
                &format!("{i:064}"),
                1_300_000,
                vec![(asset_id('a', &format!("n{i}")), 1)],
            ));
        }
        let opts = DebagOptions {
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert_eq!(plan.items.len(), 1, "same-policy dust shares one tx");
        let item = &plan.items[0];
        assert_eq!(item.inputs.len(), 5);
        assert_eq!(
            item.output_groups.len(),
            1,
            "5 dust UTxOs merge into 1 output"
        );
        assert_eq!(item.output_groups[0].assets.len(), 5);
        assert!(
            plan.summary.locked_delta < 0,
            "consolidation frees locked ADA: {}",
            plan.summary.locked_delta
        );
    }

    /// Unmergeable dust (no same-policy partner anywhere) is already minimal —
    /// wallet-context classification ranks it Clean and the sweep leaves it.
    #[test]
    fn plan_leaves_unmergeable_dust_alone() {
        let wallet = vec![
            utxo(&"a".repeat(64), 1_300_000, vec![(asset_id('a', "only"), 1)]),
            utxo(&"b".repeat(64), 1_300_000, vec![(asset_id('b', "solo"), 1)]),
            utxo(&"f".repeat(64), 50_000_000, vec![]),
        ];
        let opts = DebagOptions {
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert!(
            plan.items.is_empty(),
            "re-emitting singletons at a fee is a no-op: {:?}",
            plan.summary
        );
    }

    /// A dust UTxO whose only merge partner is a Clean UTxO pulls that UTxO in
    /// as the merge host, consolidating both into one output.
    #[test]
    fn plan_dust_pulls_clean_merge_host() {
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                1_300_000,
                vec![(asset_id('a', "dusty"), 1)],
            ),
            utxo(
                &"b".repeat(64),
                3_000_000,
                vec![(asset_id('a', "kept1"), 1), (asset_id('a', "kept2"), 1)],
            ), // Clean host, same policy
            utxo(&"f".repeat(64), 50_000_000, vec![]),
        ];
        let opts = DebagOptions {
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert_eq!(plan.items.len(), 1);
        let item = &plan.items[0];
        assert_eq!(item.inputs.len(), 2, "dust + its Clean merge host");
        assert_eq!(item.output_groups.len(), 1);
        assert_eq!(item.output_groups[0].assets.len(), 3);
    }

    /// Bags sharing a policy pack together ahead of unrelated bags, so the
    /// shared policy lands in ONE output instead of one per tx.
    #[test]
    fn plan_affinity_prefers_shared_policy_bags() {
        // Two cluttered bags share policy `s`; a third is unrelated. All fit in
        // one tx anyway, but the shared policy must merge to a single group.
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                4_000_000,
                vec![(asset_id('s', "x1"), 1), (asset_id('b', "y"), 1)],
            ),
            utxo(
                &"b".repeat(64),
                4_000_000,
                vec![(asset_id('s', "x2"), 1), (asset_id('c', "z"), 1)],
            ),
            utxo(
                &"c".repeat(64),
                4_000_000,
                vec![(asset_id('d', "w"), 1), (asset_id('e', "v"), 1)],
            ),
        ];
        let plan = plan_debag(&wallet, &params(), &DebagOptions::default());
        assert_eq!(plan.items.len(), 1);
        let s_groups: Vec<_> = plan.items[0]
            .output_groups
            .iter()
            .filter(|g| g.policy_id == "s".repeat(56))
            .collect();
        assert_eq!(s_groups.len(), 1, "shared policy merges into one output");
        assert_eq!(s_groups[0].assets.len(), 2);
    }

    /// MajorPolicies: big holdings isolate; one-offs share a mixed drawer.
    #[test]
    fn strategy_major_policies_drawers_one_offs() {
        // Bloated bag: policy `a` × 5 (major at threshold 4) + three one-offs.
        let wallet = vec![utxo(
            &"a".repeat(64),
            20_000_000,
            vec![
                (asset_id('a', "a1"), 1),
                (asset_id('a', "a2"), 1),
                (asset_id('a', "a3"), 1),
                (asset_id('a', "a4"), 1),
                (asset_id('a', "a5"), 1),
                (asset_id('b', "solo"), 1),
                (asset_id('c', "solo"), 1),
                (asset_id('d', "solo"), 1),
            ],
        )];
        let opts = DebagOptions {
            strategy: UtxoDistributionStrategy::MajorPolicies,
            major_threshold: 4,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        let item = &plan.items[0];
        assert_eq!(item.output_groups.len(), 2, "one major + one drawer");
        let major = item
            .output_groups
            .iter()
            .find(|g| g.policy_id == "a".repeat(56))
            .expect("major policy isolated");
        assert_eq!(major.assets.len(), 5);
        let drawer = item
            .output_groups
            .iter()
            .find(|g| g.policy_id.starts_with("(mixed"))
            .expect("one-offs share a drawer");
        assert_eq!(drawer.assets.len(), 3);
    }

    /// Compact: everything asset-bearing (Clean included) consolidates into as
    /// few outputs as possible.
    #[test]
    fn strategy_compact_consolidates_clean_stacks() {
        // Three Clean single-policy UTxOs + one cluttered bag.
        let wallet = vec![
            utxo(&"a".repeat(64), 3_000_000, vec![(asset_id('a', "x"), 1)]),
            utxo(&"b".repeat(64), 3_000_000, vec![(asset_id('b', "y"), 1)]),
            utxo(&"c".repeat(64), 3_000_000, vec![(asset_id('c', "z"), 1)]),
            utxo(
                &"d".repeat(64),
                4_000_000,
                vec![(asset_id('d', "w"), 1), (asset_id('e', "v"), 1)],
            ),
        ];
        let opts = DebagOptions {
            strategy: UtxoDistributionStrategy::Compact,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert_eq!(plan.items.len(), 1);
        let item = &plan.items[0];
        assert_eq!(
            item.inputs.len(),
            4,
            "Clean UTxOs are in scope under Compact"
        );
        assert_eq!(item.output_groups.len(), 1, "everything in one output");
        assert_eq!(item.output_groups[0].assets.len(), 5);
        assert!(
            plan.summary.locked_delta < 0,
            "consolidation frees locked ADA"
        );
    }

    /// Re-running the SAME strategy on its own output is a no-op (identity
    /// guard) — the planner is idempotent.
    #[test]
    fn strategy_replan_is_idempotent() {
        // A wallet already in MajorPolicies shape: one major stack + one drawer.
        let wallet = vec![
            utxo(
                &"a".repeat(64),
                3_000_000,
                vec![
                    (asset_id('a', "a1"), 1),
                    (asset_id('a', "a2"), 1),
                    (asset_id('a', "a3"), 1),
                    (asset_id('a', "a4"), 1),
                ],
            ),
            utxo(
                &"b".repeat(64),
                3_000_000,
                vec![(asset_id('b', "solo"), 1), (asset_id('c', "solo"), 1)],
            ),
        ];
        let opts = DebagOptions {
            strategy: UtxoDistributionStrategy::MajorPolicies,
            major_threshold: 4,
            sweep_dust: true,
            ..Default::default()
        };
        let plan = plan_debag(&wallet, &params(), &opts);
        assert!(
            plan.items.is_empty(),
            "already in target shape — replan must be empty: {:?}",
            plan.summary
        );
    }

    /// Stale plan (input no longer in the pool) errors rather than mis-building.
    #[test]
    fn build_rejects_stale_plan() {
        let wallet = vec![utxo(
            &"a".repeat(64),
            10_000_000,
            vec![(asset_id('a', "x"), 1), (asset_id('b', "y"), 1)],
        )];
        let p = params();
        let plan = plan_debag(&wallet, &p, &DebagOptions::default());
        let owner = Address::from_bech32(TEST_ADDR).unwrap();
        // Simulate the bag being spent since planning.
        let fresh: Vec<UtxoApi> = vec![utxo(&"f".repeat(64), 10_000_000, vec![])];
        match build_debag(&fresh, &plan.items[0], &owner, &p, 0) {
            Err(TxBuildError::NoSuitableUtxo) => {}
            Err(other) => panic!("expected NoSuitableUtxo, got {other:?}"),
            Ok(_) => panic!("stale plan must not build"),
        }
    }
}
