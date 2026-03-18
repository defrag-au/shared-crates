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
    bail_size, estimate_fee, estimate_min_lovelace_for_assets, estimate_tx_size, InputTokenData,
    OutputTokenData,
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

/// An indexed reference to a wallet UTxO.
#[derive(Clone, Debug)]
struct IndexedUtxo {
    index: usize,
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

    for (i, u) in utxos.iter().enumerate() {
        let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
        if is_script_locked(u) || collateral_refs.contains(&utxo_ref) {
            excluded.push(u);
        } else {
            working.push(IndexedUtxo {
                index: i,
                utxo: u.clone(),
            });
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

    // Build working set (excludes script-locked + collateral)
    let working: Vec<IndexedUtxo> = utxos
        .iter()
        .enumerate()
        .filter(|(_, u)| {
            let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
            !is_script_locked(u) && !collateral_refs.contains(&utxo_ref)
        })
        .map(|(i, u)| IndexedUtxo {
            index: i,
            utxo: u.clone(),
        })
        .collect();

    // Classify and pack ideal outputs (same as compute_ideal_state)
    let (fungibles, nonfungibles) = classify_tokens(&working);
    let ideal_fungible = process_tokens(&fungibles, config, config.isolate_fungible);
    let ideal_nonfungible = process_tokens(&nonfungibles, config, config.isolate_nonfungible);

    let all_ideal: Vec<Vec<Token>> = ideal_fungible
        .into_iter()
        .chain(ideal_nonfungible)
        .collect();

    // For each ideal output, find which UTxOs contain its tokens.
    // An ideal output is "already satisfied" if a single UTxO contains exactly
    // those tokens (and only those tokens).
    let bail = bail_size(fee_params);

    // Build a lookup: utxo_ref -> IndexedUtxo
    let utxo_map: HashMap<String, &IndexedUtxo> = working
        .iter()
        .map(|iu| {
            let r = format!("{}#{}", iu.utxo.tx_hash, iu.utxo.output_index);
            (r, iu)
        })
        .collect();

    // For each ideal output, determine needed inputs and whether it's already optimal.
    struct PendingOutput<'a> {
        tokens: Vec<Token>,
        needed_inputs: Vec<(String, &'a IndexedUtxo)>,
    }

    let mut pending: Vec<PendingOutput<'_>> = Vec::new();
    let mut already_optimal_refs: HashSet<String> = HashSet::new();

    for ideal_tokens in &all_ideal {
        let needed = find_needed_inputs(ideal_tokens, &working, &already_optimal_refs);

        // Check if already optimal: single input with exact same assets
        if needed.len() == 1 {
            let iu = needed[0].1;
            if is_exact_match(&iu.utxo, ideal_tokens) {
                already_optimal_refs.insert(needed[0].0.clone());
                continue;
            }
        }

        if needed.is_empty() {
            continue;
        }

        pending.push(PendingOutput {
            tokens: ideal_tokens.clone(),
            needed_inputs: needed,
        });
    }

    // Identify ADA-only UTxOs for rollup (not already used as optimal matches)
    let ada_only_refs: Vec<String> = working
        .iter()
        .filter(|iu| iu.utxo.assets.is_empty())
        .map(|iu| format!("{}#{}", iu.utxo.tx_hash, iu.utxo.output_index))
        .filter(|r| !already_optimal_refs.contains(r))
        .collect();

    // If nothing to do, return empty plan
    let do_ada_rollup = config.ada_strategy != AdaStrategy::Leave;

    if pending.is_empty() && (ada_only_refs.len() <= 1 || !do_ada_rollup) {
        return empty_plan(utxos.len());
    }

    // Chunk pending outputs into TX-sized steps.
    // Each step greedily accumulates outputs until the bail threshold.
    let mut steps: Vec<OptimizationStep> = Vec::new();
    let mut consumed_refs: HashSet<String> = HashSet::new();

    let mut remaining_pending: Vec<&PendingOutput<'_>> = pending.iter().collect();
    let mut remaining_ada_refs: Vec<&str> = ada_only_refs.iter().map(|s| s.as_str()).collect();

    let mut step_index = 0;

    while !remaining_pending.is_empty() || (!remaining_ada_refs.is_empty() && do_ada_rollup) {
        let mut step_inputs: HashMap<String, &IndexedUtxo> = HashMap::new();
        let mut step_outputs: Vec<ProposedOutput> = Vec::new();
        let mut step_size: u64 = 0;
        let mut processed_indices: Vec<usize> = Vec::new();

        // Add token outputs
        for (idx, po) in remaining_pending.iter().enumerate() {
            // Check if any needed input was already consumed in a prior step
            let all_available = po
                .needed_inputs
                .iter()
                .all(|(r, _)| !consumed_refs.contains(r) || step_inputs.contains_key(r));

            if !all_available {
                continue;
            }

            // Estimate size if we add this output + its new inputs
            let new_inputs: Vec<(&str, &IndexedUtxo)> = po
                .needed_inputs
                .iter()
                .filter(|(r, _)| !step_inputs.contains_key(r))
                .map(|(r, iu)| (r.as_str(), *iu))
                .collect();

            let size_delta = estimate_addition_size(&po.tokens, &new_inputs);

            if step_size + size_delta > bail && !step_inputs.is_empty() {
                // Would exceed bail — skip for now (try in next step)
                continue;
            }

            // Add this output
            for (r, iu) in &po.needed_inputs {
                step_inputs.entry(r.clone()).or_insert(iu);
            }
            step_outputs.push(ProposedOutput {
                tokens: po.tokens.clone(),
                kind: classify_output_kind(&po.tokens),
                source_refs: po.needed_inputs.iter().map(|(r, _)| r.clone()).collect(),
            });

            step_size = calc_step_size(&step_inputs, &step_outputs);
            processed_indices.push(idx);

            if step_size >= bail {
                break;
            }
        }

        // Remove processed outputs (reverse order to preserve indices)
        processed_indices.sort_unstable();
        for &idx in processed_indices.iter().rev() {
            remaining_pending.remove(idx);
        }

        // Backfill: tokens from selected inputs not in any output
        let backfill = compute_backfill(&step_inputs, &step_outputs, config);
        step_outputs.extend(backfill);

        // Add ADA-only rollup if space remains
        if do_ada_rollup && step_size < bail {
            let mut ada_consumed: Vec<usize> = Vec::new();
            for (i, &ada_ref) in remaining_ada_refs.iter().enumerate() {
                if step_inputs.contains_key(ada_ref) {
                    ada_consumed.push(i);
                    continue;
                }
                if let Some(iu) = utxo_map.get(ada_ref) {
                    step_inputs.insert(ada_ref.to_string(), iu);
                    step_size = calc_step_size(&step_inputs, &step_outputs);
                    ada_consumed.push(i);
                    if step_size >= bail {
                        break;
                    }
                }
            }
            for &i in ada_consumed.iter().rev() {
                remaining_ada_refs.remove(i);
            }
        }

        // If nothing got selected, break to avoid infinite loop
        if step_inputs.is_empty() {
            break;
        }

        // Check for no-op: only ADA inputs and count <= 1
        if step_outputs.is_empty()
            && step_inputs.values().all(|iu| iu.utxo.assets.is_empty())
            && step_inputs.len() <= 1
            && config.ada_strategy != AdaStrategy::Split
        {
            break;
        }

        // Balance ADA for this step
        let finalized_outputs =
            balance_ada(&step_inputs, &step_outputs, config, fee_params, step_index);

        let estimated_size = calc_step_size(&step_inputs, &step_outputs);
        let estimated_fee = estimate_fee(estimated_size, fee_params);

        // Track consumed refs
        for r in step_inputs.keys() {
            consumed_refs.insert(r.clone());
        }

        // Build resulting_utxos: all non-consumed working UTxOs + new outputs
        let mut resulting_utxos: Vec<UtxoSnapshot> = Vec::new();

        // Untouched working UTxOs
        for iu in &working {
            let r = format!("{}#{}", iu.utxo.tx_hash, iu.utxo.output_index);
            if !consumed_refs.contains(&r) {
                resulting_utxos.push(UtxoSnapshot {
                    utxo_ref: r,
                    lovelace: iu.utxo.lovelace,
                    assets: iu.utxo.assets.clone(),
                    tags: iu.utxo.tags.clone(),
                    produced_by_step: None,
                    consumed_by_step: None,
                });
            }
        }

        // New outputs from all previous steps
        for prev_step in &steps {
            for out in &prev_step.outputs {
                resulting_utxos.push(UtxoSnapshot {
                    utxo_ref: out.output_id.clone(),
                    lovelace: out.lovelace,
                    assets: out.assets.clone(),
                    tags: vec![],
                    produced_by_step: Some(prev_step.step_index),
                    consumed_by_step: None,
                });
            }
        }

        // New outputs from this step
        for out in &finalized_outputs {
            resulting_utxos.push(UtxoSnapshot {
                utxo_ref: out.output_id.clone(),
                lovelace: out.lovelace,
                assets: out.assets.clone(),
                tags: vec![],
                produced_by_step: Some(step_index),
                consumed_by_step: None,
            });
        }

        // Excluded UTxOs (collateral + script-locked) — always present
        for u in utxos.iter() {
            let r = format!("{}#{}", u.tx_hash, u.output_index);
            if is_script_locked(u) || collateral_refs.contains(&r) {
                resulting_utxos.push(UtxoSnapshot {
                    utxo_ref: r,
                    lovelace: u.lovelace,
                    assets: u.assets.clone(),
                    tags: u.tags.clone(),
                    produced_by_step: None,
                    consumed_by_step: None,
                });
            }
        }

        let inputs: Vec<InputRef> = step_inputs
            .iter()
            .map(|(r, iu)| InputRef {
                utxo_ref: r.clone(),
                original_index: iu.index,
            })
            .collect();

        steps.push(OptimizationStep {
            step_index,
            inputs,
            outputs: finalized_outputs,
            estimated_size,
            estimated_fee,
            resulting_utxos,
        });

        step_index += 1;

        // Safety cap
        if step_index >= 20 {
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

fn empty_plan(utxos_before: usize) -> OptimizationPlan {
    OptimizationPlan {
        summary: PlanSummary {
            utxos_before,
            utxos_after: utxos_before,
            total_fees: 0,
            num_steps: 0,
            ada_freed: 0,
        },
        steps: vec![],
    }
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

// ============================================================================
// Input selection & matching
// ============================================================================

/// Find which UTxOs contain the tokens for an ideal output.
fn find_needed_inputs<'a>(
    ideal_tokens: &[Token],
    working_set: &'a [IndexedUtxo],
    skip_refs: &HashSet<String>,
) -> Vec<(String, &'a IndexedUtxo)> {
    let mut needed: Vec<(String, &IndexedUtxo)> = Vec::new();
    let mut needed_refs: HashSet<String> = HashSet::new();

    for token in ideal_tokens {
        for iu in working_set {
            if iu.utxo.assets.is_empty() {
                continue;
            }

            let has_token = iu.utxo.assets.iter().any(|aq| {
                aq.asset_id.policy_id == token.policy_id
                    && aq.asset_id.asset_name_hex == token.asset_name_hex
            });

            if has_token {
                let utxo_ref = format!("{}#{}", iu.utxo.tx_hash, iu.utxo.output_index);
                if !needed_refs.contains(&utxo_ref) && !skip_refs.contains(&utxo_ref) {
                    needed_refs.insert(utxo_ref.clone());
                    needed.push((utxo_ref, iu));
                }
            }
        }
    }

    needed
}

/// Check if a UTxO already has exactly the tokens of an ideal output (and nothing else).
fn is_exact_match(utxo: &UtxoApi, ideal_tokens: &[Token]) -> bool {
    if utxo.assets.len() != ideal_tokens.len() {
        return false;
    }

    for token in ideal_tokens {
        let found = utxo.assets.iter().any(|aq| {
            aq.asset_id.policy_id == token.policy_id
                && aq.asset_id.asset_name_hex == token.asset_name_hex
                && aq.quantity == token.quantity
        });
        if !found {
            return false;
        }
    }

    true
}

/// Classify an output kind from its tokens.
fn classify_output_kind(tokens: &[Token]) -> OutputKind {
    if tokens.is_empty() {
        return OutputKind::AdaRollup;
    }
    let policies: HashSet<&str> = tokens.iter().map(|t| t.policy_id.as_str()).collect();
    if policies.len() == 1 {
        if tokens.iter().any(|t| t.quantity > 1) {
            OutputKind::FungibleIsolate
        } else {
            OutputKind::PolicyBundle
        }
    } else {
        OutputKind::PolicyBundle
    }
}

// ============================================================================
// Backfill
// ============================================================================

/// Find tokens from selected inputs that aren't accounted for in any output.
fn compute_backfill(
    selected_inputs: &HashMap<String, &IndexedUtxo>,
    selected_outputs: &[ProposedOutput],
    config: &OptimizeConfig,
) -> Vec<ProposedOutput> {
    let mut output_assets: HashSet<String> = HashSet::new();
    for output in selected_outputs {
        for token in &output.tokens {
            output_assets.insert(format!("{}{}", token.policy_id, token.asset_name_hex));
        }
    }

    let mut surplus_fungible: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    let mut surplus_nonfungible: BTreeMap<String, Vec<Token>> = BTreeMap::new();

    for iu in selected_inputs.values() {
        for aq in &iu.utxo.assets {
            let key = format!("{}{}", aq.asset_id.policy_id, aq.asset_id.asset_name_hex);
            if !output_assets.contains(&key) {
                let token = Token {
                    policy_id: aq.asset_id.policy_id.clone(),
                    asset_name_hex: aq.asset_id.asset_name_hex.clone(),
                    quantity: aq.quantity,
                };
                if aq.quantity > 1 {
                    surplus_fungible
                        .entry(aq.asset_id.policy_id.clone())
                        .or_default()
                        .push(token);
                } else {
                    surplus_nonfungible
                        .entry(aq.asset_id.policy_id.clone())
                        .or_default()
                        .push(token);
                }
            }
        }
    }

    let mut backfill_outputs = Vec::new();

    let to_policy_tokens = |map: BTreeMap<String, Vec<Token>>| -> Vec<PolicyTokens> {
        map.into_iter()
            .map(|(policy_id, tokens)| PolicyTokens { policy_id, tokens })
            .collect()
    };

    if !surplus_fungible.is_empty() {
        let groups = to_policy_tokens(surplus_fungible);
        let packed = process_tokens(&groups, config, config.isolate_fungible);
        for tokens in packed {
            backfill_outputs.push(ProposedOutput {
                tokens,
                kind: OutputKind::Backfill,
                source_refs: vec![],
            });
        }
    }

    if !surplus_nonfungible.is_empty() {
        let groups = to_policy_tokens(surplus_nonfungible);
        let packed = process_tokens(&groups, config, config.isolate_nonfungible);
        for tokens in packed {
            backfill_outputs.push(ProposedOutput {
                tokens,
                kind: OutputKind::Backfill,
                source_refs: vec![],
            });
        }
    }

    backfill_outputs
}

// ============================================================================
// ADA balancing
// ============================================================================

const ADA_SPLIT_THRESHOLD: u64 = 100_000_000;

/// Assign lovelace to each output and create ADA change/split outputs.
fn balance_ada(
    selected_inputs: &HashMap<String, &IndexedUtxo>,
    proposed_outputs: &[ProposedOutput],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
    step_index: usize,
) -> Vec<PlannedOutput> {
    let total_input_lovelace: u64 = selected_inputs.values().map(|iu| iu.utxo.lovelace).sum();

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

    let estimated_size = calc_step_size(selected_inputs, proposed_outputs);
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

fn calc_step_size(inputs: &HashMap<String, &IndexedUtxo>, outputs: &[ProposedOutput]) -> u64 {
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
        .map(|iu| InputTokenData {
            assets: &iu.utxo.assets,
        })
        .collect();

    estimate_tx_size(inputs.len(), &output_refs, &input_refs)
}

fn estimate_addition_size(tokens: &[Token], new_inputs: &[(&str, &IndexedUtxo)]) -> u64 {
    let assets = tokens_to_assets(tokens);
    let output_data = [OutputTokenData { assets: &assets }];

    let input_refs: Vec<InputTokenData<'_>> = new_inputs
        .iter()
        .map(|(_, iu)| InputTokenData {
            assets: &iu.utxo.assets,
        })
        .collect();

    let full_size = estimate_tx_size(new_inputs.len(), &output_data, &input_refs);
    let base_overhead = full_size.min(292);
    full_size.saturating_sub(base_overhead)
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FeeParams, OptimizeConfig};

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
            .enumerate()
            .map(|(i, u)| IndexedUtxo {
                index: i,
                utxo: u.clone(),
            })
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
    // Backfill tests
    // ========================================================================

    #[test]
    fn test_backfill_surplus_tokens() {
        let input_utxo = bloated_utxo(
            "tx1",
            5_000_000,
            &[(POLICY_A, &["a1"]), (POLICY_B, &["b1"])],
        );
        let working_set = [IndexedUtxo {
            index: 0,
            utxo: input_utxo,
        }];

        let mut selected_inputs: HashMap<String, &IndexedUtxo> = HashMap::new();
        selected_inputs.insert("tx1#0".to_string(), &working_set[0]);

        let outputs = vec![ProposedOutput {
            tokens: vec![Token {
                policy_id: POLICY_A.to_string(),
                asset_name_hex: hex::encode("a1"),
                quantity: 1,
            }],
            kind: OutputKind::PolicyBundle,
            source_refs: vec!["tx1#0".to_string()],
        }];

        let backfill = compute_backfill(&selected_inputs, &outputs, &OptimizeConfig::default());
        assert!(!backfill.is_empty());
        let b_found = backfill
            .iter()
            .any(|o| o.tokens.iter().any(|t| t.policy_id == POLICY_B));
        assert!(b_found);
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
