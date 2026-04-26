//! Atomic swap transaction builder.
//!
//! Builds a regular Cardano transaction where two parties exchange assets
//! without smart contracts. Each party's UTxOs are used as inputs, and
//! outputs deliver the swapped assets to each party's address.
//!
//! The transaction requires VKey witnesses from both parties (CIP-30 partial signing).

use std::collections::HashMap;

use cardano_assets::{AssetId, UtxoApi};
use pallas_addresses::Address;
use pallas_txbuilder::StagingTransaction;

use super::{converge_fee, UnsignedTx};
use crate::error::TxBuildError;
use crate::helpers::input::add_utxo_inputs;
use crate::helpers::output::{add_assets_from_map, create_ada_output};
use crate::helpers::utxo_query::{collect_utxo_native_assets, total_utxo_lovelace};
use crate::params::TxBuildParams;

/// One side of an atomic swap.
pub struct SwapSide {
    /// UTxOs available from this party (pre-selected to cover offered assets).
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
    /// Total network TX fee (split between parties).
    pub network_fee: u64,
    /// Total platform/orchestration fee.
    pub platform_fee: u64,
    /// Min UTxO lovelace Party A must fund for the output carrying their assets to B.
    pub a_min_utxo_cost: u64,
    /// Min UTxO lovelace Party B must fund for the output carrying their assets to A.
    pub b_min_utxo_cost: u64,
    /// Net ADA gain/loss for Party A (positive = gains ADA, negative = loses ADA).
    pub a_net_ada: i64,
    /// Net ADA gain/loss for Party B.
    pub b_net_ada: i64,
}

/// Additional fee per VKey witness beyond the first (bytes × min_fee_coefficient).
/// Each VKey witness is ~100 bytes CBOR. `converge_fee` accounts for one witness
/// via its dummy-sign approach, so we add the marginal cost of each extra signer.
const EXTRA_WITNESS_BYTES: u64 = 100;

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

    let a_utxo_refs: Vec<&UtxoApi> = a.utxos.iter().collect();
    let b_utxo_refs: Vec<&UtxoApi> = b.utxos.iter().collect();

    let a_total_lovelace = total_utxo_lovelace(&a_utxo_refs);
    let b_total_lovelace = total_utxo_lovelace(&b_utxo_refs);

    // Collect all native assets from each side's UTxOs
    let a_all_assets = collect_utxo_native_assets(&a_utxo_refs, None);
    let b_all_assets = collect_utxo_native_assets(&b_utxo_refs, None);

    // Calculate assets each side keeps (all assets minus what they offered)
    let a_kept_assets = subtract_assets(&a_all_assets, &a.offered_assets);
    let b_kept_assets = subtract_assets(&b_all_assets, &b.offered_assets);

    // Fee output amount (0 if no fee)
    let fee_output_amount = fee_output.as_ref().map(|(_, amt)| *amt).unwrap_or(0);

    // Extra witness cost: 1 extra signer beyond what converge_fee accounts for
    let extra_witness_fee = EXTRA_WITNESS_BYTES * params.min_fee_coefficient;

    // Each side pays half the TX fee + half the orchestration fee
    // (Fee is split evenly; the orchestration fee is already accounted for in fee_output)
    let fee_output_clone = fee_output.clone();
    let a_addr = a.address.clone();
    let b_addr = b.address.clone();
    let a_offered = a.offered_assets.clone();
    let b_offered = b.offered_assets.clone();
    let a_ada = a.ada_lovelace;
    let b_ada = b.ada_lovelace;
    let a_kept = a_kept_assets.clone();
    let b_kept = b_kept_assets.clone();

    // Compute min UTxO for each receive output based on actual assets being delivered.
    // Each party funds the output that delivers their offered assets to the peer.
    let a_offered_ids: Vec<AssetId> = a.offered_assets.keys().cloned().collect();
    let b_offered_ids: Vec<AssetId> = b.offered_assets.keys().cloned().collect();
    let a_min_utxo = min_utxo_for_assets(params, &a_offered_ids);
    let b_min_utxo = min_utxo_for_assets(params, &b_offered_ids);

    let a_receive_min = a.ada_lovelace.max(a_min_utxo);
    let b_receive_min = b.ada_lovelace.max(b_min_utxo);

    let params_clone = params.clone();
    let unsigned = converge_fee(
        move |fee| {
            let total_fee = fee + extra_witness_fee;
            // Split TX fee evenly between parties
            let fee_per_side = total_fee / 2;
            let fee_remainder = total_fee % 2; // Party A absorbs the rounding remainder

            // Party A's change = total input - receive output they fund (B gets A's assets)
            //                    - fee share - orchestration fee share
            // A funds the output that delivers A's offered assets to B (a_receive_min ADA)
            let a_change = a_total_lovelace
                .checked_sub(a_receive_min)
                .and_then(|v| v.checked_sub(fee_per_side + fee_remainder))
                .and_then(|v| v.checked_sub(fee_output_amount / 2))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: a_receive_min + fee_per_side + fee_remainder + fee_output_amount / 2,
                    available: a_total_lovelace,
                })?;

            // Party B's change = total input - receive output they fund (A gets B's assets)
            //                    - fee share - orchestration fee share
            let b_change = b_total_lovelace
                .checked_sub(b_receive_min)
                .and_then(|v| v.checked_sub(fee_per_side))
                .and_then(|v| v.checked_sub(fee_output_amount - fee_output_amount / 2))
                .ok_or(TxBuildError::InsufficientFunds {
                    needed: b_receive_min + fee_per_side + fee_output_amount
                        - fee_output_amount / 2,
                    available: b_total_lovelace,
                })?;

            // Build the transaction
            let all_inputs: Vec<&UtxoApi> = a.utxos.iter().chain(b.utxos.iter()).collect();
            let mut tx = add_utxo_inputs(StagingTransaction::new(), &all_inputs)?;

            // Output 1: Party A receives B's offered assets + B's ADA sweetener
            tx = add_receive_output(tx, a_addr.clone(), &b_offered, b_ada, b_min_utxo)?;

            // Output 2: Party B receives A's offered assets + A's ADA sweetener
            tx = add_receive_output(tx, b_addr.clone(), &a_offered, a_ada, a_min_utxo)?;

            // Outputs 3+: Party A's change (split if needed)
            if a_change > 0 || !a_kept.is_empty() {
                let (new_tx, _) =
                    add_split_change_outputs(tx, a_addr.clone(), a_change, &a_kept, &params_clone)?;
                tx = new_tx;
            }

            // Outputs N+: Party B's change (split if needed)
            if b_change > 0 || !b_kept.is_empty() {
                let (new_tx, _) =
                    add_split_change_outputs(tx, b_addr.clone(), b_change, &b_kept, &params_clone)?;
                tx = new_tx;
            }

            // Final output: Optional orchestration fee to community wallet
            if let Some((ref fee_addr, fee_amount)) = fee_output_clone {
                if fee_amount > 0 {
                    tx = tx.output(create_ada_output(fee_addr.clone(), fee_amount));
                }
            }

            Ok(tx.fee(total_fee).network_id(network_id))
        },
        300_000, // Initial fee estimate (generous for multi-input/output TX)
        params,
    )?;

    // Compute cost breakdown from the converged fee
    let network_fee = unsigned.fee;
    let total_fee_with_witness = network_fee + extra_witness_fee;
    let fee_per_side = total_fee_with_witness / 2;
    let fee_remainder = total_fee_with_witness % 2;

    let a_platform_share = fee_output_amount / 2;
    let b_platform_share = fee_output_amount - a_platform_share;
    let a_network_share = fee_per_side + fee_remainder;
    let b_network_share = fee_per_side;

    // Net ADA = received from peer - sent to peer - min UTxO cost - network fee - platform fee
    // Note: change output splitting is NOT a cost — that ADA stays in your wallet.
    let a_net_ada = b_ada as i64
        - a_ada as i64
        - a_min_utxo as i64
        - a_network_share as i64
        - a_platform_share as i64;
    let b_net_ada = a_ada as i64
        - b_ada as i64
        - b_min_utxo as i64
        - b_network_share as i64
        - b_platform_share as i64;

    Ok(SwapBuildResult {
        unsigned,
        costs: SwapCostBreakdown {
            network_fee: total_fee_with_witness,
            platform_fee: fee_output_amount,
            a_min_utxo_cost: a_min_utxo,
            b_min_utxo_cost: b_min_utxo,
            a_net_ada,
            b_net_ada,
        },
    })
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
        // Party A offers NFT (has min UTxO cost), Party B offers pure ADA (no min UTxO)
        assert!(swap.costs.a_min_utxo_cost > 0);
        assert_eq!(swap.costs.b_min_utxo_cost, 0);
        // Party A receives 5 ADA sweetener, net should be positive
        assert!(
            swap.costs.a_net_ada > 0,
            "seller should profit: {}",
            swap.costs.a_net_ada
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
}
