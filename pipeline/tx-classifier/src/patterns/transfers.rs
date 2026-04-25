//! Transfer transaction pattern detection

use super::{PatternContext, PatternDetectionResult};
use crate::*;
use pipeline_types::AssetId;
use std::collections::{BTreeMap, HashMap};
use tracing::debug;

/// Detect asset transfers
pub fn detect_transfers_wrapper(context: &PatternContext) -> PatternDetectionResult {
    let mut transactions = Vec::new();

    // Run asset transfer detection using refined rule
    transactions.extend(detect_asset_transfer_using_rule(context));

    // Run simple transfer detection as fallback
    transactions.extend(detect_simple_transfer(context));

    // Run other transfer-related patterns
    transactions.extend(detect_asset_burn(context));
    transactions.extend(detect_smart_contract(context));
    transactions.extend(detect_asset_staking(context));
    transactions.extend(detect_vesting(context));

    PatternDetectionResult { transactions }
}

/// Detect asset transfers between regular addresses (not marketplaces)
fn detect_asset_transfer_using_rule(context: &PatternContext) -> Vec<(TxType, f64)> {
    // Work purely with the genuine asset operations - no raw transaction data needed
    if context.asset_operations.is_empty() {
        return vec![];
    }

    // Look for asset transfers (excluding mints/burns)
    let transfer_ops: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine)
                && matches!(op.op_type, AssetOpType::Transfer)
        })
        .collect();

    if transfer_ops.is_empty() {
        return vec![];
    }

    // Group transfers by recipient to identify the primary transfer destination
    let mut recipient_assets: BTreeMap<String, Vec<&AssetOperation>> = BTreeMap::new();

    for op in &transfer_ops {
        if let Some(to_utxo) = &op.output {
            let to_addr = &to_utxo.address;
            recipient_assets
                .entry(to_addr.clone())
                .or_default()
                .push(op);
        }
    }

    // Find the main recipient (one who receives the most assets)
    let main_recipient = recipient_assets
        .iter()
        .max_by_key(|(_, ops)| ops.len())
        .map(|(addr, _)| addr.clone());

    let receiver = match main_recipient {
        Some(addr) => addr,
        None => return vec![],
    };

    // Check that the receiving address is NOT a known script address
    // (to distinguish from listings/marketplace interactions)
    if is_script_address(&receiver) {
        return vec![];
    }

    // Find the sender (most common from_address in genuine transfers)
    let sender_counts: HashMap<String, usize> = transfer_ops
        .iter()
        .filter_map(|op| op.input.as_ref().map(|utxo| &utxo.address))
        .fold(HashMap::new(), |mut acc, addr| {
            *acc.entry(addr.clone()).or_insert(0) += 1;
            acc
        });

    let sender = sender_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(addr, _)| addr)
        .unwrap_or_else(|| "unknown".to_string());

    // Collect the genuinely transferred assets to the main recipient (native tokens only)
    let transferred_assets: Vec<AssetId> = recipient_assets
        .get(&receiver)
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|op| {
            op.asset_id()
                .and_then(|id| AssetId::parse_concatenated(&id).ok())
        })
        .collect();

    if transferred_assets.is_empty() {
        return vec![];
    }

    debug!(
        "Detected asset transfer: {} assets from {} to {}",
        transferred_assets.len(),
        sender,
        receiver
    );

    let tx_type = TxType::AssetTransfer {
        assets: transferred_assets,
        sender,
        receiver,
    };

    // High confidence for simple asset transfers without script involvement
    let confidence = 0.90;

    vec![(tx_type, confidence)]
}

/// Detect simple asset transfers without payment
fn detect_simple_transfer(context: &PatternContext) -> Vec<(TxType, f64)> {
    let transfer_ops: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            if matches!(op.classification, crate::OperationClassification::Genuine)
                && op.op_type == AssetOpType::Transfer
            {
                // Exclude same-address "transfers" (these are listing updates, not true transfers)
                if let (Some(input), Some(output)) = (&op.input, &op.output) {
                    if input.address == output.address {
                        debug!(
                            "Excluding same-address transfer for asset {:?} at address {}",
                            op.payload.get_asset().map(|a| a.dot_delimited()),
                            input.address
                        );
                        return false; // Not a real transfer
                    }
                }
                true
            } else {
                false
            }
        })
        .collect();

    if transfer_ops.is_empty() {
        return vec![];
    }

    // Since we're in simple transfer detection, we assume this is just an asset transfer
    // without significant ADA payment (otherwise it would be caught by sale patterns)
    let confidence = 0.5;

    let asset_ids: Vec<AssetId> = transfer_ops
        .iter()
        .filter_map(|op| {
            op.asset_id()
                .and_then(|id| AssetId::parse_concatenated(&id).ok())
        })
        .collect();

    if asset_ids.is_empty() {
        return vec![];
    }

    debug!("Detected simple transfer: {} assets", asset_ids.len());

    vec![(TxType::Transfer { assets: asset_ids }, confidence)]
}

/// Detect asset burning transactions
fn detect_asset_burn(context: &PatternContext) -> Vec<(TxType, f64)> {
    let burn_ops: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine)
                && op.op_type == AssetOpType::Burn
        })
        .collect();

    if burn_ops.is_empty() {
        return vec![];
    }

    let confidence = 0.9; // Burn is very clear when it happens

    let policy_counts = count_assets_by_policy(&burn_ops);
    let (primary_policy, asset_count) =
        match policy_counts.into_iter().max_by_key(|(_, count)| *count) {
            Some((policy, count)) => (policy, count),
            None => return vec![],
        };

    debug!(
        "Detected asset burn: {} assets with policy {}",
        asset_count, primary_policy
    );

    vec![(
        TxType::Burn {
            policy_id: primary_policy,
            asset_count,
        },
        confidence,
    )]
}

/// Detect generic smart contract interactions
fn detect_smart_contract(context: &PatternContext) -> Vec<(TxType, f64)> {
    // Look for transactions with scripts but not matching other specific patterns
    if context.scripts.is_empty() {
        return vec![];
    }

    // If we have scripts and asset operations, this might be a smart contract interaction
    if context.asset_operations.is_empty() {
        return vec![];
    }

    // Check if this looks like a marketplace or other known pattern
    // If so, let those specific patterns handle it
    let has_marketplace_ops = context
        .asset_operations
        .iter()
        .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
        .any(|op| {
            let is_marketplace_input = op
                .input
                .as_ref()
                .is_some_and(|utxo| crate::registry::lookup_address(&utxo.address).is_some());
            let is_marketplace_output = op
                .output
                .as_ref()
                .is_some_and(|utxo| crate::registry::lookup_address(&utxo.address).is_some());
            is_marketplace_input || is_marketplace_output
        });

    if has_marketplace_ops {
        return vec![]; // Let marketplace patterns handle this
    }

    // Count the asset operations to determine complexity
    let operation_count = context.asset_operations.len();
    let script_count = context.scripts.len();

    debug!(
        "Detected generic smart contract: {} operations, {} scripts",
        operation_count, script_count
    );

    vec![(
        TxType::SmartContract {
            contract_address: "unknown".to_string(), // Could be enhanced to extract from scripts
            operation: format!("{operation_count} operations with {script_count} scripts"),
        },
        0.4, // Lower confidence since this is a catch-all
    )]
}

/// Detect asset staking/unstaking transactions
fn detect_asset_staking(context: &PatternContext) -> Vec<(TxType, f64)> {
    use crate::registry::{lookup_address, AddressCategory, ScriptCategory};

    // Look for interactions with known staking addresses
    let staking_ops: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine) && {
                let is_staking_input = op.input.as_ref().is_some_and(|utxo| {
                    matches!(
                        lookup_address(&utxo.address),
                        Some(AddressCategory::Script(ScriptCategory::Staking { .. }))
                    )
                });
                let is_staking_output = op.output.as_ref().is_some_and(|utxo| {
                    matches!(
                        lookup_address(&utxo.address),
                        Some(AddressCategory::Script(ScriptCategory::Staking { .. }))
                    )
                });
                is_staking_input || is_staking_output
            }
        })
        .collect();

    if staking_ops.is_empty() {
        return vec![];
    }

    // Determine if this is staking (lock) or unstaking (unlock)
    let stake_ops = staking_ops
        .iter()
        .filter(|op| op.op_type == AssetOpType::Lock)
        .count();
    let unstake_ops = staking_ops
        .iter()
        .filter(|op| op.op_type == AssetOpType::Unlock)
        .count();

    let direction = if stake_ops > unstake_ops {
        StakeDirection::Stake
    } else {
        StakeDirection::Unstake
    };

    // Collect the assets being staked/unstaked
    let assets: Vec<AssetId> = staking_ops
        .iter()
        .filter_map(|op| {
            op.asset_id()
                .and_then(|id| AssetId::parse_concatenated(&id).ok())
        })
        .collect();

    if assets.is_empty() {
        return vec![];
    }

    let staker_address = staking_ops
        .iter()
        .filter_map(|op| match direction {
            StakeDirection::Stake => op.input.as_ref().map(|utxo| &utxo.address),
            StakeDirection::Unstake => op.output.as_ref().map(|utxo| &utxo.address),
        })
        .next()
        .unwrap_or(&"unknown".to_string())
        .clone();

    // Extract staking contract address and label from registry
    let (staking_contract, staking_label) = staking_ops
        .iter()
        .filter_map(|op| {
            let staking_addr = match direction {
                StakeDirection::Stake => op.output.as_ref().map(|utxo| &utxo.address),
                StakeDirection::Unstake => op.input.as_ref().map(|utxo| &utxo.address),
            };
            staking_addr.and_then(|addr| {
                if let Some(AddressCategory::Script(ScriptCategory::Staking {
                    label,
                    project: _,
                })) = lookup_address(addr)
                {
                    Some((addr.clone(), label.to_string()))
                } else {
                    None
                }
            })
        })
        .next()
        .unwrap_or(("unknown".to_string(), "Generic".to_string()));

    debug!(
        "Detected asset staking: {:?} {} assets by {} in {}",
        direction,
        assets.len(),
        staker_address,
        staking_label
    );

    vec![(
        TxType::AssetStaking {
            assets,
            staker_address,
            direction,
            staking_contract,
            staking_label,
        },
        0.85, // High confidence for clear staking patterns
    )]
}

/// Detect asset vesting (lock/unlock) transactions
fn detect_vesting(context: &PatternContext) -> Vec<(TxType, f64)> {
    use crate::registry::{lookup_address, AddressCategory, ScriptCategory};

    // Look for interactions with known vesting addresses
    let vesting_ops: Vec<_> = context
        .asset_operations
        .iter()
        .filter(|op| {
            matches!(op.classification, crate::OperationClassification::Genuine) && {
                let is_vesting_input = op.input.as_ref().is_some_and(|utxo| {
                    matches!(
                        lookup_address(&utxo.address),
                        Some(AddressCategory::Script(ScriptCategory::Vesting { .. }))
                    )
                });
                let is_vesting_output = op.output.as_ref().is_some_and(|utxo| {
                    matches!(
                        lookup_address(&utxo.address),
                        Some(AddressCategory::Script(ScriptCategory::Vesting { .. }))
                    )
                });
                is_vesting_input || is_vesting_output
            }
        })
        .collect();

    if vesting_ops.is_empty() {
        return vec![];
    }

    // Determine if this is locking (Lock) or unlocking (Unlock)
    let lock_ops = vesting_ops
        .iter()
        .filter(|op| op.op_type == AssetOpType::Lock)
        .count();
    let unlock_ops = vesting_ops
        .iter()
        .filter(|op| op.op_type == AssetOpType::Unlock)
        .count();

    let direction = if lock_ops > unlock_ops {
        VestingDirection::Lock
    } else {
        VestingDirection::Unlock
    };

    // Collect the assets being vested/released
    let assets: Vec<AssetId> = vesting_ops
        .iter()
        .filter_map(|op| {
            op.asset_id()
                .and_then(|id| AssetId::parse_concatenated(&id).ok())
        })
        .collect();

    if assets.is_empty() {
        return vec![];
    }

    // Extract owner from datum payment key hash and correlate with TX addresses
    // Shield datum: Constructor 0 [ Int(unlock_ts), List[ Bytes(payment_key_hash) ] ]
    // TxUtxo only has address+idx, so look up the full TxOutput from raw_tx_data for the datum
    let datum_owner_key = vesting_ops.iter().find_map(|op| {
        let vesting_utxo = match direction {
            VestingDirection::Lock => op.output.as_ref(),
            VestingDirection::Unlock => op.input.as_ref(),
        };
        let utxo = vesting_utxo?;
        // For Lock: the vesting output is in this TX's outputs at idx
        // For Unlock: the vesting input was consumed, but outputs may also exist;
        // use raw_tx_data.outputs to find the datum by matching address
        let datum = if matches!(direction, VestingDirection::Lock) {
            context
                .raw_tx_data
                .outputs
                .get(utxo.idx as usize)
                .and_then(|out| out.datum.as_ref())
        } else {
            // For unlocks, find datum from any output at the vesting address
            context
                .raw_tx_data
                .outputs
                .iter()
                .find(|out| out.address == utxo.address)
                .and_then(|out| out.datum.as_ref())
        };
        datum
            .and_then(|d| d.json())
            .and_then(|j| j.get("fields"))
            .and_then(|f| f.as_array())
            .and_then(|a| a.get(1))
            .and_then(|f| f.get("list"))
            .and_then(|l| l.as_array())
            .and_then(|a| a.first())
            .and_then(|e| e.get("bytes"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    // Resolve the owner's full address (with stake key) from TX inputs/outputs
    // When datum has owner key hash, prefer non-vesting key-based (addr1q) input addresses
    // which carry both payment + staking credentials
    let owner_address = if datum_owner_key.is_some() {
        context
            .raw_tx_data
            .inputs
            .iter()
            .find(|input| {
                input.address.starts_with("addr1q")
                    && !matches!(
                        lookup_address(&input.address),
                        Some(AddressCategory::Script(ScriptCategory::Vesting { .. }))
                    )
            })
            .map(|input| input.address.clone())
    } else {
        None
    }
    .unwrap_or_else(|| {
        // Fallback: use the non-vesting side of the asset operations
        vesting_ops
            .iter()
            .filter_map(|op| match direction {
                VestingDirection::Lock => op.input.as_ref().map(|utxo| &utxo.address),
                VestingDirection::Unlock => op.output.as_ref().map(|utxo| &utxo.address),
            })
            .next()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    });

    // Extract vesting contract address and label from registry
    let (vesting_contract, vesting_label) = vesting_ops
        .iter()
        .filter_map(|op| {
            let vesting_addr = match direction {
                VestingDirection::Lock => op.output.as_ref().map(|utxo| &utxo.address),
                VestingDirection::Unlock => op.input.as_ref().map(|utxo| &utxo.address),
            };
            vesting_addr.and_then(|addr| {
                if let Some(AddressCategory::Script(ScriptCategory::Vesting { label })) =
                    lookup_address(addr)
                {
                    Some((addr.clone(), label.to_string()))
                } else {
                    None
                }
            })
        })
        .next()
        .unwrap_or(("unknown".to_string(), "Unknown".to_string()));

    // Resolve VestStyle from TX metadata (key 674)
    // "Shield Vest - Crowd Lock" → CrowdLock, "Shield Vest" → Shield
    let vest_style = context
        .metadata
        .as_ref()
        .and_then(|m| m.get("674"))
        .and_then(|m| m.get("msg"))
        .and_then(|msg| msg.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|v| {
                let s = v.as_str()?;
                if s.contains("Crowd Lock") {
                    Some(crate::VestStyle::CrowdLock)
                } else if s.contains("Shield") {
                    Some(crate::VestStyle::Shield)
                } else {
                    None
                }
            })
        })
        .unwrap_or(crate::VestStyle::Shield);

    debug!(
        "Detected asset vesting: {:?} {:?} {} assets by {} via {}",
        direction,
        vest_style,
        assets.len(),
        owner_address,
        vesting_label
    );

    vec![(
        TxType::AssetVesting {
            assets,
            owner_address,
            direction,
            vest_style,
            vesting_contract,
            vesting_label,
        },
        0.85,
    )]
}

/// Count assets by policy ID
fn count_assets_by_policy(ops: &[&AssetOperation]) -> HashMap<String, u32> {
    let mut policy_counts = HashMap::new();

    for op in ops {
        if let Some(asset) = op.payload.get_asset() {
            *policy_counts.entry(asset.policy_id.clone()).or_insert(0) += 1;
        }
    }

    policy_counts
}
