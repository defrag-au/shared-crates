//! Marketplace buy/sweep TX builder.
//!
//! Constructs Plutus script spend transactions to buy NFTs from marketplace
//! listings (e.g. JPG.store). Supports single-item buys and multi-item sweeps.
//!
//! # How it works
//!
//! 1. Each listing is a UTxO locked at a marketplace script address with a datum
//!    encoding payout obligations (seller, marketplace fee, royalties).
//! 2. The buy TX consumes these script UTxOs, provides the buy redeemer, and creates
//!    outputs satisfying all payout obligations.
//! 3. The buyer receives all NFTs + ADA change in a single output.
//! 4. A reference input provides the Plutus script (no script embedded in TX).

use cardano_assets::utxo::UtxoApi;
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{Input, Output, ScriptKind, StagingTransaction};

use super::cost_models::PLUTUS_V2_COST_MODEL;
use crate::builder::marketplace::ParsedListing;
use crate::builder::UnsignedTx;
use crate::error::TxBuildError;
use crate::helpers::decode::decode_tx_hash;
use crate::helpers::output::create_ada_output;
use crate::params::TxBuildParams;

/// Script reference input (not consumed, just referenced for script lookup).
#[derive(Debug, Clone)]
pub struct ScriptRefInput {
    pub tx_hash: String,
    pub output_index: u32,
}

/// Dependencies for a marketplace buy TX.
#[derive(Debug)]
pub struct BuyDeps {
    /// Buyer's wallet UTxOs (for paying fees + payout obligations)
    pub buyer_utxos: Vec<UtxoApi>,
    /// Protocol parameters for fee calculation
    pub params: TxBuildParams,
    /// Buyer's receiving address
    pub buyer_address: Address,
    /// Network ID (1 = mainnet, 0 = testnet)
    pub network_id: u8,
    /// Collateral UTxO for Plutus execution
    pub collateral_utxo: UtxoApi,
    /// Reference script UTxO (contains the marketplace Plutus script)
    pub script_ref: ScriptRefInput,
}

/// Estimated execution units for a buy redeemer.
/// These are generous estimates — the actual cost is lower but we need headroom.
const BUY_EX_UNITS_MEM: u64 = 1_400_000;
const BUY_EX_UNITS_STEPS: u64 = 500_000_000;

/// Build a marketplace buy/sweep TX.
///
/// Consumes one or more listing UTxOs from marketplace script addresses,
/// pays all datum-specified payout targets, and sends NFTs + change to the buyer.
pub fn build_buy(deps: &BuyDeps, listings: &[ParsedListing]) -> Result<UnsignedTx, TxBuildError> {
    if listings.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "No listings provided".to_string(),
        ));
    }

    // Calculate total payout obligations
    let total_payout_lovelace: u64 = listings
        .iter()
        .flat_map(|l| &l.payouts)
        .map(|p| p.lovelace)
        .sum();

    // Calculate total lovelace available from buyer wallet UTxOs
    let total_buyer_lovelace: u64 = deps.buyer_utxos.iter().map(|u| u.lovelace).sum();

    // Calculate total lovelace in listing UTxOs (returned to buyer as part of the buy)
    let total_listing_lovelace: u64 = listings.iter().map(|l| l.utxo.lovelace).sum();

    // Extract buyer payment key hash for disclosed_signer
    let buyer_pkh = extract_payment_key_hash(&deps.buyer_address)?;

    // Build reference input
    let ref_tx_hash = decode_tx_hash(&deps.script_ref.tx_hash)?;
    let ref_input = Input::new(Hash::from(ref_tx_hash), deps.script_ref.output_index as u64);

    // Build script inputs (listing UTxOs)
    let mut script_inputs = Vec::new();
    for listing in listings {
        let tx_hash = decode_tx_hash(&listing.utxo.tx_hash)?;
        script_inputs.push(Input::new(
            Hash::from(tx_hash),
            listing.utxo.output_index as u64,
        ));
    }

    // Build collateral input
    let collateral_tx_hash = decode_tx_hash(&deps.collateral_utxo.tx_hash)?;
    let collateral_input = Input::new(
        Hash::from(collateral_tx_hash),
        deps.collateral_utxo.output_index as u64,
    );

    // Decode buy redeemer CBOR
    let redeemer_bytes = hex::decode("d87980")
        .map_err(|e| TxBuildError::BuildFailed(format!("Invalid redeemer hex: {e}")))?;

    // Use estimated fee + margin for Plutus TXs (not converge_fee)
    let estimated_fee = 500_000u64; // 0.5 ADA — generous estimate for Plutus TX

    let build_tx = |fee: u64| -> Result<StagingTransaction, TxBuildError> {
        let mut tx = StagingTransaction::new();

        // 1. Add script inputs (listing UTxOs)
        for input in &script_inputs {
            tx = tx.input(input.clone());
        }

        // 2. Add buyer wallet inputs
        for utxo in &deps.buyer_utxos {
            let tx_hash = decode_tx_hash(&utxo.tx_hash)?;
            tx = tx.input(Input::new(Hash::from(tx_hash), utxo.output_index as u64));
        }

        // 3. Add reference input (script reference UTxO)
        tx = tx.reference_input(ref_input.clone());

        // 4. Add collateral
        tx = tx.collateral_input(collateral_input.clone());

        // 5. Add spend redeemers for each script input
        for input in &script_inputs {
            tx = tx.add_spend_redeemer(
                input.clone(),
                redeemer_bytes.clone(),
                Some(pallas_txbuilder::ExUnits {
                    mem: BUY_EX_UNITS_MEM,
                    steps: BUY_EX_UNITS_STEPS,
                }),
            );
        }

        // 6. Add language view (PlutusV2 cost model)
        tx = tx.language_view(ScriptKind::PlutusV2, PLUTUS_V2_COST_MODEL.to_vec());

        // 7. Add disclosed signer (buyer's payment key hash)
        tx = tx.disclosed_signer(Hash::from(buyer_pkh));

        // 8. Create payout outputs (fulfilling datum obligations)
        let payout_outputs = build_payout_outputs(listings)?;
        for output in payout_outputs {
            tx = tx.output(output);
        }

        // 9. Create buyer output with NFTs + ADA change
        let buyer_change =
            total_buyer_lovelace + total_listing_lovelace - total_payout_lovelace - fee;

        let mut buyer_output = create_ada_output(deps.buyer_address.clone(), buyer_change);

        // Add all NFTs from listings to buyer output
        for listing in listings {
            for asset in &listing.utxo.assets {
                let policy_bytes =
                    crate::helpers::decode::decode_policy_id(asset.asset_id.policy_id())?;
                let asset_name_bytes =
                    crate::helpers::decode::decode_asset_name(asset.asset_id.asset_name_hex());
                buyer_output = buyer_output
                    .add_asset(Hash::from(policy_bytes), asset_name_bytes, asset.quantity)
                    .map_err(|e| {
                        TxBuildError::BuildFailed(format!("Failed to add NFT to buyer output: {e}"))
                    })?;
            }
        }

        // Also preserve any existing buyer wallet assets
        for utxo in &deps.buyer_utxos {
            for asset in &utxo.assets {
                let policy_bytes =
                    crate::helpers::decode::decode_policy_id(asset.asset_id.policy_id())?;
                let asset_name_bytes =
                    crate::helpers::decode::decode_asset_name(asset.asset_id.asset_name_hex());
                buyer_output = buyer_output
                    .add_asset(Hash::from(policy_bytes), asset_name_bytes, asset.quantity)
                    .map_err(|e| {
                        TxBuildError::BuildFailed(format!(
                            "Failed to add existing asset to buyer output: {e}"
                        ))
                    })?;
            }
        }

        tx = tx.output(buyer_output);

        Ok(tx.fee(fee).network_id(deps.network_id))
    };

    // Two-round fee convergence for Plutus TX
    let preliminary_tx = build_tx(estimated_fee)?;
    let fee_round1 = crate::fee::calculate_fee(&preliminary_tx, &deps.params);
    // Add margin for Plutus script execution overhead
    let fee_with_margin = fee_round1 + 200_000; // 0.2 ADA margin for Plutus

    let staging = build_tx(fee_with_margin)?;

    // Verify we have enough funds
    let total_needed = total_payout_lovelace + fee_with_margin;
    let total_available = total_buyer_lovelace + total_listing_lovelace;
    if total_needed > total_available {
        return Err(TxBuildError::InsufficientFunds {
            needed: total_needed,
            available: total_available,
        });
    }

    Ok(UnsignedTx {
        staging,
        fee: fee_with_margin,
    })
}

/// Build payout outputs from all listing payouts.
///
/// Merges payouts to the same address into a single output (e.g. marketplace fee
/// from multiple listings going to the same fee address).
fn build_payout_outputs(listings: &[ParsedListing]) -> Result<Vec<Output>, TxBuildError> {
    use std::collections::HashMap;

    // Collect payouts, merging by address
    let mut merged: HashMap<Vec<u8>, (Address, u64)> = HashMap::new();

    for listing in listings {
        for payout in &listing.payouts {
            let addr_bytes = payout.address.to_vec();
            let entry = merged
                .entry(addr_bytes)
                .or_insert_with(|| (payout.address.clone(), 0));
            entry.1 += payout.lovelace;
        }
    }

    // Convert to outputs
    let outputs: Vec<Output> = merged
        .into_values()
        .map(|(address, lovelace)| create_ada_output(address, lovelace))
        .collect();

    Ok(outputs)
}

/// Extract the 28-byte payment key hash from a Shelley address.
fn extract_payment_key_hash(address: &Address) -> Result<[u8; 28], TxBuildError> {
    let addr_bytes = address.to_vec();
    if addr_bytes.len() < 29 {
        return Err(TxBuildError::BuildFailed(format!(
            "Address too short for payment key hash extraction: {} bytes",
            addr_bytes.len()
        )));
    }
    // Skip header byte, take next 28 bytes
    addr_bytes[1..29]
        .try_into()
        .map_err(|_| TxBuildError::BuildFailed("Failed to extract payment key hash".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_payment_key_hash() {
        let addr = Address::from_bech32(
            "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp"
        ).unwrap();
        let pkh = extract_payment_key_hash(&addr).unwrap();
        assert_eq!(pkh.len(), 28);
    }

    #[test]
    fn test_build_buy_empty_listings() {
        let addr = Address::from_bech32(
            "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp"
        ).unwrap();

        let deps = BuyDeps {
            buyer_utxos: vec![],
            params: TxBuildParams {
                min_fee_coefficient: 44,
                min_fee_constant: 155381,
                coins_per_utxo_byte: 4310,
                max_tx_size: 16384,
                max_value_size: 5000,
                price_mem: None,
                price_step: None,
                ..Default::default()
            },
            buyer_address: addr,
            network_id: 0,
            collateral_utxo: UtxoApi {
                tx_hash: "a".repeat(64),
                output_index: 0,
                lovelace: 5_000_000,
                assets: vec![],
                tags: vec![],
            },
            script_ref: ScriptRefInput {
                tx_hash: "b".repeat(64),
                output_index: 0,
            },
        };

        let result = build_buy(&deps, &[]);
        assert!(result.is_err());
    }
}
