//! Core UTxO optimization algorithm.
//!
//! Two-phase design:
//! 1. `compute_ideal_state` — instant preview of the optimized wallet. No TX
//!    size limits. Called on every settings change for real-time UI feedback.
//! 2. `build_optimization_steps` — diff the ideal state against the current
//!    wallet and chunk into TX-sized steps. Called when the user commits.
//!
//! Ported from the unfrackit JavaScript implementation (CC-BY-4.0, Adam Dean)
//! with modifications for multi-step support and Rust idioms.

use std::collections::{BTreeMap, HashMap, HashSet};

use cardano_assets::utxo::{AssetQuantity, UtxoApi, UtxoTag};
use cardano_assets::AssetId;

use crate::config::{AdaStrategy, FeeParams, OptimizeConfig};
use crate::plan::*;
use crate::size_estimator::{
    bail_size, digit_count, estimate_fee, estimate_min_lovelace_for_assets, estimate_tx_size,
    InputTokenData, OutputTokenData,
};

// ============================================================================
// Internal types
// ============================================================================

/// A token grouped for packing.
#[derive(Clone, Debug)]
struct Token {
    policy_id: String,
    asset_name_hex: String,
    quantity: u64,
}

/// Tokens grouped by policy for the packing algorithm.
#[derive(Clone, Debug)]
struct PolicyTokens {
    policy_id: String,
    tokens: Vec<Token>,
}

/// A reference to a wallet UTxO used in phase 1 (ideal state computation).
#[derive(Clone, Debug)]
struct IndexedUtxo {
    utxo: UtxoApi,
}

/// A proposed output before lovelace is assigned.
#[derive(Clone, Debug)]
struct ProposedOutput {
    tokens: Vec<Token>,
    kind: OutputKind,
    /// Which input UTxO refs contributed to this output.
    source_refs: Vec<String>,
}

/// A UTxO in the iterative working set (may be original or produced by a prior step).
#[derive(Clone, Debug)]
struct WorkingUtxo {
    utxo_ref: String,
    original_index: usize,
    lovelace: u64,
    assets: Vec<AssetQuantity>,
    tags: Vec<UtxoTag>,
    produced_by_step: Option<usize>,
}

// ============================================================================
// Phase 1: Ideal state computation (instant preview)
// ============================================================================

/// Result of computing the ideal wallet state.
#[derive(Debug, Clone)]
pub struct IdealState {
    /// The ideal set of token-bearing outputs.
    pub token_outputs: Vec<IdealOutput>,
    /// Summary statistics.
    pub summary: IdealSummary,
    /// UTxOs as UtxoApi for shelf classification (includes ADA outputs + excluded).
    pub as_utxos: Vec<UtxoApi>,
}

/// A single ideal output.
#[derive(Debug, Clone)]
pub struct IdealOutput {
    pub assets: Vec<AssetQuantity>,
    pub lovelace: u64,
    pub kind: OutputKind,
}

/// Summary statistics for the ideal state.
#[derive(Debug, Clone)]
pub struct IdealSummary {
    pub utxos_before: usize,
    pub utxos_after: usize,
    pub estimated_min_utxo_locked: u64,
    pub ada_freed: u64,
}

/// Compute the ideal end state for a wallet given optimization settings.
///
/// This is a fast, pure function with no TX size limits. It answers:
/// "If we could reorganize everything in one shot, what would the wallet look like?"
///
/// Used for real-time UI preview as the user tweaks settings.
pub fn compute_ideal_state(
    utxos: &[UtxoApi],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
) -> IdealState {
    let collateral_refs = select_collateral_to_preserve(utxos, &config.collateral);

    // Separate UTxOs into working set vs excluded (script-locked + collateral)
    let mut working: Vec<IndexedUtxo> = Vec::new();
    let mut excluded: Vec<&UtxoApi> = Vec::new();

    for u in utxos {
        let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
        if is_script_locked(u) || collateral_refs.contains(&utxo_ref) {
            excluded.push(u);
        } else {
            working.push(IndexedUtxo { utxo: u.clone() });
        }
    }

    // Classify all tokens across the working set
    let (fungibles, nonfungibles) = classify_tokens(&working);

    // Pack into ideal outputs (no TX size limit)
    let ideal_fungible = process_tokens(&fungibles, config, config.isolate_fungible);
    let ideal_nonfungible = process_tokens(&nonfungibles, config, config.isolate_nonfungible);

    // Build token outputs with min-UTxO lovelace
    let mut token_outputs: Vec<IdealOutput> = Vec::new();

    for tokens in ideal_fungible {
        let assets = tokens_to_assets(&tokens);
        let lovelace = estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &assets);
        let kind = if tokens.iter().all(|t| t.policy_id == tokens[0].policy_id) {
            if tokens[0].quantity > 1 {
                OutputKind::FungibleIsolate
            } else {
                OutputKind::PolicyBundle
            }
        } else {
            OutputKind::PolicyBundle
        };
        token_outputs.push(IdealOutput {
            assets,
            lovelace,
            kind,
        });
    }

    for tokens in ideal_nonfungible {
        let assets = tokens_to_assets(&tokens);
        let lovelace = estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &assets);
        let kind = if tokens.iter().all(|t| t.policy_id == tokens[0].policy_id) {
            OutputKind::NonfungibleIsolate
        } else {
            OutputKind::PolicyBundle
        };
        token_outputs.push(IdealOutput {
            assets,
            lovelace,
            kind,
        });
    }

    // Compute total ADA budget
    let total_lovelace: u64 = working.iter().map(|iu| iu.utxo.lovelace).sum();
    let token_lovelace: u64 = token_outputs.iter().map(|o| o.lovelace).sum();
    let mut remaining_ada = total_lovelace.saturating_sub(token_lovelace);

    // Create missing collateral UTxOs from the ADA pool.
    // Existing collateral is already in `excluded`. If the user wants more than
    // what currently exists, synthesize new ones at the preferred target amount.
    let mut ada_outputs: Vec<IdealOutput> = Vec::new();
    let existing_collateral_count = collateral_refs.len() as u32;

    if config.collateral.count > existing_collateral_count {
        let shortfall = config.collateral.count - existing_collateral_count;
        // Pick the smallest target as the preferred amount for new collateral
        let target = config
            .collateral
            .targets_lovelace
            .iter()
            .copied()
            .min()
            .unwrap_or(5_000_000);

        for _ in 0..shortfall {
            if remaining_ada >= target {
                ada_outputs.push(IdealOutput {
                    assets: vec![],
                    lovelace: target,
                    kind: OutputKind::Collateral,
                });
                remaining_ada -= target;
            }
        }
    }

    // Build remaining ADA outputs based on strategy
    match config.ada_strategy {
        AdaStrategy::Leave => {
            // Pass ADA-only UTxOs through unchanged as individual outputs
            for iu in &working {
                if iu.utxo.assets.is_empty() {
                    ada_outputs.push(IdealOutput {
                        assets: vec![],
                        lovelace: iu.utxo.lovelace,
                        kind: OutputKind::AdaRollup,
                    });
                }
            }
        }
        AdaStrategy::Split if remaining_ada > ADA_SPLIT_THRESHOLD => {
            let splits = [0.50, 0.15, 0.10, 0.10, 0.05, 0.05, 0.05];
            let mut accounted = 0u64;
            for (i, &pct) in splits.iter().enumerate() {
                let amount = if i == splits.len() - 1 {
                    remaining_ada.saturating_sub(accounted)
                } else {
                    (remaining_ada as f64 * pct) as u64
                };
                accounted += amount;
                if amount > 0 {
                    ada_outputs.push(IdealOutput {
                        assets: vec![],
                        lovelace: amount,
                        kind: OutputKind::AdaSplit,
                    });
                }
            }
        }
        AdaStrategy::Rollup | AdaStrategy::Split => {
            // Rollup, or Split below threshold — single output
            if remaining_ada >= fee_params.coins_per_utxo_byte * 160 {
                ada_outputs.push(IdealOutput {
                    assets: vec![],
                    lovelace: remaining_ada,
                    kind: OutputKind::AdaRollup,
                });
            }
        }
    }

    // Build as_utxos for shelf display: token outputs + ADA outputs + excluded pass-throughs
    let mut as_utxos: Vec<UtxoApi> = Vec::new();

    for (i, out) in token_outputs.iter().enumerate() {
        as_utxos.push(UtxoApi {
            tx_hash: format!("ideal_token_{i}"),
            output_index: 0,
            lovelace: out.lovelace,
            assets: out.assets.clone(),
            tags: vec![],
        });
    }

    for (i, out) in ada_outputs.iter().enumerate() {
        as_utxos.push(UtxoApi {
            tx_hash: format!("ideal_ada_{i}"),
            output_index: 0,
            lovelace: out.lovelace,
            assets: vec![],
            tags: vec![],
        });
    }

    // Pass excluded UTxOs through unchanged
    for u in &excluded {
        as_utxos.push((*u).clone());
    }

    // Compute summary
    let original_locked: u64 = working
        .iter()
        .map(|iu| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &iu.utxo.assets))
        .sum();
    let ideal_locked: u64 = token_outputs
        .iter()
        .map(|o| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &o.assets))
        .sum();

    let all_ideal_outputs: Vec<IdealOutput> = token_outputs
        .iter()
        .chain(ada_outputs.iter())
        .cloned()
        .collect();

    let summary = IdealSummary {
        utxos_before: utxos.len(),
        utxos_after: all_ideal_outputs.len() + excluded.len(),
        estimated_min_utxo_locked: ideal_locked,
        ada_freed: original_locked.saturating_sub(ideal_locked),
    };

    IdealState {
        token_outputs: all_ideal_outputs,
        summary,
        as_utxos,
    }
}

// ============================================================================
// Phase 2: Build optimization TX steps (deferred, called on commit)
// ============================================================================

/// Build an optimization plan: diff the ideal state against the current wallet
/// and chunk into TX-sized steps.
///
/// Each step consumes some current UTxOs and produces ideal-state outputs.
/// Steps are independent — each produces final-form outputs directly.
pub fn build_optimization_steps(
    utxos: &[UtxoApi],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
) -> OptimizationPlan {
    let collateral_refs = select_collateral_to_preserve(utxos, &config.collateral);
    let bail = bail_size(fee_params);

    // Separate excluded (script-locked + collateral) from working set
    let mut working: Vec<WorkingUtxo> = Vec::new();
    let mut excluded_snapshots: Vec<UtxoSnapshot> = Vec::new();

    for (i, u) in utxos.iter().enumerate() {
        let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
        if is_script_locked(u) || collateral_refs.contains(&utxo_ref) {
            excluded_snapshots.push(UtxoSnapshot {
                utxo_ref,
                lovelace: u.lovelace,
                assets: u.assets.clone(),
                tags: u.tags.clone(),
                produced_by_step: None,
                consumed_by_step: None,
            });
        } else {
            working.push(WorkingUtxo {
                utxo_ref,
                original_index: i,
                lovelace: u.lovelace,
                assets: u.assets.clone(),
                tags: u.tags.clone(),
                produced_by_step: None,
            });
        }
    }

    // Iterative: each step consumes some working UTxOs and produces clean outputs.
    // After each step, newly produced outputs replace consumed ones in the working set.
    let mut steps: Vec<OptimizationStep> = Vec::new();
    let mut step_index = 0;
    // Track UTxOs that repacking can't improve (e.g. mixed outputs the packer reproduces).
    let mut settled_refs: HashSet<String> = HashSet::new();

    loop {
        // Identify non-optimal UTxOs: anything that isn't already a clean single-policy
        // bundle within bundle_size, or a pure-ADA UTxO we want to leave alone.
        let mut candidates: Vec<usize> = Vec::new(); // indices into working
        let mut ada_only: Vec<usize> = Vec::new();

        // Track per-policy UTxO indices for consolidation detection
        let mut policy_utxo_indices: HashMap<&str, Vec<usize>> = HashMap::new();

        for (i, wu) in working.iter().enumerate() {
            if wu.assets.is_empty() {
                ada_only.push(i);
                continue;
            }

            let mut by_policy: HashMap<&str, usize> = HashMap::new();
            for a in &wu.assets {
                *by_policy.entry(a.asset_id.policy_id.as_str()).or_default() += 1;
            }

            if by_policy.len() == 1 {
                let (pid, count) = by_policy.into_iter().next().unwrap();
                policy_utxo_indices.entry(pid).or_default().push(i);
                if count > config.bundle_size as usize {
                    // Over bundle_size — needs splitting
                    candidates.push(i);
                }
                // Single-policy within limit: might consolidate with siblings (checked below)
            } else if !settled_refs.contains(&wu.utxo_ref) {
                // Multi-policy UTxOs need repacking — the ideal state
                // separates tokens by policy into single-policy outputs.
                // Skip if already settled (packer reproduced same mixed output).
                candidates.push(i);
            }
        }

        // Add single-policy UTxOs that have siblings and can be consolidated
        let candidate_set: HashSet<usize> = candidates.iter().copied().collect();
        for (pid, indices) in &policy_utxo_indices {
            if indices.len() <= 1 {
                continue;
            }
            // Total tokens across all UTxOs for this policy
            let total_tokens: usize = indices
                .iter()
                .map(|&i| {
                    working[i]
                        .assets
                        .iter()
                        .filter(|a| a.asset_id.policy_id.as_str() == *pid)
                        .count()
                })
                .sum();
            // Only consolidate if total exceeds what any single UTxO already holds
            // (i.e., there's actually a benefit to merging)
            let max_single = indices
                .iter()
                .map(|&i| {
                    working[i]
                        .assets
                        .iter()
                        .filter(|a| a.asset_id.policy_id.as_str() == *pid)
                        .count()
                })
                .max()
                .unwrap_or(0);
            if total_tokens <= config.bundle_size as usize && max_single == total_tokens {
                // Already have a single UTxO holding all tokens — no consolidation needed
                continue;
            }
            for &i in indices {
                if !candidate_set.contains(&i) {
                    candidates.push(i);
                }
            }
        }

        // Also add ADA-only UTxOs if we need to rollup/split
        let do_ada = config.ada_strategy != AdaStrategy::Leave;
        if do_ada && ada_only.len() > 1 {
            candidates.extend(&ada_only);
        }

        // Deduplicate and sort by index for deterministic order
        candidates.sort_unstable();
        candidates.dedup();

        if candidates.is_empty() {
            break;
        }

        // Greedily select inputs up to bail size
        let mut selected_indices: Vec<usize> = Vec::new();
        let mut cumulative_size: u64 = 0;

        for &ci in &candidates {
            let wu = &working[ci];
            // Estimate size contribution of this input
            let input_size: u64 = 37
                + wu.assets
                    .iter()
                    .map(|a| {
                        32 + (a.asset_id.asset_name_hex.len() as u64 / 2) + digit_count(a.quantity)
                    })
                    .sum::<u64>();

            if cumulative_size + input_size > bail && !selected_indices.is_empty() {
                continue; // Skip this one, try smaller ones
            }

            selected_indices.push(ci);
            cumulative_size += input_size;
        }

        if selected_indices.is_empty() {
            break;
        }

        // Collect all tokens from selected inputs
        let mut all_tokens: Vec<Token> = Vec::new();
        let mut total_input_lovelace: u64 = 0;

        for &si in &selected_indices {
            let wu = &working[si];
            total_input_lovelace += wu.lovelace;
            for aq in &wu.assets {
                all_tokens.push(Token {
                    policy_id: aq.asset_id.policy_id.clone(),
                    asset_name_hex: aq.asset_id.asset_name_hex.clone(),
                    quantity: aq.quantity,
                });
            }
        }

        // Repack tokens into clean outputs using the same packing algorithm
        let (fungibles, nonfungibles) = classify_token_list(&all_tokens);
        let packed_fungible = process_tokens(&fungibles, config, config.isolate_fungible);
        let packed_nonfungible = process_tokens(&nonfungibles, config, config.isolate_nonfungible);

        let mut proposed: Vec<ProposedOutput> = Vec::new();
        for tokens in packed_fungible.into_iter().chain(packed_nonfungible) {
            proposed.push(ProposedOutput {
                tokens,
                kind: OutputKind::PolicyBundle,
                source_refs: vec![],
            });
        }

        // Now check if the TX fits. If too many outputs, trim inputs.
        let mut step_inputs: HashMap<String, &WorkingUtxo> = HashMap::new();
        for &si in &selected_indices {
            let wu = &working[si];
            step_inputs.insert(wu.utxo_ref.clone(), wu);
        }

        let step_size = calc_step_size_working(&step_inputs, &proposed, fee_params);

        if step_size > bail && selected_indices.len() > 1 {
            // TX too large — binary search for max inputs that fit
            let mut lo = 1usize;
            let mut hi = selected_indices.len();
            let mut best = 1;

            while lo <= hi {
                let mid = (lo + hi) / 2;
                let trial_indices = &selected_indices[..mid];

                // Collect tokens for trial
                let mut trial_tokens: Vec<Token> = Vec::new();
                for &si in trial_indices {
                    for aq in &working[si].assets {
                        trial_tokens.push(Token {
                            policy_id: aq.asset_id.policy_id.clone(),
                            asset_name_hex: aq.asset_id.asset_name_hex.clone(),
                            quantity: aq.quantity,
                        });
                    }
                }
                let (tf, tnf) = classify_token_list(&trial_tokens);
                let pf = process_tokens(&tf, config, config.isolate_fungible);
                let pnf = process_tokens(&tnf, config, config.isolate_nonfungible);
                let trial_proposed: Vec<ProposedOutput> = pf
                    .into_iter()
                    .chain(pnf)
                    .map(|tokens| ProposedOutput {
                        tokens,
                        kind: OutputKind::PolicyBundle,
                        source_refs: vec![],
                    })
                    .collect();

                let mut trial_inputs: HashMap<String, &WorkingUtxo> = HashMap::new();
                for &si in trial_indices {
                    let wu = &working[si];
                    trial_inputs.insert(wu.utxo_ref.clone(), wu);
                }

                let trial_size = calc_step_size_working(&trial_inputs, &trial_proposed, fee_params);
                if trial_size <= bail {
                    best = mid;
                    lo = mid + 1;
                } else {
                    if mid == 0 {
                        break;
                    }
                    hi = mid - 1;
                }
            }

            // Rebuild with best count
            selected_indices.truncate(best);

            all_tokens.clear();
            total_input_lovelace = 0;
            for &si in &selected_indices {
                let wu = &working[si];
                total_input_lovelace += wu.lovelace;
                for aq in &wu.assets {
                    all_tokens.push(Token {
                        policy_id: aq.asset_id.policy_id.clone(),
                        asset_name_hex: aq.asset_id.asset_name_hex.clone(),
                        quantity: aq.quantity,
                    });
                }
            }

            let (fungibles, nonfungibles) = classify_token_list(&all_tokens);
            let pf = process_tokens(&fungibles, config, config.isolate_fungible);
            let pnf = process_tokens(&nonfungibles, config, config.isolate_nonfungible);

            proposed.clear();
            for tokens in pf.into_iter().chain(pnf) {
                proposed.push(ProposedOutput {
                    tokens,
                    kind: OutputKind::PolicyBundle,
                    source_refs: vec![],
                });
            }

            step_inputs.clear();
            for &si in &selected_indices {
                let wu = &working[si];
                step_inputs.insert(wu.utxo_ref.clone(), wu);
            }
        }

        // No-op detection: if repacking produces the same number of token outputs
        // as token inputs consumed, with the same total asset count, nothing improved.
        let input_token_count: usize = selected_indices
            .iter()
            .map(|&si| working[si].assets.len())
            .sum();
        let output_token_count: usize = proposed.iter().map(|p| p.tokens.len()).sum();
        let token_inputs = selected_indices
            .iter()
            .filter(|&&si| !working[si].assets.is_empty())
            .count();

        if proposed.len() == token_inputs
            && input_token_count == output_token_count
            && token_inputs > 0
        {
            // Repacking produced the same structure — no improvement possible
            break;
        }

        if selected_indices.len() == 1 && proposed.is_empty() {
            // Single ADA-only UTxO, nothing to consolidate
            break;
        }

        // Build finalized outputs with proper lovelace balancing
        let finalized = balance_ada_working(
            &step_inputs,
            &proposed,
            config,
            fee_params,
            step_index,
            total_input_lovelace,
        );

        let estimated_size = calc_step_size_working(&step_inputs, &proposed, fee_params);
        let estimated_fee = estimate_fee(estimated_size, fee_params);

        let inputs: Vec<InputRef> = selected_indices
            .iter()
            .map(|&si| {
                let wu = &working[si];
                InputRef {
                    utxo_ref: wu.utxo_ref.clone(),
                    original_index: wu.original_index,
                }
            })
            .collect();

        // Remove consumed UTxOs from working set (reverse order to preserve indices)
        let consumed_refs: HashSet<String> = selected_indices
            .iter()
            .map(|&si| working[si].utxo_ref.clone())
            .collect();

        working.retain(|wu| !consumed_refs.contains(&wu.utxo_ref));

        // Add new outputs to working set; mark multi-policy outputs as settled
        for out in &finalized {
            let policies: HashSet<&str> = out
                .assets
                .iter()
                .map(|a| a.asset_id.policy_id.as_str())
                .collect();
            if policies.len() > 1 {
                settled_refs.insert(out.output_id.clone());
            }
            working.push(WorkingUtxo {
                utxo_ref: out.output_id.clone(),
                original_index: usize::MAX, // synthetic
                lovelace: out.lovelace,
                assets: out.assets.clone(),
                tags: vec![],
                produced_by_step: Some(step_index),
            });
        }

        // Build resulting_utxos snapshot: current working set + excluded
        let mut resulting_utxos: Vec<UtxoSnapshot> = working
            .iter()
            .map(|wu| UtxoSnapshot {
                utxo_ref: wu.utxo_ref.clone(),
                lovelace: wu.lovelace,
                assets: wu.assets.clone(),
                tags: wu.tags.clone(),
                produced_by_step: wu.produced_by_step,
                consumed_by_step: None,
            })
            .collect();
        resulting_utxos.extend(excluded_snapshots.iter().cloned());

        steps.push(OptimizationStep {
            step_index,
            inputs,
            outputs: finalized,
            estimated_size,
            estimated_fee,
            resulting_utxos,
        });

        step_index += 1;

        // Safety cap
        if step_index >= 50 {
            break;
        }
    }

    let total_fees: u64 = steps.iter().map(|s| s.estimated_fee).sum();
    let utxos_after = if steps.is_empty() {
        utxos.len()
    } else {
        steps.last().unwrap().resulting_utxos.len()
    };

    let ada_freed = estimate_ada_freed(utxos, &steps, fee_params);

    OptimizationPlan {
        summary: PlanSummary {
            utxos_before: utxos.len(),
            utxos_after,
            total_fees,
            num_steps: steps.len(),
            ada_freed,
        },
        steps,
    }
}

/// Legacy entry point — computes both ideal state and steps.
pub fn optimize(
    utxos: &[UtxoApi],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
) -> OptimizationPlan {
    build_optimization_steps(utxos, config, fee_params)
}

// ============================================================================
// Token classification
// ============================================================================

/// Classify all tokens across UTxOs into fungible and nonfungible groups.
fn classify_tokens(working_set: &[IndexedUtxo]) -> (Vec<PolicyTokens>, Vec<PolicyTokens>) {
    let mut asset_totals: BTreeMap<String, (String, String, u64)> = BTreeMap::new();
    for iu in working_set {
        for aq in &iu.utxo.assets {
            let key = aq.asset_id.concatenated();
            asset_totals
                .entry(key)
                .and_modify(|(_, _, q)| *q += aq.quantity)
                .or_insert((
                    aq.asset_id.policy_id.clone(),
                    aq.asset_id.asset_name_hex.clone(),
                    aq.quantity,
                ));
        }
    }

    let mut fungible_map: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    let mut nonfungible_map: BTreeMap<String, Vec<Token>> = BTreeMap::new();

    for (policy_id, asset_name_hex, quantity) in asset_totals.values() {
        let token = Token {
            policy_id: policy_id.clone(),
            asset_name_hex: asset_name_hex.clone(),
            quantity: *quantity,
        };
        if *quantity > 1 {
            fungible_map
                .entry(policy_id.clone())
                .or_default()
                .push(token);
        } else {
            nonfungible_map
                .entry(policy_id.clone())
                .or_default()
                .push(token);
        }
    }

    let to_vec = |map: BTreeMap<String, Vec<Token>>| -> Vec<PolicyTokens> {
        map.into_iter()
            .map(|(policy_id, mut tokens)| {
                tokens.sort_by(|a, b| a.asset_name_hex.cmp(&b.asset_name_hex));
                PolicyTokens { policy_id, tokens }
            })
            .collect()
    };

    (to_vec(fungible_map), to_vec(nonfungible_map))
}

// ============================================================================
// Token packing
// ============================================================================

/// Process tokens with optional per-policy isolation.
fn process_tokens(
    groups: &[PolicyTokens],
    config: &OptimizeConfig,
    isolate: bool,
) -> Vec<Vec<Token>> {
    if isolate {
        let mut all = Vec::new();
        for group in groups {
            let packed = pack_tokens(std::slice::from_ref(group), config.bundle_size, true);
            all.extend(packed);
        }
        all
    } else {
        pack_tokens(groups, config.bundle_size, false)
    }
}

/// Bin-pack tokens into output bundles.
///
/// Same-policy tokens fill up to `bundle_size`. When mixing policies,
/// the limit is halved to keep outputs cleaner.
fn pack_tokens(policies: &[PolicyTokens], bundle_size: u32, skip_limits: bool) -> Vec<Vec<Token>> {
    let full_size = bundle_size as usize;
    let mixed_size = if skip_limits {
        full_size
    } else {
        (bundle_size / 2).max(1) as usize
    };

    let mut outputs: Vec<Vec<Token>> = Vec::new();
    let mut current_output: Vec<Token> = Vec::new();
    let mut bundled: usize = 0;
    let mut repack: Vec<PolicyTokens> = Vec::new();

    for group in policies {
        let is_mixed = bundled > 0;

        if is_mixed && group.tokens.len() + bundled > mixed_size {
            repack.push(group.clone());
            continue;
        }

        let limit = if is_mixed { mixed_size } else { full_size };
        let mut carry: Vec<Token> = Vec::new();

        for token in &group.tokens {
            if bundled >= limit {
                carry.push(token.clone());
            } else {
                current_output.push(token.clone());
                bundled += 1;
            }
        }

        if !carry.is_empty() {
            repack.push(PolicyTokens {
                policy_id: group.policy_id.clone(),
                tokens: carry,
            });
        }
    }

    if !current_output.is_empty() {
        outputs.push(current_output);
    }

    if !repack.is_empty() {
        let more = pack_tokens(&repack, bundle_size, skip_limits);
        outputs.extend(more);
    }

    outputs
}

const ADA_SPLIT_THRESHOLD: u64 = 100_000_000;

// ============================================================================
// Helpers
// ============================================================================

fn is_script_locked(utxo: &UtxoApi) -> bool {
    utxo.has_tag(UtxoTag::HasDatum)
        || utxo.has_tag(UtxoTag::HasScriptRef)
        || utxo.has_tag(UtxoTag::ScriptAddress)
}

fn select_collateral_to_preserve(
    utxos: &[UtxoApi],
    config: &crate::config::CollateralConfig,
) -> HashSet<String> {
    let mut preserved = HashSet::new();

    if config.count == 0 {
        return preserved;
    }

    let mut candidates: Vec<(String, u64)> = utxos
        .iter()
        .filter(|u| {
            u.assets.is_empty()
                && !is_script_locked(u)
                && u.lovelace
                    >= config
                        .targets_lovelace
                        .iter()
                        .copied()
                        .min()
                        .unwrap_or(5_000_000)
                && u.lovelace <= config.ceiling_lovelace
        })
        .map(|u| {
            let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
            (utxo_ref, u.lovelace)
        })
        .collect();

    if candidates.is_empty() {
        return preserved;
    }

    candidates.sort_by_key(|(_, lovelace)| {
        config
            .targets_lovelace
            .iter()
            .map(|&target| (*lovelace as i64 - target as i64).unsigned_abs())
            .min()
            .unwrap_or(u64::MAX)
    });

    for (utxo_ref, _) in candidates.into_iter().take(config.count as usize) {
        preserved.insert(utxo_ref);
    }

    preserved
}

fn tokens_to_assets(tokens: &[Token]) -> Vec<AssetQuantity> {
    tokens
        .iter()
        .map(|t| AssetQuantity {
            asset_id: AssetId::new_unchecked(t.policy_id.clone(), t.asset_name_hex.clone()),
            quantity: t.quantity,
        })
        .collect()
}

fn estimate_ada_freed(
    original_utxos: &[UtxoApi],
    steps: &[OptimizationStep],
    fee_params: &FeeParams,
) -> u64 {
    if steps.is_empty() {
        return 0;
    }

    let original_locked: u64 = original_utxos
        .iter()
        .filter(|u| !is_script_locked(u))
        .map(|u| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &u.assets))
        .sum();

    let final_utxos = &steps.last().unwrap().resulting_utxos;
    let final_locked: u64 = final_utxos
        .iter()
        .map(|u| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &u.assets))
        .sum();

    original_locked.saturating_sub(final_locked)
}

/// Classify a flat list of tokens (from consumed inputs) into fungible/nonfungible groups.
/// Unlike `classify_tokens` which works on `IndexedUtxo`, this works on raw `Token` lists
/// and aggregates quantities for the same asset.
fn classify_token_list(tokens: &[Token]) -> (Vec<PolicyTokens>, Vec<PolicyTokens>) {
    // Aggregate by unique asset
    let mut asset_totals: BTreeMap<String, (String, String, u64)> = BTreeMap::new();
    for t in tokens {
        let key = format!("{}{}", t.policy_id, t.asset_name_hex);
        asset_totals
            .entry(key)
            .and_modify(|(_, _, q)| *q += t.quantity)
            .or_insert((t.policy_id.clone(), t.asset_name_hex.clone(), t.quantity));
    }

    let mut fungible_map: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    let mut nonfungible_map: BTreeMap<String, Vec<Token>> = BTreeMap::new();

    for (policy_id, asset_name_hex, quantity) in asset_totals.values() {
        let token = Token {
            policy_id: policy_id.clone(),
            asset_name_hex: asset_name_hex.clone(),
            quantity: *quantity,
        };
        if *quantity > 1 {
            fungible_map
                .entry(policy_id.clone())
                .or_default()
                .push(token);
        } else {
            nonfungible_map
                .entry(policy_id.clone())
                .or_default()
                .push(token);
        }
    }

    let to_vec = |map: BTreeMap<String, Vec<Token>>| -> Vec<PolicyTokens> {
        map.into_iter()
            .map(|(policy_id, mut tokens)| {
                tokens.sort_by(|a, b| a.asset_name_hex.cmp(&b.asset_name_hex));
                PolicyTokens { policy_id, tokens }
            })
            .collect()
    };

    (to_vec(fungible_map), to_vec(nonfungible_map))
}

/// Estimate TX size for a step using WorkingUtxo inputs.
fn calc_step_size_working(
    inputs: &HashMap<String, &WorkingUtxo>,
    outputs: &[ProposedOutput],
    _fee_params: &FeeParams,
) -> u64 {
    let output_assets: Vec<Vec<AssetQuantity>> = outputs
        .iter()
        .map(|o| tokens_to_assets(&o.tokens))
        .collect();

    let output_refs: Vec<OutputTokenData<'_>> = output_assets
        .iter()
        .map(|a| OutputTokenData { assets: a })
        .collect();

    let input_refs: Vec<InputTokenData<'_>> = inputs
        .values()
        .map(|wu| InputTokenData { assets: &wu.assets })
        .collect();

    estimate_tx_size(inputs.len(), &output_refs, &input_refs)
}

/// Balance ADA across outputs for a step using WorkingUtxo inputs.
fn balance_ada_working(
    inputs: &HashMap<String, &WorkingUtxo>,
    proposed_outputs: &[ProposedOutput],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
    step_index: usize,
    total_input_lovelace: u64,
) -> Vec<PlannedOutput> {
    let mut finalized: Vec<PlannedOutput> = Vec::new();
    let mut token_lovelace_total: u64 = 0;

    for (i, proposed) in proposed_outputs.iter().enumerate() {
        let assets = tokens_to_assets(&proposed.tokens);
        let min_lovelace =
            estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &assets);

        token_lovelace_total += min_lovelace;

        finalized.push(PlannedOutput {
            output_id: format!("step{step_index}_out{i}"),
            lovelace: min_lovelace,
            assets,
            source_utxo_refs: proposed.source_refs.clone(),
            output_kind: proposed.kind,
        });
    }

    let estimated_size = calc_step_size_working(inputs, proposed_outputs, fee_params);
    let estimated_fee = estimate_fee(estimated_size, fee_params);

    let remaining = total_input_lovelace
        .saturating_sub(token_lovelace_total)
        .saturating_sub(estimated_fee);

    if remaining == 0 {
        return finalized;
    }

    if config.ada_strategy == AdaStrategy::Split && remaining > ADA_SPLIT_THRESHOLD {
        let splits = [0.50, 0.15, 0.10, 0.10, 0.05, 0.05, 0.05];
        let mut accounted = 0u64;
        for (j, &pct) in splits.iter().enumerate() {
            let amount = if j == splits.len() - 1 {
                remaining.saturating_sub(accounted)
            } else {
                (remaining as f64 * pct) as u64
            };
            accounted += amount;
            if amount > 0 {
                finalized.push(PlannedOutput {
                    output_id: format!("step{step_index}_ada{j}"),
                    lovelace: amount,
                    assets: vec![],
                    source_utxo_refs: vec![],
                    output_kind: OutputKind::AdaSplit,
                });
            }
        }
    } else if remaining >= fee_params.coins_per_utxo_byte * 160 {
        finalized.push(PlannedOutput {
            output_id: format!("step{step_index}_change"),
            lovelace: remaining,
            assets: vec![],
            source_utxo_refs: vec![],
            output_kind: OutputKind::Change,
        });
    }

    finalized
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CollateralConfig, FeeParams, OptimizeConfig};

    fn pure_ada_utxo(tx_hash: &str, lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn nft_utxo(tx_hash: &str, lovelace: u64, policy: &str, names: &[&str]) -> UtxoApi {
        let assets = names
            .iter()
            .map(|name| AssetQuantity {
                asset_id: AssetId::new_unchecked(policy.to_string(), hex::encode(name)),
                quantity: 1,
            })
            .collect();
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets,
            tags: vec![],
        }
    }

    fn ft_utxo(tx_hash: &str, lovelace: u64, policy: &str, name: &str, quantity: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets: vec![AssetQuantity {
                asset_id: AssetId::new_unchecked(policy.to_string(), hex::encode(name)),
                quantity,
            }],
            tags: vec![],
        }
    }

    fn bloated_utxo(tx_hash: &str, lovelace: u64, policies: &[(&str, &[&str])]) -> UtxoApi {
        let assets = policies
            .iter()
            .flat_map(|(policy, names)| {
                names.iter().map(move |name| AssetQuantity {
                    asset_id: AssetId::new_unchecked(policy.to_string(), hex::encode(name)),
                    quantity: 1,
                })
            })
            .collect();
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets,
            tags: vec![],
        }
    }

    fn script_utxo(tx_hash: &str, lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: tx_hash.to_string(),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![UtxoTag::HasDatum],
        }
    }

    const POLICY_A: &str = "aaaa0000000000000000000000000000000000000000000000000000";
    const POLICY_B: &str = "bbbb0000000000000000000000000000000000000000000000000000";

    // ========================================================================
    // Packing tests
    // ========================================================================

    #[test]
    fn test_pack_single_policy() {
        let groups = [PolicyTokens {
            policy_id: POLICY_A.to_string(),
            tokens: (0..5)
                .map(|i| Token {
                    policy_id: POLICY_A.to_string(),
                    asset_name_hex: format!("{i:08x}"),
                    quantity: 1,
                })
                .collect(),
        }];
        let outputs = pack_tokens(&groups, 30, false);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].len(), 5);
    }

    #[test]
    fn test_pack_exceeds_bundle_size() {
        let groups = [PolicyTokens {
            policy_id: POLICY_A.to_string(),
            tokens: (0..50)
                .map(|i| Token {
                    policy_id: POLICY_A.to_string(),
                    asset_name_hex: format!("{i:08x}"),
                    quantity: 1,
                })
                .collect(),
        }];
        let outputs = pack_tokens(&groups, 30, false);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].len(), 30);
        assert_eq!(outputs[1].len(), 20);
    }

    #[test]
    fn test_pack_mixed_policies_uses_half_bundle() {
        let groups = [
            PolicyTokens {
                policy_id: POLICY_A.to_string(),
                tokens: (0..10)
                    .map(|i| Token {
                        policy_id: POLICY_A.to_string(),
                        asset_name_hex: format!("{i:08x}"),
                        quantity: 1,
                    })
                    .collect(),
            },
            PolicyTokens {
                policy_id: POLICY_B.to_string(),
                tokens: (0..10)
                    .map(|i| Token {
                        policy_id: POLICY_B.to_string(),
                        asset_name_hex: format!("{i:08x}"),
                        quantity: 1,
                    })
                    .collect(),
            },
        ];
        let outputs = pack_tokens(&groups, 30, false);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].len(), 10);
        assert_eq!(outputs[1].len(), 10);
    }

    // ========================================================================
    // Token classification
    // ========================================================================

    #[test]
    fn test_classify_fungible_vs_nonfungible() {
        let utxos = [
            ft_utxo("tx1", 2_000_000, POLICY_A, "HOSKY", 1_000_000),
            nft_utxo("tx2", 2_000_000, POLICY_B, &["nft1", "nft2"]),
        ];
        let working_set: Vec<IndexedUtxo> = utxos
            .iter()
            .map(|u| IndexedUtxo { utxo: u.clone() })
            .collect();

        let (fungibles, nonfungibles) = classify_tokens(&working_set);
        assert_eq!(fungibles.len(), 1);
        assert_eq!(fungibles[0].policy_id, POLICY_A);
        assert_eq!(nonfungibles.len(), 1);
        assert_eq!(nonfungibles[0].policy_id, POLICY_B);
    }

    // ========================================================================
    // Ideal state tests
    // ========================================================================

    #[test]
    fn test_ideal_state_empty_wallet() {
        let ideal = compute_ideal_state(&[], &OptimizeConfig::default(), &FeeParams::default());
        assert_eq!(ideal.summary.utxos_before, 0);
        assert_eq!(ideal.summary.utxos_after, 0);
        assert!(ideal.as_utxos.is_empty());
    }

    #[test]
    fn test_ideal_state_single_ada_utxo() {
        let utxos = vec![pure_ada_utxo("tx1", 50_000_000)];
        let config = OptimizeConfig {
            collateral: crate::config::CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());
        // Single ADA UTxO → single ADA output + nothing else
        assert_eq!(ideal.summary.utxos_after, 1);
        assert!(ideal.as_utxos.iter().all(|u| u.assets.is_empty()));
    }

    #[test]
    fn test_ideal_state_consolidates_nfts() {
        let utxos = vec![
            nft_utxo("tx1", 2_000_000, POLICY_A, &["nft1"]),
            nft_utxo("tx2", 2_000_000, POLICY_A, &["nft2"]),
            nft_utxo("tx3", 2_000_000, POLICY_A, &["nft3"]),
        ];
        let ideal = compute_ideal_state(&utxos, &OptimizeConfig::default(), &FeeParams::default());

        // 3 single-NFT UTxOs → 1 bundled output (all same policy) + 1 ADA output
        let token_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| !u.assets.is_empty())
            .collect();
        assert_eq!(token_utxos.len(), 1);
        assert_eq!(token_utxos[0].assets.len(), 3);
    }

    #[test]
    fn test_ideal_state_preserves_collateral() {
        let utxos = vec![
            pure_ada_utxo("collateral5", 5_000_000),
            pure_ada_utxo("collateral10", 10_000_000),
            pure_ada_utxo("spare", 50_000_000),
            nft_utxo("nft1", 2_000_000, POLICY_A, &["a1"]),
        ];
        let config = OptimizeConfig {
            collateral: crate::config::CollateralConfig {
                count: 2,
                targets_lovelace: vec![5_000_000, 10_000_000],
                ceiling_lovelace: 15_000_000,
            },
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());

        // Collateral UTxOs should appear unchanged in as_utxos
        let collateral_5 = ideal.as_utxos.iter().find(|u| u.tx_hash == "collateral5");
        let collateral_10 = ideal.as_utxos.iter().find(|u| u.tx_hash == "collateral10");
        assert!(collateral_5.is_some(), "5 ADA collateral preserved");
        assert!(collateral_10.is_some(), "10 ADA collateral preserved");
        assert_eq!(collateral_5.unwrap().lovelace, 5_000_000);
        assert_eq!(collateral_10.unwrap().lovelace, 10_000_000);
    }

    #[test]
    fn test_ideal_state_excludes_script_locked() {
        let utxos = vec![
            script_utxo("script1", 5_000_000),
            nft_utxo("nft1", 2_000_000, POLICY_A, &["a1"]),
            pure_ada_utxo("ada1", 50_000_000),
        ];
        let ideal = compute_ideal_state(&utxos, &OptimizeConfig::default(), &FeeParams::default());

        // Script UTxO should pass through unchanged
        let script = ideal.as_utxos.iter().find(|u| u.tx_hash == "script1");
        assert!(script.is_some(), "Script UTxO passed through");
    }

    #[test]
    fn test_ideal_state_debloats_multi_policy_utxo() {
        // A bloated UTxO with tokens from 2 policies → should produce separate outputs
        let utxos = vec![bloated_utxo(
            "bloated1",
            5_000_000,
            &[(POLICY_A, &["a1", "a2"]), (POLICY_B, &["b1", "b2"])],
        )];
        let ideal = compute_ideal_state(&utxos, &OptimizeConfig::default(), &FeeParams::default());

        // Tokens should be separated by policy (or packed properly)
        let token_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| !u.assets.is_empty())
            .collect();
        // With bundle_size=30 and 4 total tokens, could be 1 mixed or 2 separate
        // Either way, total token count should be preserved
        let total_tokens: usize = token_utxos.iter().map(|u| u.assets.len()).sum();
        assert_eq!(total_tokens, 4);
    }

    // ========================================================================
    // Build steps tests
    // ========================================================================

    #[test]
    fn test_build_steps_empty_wallet() {
        let plan = build_optimization_steps(&[], &OptimizeConfig::default(), &FeeParams::default());
        assert_eq!(plan.steps.len(), 0);
    }

    #[test]
    fn test_build_steps_already_optimal() {
        // Single UTxO with one NFT — nothing to optimize
        let utxos = vec![nft_utxo("tx1", 2_000_000, POLICY_A, &["a1"])];
        let plan =
            build_optimization_steps(&utxos, &OptimizeConfig::default(), &FeeParams::default());
        assert_eq!(plan.steps.len(), 0);
    }

    #[test]
    fn test_build_steps_consolidates() {
        let utxos = vec![
            nft_utxo("tx1", 2_000_000, POLICY_A, &["a1", "a2"]),
            nft_utxo("tx2", 2_000_000, POLICY_A, &["a3", "a4"]),
        ];
        let plan =
            build_optimization_steps(&utxos, &OptimizeConfig::default(), &FeeParams::default());
        assert!(
            !plan.steps.is_empty(),
            "Should produce steps to consolidate"
        );
        assert!(plan.summary.utxos_after <= plan.summary.utxos_before);
    }

    #[test]
    fn test_build_steps_ada_rollup() {
        let utxos = vec![
            pure_ada_utxo("tx1", 10_000_000),
            pure_ada_utxo("tx2", 10_000_000),
            pure_ada_utxo("tx3", 10_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            ..Default::default()
        };
        let plan = build_optimization_steps(&utxos, &config, &FeeParams::default());
        if !plan.steps.is_empty() {
            assert!(plan.summary.total_fees > 0);
        }
    }

    #[test]
    fn test_build_steps_single_ada_utxo_noop() {
        let utxos = vec![pure_ada_utxo("tx1", 50_000_000)];
        let plan = build_optimization_steps(
            &utxos,
            &OptimizeConfig {
                ada_strategy: AdaStrategy::Rollup,
                ..Default::default()
            },
            &FeeParams::default(),
        );
        assert_eq!(plan.steps.len(), 0, "Single ADA UTxO should be no-op");
    }

    #[test]
    fn test_build_steps_preserves_collateral() {
        let utxos = vec![
            pure_ada_utxo("collateral5", 5_000_000),
            pure_ada_utxo("collateral10", 10_000_000),
            pure_ada_utxo("spare1", 20_000_000),
            pure_ada_utxo("spare2", 30_000_000),
            pure_ada_utxo("spare3", 40_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: crate::config::CollateralConfig {
                count: 2,
                targets_lovelace: vec![5_000_000, 10_000_000],
                ceiling_lovelace: 15_000_000,
            },
            ..Default::default()
        };
        let plan = build_optimization_steps(&utxos, &config, &FeeParams::default());

        let all_input_refs: HashSet<&str> = plan
            .steps
            .iter()
            .flat_map(|s| s.inputs.iter().map(|i| i.utxo_ref.as_str()))
            .collect();

        assert!(
            !all_input_refs.contains("collateral5#0"),
            "5 ADA collateral preserved"
        );
        assert!(
            !all_input_refs.contains("collateral10#0"),
            "10 ADA collateral preserved"
        );
    }

    #[test]
    fn test_build_steps_collateral_in_resulting_utxos() {
        let utxos = vec![
            pure_ada_utxo("collateral5", 5_000_000),
            pure_ada_utxo("spare1", 20_000_000),
            pure_ada_utxo("spare2", 30_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: crate::config::CollateralConfig {
                count: 1,
                targets_lovelace: vec![5_000_000],
                ceiling_lovelace: 15_000_000,
            },
            ..Default::default()
        };
        let plan = build_optimization_steps(&utxos, &config, &FeeParams::default());

        if !plan.steps.is_empty() {
            let last = plan.steps.last().unwrap();
            let has_collateral = last
                .resulting_utxos
                .iter()
                .any(|u| u.utxo_ref == "collateral5#0");
            assert!(has_collateral, "Collateral must appear in resulting_utxos");
        }
    }

    #[test]
    fn test_collateral_zero_count_preserves_nothing() {
        let utxos = vec![
            pure_ada_utxo("tx1", 5_000_000),
            pure_ada_utxo("tx2", 10_000_000),
            pure_ada_utxo("tx3", 20_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: crate::config::CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_optimization_steps(&utxos, &config, &FeeParams::default());
        if !plan.steps.is_empty() {
            assert!(
                plan.steps[0].inputs.len() >= 2,
                "All UTxOs available when no collateral preserved"
            );
        }
    }

    #[test]
    fn test_collateral_prefers_closest_to_targets() {
        let utxos = vec![
            pure_ada_utxo("exact5", 5_000_000),
            pure_ada_utxo("mid7", 7_000_000),
            pure_ada_utxo("exact10", 10_000_000),
            pure_ada_utxo("far14", 14_000_000),
        ];
        let config = crate::config::CollateralConfig {
            count: 2,
            targets_lovelace: vec![5_000_000, 10_000_000],
            ceiling_lovelace: 15_000_000,
        };
        let preserved = select_collateral_to_preserve(&utxos, &config);
        assert_eq!(preserved.len(), 2);
        assert!(preserved.contains("exact5#0"));
        assert!(preserved.contains("exact10#0"));
    }

    #[test]
    fn test_collateral_ignores_token_utxos() {
        let utxos = vec![
            nft_utxo("nft_utxo", 5_000_000, POLICY_A, &["nft1"]),
            pure_ada_utxo("ada_utxo", 10_000_000),
        ];
        let config = crate::config::CollateralConfig {
            count: 2,
            targets_lovelace: vec![5_000_000, 10_000_000],
            ceiling_lovelace: 15_000_000,
        };
        let preserved = select_collateral_to_preserve(&utxos, &config);
        assert_eq!(preserved.len(), 1);
        assert!(preserved.contains("ada_utxo#0"));
    }

    // ========================================================================
    // Multi-policy splitting tests
    // ========================================================================

    #[test]
    fn test_bloated_utxo_gets_debloated() {
        // A UTxO with many tokens from many policies (exceeds mixed_limit)
        // should be split into cleaner outputs.
        let many_policies: Vec<(&str, &[&str])> = vec![
            (
                POLICY_A,
                &["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "a10"],
            ),
            (
                POLICY_B,
                &["b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "b10"],
            ),
        ];
        let utxos = vec![bloated_utxo("tx1", 10_000_000, &many_policies)];
        let config = OptimizeConfig {
            bundle_size: 10, // mixed_limit = 5
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_optimization_steps(&utxos, &config, &FeeParams::default());
        assert!(!plan.steps.is_empty(), "Should need optimization");
        let final_step = plan.steps.last().unwrap();
        // All 20 tokens should be preserved
        let total_assets: usize = final_step
            .resulting_utxos
            .iter()
            .map(|u| u.assets.len())
            .sum();
        assert_eq!(total_assets, 20);
        // Should produce multiple outputs (splitting the bloated UTxO)
        let token_utxos: Vec<_> = final_step
            .resulting_utxos
            .iter()
            .filter(|u| !u.assets.is_empty())
            .collect();
        assert!(
            token_utxos.len() > 1,
            "Expected split into multiple outputs, got {}",
            token_utxos.len()
        );
    }

    // ========================================================================
    // Isolate tests
    // ========================================================================

    #[test]
    fn test_isolate_nonfungible() {
        let utxos = vec![
            nft_utxo("tx1", 2_000_000, POLICY_A, &["a1", "a2"]),
            nft_utxo("tx2", 2_000_000, POLICY_B, &["b1", "b2"]),
        ];
        let config = OptimizeConfig {
            isolate_nonfungible: true,
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());

        let token_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| !u.assets.is_empty())
            .collect();
        // Each policy should be in its own output
        assert!(token_utxos.len() >= 2);

        // Check no output mixes policies
        for u in &token_utxos {
            let policies: HashSet<&str> = u
                .assets
                .iter()
                .map(|a| a.asset_id.policy_id.as_str())
                .collect();
            assert_eq!(policies.len(), 1, "Each output should have single policy");
        }
    }

    #[test]
    fn test_ada_split() {
        let utxos = vec![pure_ada_utxo("tx1", 200_000_000)]; // 200 ADA
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Split,
            collateral: crate::config::CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());

        let ada_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| u.assets.is_empty())
            .collect();
        assert_eq!(ada_utxos.len(), 7, "Should split into 7 ADA outputs");

        let total_split: u64 = ada_utxos.iter().map(|u| u.lovelace).sum();
        assert_eq!(total_split, 200_000_000, "Total ADA should be preserved");
    }

    #[test]
    fn test_no_split_below_threshold() {
        let utxos = vec![pure_ada_utxo("tx1", 50_000_000)]; // 50 ADA < 100 ADA threshold
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Split,
            collateral: crate::config::CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());

        let ada_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| u.assets.is_empty())
            .collect();
        assert_eq!(ada_utxos.len(), 1, "Should not split below threshold");
    }

    #[test]
    fn test_collateral_creation_from_ada_pool() {
        // Wallet has no collateral-sized UTxOs, but user wants 3.
        // Should create them from the ADA pool.
        let utxos = vec![pure_ada_utxo("tx1", 100_000_000)]; // 100 ADA, above ceiling
        let config = OptimizeConfig {
            collateral: crate::config::CollateralConfig {
                count: 3,
                targets_lovelace: vec![5_000_000],
                ceiling_lovelace: 15_000_000,
            },
            ..Default::default()
        };
        let ideal = compute_ideal_state(&utxos, &config, &FeeParams::default());

        // Should have 3 collateral outputs at 5 ADA each + 1 remaining ADA
        let collateral_utxos: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| u.lovelace == 5_000_000 && u.assets.is_empty())
            .collect();
        assert_eq!(
            collateral_utxos.len(),
            3,
            "Should create 3 collateral UTxOs"
        );

        // Remaining ADA should be 100 - 15 = 85 ADA
        let remaining: Vec<_> = ideal
            .as_utxos
            .iter()
            .filter(|u| u.lovelace > 15_000_000 && u.assets.is_empty())
            .collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].lovelace, 85_000_000);
    }
}
