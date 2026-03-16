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

/// Minimum lovelace on a receive output (covers min UTxO for NFT outputs).
const MIN_RECEIVE_LOVELACE: u64 = 2_000_000;

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
/// An [`UnsignedTx`] ready for CIP-30 partial signing by both parties.
pub fn build_atomic_swap(
    sides: &[SwapSide; 2],
    fee_output: Option<(Address, u64)>,
    params: &TxBuildParams,
    network_id: u8,
) -> Result<UnsignedTx, TxBuildError> {
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

    // Min ADA for receive outputs — each party funds the output that delivers
    // their offered assets to the peer. If ADA sweetener exceeds min, use that.
    let a_receive_min = a.ada_lovelace.max(MIN_RECEIVE_LOVELACE);
    let b_receive_min = b.ada_lovelace.max(MIN_RECEIVE_LOVELACE);

    converge_fee(
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
            tx = add_receive_output(tx, a_addr.clone(), &b_offered, b_ada)?;

            // Output 2: Party B receives A's offered assets + A's ADA sweetener
            tx = add_receive_output(tx, b_addr.clone(), &a_offered, a_ada)?;

            // Output 3: Party A's change (kept assets + remaining ADA)
            if a_change > 0 || !a_kept.is_empty() {
                tx = add_change_output(tx, a_addr.clone(), a_change, &a_kept)?;
            }

            // Output 4: Party B's change (kept assets + remaining ADA)
            if b_change > 0 || !b_kept.is_empty() {
                tx = add_change_output(tx, b_addr.clone(), b_change, &b_kept)?;
            }

            // Output 5: Optional orchestration fee to community wallet
            if let Some((ref fee_addr, fee_amount)) = fee_output_clone {
                if fee_amount > 0 {
                    tx = tx.output(create_ada_output(fee_addr.clone(), fee_amount));
                }
            }

            Ok(tx.fee(total_fee).network_id(network_id))
        },
        300_000, // Initial fee estimate (generous for multi-input/output TX)
        params,
    )
}

/// Create a receive output with offered assets and ADA sweetener.
fn add_receive_output(
    tx: StagingTransaction,
    address: Address,
    offered_assets: &HashMap<AssetId, u64>,
    ada_sweetener: u64,
) -> Result<StagingTransaction, TxBuildError> {
    let lovelace = ada_sweetener.max(MIN_RECEIVE_LOVELACE);
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
        }
    }

    fn test_params() -> TxBuildParams {
        TxBuildParams {
            min_fee_coefficient: 44,
            min_fee_constant: 155381,
            coins_per_utxo_byte: 4310,
            max_tx_size: 16384,
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

        let unsigned = result.unwrap();
        assert!(unsigned.fee > 0);
        assert!(
            unsigned.fee < 1_000_000,
            "fee unreasonably high: {}",
            unsigned.fee
        );
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
}
