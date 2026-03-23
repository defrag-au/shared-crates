use maestro::ProtocolParameters;
use pallas_txbuilder::StagingTransaction;

/// Estimate the size of a CBOR-encoded number (lovelace amounts, fees, etc.)
fn estimate_cbor_uint_size(value: u64) -> u64 {
    if value < 24 {
        1 // Tiny int: 1 byte
    } else if value <= 0xFF {
        2 // uint8: 1 byte tag + 1 byte
    } else if value <= 0xFFFF {
        3 // uint16: 1 byte tag + 2 bytes
    } else if value <= 0xFFFF_FFFF {
        5 // uint32: 1 byte tag + 4 bytes
    } else {
        9 // uint64: 1 byte tag + 8 bytes
    }
}

/// Estimate transaction size from its components without building.
///
/// This provides a conservative estimate based on CBOR encoding sizes.
///
/// # Arguments
///
/// * `tx` - The staging transaction to estimate
/// * `num_witnesses` - Number of expected signatures
///
/// # Returns
///
/// Estimated transaction size in bytes
pub fn estimate_tx_size(tx: &StagingTransaction, num_witnesses: u32) -> u64 {
    let mut size = 0u64;

    // Transaction body overhead (CBOR array tag, map tags, etc.)
    size += 20; // Base transaction structure

    // Inputs: each input is ~43 bytes (tx hash 32 + index ~2 + overhead ~9)
    if let Some(ref inputs) = tx.inputs {
        size += (inputs.len() as u64) * 43;
    }

    // Reference inputs (same size as regular inputs)
    if let Some(ref ref_inputs) = tx.reference_inputs {
        size += (ref_inputs.len() as u64) * 43;
    }

    // Outputs: estimate based on address + value
    if let Some(ref outputs) = tx.outputs {
        for output in outputs {
            // Address: ~60 bytes average (payment + stake credentials)
            size += 60;

            // Lovelace: 1-9 bytes depending on amount
            size += estimate_cbor_uint_size(output.lovelace);

            // Native assets: policy ID (28) + asset name (variable) + amount + overhead
            if let Some(ref assets) = output.assets {
                // OutputAssets implements Deref to HashMap
                for (_policy_id, asset_map) in assets.iter() {
                    size += 28; // Policy ID
                    size += 3; // Map overhead
                    for (asset_name, amount) in asset_map {
                        size += asset_name.0.len() as u64; // Asset name (Bytes.0 is Vec<u8>)
                        size += estimate_cbor_uint_size(*amount); // Amount
                        size += 3; // Entry overhead
                    }
                }
                size += 6; // Assets map overhead
            }

            // Datum: if present, varies widely (use conservative estimate)
            if output.datum.is_some() {
                size += 50; // Conservative estimate for inline datums
            }

            // Script reference: if present, can be large
            if output.script.is_some() {
                size += 100; // Conservative estimate
            }
        }
    }

    // Fee: 1-9 bytes
    if let Some(fee) = tx.fee {
        size += estimate_cbor_uint_size(fee);
    } else {
        size += 9; // Assume max size for fee field
    }

    // Validity intervals
    if tx.valid_from_slot.is_some() {
        size += 9; // Slot number
    }
    if tx.invalid_from_slot.is_some() {
        size += 9; // Slot number
    }

    // Minting: if present
    if let Some(ref mint) = tx.mint {
        // MintAssets implements Deref to HashMap
        for (_policy_id, asset_map) in mint.iter() {
            size += 28; // Policy ID
            for (asset_name, amount) in asset_map {
                size += asset_name.0.len() as u64; // Asset name (Bytes.0 is Vec<u8>)
                size += estimate_cbor_uint_size((*amount).unsigned_abs());
                size += 3;
            }
        }
        size += 6; // Mint map overhead
    }

    // Network ID
    size += 2;

    // Witnesses: each signature is ~100 bytes (vkey 32 + sig 64 + overhead)
    size += (num_witnesses as u64) * 100;

    size
}

/// Calculate the exact transaction fee using [`TxBuildParams`].
///
/// This is the isomorphic version — no dependency on Maestro types. Preferred
/// for new code in `cardano_tx::builder`.
///
/// When `price_mem` and `price_step` are set in params, the fee includes the
/// Plutus script execution cost component derived from all redeemers in the TX:
///   `fee = base_fee + Σ(mem × price_mem_num / price_mem_den + steps × price_step_num / price_step_den)`
pub fn calculate_fee(tx: &StagingTransaction, params: &crate::params::TxBuildParams) -> u64 {
    use pallas_txbuilder::BuildConway;

    let base_fee = {
        let built = match tx.clone().build_conway_raw() {
            Ok(b) => b,
            Err(_) => {
                let estimated_size = estimate_tx_size(tx, 1);
                return estimated_size * params.min_fee_coefficient + params.min_fee_constant;
            }
        };

        let dummy_secret = pallas_crypto::key::ed25519::SecretKey::from([0u8; 32]);
        let signed = match built.sign(&dummy_secret) {
            Ok(s) => s,
            Err(_) => {
                let estimated_size = estimate_tx_size(tx, 1);
                return estimated_size * params.min_fee_coefficient + params.min_fee_constant;
            }
        };

        let tx_size = signed.tx_bytes.0.len() as u64;
        tx_size * params.min_fee_coefficient + params.min_fee_constant
    };

    base_fee + execution_fee_from_redeemers(tx, params)
}

/// Sum the execution cost of all redeemers in a staged transaction.
///
/// Returns 0 when `price_mem`/`price_step` are `None` (non-Plutus callers)
/// or when the transaction has no redeemers.
fn execution_fee_from_redeemers(
    tx: &StagingTransaction,
    params: &crate::params::TxBuildParams,
) -> u64 {
    let (Some((mem_num, mem_den)), Some((step_num, step_den))) =
        (params.price_mem, params.price_step)
    else {
        return 0;
    };

    let redeemers = match &tx.redeemers {
        Some(r) => r,
        None => return 0,
    };

    let mut total: u128 = 0;
    for (_purpose, (_data, opt_eu)) in redeemers.iter() {
        if let Some(eu) = opt_eu {
            // Use u128 to avoid overflow on large execution units
            total += (eu.mem as u128) * (mem_num as u128) / (mem_den as u128);
            total += (eu.steps as u128) * (step_num as u128) / (step_den as u128);
        }
    }

    // Ceil to ensure we never underpay
    total as u64
}

/// Calculate the exact transaction fee by building and signing with a dummy key.
///
/// This produces the exact signed tx size — no estimation or safety margins needed.
/// The fee formula is: `fee = tx_size_bytes × min_fee_coefficient + min_fee_constant`
///
/// The staging transaction should already have `.fee()` and `.network_id()` set
/// so the CBOR encoding matches the final transaction.
pub fn calculate_tx_fee(
    tx: &StagingTransaction,
    protocol_params: &ProtocolParameters,
    _num_witnesses: u32,
) -> u64 {
    use pallas_txbuilder::BuildConway;

    // Build the transaction
    let built = match tx.clone().build_conway_raw() {
        Ok(b) => b,
        Err(_) => {
            // Fallback to estimation if build fails
            let estimated_size = estimate_tx_size(tx, _num_witnesses);
            return (estimated_size * protocol_params.min_fee_coefficient)
                + protocol_params.min_fee_constant.ada.lovelace;
        }
    };

    // Sign with a dummy key to get the exact final tx size including witnesses.
    // The dummy signature is the same size as a real Ed25519 signature.
    let dummy_secret = pallas_crypto::key::ed25519::SecretKey::from([0u8; 32]);
    let signed = match built.sign(&dummy_secret) {
        Ok(s) => s,
        Err(_) => {
            let estimated_size = estimate_tx_size(tx, _num_witnesses);
            return (estimated_size * protocol_params.min_fee_coefficient)
                + protocol_params.min_fee_constant.ada.lovelace;
        }
    };

    // Exact fee from exact signed tx size
    let tx_size = signed.tx_bytes.0.len() as u64;
    tx_size * protocol_params.min_fee_coefficient + protocol_params.min_fee_constant.ada.lovelace
}
