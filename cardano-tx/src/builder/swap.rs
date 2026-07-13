//! Atomic swap transaction builder.
//!
//! Builds a regular Cardano transaction where two parties exchange assets
//! without smart contracts. Each party's UTxOs are used as inputs, and
//! outputs deliver the swapped assets to each party's address.
//!
//! The transaction requires VKey witnesses from both parties (CIP-30 partial signing).

use std::collections::{HashMap, HashSet};

use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;

use super::{converge_fee_with_witnesses, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::input::add_utxo_inputs;
use crate::helpers::output::{add_assets_from_map, create_ada_output};
use crate::helpers::utxo_query::{collect_utxo_native_assets, total_utxo_lovelace};
use crate::params::TxBuildParams;
use crate::select::{select, Selection, Strategy};

/// One side of an atomic swap.
pub struct SwapSide {
    /// This party's available UTxO pool. `build_atomic_swap` selects a minimal
    /// covering subset (the UTxOs holding the offered assets + an ADA top-up) —
    /// pass the full wallet set; it is not consumed wholesale.
    pub utxos: Vec<UtxoApi>,
    /// Payment address for this party's receive output and change.
    pub address: Address,
    /// Assets this side is giving (asset → quantity).
    pub offered_assets: HashMap<AssetId, u64>,
    /// ADA sweetener this side adds (lovelace).
    pub ada_lovelace: u64,
}

/// Result of building an atomic swap, including the unsigned TX and a cost breakdown.
#[derive(Debug)]
pub struct SwapBuildResult {
    pub unsigned: UnsignedTx,
    pub costs: SwapCostBreakdown,
}

/// Detailed cost breakdown for an atomic swap, one entry per side.
#[derive(Debug)]
pub struct SwapCostBreakdown {
    /// Total network TX fee.
    pub network_fee: u64,
    /// Network fee borne by Party A (an even split of `network_fee`; A absorbs the
    /// rounding remainder).
    pub a_network_fee: u64,
    /// Network fee borne by Party B (an even split of `network_fee`).
    pub b_network_fee: u64,
    /// Total platform/orchestration fee.
    pub platform_fee: u64,
    /// Min-UTxO lovelace Party A funds for the assets A *receives* (the wrapper
    /// lands in A's own wallet and is recoverable). 0 when A receives only ADA.
    pub a_min_utxo_cost: u64,
    /// Min-UTxO lovelace Party B funds for the assets B *receives*.
    pub b_min_utxo_cost: u64,
    /// Net ADA gain/loss for Party A (positive = gains ADA, negative = loses ADA).
    pub a_net_ada: i64,
    /// Net ADA gain/loss for Party B.
    pub b_net_ada: i64,
    /// Party A's own ADA that passes through the tx and returns to them as change.
    /// Not a cost — but it appears as the *counterparty's* "send" on a hardware
    /// wallet, so surfacing it lets the UI explain that inflated figure.
    pub a_passthrough: u64,
    /// Party B's own ADA that passes through the tx and returns to them as change.
    pub b_passthrough: u64,
}

/// A swap requires a vkey witness from BOTH parties (CIP-30 partial signing).
const SWAP_WITNESSES: u32 = 2;

/// Generous flat network-fee headroom (lovelace) added to each side's coin-selection
/// target. The exact fee is computed by `converge_fee`; over-selecting here only
/// returns as change, so ~1 ADA is a safe upper bound for a native multi-sig swap.
const SELECTION_FEE_ESTIMATE: u64 = 1_000_000;

/// Build an atomic swap transaction between two parties.
///
/// # Arguments
/// * `sides` — Exactly two swap sides (party A at index 0, party B at index 1)
/// * `fee_output` — Optional community wallet fee: `(address, lovelace)`. Omitted when 0.
/// * `params` — Protocol parameters for fee calculation
/// * `network_id` — Cardano network ID (1 = mainnet, 0 = testnet)
///
/// # Returns
/// A [`SwapBuildResult`] with the unsigned TX and a full cost breakdown.
pub fn build_atomic_swap(
    sides: &[SwapSide; 2],
    fee_output: Option<(Address, u64)>,
    params: &TxBuildParams,
    network_id: u8,
) -> Result<SwapBuildResult, TxBuildError> {
    let a = &sides[0];
    let b = &sides[1];

    // Validate both sides have UTxOs
    if a.utxos.is_empty() || b.utxos.is_empty() {
        return Err(TxBuildError::NoSuitableUtxo);
    }

    // Fee output amount (0 if no fee)
    let fee_output_amount = fee_output.as_ref().map(|(_, amt)| *amt).unwrap_or(0);

    // Compute min UTxO for each receive output based on actual assets being delivered.
    // Each party funds the output that delivers their offered assets to the peer.
    let a_offered_ids: Vec<AssetId> = a.offered_assets.keys().cloned().collect();
    let b_offered_ids: Vec<AssetId> = b.offered_assets.keys().cloned().collect();
    let a_min_utxo = min_utxo_for_assets(params, &a_offered_ids);
    let b_min_utxo = min_utxo_for_assets(params, &b_offered_ids);

    let a_ada = a.ada_lovelace;
    let b_ada = b.ada_lovelace;

    // ── Cost allocation ──────────────────────────────────────────────────
    // Min-UTxO wrapper: the mandatory ADA that must ride with an asset lands in
    // the *recipient's* wallet (and is recoverable when they later move it), so
    // the recipient funds it — not the sender. A sender's ADA sweetener already
    // covers part of the receive output; the recipient funds only the shortfall
    // up to min-UTxO. This lets an asset seller receive clean proceeds instead of
    // subsidising the buyer's min-UTxO.
    let a_wrapper = b_min_utxo.saturating_sub(b_ada); // A receives B's assets
    let b_wrapper = a_min_utxo.saturating_sub(a_ada); // B receives A's assets

    let a_platform_share = fee_output_amount / 2;
    let b_platform_share = fee_output_amount - a_platform_share;

    // ── Coin selection ───────────────────────────────────────────────────
    // Select a minimal input set per side instead of consuming the whole wallet:
    // must-spend the UTxOs that hold the offered assets, then top up ADA to cover
    // that side's outflow (ADA sweetener + wrapper it funds + fee share + platform
    // share). A generous flat fee estimate is fine — `converge_fee` computes the
    // exact fee below and any surplus returns as change.
    let (a_fee_est, b_fee_est) = split_network_fee(SELECTION_FEE_ESTIMATE);
    let a_target = a_ada + a_wrapper + a_fee_est + a_platform_share;
    let b_target = b_ada + b_wrapper + b_fee_est + b_platform_share;

    let a_selected = select_side_inputs(a, a_target)?;
    let b_selected = select_side_inputs(b, b_target)?;

    let a_total_lovelace = total_utxo_lovelace(&a_selected);
    let b_total_lovelace = total_utxo_lovelace(&b_selected);

    // Collect all native assets from each side's *selected* UTxOs
    let a_all_assets = collect_utxo_native_assets(&a_selected, None);
    let b_all_assets = collect_utxo_native_assets(&b_selected, None);

    // Calculate assets each side keeps (assets in selected UTxOs minus what they offered)
    let a_kept_assets = subtract_assets(&a_all_assets, &a.offered_assets);
    let b_kept_assets = subtract_assets(&b_all_assets, &b.offered_assets);

    // Each side pays half the network fee + half the orchestration fee.
    let fee_output_clone = fee_output.clone();
    let a_addr = a.address.clone();
    let b_addr = b.address.clone();
    let a_offered = a.offered_assets.clone();
    let b_offered = b.offered_assets.clone();
    let a_kept = a_kept_assets.clone();
    let b_kept = b_kept_assets.clone();

    let params_clone = params.clone();
    // Size the fee for BOTH parties' witnesses — `converge_fee` (1 witness) would
    // undersize the 2-signature swap and the node would reject it as below the
    // minimum fee.
    let unsigned = converge_fee_with_witnesses(
        move |fee| {
            // Network fee split evenly between the two parties.
            let (a_fee_share, b_fee_share) = split_network_fee(fee);

            // Party A's outflow = ADA sweetener A gives + the min-UTxO wrapper A
            //   funds for the assets A receives + A's fee share + A's platform share.
            //   A's own asset-input ADA (freed when A's offered UTxO is spent) returns
            //   to A as change — A no longer funds the wrapper for what it sends.
            let a_outflow = a_ada + a_wrapper + a_fee_share + a_platform_share;
            let a_change =
                a_total_lovelace
                    .checked_sub(a_outflow)
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: a_outflow,
                        available: a_total_lovelace,
                    })?;

            // Party B's outflow, symmetric.
            let b_outflow = b_ada + b_wrapper + b_fee_share + b_platform_share;
            let b_change =
                b_total_lovelace
                    .checked_sub(b_outflow)
                    .ok_or(TxBuildError::InsufficientFunds {
                        needed: b_outflow,
                        available: b_total_lovelace,
                    })?;

            // Build the transaction from the pre-selected minimal input set
            let all_inputs: Vec<&UtxoApi> = a_selected
                .iter()
                .copied()
                .chain(b_selected.iter().copied())
                .collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &all_inputs)?;

            // Keep each party's *payment* output separate from their *change*
            // output. A buyer's payment then shows as a discrete, recognisable
            // output line on a hardware wallet (Ledger); a merged payment+change
            // output displays as one inflated "send" the buyer can't verify.

            // Output 1: Party A receives B's offered assets + B's ADA sweetener
            tx = add_receive_output(tx, a_addr.clone(), &b_offered, b_ada, b_min_utxo)?;

            // Output 2: Party B receives A's offered assets + A's ADA sweetener
            tx = add_receive_output(tx, b_addr.clone(), &a_offered, a_ada, a_min_utxo)?;

            // Party A's change (split if needed)
            if a_change > 0 || !a_kept.is_empty() {
                let (new_tx, _) =
                    add_split_change_outputs(tx, a_addr.clone(), a_change, &a_kept, &params_clone)?;
                tx = new_tx;
            }

            // Party B's change (split if needed)
            if b_change > 0 || !b_kept.is_empty() {
                let (new_tx, _) =
                    add_split_change_outputs(tx, b_addr.clone(), b_change, &b_kept, &params_clone)?;
                tx = new_tx;
            }

            // Optional orchestration fee to community wallet
            if let Some((ref fee_addr, fee_amount)) = fee_output_clone {
                if fee_amount > 0 {
                    tx = tx.output(create_ada_output(fee_addr.clone(), fee_amount));
                }
            }

            Ok(tx.fee(fee).network_id(network_id))
        },
        300_000, // Initial fee estimate (generous for multi-input/output TX)
        params,
        SWAP_WITNESSES,
    )?;

    // Compute cost breakdown from the converged fee (already sized for both witnesses)
    let network_fee = unsigned.fee;
    let (a_network_share, b_network_share) = split_network_fee(network_fee);

    // Net ADA = ADA received from peer - ADA sweetener given - min-UTxO wrapper the
    // party funds for assets it *receives* - its network fee - its platform fee.
    // The wrapper is locked in the party's own wallet (recoverable), but counted as
    // a cost here since it isn't liquid. Change splitting is NOT a cost — that ADA
    // stays in the wallet.
    let a_net_ada = b_ada as i64
        - a_ada as i64
        - a_wrapper as i64
        - a_network_share as i64
        - a_platform_share as i64;
    let b_net_ada = a_ada as i64
        - b_ada as i64
        - b_wrapper as i64
        - b_network_share as i64
        - b_platform_share as i64;

    // Each side's own ADA that returns to them as change (= inputs - their outflow).
    let a_passthrough =
        a_total_lovelace.saturating_sub(a_ada + a_wrapper + a_network_share + a_platform_share);
    let b_passthrough =
        b_total_lovelace.saturating_sub(b_ada + b_wrapper + b_network_share + b_platform_share);

    Ok(SwapBuildResult {
        unsigned,
        costs: SwapCostBreakdown {
            network_fee,
            a_network_fee: a_network_share,
            b_network_fee: b_network_share,
            platform_fee: fee_output_amount,
            a_min_utxo_cost: a_wrapper,
            b_min_utxo_cost: b_wrapper,
            a_net_ada,
            b_net_ada,
            a_passthrough,
            b_passthrough,
        },
    })
}

/// Split the network fee evenly between the two parties (Party A absorbs the
/// rounding remainder). The min-UTxO wrapper is already allocated to the asset
/// recipient separately, so an even fee split still leaves an NFT seller with
/// essentially their full sale price (they bear only ~half the ~0.18 ADA fee).
/// Returns `(a_share, b_share)` with `a_share + b_share == total_fee`.
fn split_network_fee(total_fee: u64) -> (u64, u64) {
    (total_fee / 2 + total_fee % 2, total_fee / 2)
}

/// Create a receive output with offered assets and ADA sweetener.
fn add_receive_output(
    tx: StagingTransaction,
    address: Address,
    offered_assets: &HashMap<AssetId, u64>,
    ada_sweetener: u64,
    min_utxo: u64,
) -> Result<StagingTransaction, TxBuildError> {
    let lovelace = ada_sweetener.max(min_utxo);
    let output = create_ada_output(address, lovelace);
    if offered_assets.is_empty() {
        Ok(tx.output(output))
    } else {
        let output = add_assets_from_map(output, offered_assets)?;
        Ok(tx.output(output))
    }
}

/// Create a change output with kept assets and remaining ADA.
fn add_change_output(
    tx: StagingTransaction,
    address: Address,
    lovelace: u64,
    kept_assets: &HashMap<AssetId, u64>,
) -> Result<StagingTransaction, TxBuildError> {
    if kept_assets.is_empty() {
        Ok(tx.output(create_ada_output(address, lovelace)))
    } else {
        let output = create_ada_output(address, lovelace);
        let output = add_assets_from_map(output, kept_assets)?;
        Ok(tx.output(output))
    }
}

/// Add change outputs for one party, splitting across multiple outputs if the
/// value portion would exceed `max_value_size`.
///
/// Returns the total min UTxO locked in non-final chunks (the "change overhead"
/// cost borne by this party). The final chunk receives all remaining ADA.
fn add_split_change_outputs(
    mut tx: StagingTransaction,
    address: Address,
    total_lovelace: u64,
    kept_assets: &HashMap<AssetId, u64>,
    params: &TxBuildParams,
) -> Result<(StagingTransaction, u64), TxBuildError> {
    if kept_assets.is_empty() {
        if total_lovelace > 0 {
            tx = tx.output(create_ada_output(address, total_lovelace));
        }
        return Ok((tx, 0));
    }

    let chunks = split_assets_for_change(kept_assets, params.max_value_size);

    if chunks.len() <= 1 {
        // Single chunk — no splitting overhead
        tx = add_change_output(tx, address, total_lovelace, kept_assets)?;
        return Ok((tx, 0));
    }

    // Multiple chunks: each non-final chunk gets min UTxO, final gets remainder
    let mut remaining = total_lovelace;
    let mut overhead = 0u64;
    let last_idx = chunks.len() - 1;

    for (i, chunk) in chunks.iter().enumerate() {
        if i == last_idx {
            tx = add_change_output(tx, address.clone(), remaining, chunk)?;
        } else {
            let asset_ids: Vec<AssetId> = chunk.keys().cloned().collect();
            let min = min_utxo_for_assets(params, &asset_ids);
            overhead += min;
            remaining = remaining.saturating_sub(min);
            tx = add_change_output(tx, address.clone(), min, chunk)?;
        }
    }

    Ok((tx, overhead))
}

/// Split a set of kept assets into chunks whose estimated output value size
/// stays under `max_value_size`. Groups by policy to minimise chunk count.
fn split_assets_for_change(
    kept: &HashMap<AssetId, u64>,
    max_value_size: u64,
) -> Vec<HashMap<AssetId, u64>> {
    if kept.is_empty() {
        return vec![];
    }

    let threshold = max_value_size * 9 / 10; // 90% safety margin

    // Fast path: everything fits in one output
    if estimate_value_size_from_map(kept) <= threshold {
        return vec![kept.clone()];
    }

    // Group by policy for efficient packing (same-policy assets are cheap)
    let mut by_policy: std::collections::BTreeMap<&str, Vec<(&AssetId, u64)>> =
        std::collections::BTreeMap::new();
    for (asset_id, &qty) in kept {
        by_policy
            .entry(&asset_id.policy_id)
            .or_default()
            .push((asset_id, qty));
    }

    let mut chunks: Vec<HashMap<AssetId, u64>> = Vec::new();
    let mut current: HashMap<AssetId, u64> = HashMap::new();

    for (_policy, assets) in by_policy {
        // Try adding this policy's assets to the current chunk
        let mut tentative = current.clone();
        for &(asset_id, qty) in &assets {
            tentative.insert(asset_id.clone(), qty);
        }

        if estimate_value_size_from_map(&tentative) <= threshold || current.is_empty() {
            current = tentative;
        } else {
            // Current chunk is full — push it and start a new one
            chunks.push(std::mem::take(&mut current));
            for &(asset_id, qty) in &assets {
                current.insert(asset_id.clone(), qty);
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Estimate the CBOR-encoded size of the *value* portion of an output
/// carrying the given asset map. Used for `max_value_size` checks.
fn estimate_value_size_from_map(assets: &HashMap<AssetId, u64>) -> u64 {
    if assets.is_empty() {
        return 6; // lovelace only
    }

    let mut policies: HashMap<&str, Vec<&AssetId>> = HashMap::new();
    for asset_id in assets.keys() {
        policies
            .entry(&asset_id.policy_id)
            .or_default()
            .push(asset_id);
    }

    estimate_value_size_inner(&policies)
}

/// Estimate the CBOR-encoded size of the *value* portion of an output
/// carrying the given assets (grouped by policy). Used for both
/// `min_utxo_for_assets` and `max_value_size` checks.
fn estimate_value_size_inner(policies: &HashMap<&str, Vec<&AssetId>>) -> u64 {
    // lovelace: array(2) tag (1) + lovelace (5)
    let lovelace_size: u64 = 1 + 5;

    let policies_map_tag: u64 = if policies.len() < 24 { 1 } else { 3 };
    let mut policy_size: u64 = 0;
    for assets_in_policy in policies.values() {
        // Policy ID: 2-byte tag + 28 bytes = 30
        policy_size += 30;
        // Inner assets map tag
        let inner_tag: u64 = if assets_in_policy.len() < 24 { 1 } else { 3 };
        policy_size += inner_tag;
        for asset in assets_in_policy {
            let name_len = (asset.asset_name_hex.len() / 2) as u64;
            let name_tag: u64 = if name_len < 24 { 1 } else { 2 };
            policy_size += name_tag + name_len + 1; // +1 for quantity
        }
    }

    lovelace_size + policies_map_tag + policy_size
}

/// Compute min UTxO lovelace for a receive output carrying the given assets.
///
/// Uses the Babbage/Conway formula: `(160 + |serialized_output|) × coinsPerUTxOByte`
/// with a 10% safety margin. Returns 0 for pure-ADA outputs (no assets).
fn min_utxo_for_assets(params: &TxBuildParams, assets: &[AssetId]) -> u64 {
    if assets.is_empty() {
        return 0;
    }

    const UTXO_OVERHEAD: u64 = 160;
    // Map header (1) + key 0 (1) + address with stake key (59) + key 1 (1)
    let fixed_overhead: u64 = 1 + 1 + 59 + 1;

    // Group assets by policy to estimate map sizes
    let mut policies: HashMap<&str, Vec<&AssetId>> = HashMap::new();
    for asset in assets {
        policies.entry(&asset.policy_id).or_default().push(asset);
    }

    let value_size = estimate_value_size_inner(&policies);
    let output_size = fixed_overhead + value_size;
    let raw = (UTXO_OVERHEAD + output_size) * params.coins_per_utxo_byte;

    // 10% safety margin — protects against minor CBOR encoding variations
    raw + raw / 10
}

/// Subtract offered assets from total assets, returning what's kept.
fn subtract_assets(
    total: &HashMap<AssetId, u64>,
    offered: &HashMap<AssetId, u64>,
) -> HashMap<AssetId, u64> {
    let mut kept = total.clone();
    for (asset_id, offered_qty) in offered {
        if let Some(total_qty) = kept.get_mut(asset_id) {
            if *offered_qty >= *total_qty {
                kept.remove(asset_id);
            } else {
                *total_qty -= offered_qty;
            }
        }
    }
    kept
}

/// Select a minimal input set for one swap side: the UTxOs holding the offered
/// assets (must-spend), topped up with the smallest pure-ADA UTxO(s) needed to
/// cover `target_lovelace`. Replaces the old whole-wallet input sweep — the pool
/// top-up draws only pure-ADA UTxOs (see [`crate::select::select`]), so no
/// unrelated assets get dragged into the transaction as change.
fn select_side_inputs(
    side: &SwapSide,
    target_lovelace: u64,
) -> Result<Vec<&UtxoApi>, TxBuildError> {
    // A swap draws from the party's whole pool — nothing is earmarked/excluded.
    // Must be `'static` so the returned refs borrow only from `side.utxos`, not a
    // local (the `Selection` unifies all three borrows under one lifetime).
    static EMPTY_EXCLUDE: std::sync::OnceLock<HashSet<(String, u32)>> = std::sync::OnceLock::new();
    let exclude = EMPTY_EXCLUDE.get_or_init(HashSet::new);

    let must_spend = utxos_covering_offered(&side.utxos, &side.offered_assets)?;
    let sel = Selection {
        must_spend,
        pool: &side.utxos,
        exclude,
        strategy: Strategy::SmallestSufficient,
    };
    select(&sel, target_lovelace).map_err(|e| match e {
        crate::select::SelectError::Insufficient { target, available } => {
            TxBuildError::InsufficientFunds {
                needed: target,
                available,
            }
        }
        // A duplicate must-spend would only arise from a genuinely malformed UTxO
        // pool (same ref twice); surface it as "no suitable UTxO" rather than a
        // funds shortfall.
        crate::select::SelectError::DuplicateMustSpend { .. } => TxBuildError::NoSuitableUtxo,
    })
}

/// Greedily pick the UTxOs that hold a side's offered assets, accumulating UTxOs
/// per asset until the offered quantity is covered (an NFT = one UTxO; a fungible
/// token = however many UTxOs sum to the offered quantity). Returns
/// [`TxBuildError::NoSuitableUtxo`] if the pool can't cover an offered asset.
fn utxos_covering_offered<'a>(
    utxos: &'a [UtxoApi],
    offered: &HashMap<AssetId, u64>,
) -> Result<Vec<&'a UtxoApi>, TxBuildError> {
    if offered.is_empty() {
        return Ok(Vec::new());
    }

    // Remaining quantity still needed per offered asset.
    let mut needed: HashMap<AssetId, u64> = offered.clone();
    let mut chosen: Vec<&'a UtxoApi> = Vec::new();
    let mut chosen_refs: HashSet<(&'a str, u32)> = HashSet::new();

    for utxo in utxos {
        // Only take this UTxO if it still contributes to an uncovered offered asset.
        let contributes = utxo
            .assets
            .iter()
            .any(|aq| needed.get(&aq.asset_id).is_some_and(|&rem| rem > 0));
        if !contributes {
            continue;
        }
        if chosen_refs.insert((utxo.tx_hash.as_str(), utxo.output_index)) {
            for aq in &utxo.assets {
                if let Some(rem) = needed.get_mut(&aq.asset_id) {
                    *rem = rem.saturating_sub(aq.quantity);
                }
            }
            chosen.push(utxo);
        }
    }

    if needed.values().any(|&rem| rem > 0) {
        return Err(TxBuildError::NoSuitableUtxo);
    }

    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::AssetQuantity;

    const TEST_POLICY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_ADDR_A: &str = "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp";
    // Second valid testnet address (different key hash)
    const TEST_ADDR_B: &str = "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp";

    fn make_asset_id(name: &str) -> AssetId {
        AssetId::new_unchecked(TEST_POLICY.to_string(), hex::encode(name))
    }

    fn make_utxo(tx_hash: &str, lovelace: u64, assets: Vec<(AssetId, u64)>) -> UtxoApi {
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

    fn test_params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
            max_value_size: 5000,
            price_mem: None,
            price_step: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_simple_nft_swap() {
        let nft_a = make_asset_id("PirateA");
        let nft_b = make_asset_id("PirateB");

        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    10_000_000,
                    vec![(nft_a.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(
                    &"b".repeat(64),
                    10_000_000,
                    vec![(nft_b.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::from([(nft_b.clone(), 1)]),
                ada_lovelace: 0,
            },
        ];

        let result = build_atomic_swap(&sides, None, &test_params(), 0);
        assert!(result.is_ok(), "swap build failed: {result:?}");

        let swap = result.unwrap();
        assert!(swap.unsigned.fee > 0);
        assert!(
            swap.unsigned.fee < 1_000_000,
            "fee unreasonably high: {}",
            swap.unsigned.fee
        );
        // Both sides offer NFTs, both have min UTxO costs
        assert!(swap.costs.a_min_utxo_cost > 0);
        assert!(swap.costs.b_min_utxo_cost > 0);
    }

    #[test]
    fn test_swap_with_ada_sweetener() {
        let nft_a = make_asset_id("RareNFT");

        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    15_000_000,
                    vec![(nft_a.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(&"b".repeat(64), 20_000_000, vec![])],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::new(),
                ada_lovelace: 5_000_000, // 5 ADA sweetener
            },
        ];

        let result = build_atomic_swap(&sides, None, &test_params(), 0);
        assert!(result.is_ok(), "swap with sweetener failed: {result:?}");

        let swap = result.unwrap();
        // Min-UTxO is funded by the *recipient* of the assets:
        //  - Party A receives only ADA (B's sweetener) → funds no wrapper.
        //  - Party B receives A's NFT → funds its min-UTxO wrapper.
        assert_eq!(swap.costs.a_min_utxo_cost, 0);
        assert!(swap.costs.b_min_utxo_cost > 0);
        // Party A receives a 5 ADA sweetener with no costs → net positive.
        assert!(
            swap.costs.a_net_ada > 0,
            "seller should profit: {}",
            swap.costs.a_net_ada
        );
    }

    #[test]
    fn test_swap_fee_covers_two_witnesses() {
        // Regression: the fee must be >= the node's minimum for a *2-signature* tx.
        // The builder previously sized for 1 witness + a too-small fudge, so the
        // node rejected the tx as "fee below the minimum fee required".
        let nft_a = make_asset_id("A");
        let nft_b = make_asset_id("B");
        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    10_000_000,
                    vec![(nft_a.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(
                    &"b".repeat(64),
                    10_000_000,
                    vec![(nft_b.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::from([(nft_b.clone(), 1)]),
                ada_lovelace: 0,
            },
        ];
        let params = test_params();
        let swap = build_atomic_swap(&sides, None, &params, 0).unwrap();

        // The converged fee must cover the fully-signed (2-witness) transaction.
        let min_fee = crate::fee::calculate_fee_with_witnesses(&swap.unsigned.staging, &params, 2);
        assert!(
            swap.unsigned.fee >= min_fee,
            "fee {} is below the 2-witness minimum {min_fee}",
            swap.unsigned.fee
        );
    }

    #[test]
    fn test_swap_keeps_payment_separate_from_change() {
        // A sells an NFT for 2500 ADA. Each party's payment output is kept SEPARATE
        // from their change output (4 outputs total, not a merged 2) so the buyer's
        // payment shows as a discrete, verifiable line on a hardware wallet — do not
        // consolidate these, it produces one inflated "send" a Ledger can't verify.
        let nft = make_asset_id("ForSale");
        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    3_000_000,
                    vec![(nft.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(&"b".repeat(64), 2_600_000_000, vec![])],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::new(),
                ada_lovelace: 2_500_000_000,
            },
        ];
        let swap = build_atomic_swap(&sides, None, &test_params(), 0).unwrap();
        let output_count = swap
            .unsigned
            .staging
            .outputs
            .as_ref()
            .map(|o| o.len())
            .unwrap_or(0);
        assert_eq!(
            output_count, 4,
            "payment and change must stay as separate outputs per party"
        );
    }

    #[test]
    fn test_nft_sale_cost_allocation() {
        // A sells an NFT; B pays 2500 ADA. The buyer funds the NFT's min-UTxO
        // wrapper (it lands in the buyer's wallet), and the network fee is split
        // evenly — so the seller nets the price minus only their half of the fee.
        let nft = make_asset_id("ForSale");
        let price = 2_500_000_000u64;
        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    3_000_000,
                    vec![(nft.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                // Buyer's 2500 ADA across a couple of UTxOs + headroom for fee/wrapper.
                utxos: vec![
                    make_utxo(&"b".repeat(64), 2_000_000_000, vec![]),
                    make_utxo(&"c".repeat(64), 510_000_000, vec![]),
                ],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::new(),
                ada_lovelace: price,
            },
        ];

        let swap = build_atomic_swap(&sides, None, &test_params(), 0).unwrap();

        // Seller (A) funds no wrapper — the ADA lands cleanly.
        assert_eq!(swap.costs.a_min_utxo_cost, 0, "seller funds no wrapper");
        // Buyer (B) funds the NFT's min-UTxO wrapper (lands in the buyer's wallet).
        assert!(
            swap.costs.b_min_utxo_cost > 0,
            "buyer funds the NFT wrapper"
        );
        // Network fee split evenly between the two parties.
        assert_eq!(
            swap.costs.a_network_fee + swap.costs.b_network_fee,
            swap.costs.network_fee,
            "fee shares sum to the total"
        );
        assert_eq!(
            swap.costs.a_network_fee,
            swap.costs.network_fee / 2 + swap.costs.network_fee % 2,
            "seller bears half the fee (plus remainder)"
        );
        // Seller receives the price minus only their half of the network fee.
        assert_eq!(
            swap.costs.a_net_ada,
            price as i64 - swap.costs.a_network_fee as i64,
            "seller nets price minus half the fee"
        );
    }

    #[test]
    fn test_swap_with_fee_output() {
        let nft_a = make_asset_id("NFT1");
        let nft_b = make_asset_id("NFT2");

        let fee_addr = Address::from_bech32(TEST_ADDR_A).unwrap();

        let sides = [
            SwapSide {
                utxos: vec![make_utxo(
                    &"a".repeat(64),
                    10_000_000,
                    vec![(nft_a.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(
                    &"b".repeat(64),
                    10_000_000,
                    vec![(nft_b.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::from([(nft_b.clone(), 1)]),
                ada_lovelace: 0,
            },
        ];

        let result = build_atomic_swap(&sides, Some((fee_addr, 2_000_000)), &test_params(), 0);
        assert!(result.is_ok(), "swap with fee output failed: {result:?}");
    }

    #[test]
    fn test_subtract_assets() {
        let nft1 = make_asset_id("NFT1");
        let nft2 = make_asset_id("NFT2");
        let token = make_asset_id("TOKEN");

        let total = HashMap::from([(nft1.clone(), 1), (nft2.clone(), 1), (token.clone(), 100)]);

        let offered = HashMap::from([(nft1.clone(), 1), (token.clone(), 30)]);

        let kept = subtract_assets(&total, &offered);
        assert_eq!(kept.len(), 2);
        assert!(!kept.contains_key(&nft1));
        assert_eq!(kept[&nft2], 1);
        assert_eq!(kept[&token], 70);
    }

    #[test]
    fn test_insufficient_funds() {
        let nft = make_asset_id("NFT");

        // Party A has only 500K lovelace — not enough for receive output min (2 ADA)
        // plus fee share plus change output
        let sides = [
            SwapSide {
                utxos: vec![make_utxo(&"a".repeat(64), 500_000, vec![(nft.clone(), 1)])],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(&"b".repeat(64), 500_000, vec![])],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::new(),
                ada_lovelace: 0,
            },
        ];

        let result = build_atomic_swap(&sides, None, &test_params(), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_swap_with_many_policies() {
        // Simulate a wallet with 150 distinct policies — a single change output
        // would exceed maxValueSize (5000 bytes) without splitting.
        // 150 policies × ~38 bytes each ≈ 5700 bytes > 4500 threshold.
        let nft_a = make_asset_id("OfferedNFT");

        // Build 150 distinct policy assets in party A's UTxOs (kept, not offered)
        let mut kept_assets: Vec<(AssetId, u64)> = Vec::new();
        for i in 0..150 {
            let policy = format!("{:0>56}", format!("{i:028x}"));
            let asset = AssetId::new_unchecked(policy, hex::encode(format!("NFT{i}")));
            kept_assets.push((asset, 1));
        }

        // Party A offers one NFT from the test policy, but their UTxOs also contain 70 others
        let mut a_utxo_assets = vec![(nft_a.clone(), 1)];
        a_utxo_assets.extend(kept_assets.clone());

        let nft_b = make_asset_id("TheirNFT");

        let sides = [
            SwapSide {
                utxos: vec![make_utxo(&"a".repeat(64), 50_000_000, a_utxo_assets)],
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: vec![make_utxo(
                    &"b".repeat(64),
                    10_000_000,
                    vec![(nft_b.clone(), 1)],
                )],
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::from([(nft_b.clone(), 1)]),
                ada_lovelace: 0,
            },
        ];

        let result = build_atomic_swap(&sides, None, &test_params(), 0);
        assert!(result.is_ok(), "many-policy swap failed: {result:?}");

        let swap = result.unwrap();
        // TX should build successfully despite many policies (change gets split)
        assert!(swap.unsigned.fee > 0);
        // Net ADA should NOT include change overhead — split change is still yours
        assert!(
            swap.costs.a_net_ada > -5_000_000,
            "net ADA unreasonably negative (change overhead leaked?): {}",
            swap.costs.a_net_ada
        );
    }

    #[test]
    fn test_coin_selection_bounds_inputs() {
        // Party offers one NFT held in a single UTxO, but the wallet also holds
        // 50 unrelated pure-ADA UTxOs. Selection must NOT sweep them all.
        let nft = make_asset_id("OfferedNFT");
        let mut utxos = vec![make_utxo(
            &"f".repeat(64),
            3_000_000,
            vec![(nft.clone(), 1)],
        )];
        for i in 0..50u32 {
            utxos.push(make_utxo(&format!("{i:064x}"), 2_000_000, vec![]));
        }
        let side = SwapSide {
            utxos,
            address: Address::from_bech32(TEST_ADDR_A).unwrap(),
            offered_assets: HashMap::from([(nft.clone(), 1)]),
            ada_lovelace: 0,
        };
        // The NFT UTxO alone (3 ADA) already covers the small target → 1 input.
        let selected = select_side_inputs(&side, 2_500_000).unwrap();
        assert_eq!(selected.len(), 1, "expected only the offered-asset UTxO");
        assert!(selected[0].assets.iter().any(|aq| aq.asset_id == nft));
    }

    #[test]
    fn test_coin_selection_tops_up_ada() {
        // NFT sits in a UTxO with only 1 ADA — selection must add ADA UTxOs to
        // reach the target, but still far fewer than the whole wallet.
        let nft = make_asset_id("OfferedNFT");
        let mut utxos = vec![make_utxo(
            &"f".repeat(64),
            1_000_000,
            vec![(nft.clone(), 1)],
        )];
        for i in 0..8u32 {
            utxos.push(make_utxo(&format!("{i:064x}"), 2_000_000, vec![]));
        }
        let side = SwapSide {
            utxos,
            address: Address::from_bech32(TEST_ADDR_A).unwrap(),
            offered_assets: HashMap::from([(nft.clone(), 1)]),
            ada_lovelace: 0,
        };
        let selected = select_side_inputs(&side, 4_000_000).unwrap();
        assert!(
            (2..6).contains(&selected.len()),
            "expected a small topped-up set, got {}",
            selected.len()
        );
        assert!(selected
            .iter()
            .any(|u| u.assets.iter().any(|aq| aq.asset_id == nft)));
        let total: u64 = selected.iter().map(|u| u.lovelace).sum();
        assert!(total >= 4_000_000);
    }

    #[test]
    fn test_utxos_covering_offered_fungible_accumulates() {
        // A fungible offer of 100 spread across two 60-qty UTxOs needs both.
        let ft = make_asset_id("TOKEN");
        let utxos = vec![
            make_utxo(&"a".repeat(64), 2_000_000, vec![(ft.clone(), 60)]),
            make_utxo(&"b".repeat(64), 2_000_000, vec![(ft.clone(), 60)]),
            make_utxo(&"c".repeat(64), 2_000_000, vec![]),
        ];
        let offered = HashMap::from([(ft.clone(), 100)]);
        let covering = utxos_covering_offered(&utxos, &offered).unwrap();
        assert_eq!(covering.len(), 2, "need both token UTxOs to cover qty 100");
    }

    #[test]
    fn test_swap_ignores_unrelated_utxos() {
        // Each side offers one NFT (in one UTxO) but holds 40 unrelated ADA UTxOs.
        // Fee scales with tx size, so a bounded fee proves inputs weren't swept.
        let nft_a = make_asset_id("PirateA");
        let nft_b = make_asset_id("PirateB");

        let mut a_utxos = vec![make_utxo(
            &"a".repeat(64),
            5_000_000,
            vec![(nft_a.clone(), 1)],
        )];
        let mut b_utxos = vec![make_utxo(
            &"b".repeat(64),
            5_000_000,
            vec![(nft_b.clone(), 1)],
        )];
        for i in 0..40u32 {
            a_utxos.push(make_utxo(&format!("a{i:063x}"), 2_000_000, vec![]));
            b_utxos.push(make_utxo(&format!("b{i:063x}"), 2_000_000, vec![]));
        }

        let sides = [
            SwapSide {
                utxos: a_utxos,
                address: Address::from_bech32(TEST_ADDR_A).unwrap(),
                offered_assets: HashMap::from([(nft_a.clone(), 1)]),
                ada_lovelace: 0,
            },
            SwapSide {
                utxos: b_utxos,
                address: Address::from_bech32(TEST_ADDR_B).unwrap(),
                offered_assets: HashMap::from([(nft_b.clone(), 1)]),
                ada_lovelace: 0,
            },
        ];

        let swap = build_atomic_swap(&sides, None, &test_params(), 0).unwrap();
        // Each NFT UTxO (5 ADA) covers its side's small target alone → ~2 inputs.
        // If all 82 UTxOs were swept in, the size-driven fee would balloon.
        assert!(
            swap.unsigned.fee < 300_000,
            "fee too high — inputs not minimised: {}",
            swap.unsigned.fee
        );
    }
}
