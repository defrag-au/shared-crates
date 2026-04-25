//! DEX (Decentralized Exchange) pattern detection
//!
//! This module detects DEX-related transactions using **structural analysis** of UTxO flows.
//! Instead of requiring every DEX address to be registered, it identifies swap patterns
//! by analyzing token/ADA flows between script-controlled UTxOs and user addresses.
//!
//! Handles two main DEX architectures:
//! - **AMM pools** (Splash, DexHunter, Minswap, CSWAP): pool address holds both ADA and tokens
//!   in inputs and outputs; balance diffs give exact swap amounts.
//! - **Order-book style** (SaturnSwap): pool holds only ADA, batcher outputs hold order details;
//!   batcher output amounts represent the swap order.
//!
//! The address registry is used only for **labelling** the DEX platform name.

use super::{PatternContext, PatternDetectionResult};
use crate::TxType;
use address_registry::AddressLookup;
use pipeline_types::OperationPayload;
use std::collections::HashMap;
use tracing::debug;
use transactions::RawTxData;

/// Detect DEX swap transactions using structural UTxO analysis.
pub fn detect_dex_swap(context: &PatternContext) -> PatternDetectionResult {
    let mut transactions = Vec::new();

    // Early exit: DEX execution txs involve script interactions (scripts or redeemers)
    let has_script_activity = !context.scripts.is_empty()
        || context
            .raw_tx_data
            .redeemers
            .as_ref()
            .map(|r| !r.is_null())
            .unwrap_or(false);
    if !has_script_activity {
        return PatternDetectionResult { transactions };
    }

    let registry = address_registry::SmartContractRegistry::new();
    let raw = context.raw_tx_data;

    // Must have datum-bearing addresses in both inputs and outputs
    let has_datum_inputs = raw.inputs.iter().any(|i| i.datum.is_some());
    let has_datum_outputs = raw.outputs.iter().any(|o| o.datum.is_some());
    if !has_datum_inputs || !has_datum_outputs {
        return PatternDetectionResult { transactions };
    }

    if let Some(swap) = detect_structural_swap(raw, &registry) {
        transactions.push((swap, 0.8));
    }

    PatternDetectionResult { transactions }
}

/// Structural swap detection using three address categories:
/// - **pool_addrs**: datum-bearing addresses in BOTH inputs AND outputs (persistent UTxOs)
/// - **batcher_addrs**: datum-bearing addresses ONLY in outputs (order/batcher UTxOs)
/// - **all_script_addrs**: union of both (for token flow analysis)
fn detect_structural_swap(raw: &RawTxData, registry: &dyn AddressLookup) -> Option<TxType> {
    // Collect datum-bearing addresses, split into input-side and output-side
    let mut datum_input_addrs: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut datum_output_addrs: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for input in &raw.inputs {
        if input.datum.is_some() && !registry.is_any_marketplace_address(&input.address) {
            datum_input_addrs.insert(&input.address);
        }
    }
    for output in &raw.outputs {
        if output.datum.is_some() && !registry.is_any_marketplace_address(&output.address) {
            datum_output_addrs.insert(&output.address);
        }
    }

    // Pool addresses: in both inputs AND outputs (persistent UTxOs)
    let pool_addrs: std::collections::HashSet<&str> = datum_input_addrs
        .intersection(&datum_output_addrs)
        .copied()
        .collect();

    // Batcher addresses: only in outputs (order UTxOs created by this tx)
    let batcher_addrs: std::collections::HashSet<&str> = datum_output_addrs
        .difference(&datum_input_addrs)
        .copied()
        .collect();

    // Order addresses: only in inputs (consumed order UTxOs from prior tx)
    let order_addrs: std::collections::HashSet<&str> = datum_input_addrs
        .difference(&datum_output_addrs)
        .copied()
        .collect();

    // All script addresses
    let all_script_addrs: std::collections::HashSet<&str> = datum_input_addrs
        .union(&datum_output_addrs)
        .copied()
        .collect();

    if pool_addrs.is_empty() && batcher_addrs.is_empty() && order_addrs.is_empty() {
        return None;
    }

    // Compute aggregate token flows across ALL script addresses
    let mut script_input_tokens: HashMap<&str, u64> = HashMap::new();
    for input in &raw.inputs {
        if all_script_addrs.contains(input.address.as_str()) {
            for (asset_key, amount) in &input.assets {
                *script_input_tokens.entry(asset_key.as_str()).or_default() += amount;
            }
        }
    }

    let mut script_output_tokens: HashMap<&str, u64> = HashMap::new();
    for output in &raw.outputs {
        if all_script_addrs.contains(output.address.as_str()) {
            for (asset_key, amount) in &output.assets {
                *script_output_tokens.entry(asset_key.as_str()).or_default() += amount;
            }
        }
    }

    // Tokens leaving scripts → user received
    let mut tokens_leaving: Vec<(&str, u64)> = Vec::new();
    for (asset_key, &in_amount) in &script_input_tokens {
        let out_amount = script_output_tokens.get(asset_key).copied().unwrap_or(0);
        if in_amount > out_amount {
            tokens_leaving.push((asset_key, in_amount - out_amount));
        }
    }

    // Tokens entering scripts → user sent
    // Start with aggregate net flows (tokens scripts gained)
    let mut tokens_entering: Vec<(&str, u64)> = Vec::new();
    for (asset_key, &out_amount) in &script_output_tokens {
        let in_amount = script_input_tokens.get(asset_key).copied().unwrap_or(0);
        if out_amount > in_amount {
            tokens_entering.push((asset_key, out_amount - in_amount));
        }
    }

    // In multi-hop routed swaps, order UTxO tokens get perfectly absorbed into pools,
    // making the aggregate net zero. Detect this by checking consumed order UTxOs:
    // if they carried tokens that net to zero in the aggregate, those are the user's input.
    if tokens_entering.is_empty() && !order_addrs.is_empty() {
        let mut order_tokens: HashMap<&str, u64> = HashMap::new();
        for input in &raw.inputs {
            if order_addrs.contains(input.address.as_str()) {
                for (asset_key, amount) in &input.assets {
                    *order_tokens.entry(asset_key.as_str()).or_default() += amount;
                }
            }
        }
        // Order tokens that were absorbed (net zero in aggregate) represent user input
        for (asset_key, amount) in order_tokens {
            let net_in = script_input_tokens.get(asset_key).copied().unwrap_or(0);
            let net_out = script_output_tokens.get(asset_key).copied().unwrap_or(0);
            if net_in <= net_out {
                // This token was absorbed into pools (not leaving scripts)
                tokens_entering.push((asset_key, amount));
            }
        }
    }

    // Pool ADA flow (persistent addresses only — excludes one-shot fee/batcher)
    let pool_input_ada: u64 = raw
        .inputs
        .iter()
        .filter(|i| pool_addrs.contains(i.address.as_str()))
        .map(|i| i.amount_lovelace)
        .sum();
    let pool_output_ada: u64 = raw
        .outputs
        .iter()
        .filter(|o| pool_addrs.contains(o.address.as_str()))
        .map(|o| o.amount_lovelace)
        .sum();

    let pool_ada_gained = pool_output_ada.saturating_sub(pool_input_ada);
    let pool_ada_lost = pool_input_ada.saturating_sub(pool_output_ada);

    debug!(
        "Script flows: {} tokens leaving, {} tokens entering. Pool ADA: +{pool_ada_gained} -{pool_ada_lost}. Pools: {} Batchers: {}",
        tokens_leaving.len(),
        tokens_entering.len(),
        pool_addrs.len(),
        batcher_addrs.len()
    );

    // === BUY: tokens leave scripts (user received tokens) ===
    if !tokens_leaving.is_empty() && tokens_entering.is_empty() {
        tokens_leaving.sort_by_key(|b| std::cmp::Reverse(b.1));
        let (asset_key, token_amount) = tokens_leaving[0];
        let (policy_id, encoded_name) = parse_asset_key(asset_key)?;

        let swapper = find_swapper_received_token(raw, &all_script_addrs, asset_key)?;
        let dex_platform = find_dex_label(raw, registry, &all_script_addrs);

        // ADA amount priority:
        // 1. Pool ADA gain (AMM pools where ADA goes directly to pool)
        // 2. Batcher output ADA (order-book DEXes where ADA goes to batcher)
        let ada_amount = if pool_ada_gained > 0 {
            pool_ada_gained
        } else {
            batcher_ada_amount(raw, &batcher_addrs)
        };

        debug!("DEX BUY: {ada_amount} lovelace → {token_amount} {policy_id}");

        return Some(TxType::DexSwap {
            dex_platform,
            asset_in: OperationPayload::Lovelace { amount: ada_amount },
            asset_out: OperationPayload::NativeToken {
                policy_id,
                encoded_name,
                amount: token_amount,
            },
            swapper,
        });
    }

    // === SELL: tokens enter scripts (user sent tokens), ADA leaves pool ===
    if !tokens_entering.is_empty() && tokens_leaving.is_empty() && pool_ada_lost > 0 {
        tokens_entering.sort_by_key(|b| std::cmp::Reverse(b.1));

        // For sells: use the largest batcher token deposit if available,
        // otherwise use the total aggregate
        let (asset_key, token_amount) = if !batcher_addrs.is_empty() {
            largest_batcher_token(raw, &batcher_addrs).unwrap_or(tokens_entering[0])
        } else {
            tokens_entering[0]
        };

        let (policy_id, encoded_name) = parse_asset_key(asset_key)?;

        // Use pool_ada_lost as a hint to pick the right user output — the swap
        // proceeds should be close to what left the pool. Without this, the largest
        // pure-ADA output gets selected, which can be a change UTxO when the user
        // had existing ADA balance.
        let (swapper, ada_amount) =
            find_swapper_received_ada(raw, &all_script_addrs, Some(pool_ada_lost))?;
        let dex_platform = find_dex_label(raw, registry, &all_script_addrs);

        debug!("DEX SELL: {token_amount} {policy_id} → {ada_amount} lovelace");

        return Some(TxType::DexSwap {
            dex_platform,
            asset_in: OperationPayload::NativeToken {
                policy_id,
                encoded_name,
                amount: token_amount,
            },
            asset_out: OperationPayload::Lovelace { amount: ada_amount },
            swapper,
        });
    }

    // === TOKEN-TO-TOKEN: tokens both enter and leave ===
    if !tokens_leaving.is_empty() && !tokens_entering.is_empty() {
        tokens_entering.sort_by_key(|b| std::cmp::Reverse(b.1));
        tokens_leaving.sort_by_key(|b| std::cmp::Reverse(b.1));

        let (in_key, in_amount) = tokens_entering[0];
        let (out_key, out_amount) = tokens_leaving[0];

        let (in_policy, in_name) = parse_asset_key(in_key)?;
        let (out_policy, out_name) = parse_asset_key(out_key)?;

        let swapper = find_swapper_received_token(raw, &all_script_addrs, out_key)?;
        let dex_platform = find_dex_label(raw, registry, &all_script_addrs);

        debug!("DEX TOKEN SWAP: {in_amount} {in_policy} → {out_amount} {out_policy}");

        return Some(TxType::DexSwap {
            dex_platform,
            asset_in: OperationPayload::NativeToken {
                policy_id: in_policy,
                encoded_name: in_name,
                amount: in_amount,
            },
            asset_out: OperationPayload::NativeToken {
                policy_id: out_policy,
                encoded_name: out_name,
                amount: out_amount,
            },
            swapper,
        });
    }

    debug!("No swap pattern detected");
    None
}

/// Find the largest ADA amount across batcher (output-only datum-bearing) outputs.
/// The largest output is the swap order; smaller ones are typically protocol fees.
fn batcher_ada_amount(raw: &RawTxData, batcher_addrs: &std::collections::HashSet<&str>) -> u64 {
    raw.outputs
        .iter()
        .filter(|o| batcher_addrs.contains(o.address.as_str()))
        .map(|o| o.amount_lovelace)
        .max()
        .unwrap_or(0)
}

/// Find the largest token deposit in batcher outputs (for sell detection).
fn largest_batcher_token<'a>(
    raw: &'a RawTxData,
    batcher_addrs: &std::collections::HashSet<&str>,
) -> Option<(&'a str, u64)> {
    let mut best: Option<(&str, u64)> = None;

    for output in &raw.outputs {
        if batcher_addrs.contains(output.address.as_str()) {
            for (asset_key, &amount) in &output.assets {
                if best.is_none() || amount > best.unwrap().1 {
                    best = Some((asset_key.as_str(), amount));
                }
            }
        }
    }

    best
}

/// Find the user address that received tokens from scripts.
fn find_swapper_received_token(
    raw: &RawTxData,
    script_addrs: &std::collections::HashSet<&str>,
    asset_key: &str,
) -> Option<String> {
    // Non-datum, non-script outputs with the token
    for output in &raw.outputs {
        if output.datum.is_none()
            && !script_addrs.contains(output.address.as_str())
            && output.assets.contains_key(asset_key)
        {
            return Some(output.address.clone());
        }
    }

    // Fallback: any non-script output with the token
    for output in &raw.outputs {
        if !script_addrs.contains(output.address.as_str()) && output.assets.contains_key(asset_key)
        {
            return Some(output.address.clone());
        }
    }

    debug!("Could not find swapper for token {asset_key}");
    None
}

/// Find the user address and ADA amount received from a sell swap.
///
/// When `pool_ada_hint` is provided (ADA that left the pool), picks the candidate
/// closest to that amount — this avoids selecting large change UTxOs that dwarf the
/// actual swap proceeds. Falls back to the largest candidate when no hint is given.
fn find_swapper_received_ada(
    raw: &RawTxData,
    script_addrs: &std::collections::HashSet<&str>,
    pool_ada_hint: Option<u64>,
) -> Option<(String, u64)> {
    // Non-script, non-datum, pure-ADA outputs
    let candidates: Vec<(&str, u64)> = raw
        .outputs
        .iter()
        .filter(|o| {
            o.datum.is_none() && !script_addrs.contains(o.address.as_str()) && o.assets.is_empty()
        })
        .map(|o| (o.address.as_str(), o.amount_lovelace))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    if let Some(hint) = pool_ada_hint {
        // Pick the candidate closest to pool_ada_lost — the swap proceeds should be
        // in the same ballpark as what left the pool (minus fees).
        candidates
            .iter()
            .min_by_key(|(_, amount)| (*amount as i128 - hint as i128).unsigned_abs())
            .map(|(addr, amount)| (addr.to_string(), *amount))
    } else {
        // No hint — fall back to largest
        candidates
            .iter()
            .max_by_key(|(_, amount)| *amount)
            .map(|(addr, amount)| (addr.to_string(), *amount))
    }
}

/// Find the best DEX platform label from registry.
fn find_dex_label(
    raw: &RawTxData,
    registry: &dyn AddressLookup,
    script_addrs: &std::collections::HashSet<&str>,
) -> String {
    // Prefer pool addresses (in both inputs and outputs)
    for addr in script_addrs {
        let in_inputs = raw.inputs.iter().any(|i| i.address == *addr);
        let in_outputs = raw.outputs.iter().any(|o| o.address == *addr);
        if in_inputs && in_outputs {
            if let Some(name) = get_dex_platform_name(registry, addr) {
                return name;
            }
        }
    }

    // Then any script address
    for addr in script_addrs {
        if let Some(name) = get_dex_platform_name(registry, addr) {
            return name;
        }
    }

    "Unknown DEX".to_string()
}

/// Parse a Maestro-format asset unit (concatenated policy_id + hex_asset_name).
fn parse_asset_key(asset_key: &str) -> Option<(String, String)> {
    if asset_key.len() >= 56 {
        let policy_id = &asset_key[..56];
        let encoded_name = if asset_key.len() > 56 {
            &asset_key[56..]
        } else {
            ""
        };
        Some((policy_id.to_string(), encoded_name.to_string()))
    } else {
        debug!("Could not parse asset key (too short): {asset_key}");
        None
    }
}

/// Get the DEX platform name from the registry.
fn get_dex_platform_name(registry: &dyn AddressLookup, address: &str) -> Option<String> {
    match registry.get_address_category(address) {
        Some(address_registry::AddressCategory::Script(
            address_registry::ScriptCategory::Exchange { label },
        )) => Some(label.to_string()),
        _ => None,
    }
}

/// Wrapper function for the pattern registry
pub fn detect_dex_swap_wrapper(context: &PatternContext) -> PatternDetectionResult {
    detect_dex_swap(context)
}

/// Detect DEX liquidity provision transactions
pub fn detect_dex_liquidity_add(_context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: Vec::new(),
    }
}

pub fn detect_dex_liquidity_add_wrapper(context: &PatternContext) -> PatternDetectionResult {
    detect_dex_liquidity_add(context)
}

/// Detect DEX liquidity removal transactions
pub fn detect_dex_liquidity_remove(_context: &PatternContext) -> PatternDetectionResult {
    PatternDetectionResult {
        transactions: Vec::new(),
    }
}

pub fn detect_dex_liquidity_remove_wrapper(context: &PatternContext) -> PatternDetectionResult {
    detect_dex_liquidity_remove(context)
}
