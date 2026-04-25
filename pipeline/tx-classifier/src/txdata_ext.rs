//! Extension trait for RawTxData with classifier-specific methods

use crate::{is_script_address, AssetOperation};
use pipeline_types::parse_asset_id;
use transactions::{RawTxData, TxInput, TxOutput};

/// Extension trait for RawTxData with classifier-specific functionality
pub trait RawTxDataExt {
    /// Calculate ADA flows based only on genuine asset operations (filtered)
    fn calculate_filtered_ada_flows(
        &self,
        genuine_operations: &[AssetOperation],
    ) -> crate::AdaFlows;

    /// Get significant ADA inputs that could represent sale prices
    fn get_potential_sale_price_inputs(&self) -> Vec<&TxInput>;

    /// Get change outputs (outputs that return to input addresses without asset transfers)
    fn get_change_outputs(&self) -> Vec<&TxOutput>;

    /// Get actual transfer outputs (payments, sales, fees - not change)
    fn get_transfer_outputs(&self) -> Vec<&TxOutput>;

    /// Calculate net ADA spent (excluding change)
    fn calculate_net_ada_spent(&self) -> u64;

    /// Extract asset operations from transaction by analyzing complete UTXO flows
    fn extract_asset_operations(&self) -> Vec<AssetOperation>;

    /// Check if assets were transferred away from a specific address
    fn has_assets_transferred_from_address(&self, address: &str) -> bool;

    /// Extract native token operations from UTXO flows
    fn extract_native_token_operations(&self, operations: &mut Vec<AssetOperation>);

    /// Extract all ADA flows, classifying them based on UTXO context
    fn extract_all_ada_flows(&self, operations: &mut Vec<AssetOperation>);
}

impl RawTxDataExt for RawTxData {
    /// Calculate ADA flows based only on genuine asset operations (filtered)
    ///
    /// IMPORTANT: This function focuses on the "wheat" (genuine economic activity)
    /// and excludes the "chaff" (UTXO housekeeping operations like consolidation).
    ///
    /// Only creates flows between addresses involved in genuine asset operations,
    /// excluding all self-to-self flows which represent change/consolidation.
    fn calculate_filtered_ada_flows(
        &self,
        genuine_operations: &[crate::AssetOperation],
    ) -> crate::AdaFlows {
        use std::collections::HashSet;

        // Get addresses involved in genuine operations only
        let mut genuine_addresses = HashSet::new();
        for op in genuine_operations {
            if let Some(from_utxo) = &op.input {
                genuine_addresses.insert(from_utxo.address.clone());
            }
            if let Some(to_utxo) = &op.output {
                genuine_addresses.insert(to_utxo.address.clone());
            }
        }

        // Calculate flows only between addresses involved in genuine operations
        let mut flows = Vec::new();
        let mut largest_transfer = None;

        // For filtered flows, only create flows between DIFFERENT addresses involved in genuine operations
        // Exclude all self-to-self flows which represent change/consolidation, not payments

        // Find outputs that might represent genuine payments (to different addresses)
        for op in genuine_operations {
            if op.op_type != crate::AssetOpType::Transfer {
                continue;
            }

            let from_addr = match &op.input {
                Some(utxo) => &utxo.address,
                None => continue,
            };

            let to_addr = match &op.output {
                Some(utxo) => &utxo.address,
                None => continue,
            };

            // Skip if same address (no payment should occur)
            if from_addr == to_addr {
                continue;
            }

            // Look for ADA outputs to the receiver that might represent payment for the asset
            // But only if it's a reasonable payment amount (not just dust/fees)
            for output in &self.outputs {
                if output.address == *to_addr && output.amount_lovelace >= 5_000_000 {
                    // >= 5 ADA minimum
                    // Check if this could be a sale payment by looking for corresponding ADA input from buyer
                    let buyer_has_input = self.inputs.iter().any(|input| {
                        input.address == *to_addr && input.amount_lovelace >= output.amount_lovelace
                    });

                    // Only create flow if buyer provided ADA (suggesting payment)
                    // and asset moved to buyer
                    if buyer_has_input {
                        flows.push(crate::AdaFlow {
                            from_address: to_addr.clone(), // Buyer pays
                            to_address: from_addr.clone(), // Seller receives
                            amount: output.amount_lovelace,
                        });

                        if largest_transfer.is_none()
                            || output.amount_lovelace > largest_transfer.unwrap()
                        {
                            largest_transfer = Some(output.amount_lovelace);
                        }
                    }
                }
            }
        }

        // Calculate totals including all flows (genuine + housekeeping)
        let total_input: u64 = self.inputs.iter().map(|i| i.amount_lovelace).sum();
        let total_output: u64 = self.outputs.iter().map(|o| o.amount_lovelace).sum();
        let fee = self.fee.unwrap_or(0);
        let collateral_input: u64 = self
            .collateral_inputs
            .iter()
            .map(|i| i.amount_lovelace)
            .sum();
        let collateral_output: u64 = self
            .collateral_outputs
            .iter()
            .map(|o| o.amount_lovelace)
            .sum();

        crate::AdaFlows {
            total_input,
            total_output,
            fee,
            collateral_input,
            collateral_output,
            largest_transfer,
            flows,                        // Only genuine flows
            collateral_flows: Vec::new(), // Skip collateral for genuine operations
        }
    }

    /// Get significant ADA inputs that could represent sale prices
    /// Returns inputs filtered by amount (10-1000 ADA range typical for NFT sales)
    fn get_potential_sale_price_inputs(&self) -> Vec<&TxInput> {
        self.inputs
            .iter()
            .filter(|input| {
                // Look for inputs in typical NFT sale price range
                input.amount_lovelace >= 10_000_000 && // >= 10 ADA
                input.amount_lovelace <= 1_000_000_000 // <= 1000 ADA
            })
            .collect()
    }

    /// Check if assets were transferred away from a specific address
    fn has_assets_transferred_from_address(&self, address: &str) -> bool {
        // Get all assets that this address provided as input
        let mut input_assets: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for input in &self.inputs {
            if input.address == address {
                for (asset_id, quantity) in &input.assets {
                    *input_assets.entry(asset_id.clone()).or_insert(0) += quantity;
                }
            }
        }

        // Get all assets that this address received as output
        let mut output_assets: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for output in &self.outputs {
            if output.address == address {
                for (asset_id, quantity) in &output.assets {
                    *output_assets.entry(asset_id.clone()).or_insert(0) += quantity;
                }
            }
        }

        // Check if any assets that came from this address didn't return to it
        for (asset_id, input_quantity) in &input_assets {
            let output_quantity = output_assets.get(asset_id).copied().unwrap_or(0);
            if output_quantity < *input_quantity {
                // Asset was transferred away (sold/sent)
                return true;
            }
        }

        false
    }

    /// Get change outputs (outputs that return to input addresses without asset transfers)
    fn get_change_outputs(&self) -> Vec<&TxOutput> {
        let mut input_addresses: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut script_addresses: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for input in &self.inputs {
            input_addresses.insert(input.address.clone());

            // Check if this is a script address using bech32 address format analysis
            if is_script_address(&input.address) {
                script_addresses.insert(input.address.clone());
            }
        }

        self.outputs
            .iter()
            .filter(|output| {
                if !input_addresses.contains(&output.address)
                    || script_addresses.contains(&output.address)
                {
                    false // Different address or script address, not change
                } else {
                    // User address - only change if no assets transferred away
                    !self.has_assets_transferred_from_address(&output.address)
                }
            })
            .collect()
    }

    /// Get actual transfer outputs (payments, sales, fees - not change)
    fn get_transfer_outputs(&self) -> Vec<&TxOutput> {
        let mut input_addresses: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut script_addresses: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for input in &self.inputs {
            input_addresses.insert(input.address.clone());

            // Check if this is a script address using bech32 address format analysis
            if is_script_address(&input.address) {
                script_addresses.insert(input.address.clone());
            }
        }

        self.outputs
            .iter()
            .filter(|output| {
                if !input_addresses.contains(&output.address)
                    || script_addresses.contains(&output.address)
                {
                    true // Different address or script address, definitely a transfer
                } else {
                    // User address - transfer if assets moved away (sale)
                    self.has_assets_transferred_from_address(&output.address)
                }
            })
            .collect()
    }

    /// Calculate net ADA spent (excluding change)
    fn calculate_net_ada_spent(&self) -> u64 {
        let total_input: u64 = self.inputs.iter().map(|i| i.amount_lovelace).sum();
        let change_amount: u64 = self
            .get_change_outputs()
            .iter()
            .map(|o| o.amount_lovelace)
            .sum();
        let fee = self.fee.unwrap_or(0);

        // Net spent = Total input - Change - Fee
        total_input
            .saturating_sub(change_amount)
            .saturating_sub(fee)
    }

    /// Extract asset operations from transaction by analyzing complete UTXO flows
    /// Treats each UTXO as a complete package (ADA + native tokens) rather than isolating components
    fn extract_asset_operations(&self) -> Vec<AssetOperation> {
        let mut operations = Vec::new();

        // First, analyze native token flows (these drive the primary operations)
        self.extract_native_token_operations(&mut operations);

        // Then, analyze all ADA flows with proper context classification
        self.extract_all_ada_flows(&mut operations);

        operations
    }

    /// Extract native token operations from UTXO flows
    fn extract_native_token_operations(&self, operations: &mut Vec<AssetOperation>) {
        // Get all unique native tokens across all UTXOs
        let mut all_assets = std::collections::BTreeSet::new();

        for input in &self.inputs {
            for asset_id in input.assets.keys() {
                all_assets.insert(asset_id.clone());
            }
        }

        for output in &self.outputs {
            for asset_id in output.assets.keys() {
                all_assets.insert(asset_id.clone());
            }
        }

        // For each asset, create individual operations for each UTXO containing it
        for asset_id in all_assets {
            let (policy_id, asset_name) = parse_asset_id(&asset_id);

            // Find all inputs containing this asset
            let mut inputs_with_asset = Vec::new();
            for (idx, input) in self.inputs.iter().enumerate() {
                if let Some(&amount) = input.assets.get(&asset_id) {
                    inputs_with_asset.push((idx, input, amount));
                }
            }

            // Find all outputs containing this asset
            let mut outputs_with_asset = Vec::new();
            for (idx, output) in self.outputs.iter().enumerate() {
                if let Some(&amount) = output.assets.get(&asset_id) {
                    outputs_with_asset.push((idx, output, amount));
                }
            }

            match (inputs_with_asset.is_empty(), outputs_with_asset.is_empty()) {
                (true, false) => {
                    // Mint: token appears only in outputs
                    for (output_idx, output, amount) in outputs_with_asset {
                        let output_datum = output.datum.clone();

                        operations.push(AssetOperation {
                            payload: crate::OperationPayload::NativeToken {
                                policy_id: policy_id.clone(),
                                encoded_name: asset_name.clone(),
                                amount,
                            },
                            op_type: crate::AssetOpType::Mint,
                            input: None,
                            output: Some(crate::TxUtxo {
                                address: output.address.clone(),
                                idx: output_idx as u32,
                            }),
                            input_datum: None,
                            output_datum,
                            classification: crate::OperationClassification::Genuine,
                        });
                    }
                }
                (false, true) => {
                    // Burn: token appears only in inputs
                    for (input_idx, input, amount) in inputs_with_asset {
                        let input_datum = input.datum.clone();

                        operations.push(AssetOperation {
                            payload: crate::OperationPayload::NativeToken {
                                policy_id: policy_id.clone(),
                                encoded_name: asset_name.clone(),
                                amount,
                            },
                            op_type: crate::AssetOpType::Burn,
                            input: Some(crate::TxUtxo {
                                address: input.address.clone(),
                                idx: input_idx as u32,
                            }),
                            output: None,
                            input_datum,
                            output_datum: None,
                            classification: crate::OperationClassification::Genuine,
                        });
                    }
                }
                (false, false) => {
                    // Transfer: token appears in both inputs and outputs
                    // Create separate operations for each output (representing individual transfers)
                    for (output_idx, output, output_amount) in outputs_with_asset {
                        // For DEX transactions and complex flows, we create one operation per output
                        // The input represents the source pool/address, output represents destination
                        let (input_idx, input, _input_amount) = if inputs_with_asset.len() == 1 {
                            // Simple case: one input, possibly multiple outputs
                            inputs_with_asset[0]
                        } else {
                            // Complex case: multiple inputs - use the first one as source
                            // In practice, this could be refined with more sophisticated matching
                            inputs_with_asset[0]
                        };

                        // Determine operation type based on addresses
                        let from_is_script = is_script_address(&input.address);
                        let to_is_script = is_script_address(&output.address);

                        let operation = match (from_is_script, to_is_script) {
                            (false, true) => crate::AssetOpType::Lock,
                            (true, false) => crate::AssetOpType::Unlock,
                            _ => crate::AssetOpType::Transfer,
                        };

                        let input_datum = input.datum.clone();
                        let output_datum = output.datum.clone();

                        operations.push(AssetOperation {
                            payload: crate::OperationPayload::NativeToken {
                                policy_id: policy_id.clone(),
                                encoded_name: asset_name.clone(),
                                amount: output_amount, // Use the actual output amount, not aggregated
                            },
                            op_type: operation,
                            input: Some(crate::TxUtxo {
                                address: input.address.clone(),
                                idx: input_idx as u32,
                            }),
                            output: Some(crate::TxUtxo {
                                address: output.address.clone(),
                                idx: output_idx as u32,
                            }),
                            input_datum,
                            output_datum,
                            classification: crate::OperationClassification::Genuine,
                        });
                    }
                }
                (true, true) => {
                    // Should never happen - asset exists in neither inputs nor outputs
                }
            }
        }
    }

    /// Extract all ADA flows, classifying them based on UTXO context
    /// ADA accompanying native tokens = likely protocol requirements
    /// ADA in standalone UTXOs = likely economic transfers
    fn extract_all_ada_flows(&self, operations: &mut Vec<AssetOperation>) {
        // Get addresses that appear in both inputs and outputs (potential change addresses)
        let input_addresses: std::collections::HashSet<String> =
            self.inputs.iter().map(|i| i.address.clone()).collect();

        // Get script addresses from inputs to detect unlocking scenarios
        let script_input_addresses: std::collections::HashSet<String> = self
            .inputs
            .iter()
            .filter(|input| is_script_address(&input.address))
            .map(|input| input.address.clone())
            .collect();

        // Track which inputs have been used to avoid double-matching
        let mut used_inputs = std::collections::HashSet::new();

        // Analyze each output to see if it represents an ADA flow
        for (output_idx, output) in self.outputs.iter().enumerate() {
            if output.amount_lovelace > 0 {
                // Skip change outputs UNLESS they come from script addresses (which could be unlocking/cancellation)
                if input_addresses.contains(&output.address) && script_input_addresses.is_empty() {
                    // This is likely change to the same user address with no script unlocking
                    continue;
                }

                // Find the source of this ADA from inputs
                // Prioritize script address sources for unlocking detection
                let source_input = self.inputs.iter().enumerate().find(|(input_idx, input)| {
                    // Skip already used inputs UNLESS the output is going to a script address
                    // (allow one-to-many for offer creation, minting, etc.)
                    if used_inputs.contains(input_idx) && !is_script_address(&output.address) {
                        return false;
                    }

                    // For script address inputs, always create operations (offer cancellations, unlocking, etc.)
                    if is_script_address(&input.address) && input.amount_lovelace > 0 {
                        return true;
                    }
                    // For regular addresses, require different output address and sufficient ADA
                    input.address != output.address
                        && input.amount_lovelace >= output.amount_lovelace
                });

                if let Some((input_idx, input)) = source_input {
                    // Mark this input as used ONLY if output is not going to a script address
                    // (allow one-to-many for script address outputs)
                    if !is_script_address(&output.address) {
                        used_inputs.insert(input_idx);
                    }

                    // Determine operation type based on addresses
                    let op_type = if is_script_address(&input.address)
                        && !is_script_address(&output.address)
                    {
                        crate::AssetOpType::Unlock
                    } else if !is_script_address(&input.address)
                        && is_script_address(&output.address)
                    {
                        crate::AssetOpType::Lock
                    } else {
                        crate::AssetOpType::Transfer
                    };

                    // Get input and output datums if available
                    let input_datum = input.datum.clone();
                    let output_datum = output.datum.clone();

                    operations.push(AssetOperation {
                        payload: crate::OperationPayload::Lovelace {
                            amount: output.amount_lovelace,
                        },
                        op_type,
                        input: Some(crate::TxUtxo {
                            address: input.address.clone(),
                            idx: input_idx as u32,
                        }),
                        output: Some(crate::TxUtxo {
                            address: output.address.clone(),
                            idx: output_idx as u32,
                        }),
                        input_datum,
                        output_datum,
                        classification: crate::OperationClassification::Genuine,
                    });
                }
            }
        }
    }
}
