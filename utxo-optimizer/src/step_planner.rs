//! Diff-based step planner using MILP for optimal TX grouping.
//!
//! Given the current wallet UTxOs and an ideal end-state (from `compute_ideal_state`),
//! this module:
//! 1. Builds a provenance map: which current UTxOs supply each ideal output's tokens.
//! 2. Identifies connected components: ideal outputs that share input UTxOs.
//! 3. Uses MILP (via good_lp/microlp) to optimally group components into TX steps
//!    respecting the 16KB max TX size constraint.
//! 4. Produces an `OptimizationPlan` with concrete TX steps.

use std::collections::{BTreeMap, HashMap, HashSet};

use cardano_assets::utxo::UtxoApi;
use good_lp::{constraint, variable, Expression, ProblemVariables, Solution, SolverModel};

use crate::config::{AdaStrategy, FeeParams, OptimizeConfig};
use crate::optimizer::{compute_ideal_state, IdealOutput, IdealState};
use crate::plan::*;
use crate::size_estimator::{
    bail_size, estimate_fee, estimate_min_lovelace_for_assets, estimate_tx_size, InputTokenData,
    OutputTokenData,
};

// ============================================================================
// Provenance: map ideal outputs → required input UTxOs
// ============================================================================

/// For each ideal output, the set of current UTxO indices that must be consumed
/// to supply its tokens.
#[derive(Debug)]
struct Provenance {
    /// ideal_output_index → set of current UTxO indices
    output_inputs: Vec<HashSet<usize>>,
    /// Indices of current UTxOs that are ADA-only and participate in ADA strategy
    ada_only_indices: Vec<usize>,
}

/// Build the provenance map by matching ideal output tokens to current UTxOs.
///
/// For each token (policy_id + asset_name) in each ideal output, we find the
/// current UTxO(s) that hold that token. We greedily assign tokens from UTxOs
/// until the ideal output's quantity is satisfied.
fn build_provenance(
    current_utxos: &[UtxoApi],
    ideal: &IdealState,
    excluded_indices: &HashSet<usize>,
) -> Provenance {
    // Build an index: asset_key → [(utxo_index, available_quantity)]
    // We'll decrement available_quantity as tokens are assigned.
    let mut asset_supply: BTreeMap<String, Vec<(usize, u64)>> = BTreeMap::new();

    for (i, u) in current_utxos.iter().enumerate() {
        if excluded_indices.contains(&i) {
            continue;
        }
        for aq in &u.assets {
            let key = format!("{}{}", aq.asset_id.policy_id, aq.asset_id.asset_name_hex);
            asset_supply.entry(key).or_default().push((i, aq.quantity));
        }
    }

    let mut output_inputs: Vec<HashSet<usize>> = Vec::new();

    for ideal_out in &ideal.token_outputs {
        let mut needed_utxos: HashSet<usize> = HashSet::new();

        for aq in &ideal_out.assets {
            let key = format!("{}{}", aq.asset_id.policy_id, aq.asset_id.asset_name_hex);
            let mut remaining = aq.quantity;

            if let Some(suppliers) = asset_supply.get_mut(&key) {
                for (utxo_idx, avail) in suppliers.iter_mut() {
                    if remaining == 0 {
                        break;
                    }
                    if *avail == 0 {
                        continue;
                    }
                    let take = remaining.min(*avail);
                    *avail -= take;
                    remaining -= take;
                    needed_utxos.insert(*utxo_idx);
                }
            }
            // If remaining > 0, the ideal state asks for tokens we don't have.
            // This shouldn't happen if compute_ideal_state is correct.
        }

        output_inputs.push(needed_utxos);
    }

    // Identify ADA-only UTxOs not excluded
    let ada_only_indices: Vec<usize> = current_utxos
        .iter()
        .enumerate()
        .filter(|(i, u)| !excluded_indices.contains(i) && u.assets.is_empty())
        .map(|(i, _)| i)
        .collect();

    Provenance {
        output_inputs,
        ada_only_indices,
    }
}

// ============================================================================
// Connected components: group ideal outputs that share inputs
// ============================================================================

/// A component is a group of ideal output indices that transitively share input UTxOs.
#[derive(Debug)]
struct Component {
    /// Indices into ideal.token_outputs
    output_indices: Vec<usize>,
    /// Union of all input UTxO indices needed by these outputs
    input_indices: HashSet<usize>,
    /// Estimated TX size for this component
    estimated_size: u64,
}

/// Find connected components among ideal outputs based on shared input UTxOs.
fn find_components(provenance: &Provenance) -> Vec<Component> {
    let n = provenance.output_inputs.len();
    if n == 0 {
        return vec![];
    }

    // Union-Find
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    // Build UTxO → ideal output index map
    let mut utxo_to_outputs: HashMap<usize, Vec<usize>> = HashMap::new();
    for (out_idx, inputs) in provenance.output_inputs.iter().enumerate() {
        for &utxo_idx in inputs {
            utxo_to_outputs.entry(utxo_idx).or_default().push(out_idx);
        }
    }

    // Union outputs that share any input UTxO
    for outputs in utxo_to_outputs.values() {
        if outputs.len() > 1 {
            for i in 1..outputs.len() {
                union(&mut parent, outputs[0], outputs[i]);
            }
        }
    }

    // Group by root
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    // Build components
    groups
        .into_values()
        .map(|output_indices| {
            let input_indices: HashSet<usize> = output_indices
                .iter()
                .flat_map(|&oi| provenance.output_inputs[oi].iter().copied())
                .collect();
            Component {
                output_indices,
                input_indices,
                estimated_size: 0, // computed later
            }
        })
        .collect()
}

// ============================================================================
// Size estimation for components
// ============================================================================

fn estimate_component_size(
    component: &Component,
    current_utxos: &[UtxoApi],
    ideal_outputs: &[IdealOutput],
) -> u64 {
    let input_tokens: Vec<InputTokenData<'_>> = component
        .input_indices
        .iter()
        .map(|&i| InputTokenData {
            assets: &current_utxos[i].assets,
        })
        .collect();

    let output_tokens: Vec<OutputTokenData<'_>> = component
        .output_indices
        .iter()
        .map(|&i| OutputTokenData {
            assets: &ideal_outputs[i].assets,
        })
        .collect();

    estimate_tx_size(component.input_indices.len(), &output_tokens, &input_tokens)
}

// ============================================================================
// MILP: pack components into minimum TX steps respecting size limits
// ============================================================================

/// Assign components to TX steps using MILP.
///
/// Variables: `assign[c][t]` = 1 if component c is in TX step t.
/// Constraints:
///   - Each component assigned to exactly one step.
///   - Each step's total estimated size ≤ bail threshold.
///   - `use_step[t]` = 1 if any component assigned to step t.
///
/// Objective: minimize sum of `use_step[t]` (minimize number of TXs).
#[allow(clippy::needless_range_loop)] // Index-based loops are clearer for MILP formulation
fn assign_components_to_steps(components: &[Component], bail: u64) -> Vec<Vec<usize>> {
    if components.is_empty() {
        return vec![];
    }

    // If only one component and it fits, trivial case
    if components.len() == 1 {
        if components[0].estimated_size <= bail {
            return vec![vec![0]];
        }
        // Single component too large — can't split a component (shared inputs).
        // Return it as a single step anyway; the caller will handle oversized TXs.
        return vec![vec![0]];
    }

    // Upper bound on steps = number of components (worst case: each in its own TX)
    let max_steps = components.len();

    let mut vars = ProblemVariables::new();

    // assign[c][t] = binary: component c assigned to step t
    let assign: Vec<Vec<good_lp::Variable>> = (0..components.len())
        .map(|_| vars.add_vector(variable().binary(), max_steps))
        .collect();

    // use_step[t] = binary: step t is used (has at least one component)
    let use_step: Vec<good_lp::Variable> = vars.add_vector(variable().binary(), max_steps);

    // Objective: minimize number of steps used
    let objective: Expression = use_step.iter().copied().sum();

    let mut problem = vars
        .minimise(objective)
        .using(good_lp::solvers::microlp::microlp);

    // Constraint: each component assigned to exactly one step
    for c in 0..components.len() {
        let row_sum: Expression = assign[c].iter().copied().sum();
        problem = problem.with(constraint!(row_sum == 1));
    }

    // Constraint: each step's total size ≤ bail
    for t in 0..max_steps {
        let step_size: Expression = (0..components.len())
            .map(|c| components[c].estimated_size as f64 * assign[c][t])
            .sum();
        problem = problem.with(constraint!(step_size <= bail as f64));
    }

    // Constraint: use_step[t] >= assign[c][t] for all c
    // (if any component is in step t, use_step[t] must be 1)
    for t in 0..max_steps {
        for c in 0..components.len() {
            problem = problem.with(constraint!(use_step[t] >= assign[c][t]));
        }
    }

    // Symmetry breaking: steps used in order (use_step[t] >= use_step[t+1])
    for t in 0..max_steps.saturating_sub(1) {
        problem = problem.with(constraint!(use_step[t] >= use_step[t + 1]));
    }

    // Solve
    match problem.solve() {
        Ok(solution) => {
            let mut steps: Vec<Vec<usize>> = Vec::new();
            for t in 0..max_steps {
                let mut step_components: Vec<usize> = Vec::new();
                for c in 0..components.len() {
                    if solution.value(assign[c][t]) > 0.5 {
                        step_components.push(c);
                    }
                }
                if !step_components.is_empty() {
                    steps.push(step_components);
                }
            }
            steps
        }
        Err(_) => {
            // Fallback: one component per step
            (0..components.len()).map(|c| vec![c]).collect()
        }
    }
}

// ============================================================================
// Public API: build_steps_from_diff
// ============================================================================

/// Build an optimization plan by diffing current UTxOs against the ideal state
/// and using MILP to optimally group the work into TX-sized steps.
pub fn build_steps_from_diff(
    utxos: &[UtxoApi],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
) -> OptimizationPlan {
    let ideal = compute_ideal_state(utxos, config, fee_params);
    let bail = bail_size(fee_params);

    // Identify excluded UTxOs (script-locked + collateral)
    let collateral_refs = crate::optimizer::select_collateral_refs(utxos, &config.collateral);
    let excluded_indices: HashSet<usize> = utxos
        .iter()
        .enumerate()
        .filter(|(_, u)| {
            let utxo_ref = format!("{}#{}", u.tx_hash, u.output_index);
            crate::optimizer::is_script_locked_pub(u) || collateral_refs.contains(&utxo_ref)
        })
        .map(|(i, _)| i)
        .collect();

    // Separate actual token outputs from ADA-only outputs in the ideal state.
    // `IdealState.token_outputs` contains both — filter to those with assets.
    let has_token_outputs = ideal.token_outputs.iter().any(|o| !o.assets.is_empty());

    if !has_token_outputs {
        return build_ada_only_plan(utxos, config, fee_params, &excluded_indices);
    }

    // Filter ideal state to only token-bearing outputs (ADA outputs handled separately)
    let token_ideal = IdealState {
        token_outputs: ideal
            .token_outputs
            .iter()
            .filter(|o| !o.assets.is_empty())
            .cloned()
            .collect(),
        summary: ideal.summary.clone(),
        as_utxos: ideal.as_utxos.clone(),
    };

    // Step 1: Build provenance map
    let provenance = build_provenance(utxos, &token_ideal, &excluded_indices);

    // Step 2: Find connected components
    let mut components = find_components(&provenance);

    // Step 2b: Filter out no-op components.
    // A component is a no-op if each ideal output maps 1:1 to a single input UTxO
    // and that input's tokens exactly match the ideal output's tokens.
    components.retain(|comp| {
        if comp.output_indices.len() != comp.input_indices.len() {
            return true; // different count → not a no-op
        }
        // Check each ideal output: does it need exactly 1 input, and does that
        // input have exactly the same assets?
        for &oi in &comp.output_indices {
            let needed = &provenance.output_inputs[oi];
            if needed.len() != 1 {
                return true; // needs multiple inputs → real work
            }
            let input_idx = *needed.iter().next().unwrap();
            let input_assets = &utxos[input_idx].assets;
            let ideal_assets = &token_ideal.token_outputs[oi].assets;
            if input_assets.len() != ideal_assets.len() {
                return true; // different asset count → real work
            }
            // Compare asset contents (sorted by key for determinism)
            let mut input_keys: Vec<_> = input_assets
                .iter()
                .map(|a| {
                    (
                        &a.asset_id.policy_id,
                        &a.asset_id.asset_name_hex,
                        a.quantity,
                    )
                })
                .collect();
            let mut ideal_keys: Vec<_> = ideal_assets
                .iter()
                .map(|a| {
                    (
                        &a.asset_id.policy_id,
                        &a.asset_id.asset_name_hex,
                        a.quantity,
                    )
                })
                .collect();
            input_keys.sort();
            ideal_keys.sort();
            if input_keys != ideal_keys {
                return true; // different assets → real work
            }
        }
        false // all outputs are 1:1 matches → no-op
    });

    if components.is_empty() {
        // All token components are no-ops. Still handle ADA if needed.
        return build_ada_only_plan(utxos, config, fee_params, &excluded_indices);
    }

    // Step 3: Estimate sizes
    for comp in &mut components {
        comp.estimated_size = estimate_component_size(comp, utxos, &token_ideal.token_outputs);
    }

    // Step 4: Handle ADA-only UTxOs
    // If ada_strategy != Leave and there are multiple ADA-only UTxOs, create
    // a synthetic component for ADA consolidation.
    let ada_component_idx =
        if config.ada_strategy != AdaStrategy::Leave && provenance.ada_only_indices.len() > 1 {
            let ada_input_indices: HashSet<usize> =
                provenance.ada_only_indices.iter().copied().collect();
            // Estimate: N inputs → 1 output (or 7 for split)
            let n_ada_outputs = match config.ada_strategy {
                AdaStrategy::Split => 7,
                _ => 1,
            };
            let ada_output_data: Vec<OutputTokenData<'_>> = (0..n_ada_outputs)
                .map(|_| OutputTokenData { assets: &[] })
                .collect();
            let ada_size = estimate_tx_size(ada_input_indices.len(), &ada_output_data, &[]);
            let idx = components.len();
            components.push(Component {
                output_indices: vec![], // no token outputs — ADA only
                input_indices: ada_input_indices,
                estimated_size: ada_size,
            });
            Some(idx)
        } else {
            None
        };

    // Step 5: MILP assignment
    let step_assignments = assign_components_to_steps(&components, bail);

    // Step 6: Build OptimizationPlan
    let mut steps: Vec<OptimizationStep> = Vec::new();
    // Track consumed UTxO refs across all steps for resulting_utxos computation
    let mut all_consumed: HashSet<usize> = HashSet::new();
    let mut all_produced: Vec<(usize, PlannedOutput)> = Vec::new(); // (step_index, output)

    for (step_idx, component_indices) in step_assignments.iter().enumerate() {
        let mut step_input_indices: HashSet<usize> = HashSet::new();
        let mut step_ideal_output_indices: Vec<usize> = Vec::new();
        let mut is_ada_step = false;

        for &ci in component_indices {
            step_input_indices.extend(&components[ci].input_indices);
            step_ideal_output_indices.extend(&components[ci].output_indices);
            if ada_component_idx == Some(ci) {
                is_ada_step = true;
            }
        }

        let total_input_lovelace: u64 = step_input_indices.iter().map(|&i| utxos[i].lovelace).sum();

        // Build planned outputs from ideal outputs
        let mut planned_outputs: Vec<PlannedOutput> = Vec::new();
        let mut token_lovelace: u64 = 0;

        for (out_idx, &ideal_idx) in step_ideal_output_indices.iter().enumerate() {
            let ideal_out = &token_ideal.token_outputs[ideal_idx];
            let min_lovelace =
                estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &ideal_out.assets);
            token_lovelace += min_lovelace;

            // Source UTxO refs for this ideal output
            let source_refs: Vec<String> = provenance.output_inputs[ideal_idx]
                .iter()
                .map(|&i| format!("{}#{}", utxos[i].tx_hash, utxos[i].output_index))
                .collect();

            planned_outputs.push(PlannedOutput {
                output_id: format!("step{step_idx}_out{out_idx}"),
                lovelace: min_lovelace,
                assets: ideal_out.assets.clone(),
                source_utxo_refs: source_refs,
                output_kind: ideal_out.kind,
            });
        }

        // Estimate fee
        let output_token_data: Vec<OutputTokenData<'_>> = planned_outputs
            .iter()
            .map(|o| OutputTokenData { assets: &o.assets })
            .collect();
        let input_token_data: Vec<InputTokenData<'_>> = step_input_indices
            .iter()
            .map(|&i| InputTokenData {
                assets: &utxos[i].assets,
            })
            .collect();
        let est_size = estimate_tx_size(
            step_input_indices.len(),
            &output_token_data,
            &input_token_data,
        );
        let est_fee = estimate_fee(est_size, fee_params);

        // Remaining ADA → change/split/rollup output
        let remaining = total_input_lovelace
            .saturating_sub(token_lovelace)
            .saturating_sub(est_fee);

        if is_ada_step && config.ada_strategy == AdaStrategy::Split && remaining > 100_000_000 {
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
                    planned_outputs.push(PlannedOutput {
                        output_id: format!("step{step_idx}_ada{j}"),
                        lovelace: amount,
                        assets: vec![],
                        source_utxo_refs: vec![],
                        output_kind: OutputKind::AdaSplit,
                    });
                }
            }
        } else if remaining >= fee_params.coins_per_utxo_byte * 160 {
            let kind = if is_ada_step {
                OutputKind::AdaRollup
            } else {
                OutputKind::Change
            };
            planned_outputs.push(PlannedOutput {
                output_id: format!("step{step_idx}_change"),
                lovelace: remaining,
                assets: vec![],
                source_utxo_refs: vec![],
                output_kind: kind,
            });
        }

        let inputs: Vec<InputRef> = step_input_indices
            .iter()
            .map(|&i| InputRef {
                utxo_ref: format!("{}#{}", utxos[i].tx_hash, utxos[i].output_index),
                original_index: i,
            })
            .collect();

        all_consumed.extend(&step_input_indices);
        for out in &planned_outputs {
            all_produced.push((step_idx, out.clone()));
        }

        // Build resulting_utxos: everything not consumed + everything produced so far
        let mut resulting_utxos: Vec<UtxoSnapshot> = Vec::new();

        // Unconsumed original UTxOs (including excluded)
        for (i, u) in utxos.iter().enumerate() {
            if !all_consumed.contains(&i) {
                resulting_utxos.push(UtxoSnapshot {
                    utxo_ref: format!("{}#{}", u.tx_hash, u.output_index),
                    lovelace: u.lovelace,
                    assets: u.assets.clone(),
                    tags: u.tags.clone(),
                    produced_by_step: None,
                    consumed_by_step: None,
                });
            }
        }

        // All produced outputs from all steps so far
        for (prod_step, out) in &all_produced {
            resulting_utxos.push(UtxoSnapshot {
                utxo_ref: out.output_id.clone(),
                lovelace: out.lovelace,
                assets: out.assets.clone(),
                tags: vec![],
                produced_by_step: Some(*prod_step),
                consumed_by_step: None,
            });
        }

        steps.push(OptimizationStep {
            step_index: step_idx,
            inputs,
            outputs: planned_outputs,
            estimated_size: est_size,
            estimated_fee: est_fee,
            resulting_utxos,
        });
    }

    // Summary
    let total_fees: u64 = steps.iter().map(|s| s.estimated_fee).sum();
    let utxos_after = if steps.is_empty() {
        utxos.len()
    } else {
        steps.last().unwrap().resulting_utxos.len()
    };

    let original_locked: u64 = utxos
        .iter()
        .filter(|u| !crate::optimizer::is_script_locked_pub(u))
        .map(|u| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &u.assets))
        .sum();
    let final_locked: u64 = if let Some(last) = steps.last() {
        last.resulting_utxos
            .iter()
            .map(|u| estimate_min_lovelace_for_assets(fee_params.coins_per_utxo_byte, &u.assets))
            .sum()
    } else {
        original_locked
    };

    OptimizationPlan {
        summary: PlanSummary {
            utxos_before: utxos.len(),
            utxos_after,
            total_fees,
            num_steps: steps.len(),
            ada_freed: original_locked.saturating_sub(final_locked),
        },
        steps,
    }
}

/// Handle the case where there are no token outputs — just ADA consolidation/splitting.
fn build_ada_only_plan(
    utxos: &[UtxoApi],
    config: &OptimizeConfig,
    fee_params: &FeeParams,
    excluded_indices: &HashSet<usize>,
) -> OptimizationPlan {
    if config.ada_strategy == AdaStrategy::Leave {
        return OptimizationPlan {
            summary: PlanSummary {
                utxos_before: utxos.len(),
                utxos_after: utxos.len(),
                total_fees: 0,
                num_steps: 0,
                ada_freed: 0,
            },
            steps: vec![],
        };
    }

    let ada_indices: Vec<usize> = utxos
        .iter()
        .enumerate()
        .filter(|(i, u)| !excluded_indices.contains(i) && u.assets.is_empty())
        .map(|(i, _)| i)
        .collect();

    if ada_indices.len() <= 1 {
        return OptimizationPlan {
            summary: PlanSummary {
                utxos_before: utxos.len(),
                utxos_after: utxos.len(),
                total_fees: 0,
                num_steps: 0,
                ada_freed: 0,
            },
            steps: vec![],
        };
    }

    let bail = bail_size(fee_params);
    let mut steps: Vec<OptimizationStep> = Vec::new();
    let mut remaining_indices = ada_indices.clone();
    let mut step_idx = 0;
    let mut all_consumed: HashSet<usize> = HashSet::new();

    while remaining_indices.len() > 1 {
        // Greedily take as many ADA inputs as fit in one TX
        let mut batch: Vec<usize> = Vec::new();
        let mut cumulative_size: u64 = 0;

        for &idx in &remaining_indices {
            let input_size = 37u64; // pure ADA input
            let new_size = cumulative_size + input_size;
            if new_size > bail && !batch.is_empty() {
                break;
            }
            batch.push(idx);
            cumulative_size = new_size;
        }

        if batch.len() <= 1 {
            break;
        }

        let total_lovelace: u64 = batch.iter().map(|&i| utxos[i].lovelace).sum();
        let est_size = estimate_tx_size(batch.len(), &[OutputTokenData { assets: &[] }], &[]);
        let est_fee = estimate_fee(est_size, fee_params);
        let remaining_ada = total_lovelace.saturating_sub(est_fee);

        let mut outputs: Vec<PlannedOutput> = Vec::new();

        if config.ada_strategy == AdaStrategy::Split && remaining_ada > 100_000_000 {
            let splits = [0.50, 0.15, 0.10, 0.10, 0.05, 0.05, 0.05];
            let mut accounted = 0u64;
            for (j, &pct) in splits.iter().enumerate() {
                let amount = if j == splits.len() - 1 {
                    remaining_ada.saturating_sub(accounted)
                } else {
                    (remaining_ada as f64 * pct) as u64
                };
                accounted += amount;
                if amount > 0 {
                    outputs.push(PlannedOutput {
                        output_id: format!("step{step_idx}_ada{j}"),
                        lovelace: amount,
                        assets: vec![],
                        source_utxo_refs: vec![],
                        output_kind: OutputKind::AdaSplit,
                    });
                }
            }
        } else if remaining_ada >= fee_params.coins_per_utxo_byte * 160 {
            outputs.push(PlannedOutput {
                output_id: format!("step{step_idx}_change"),
                lovelace: remaining_ada,
                assets: vec![],
                source_utxo_refs: vec![],
                output_kind: OutputKind::AdaRollup,
            });
        }

        let inputs: Vec<InputRef> = batch
            .iter()
            .map(|&i| InputRef {
                utxo_ref: format!("{}#{}", utxos[i].tx_hash, utxos[i].output_index),
                original_index: i,
            })
            .collect();

        for &i in &batch {
            all_consumed.insert(i);
        }

        // Remove batch from remaining
        let batch_set: HashSet<usize> = batch.iter().copied().collect();
        remaining_indices.retain(|i| !batch_set.contains(i));

        // If we produced a single output and there are more remaining, the output
        // becomes a "virtual" UTxO for the next step. But since ADA-only UTxOs
        // produce ADA-only outputs, subsequent steps just consume the original
        // remaining ones. No need for virtual UTxOs here.

        // Build resulting_utxos
        let mut resulting_utxos: Vec<UtxoSnapshot> = Vec::new();
        for (i, u) in utxos.iter().enumerate() {
            if !all_consumed.contains(&i) {
                resulting_utxos.push(UtxoSnapshot {
                    utxo_ref: format!("{}#{}", u.tx_hash, u.output_index),
                    lovelace: u.lovelace,
                    assets: u.assets.clone(),
                    tags: u.tags.clone(),
                    produced_by_step: None,
                    consumed_by_step: None,
                });
            }
        }
        for out in &outputs {
            resulting_utxos.push(UtxoSnapshot {
                utxo_ref: out.output_id.clone(),
                lovelace: out.lovelace,
                assets: vec![],
                tags: vec![],
                produced_by_step: Some(step_idx),
                consumed_by_step: None,
            });
        }

        steps.push(OptimizationStep {
            step_index: step_idx,
            inputs,
            outputs,
            estimated_size: est_size,
            estimated_fee: est_fee,
            resulting_utxos,
        });

        step_idx += 1;
        if step_idx >= 50 {
            break;
        }
    }

    let total_fees: u64 = steps.iter().map(|s| s.estimated_fee).sum();
    let utxos_after = if steps.is_empty() {
        utxos.len()
    } else {
        steps.last().unwrap().resulting_utxos.len()
    };

    OptimizationPlan {
        summary: PlanSummary {
            utxos_before: utxos.len(),
            utxos_after,
            total_fees,
            num_steps: steps.len(),
            ada_freed: 0,
        },
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CollateralConfig;
    use cardano_assets::utxo::AssetQuantity;
    use cardano_assets::AssetId;

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

    const POLICY_A: &str = "aaaa0000000000000000000000000000000000000000000000000000";
    const POLICY_B: &str = "bbbb0000000000000000000000000000000000000000000000000000";

    #[test]
    fn test_empty_wallet() {
        let plan = build_steps_from_diff(&[], &OptimizeConfig::default(), &FeeParams::default());
        assert_eq!(plan.steps.len(), 0);
        assert_eq!(plan.summary.utxos_before, 0);
    }

    #[test]
    fn test_single_nft_no_action() {
        let utxos = vec![nft_utxo("tx1", 2_000_000, POLICY_A, &["a1"])];
        let plan = build_steps_from_diff(&utxos, &OptimizeConfig::default(), &FeeParams::default());
        // Single NFT UTxO — already optimal, no steps needed
        // (Though there may be an ADA rollup step if there's ADA change)
        assert_eq!(plan.summary.utxos_before, 1);
    }

    #[test]
    fn test_consolidates_same_policy_nfts() {
        let utxos = vec![
            nft_utxo("tx1", 2_000_000, POLICY_A, &["a1"]),
            nft_utxo("tx2", 2_000_000, POLICY_A, &["a2"]),
            nft_utxo("tx3", 2_000_000, POLICY_A, &["a3"]),
        ];
        let config = OptimizeConfig {
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());
        assert!(!plan.steps.is_empty(), "Should produce consolidation steps");
        // Final state should have fewer UTxOs
        assert!(plan.summary.utxos_after <= plan.summary.utxos_before);

        // All 3 NFTs should be preserved in the final state
        let last = plan.steps.last().unwrap();
        let total_assets: usize = last.resulting_utxos.iter().map(|u| u.assets.len()).sum();
        assert_eq!(total_assets, 3, "All 3 NFTs preserved");
    }

    #[test]
    fn test_debloats_multi_policy() {
        let utxos = vec![bloated_utxo(
            "tx1",
            10_000_000,
            &[
                (POLICY_A, &["a1", "a2", "a3", "a4", "a5"]),
                (POLICY_B, &["b1", "b2", "b3", "b4", "b5"]),
            ],
        )];
        let config = OptimizeConfig {
            bundle_size: 10,
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());
        assert!(!plan.steps.is_empty(), "Should produce debloating steps");

        let last = plan.steps.last().unwrap();
        let total_assets: usize = last.resulting_utxos.iter().map(|u| u.assets.len()).sum();
        assert_eq!(total_assets, 10, "All 10 tokens preserved");
    }

    #[test]
    fn test_ada_rollup() {
        let utxos = vec![
            pure_ada_utxo("tx1", 10_000_000),
            pure_ada_utxo("tx2", 10_000_000),
            pure_ada_utxo("tx3", 10_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());
        assert!(!plan.steps.is_empty(), "Should consolidate ADA");
        assert!(plan.summary.total_fees > 0);
    }

    #[test]
    fn test_single_ada_noop() {
        let utxos = vec![pure_ada_utxo("tx1", 50_000_000)];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());
        assert_eq!(plan.steps.len(), 0, "Single ADA UTxO should be no-op");
    }

    #[test]
    fn test_preserves_collateral() {
        let utxos = vec![
            pure_ada_utxo("collateral5", 5_000_000),
            pure_ada_utxo("collateral10", 10_000_000),
            pure_ada_utxo("spare1", 20_000_000),
            pure_ada_utxo("spare2", 30_000_000),
            pure_ada_utxo("spare3", 40_000_000),
        ];
        let config = OptimizeConfig {
            ada_strategy: AdaStrategy::Rollup,
            collateral: CollateralConfig {
                count: 2,
                targets_lovelace: vec![5_000_000, 10_000_000],
                ceiling_lovelace: 15_000_000,
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());

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
    fn test_token_conservation() {
        // Complex wallet: multiple policies, fungibles, ADA — verify nothing lost
        let utxos = vec![
            nft_utxo("tx1", 3_000_000, POLICY_A, &["a1", "a2"]),
            nft_utxo("tx2", 3_000_000, POLICY_A, &["a3", "a4"]),
            nft_utxo("tx3", 3_000_000, POLICY_B, &["b1", "b2", "b3"]),
            ft_utxo("tx4", 3_000_000, POLICY_B, "fungible", 1_000_000),
            pure_ada_utxo("tx5", 50_000_000),
        ];
        let config = OptimizeConfig {
            collateral: CollateralConfig {
                count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let plan = build_steps_from_diff(&utxos, &config, &FeeParams::default());

        if !plan.steps.is_empty() {
            let last = plan.steps.last().unwrap();
            // Count tokens by asset key
            let mut final_tokens: HashMap<String, u64> = HashMap::new();
            for u in &last.resulting_utxos {
                for a in &u.assets {
                    let key = format!("{}{}", a.asset_id.policy_id, a.asset_id.asset_name_hex);
                    *final_tokens.entry(key).or_default() += a.quantity;
                }
            }

            let mut original_tokens: HashMap<String, u64> = HashMap::new();
            for u in &utxos {
                for a in &u.assets {
                    let key = format!("{}{}", a.asset_id.policy_id, a.asset_id.asset_name_hex);
                    *original_tokens.entry(key).or_default() += a.quantity;
                }
            }

            assert_eq!(
                final_tokens, original_tokens,
                "All tokens must be conserved"
            );
        }
    }
}
