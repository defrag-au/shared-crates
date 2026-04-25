use serde::Deserialize;
use transactions::{MintOperation, RawTxData, TxOutput};

/// Oura webhook transaction structure
#[derive(Debug, Deserialize)]
pub struct OuraWebhook {
    pub hash: String, // base64 encoded
    pub inputs: Vec<OuraInput>,
    pub outputs: Vec<OuraOutput>,
    pub mint: Option<Vec<OuraMint>>,
    pub fee: String,
    pub successful: bool,
    #[serde(default)]
    pub auxiliary: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OuraInput {
    #[serde(rename = "txHash")]
    pub tx_hash: String, // base64 encoded
    #[serde(rename = "outputIndex")]
    pub output_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct OuraOutput {
    pub address: String, // base64 encoded
    pub coin: String,    // lovelace as string
    pub assets: Option<Vec<OuraAssetGroup>>,
    #[serde(default)]
    pub datum: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OuraAssetGroup {
    #[serde(rename = "policyId")]
    pub policy_id: String, // base64 encoded
    pub assets: Vec<OuraAsset>,
}

#[derive(Debug, Deserialize)]
pub struct OuraAsset {
    pub name: String, // base64 encoded asset name
    #[serde(rename = "outputCoin")]
    pub output_coin: String, // amount as string
}

#[derive(Debug, Deserialize)]
pub struct OuraMint {
    #[serde(rename = "policyId")]
    pub policy_id: String, // base64 encoded
    pub assets: Vec<OuraMintAsset>,
}

#[derive(Debug, Deserialize)]
pub struct OuraMintAsset {
    pub name: String, // base64 encoded asset name
    #[serde(rename = "mintCoin")]
    pub mint_coin: String, // amount as string
}

/// Convert Oura webhook data to RawTxData format
pub fn convert_oura_webhook_to_raw_tx_data(webhook: &OuraWebhook) -> Result<RawTxData, String> {
    // Convert main transaction hash from base64 to hex
    let tx_hash = base64_to_hex(&webhook.hash)?;

    // Convert outputs
    let mut outputs = Vec::new();
    for output in webhook.outputs.iter() {
        // Convert address from base64 to hex (Cardano address format)
        let address = base64_to_cardano_address(&output.address)?;

        // Parse ADA amount
        let amount_lovelace: u64 = output
            .coin
            .parse()
            .map_err(|e| format!("Failed to parse coin amount '{}': {}", output.coin, e))?;

        // Build assets map
        let mut assets = std::collections::HashMap::new();
        assets.insert("lovelace".to_string(), amount_lovelace);

        if let Some(asset_groups) = &output.assets {
            for group in asset_groups {
                let policy_id = base64_to_hex(&group.policy_id)?;
                for asset in &group.assets {
                    let asset_name = base64_to_hex(&asset.name)?;
                    let asset_id = format!("{policy_id}{asset_name}");
                    let amount: u64 = asset.output_coin.parse().map_err(|e| {
                        format!(
                            "Failed to parse asset amount '{}': {}",
                            asset.output_coin, e
                        )
                    })?;
                    assets.insert(asset_id, amount);
                }
            }
        }

        outputs.push(TxOutput {
            address,
            amount_lovelace,
            assets,
            datum: None, // TODO: Parse datum if needed
            script_ref: None,
        });
    }

    // For Oura inputs, we only have references (txHash + outputIndex) but not the actual UTXO data
    // This is a limitation - we can't do full UTXO analysis without resolving these references
    // For now, create empty inputs to maintain structure compatibility
    let inputs = Vec::new(); // Empty because we don't have UTXO resolution

    let collateral_inputs = Vec::new();
    let collateral_outputs = Vec::new();

    // Parse mint operations from the mint section
    let mut mint_ops = Vec::new();
    if let Some(mints) = &webhook.mint {
        for mint_group in mints {
            let policy_id = base64_to_hex(&mint_group.policy_id)?;
            for mint_asset in &mint_group.assets {
                let asset_name = base64_to_hex(&mint_asset.name)?;
                let asset_id = format!("{policy_id}{asset_name}");
                let amount: i64 = mint_asset.mint_coin.parse().map_err(|e| {
                    format!(
                        "Failed to parse mint amount '{}': {}",
                        mint_asset.mint_coin, e
                    )
                })?;

                mint_ops.push(MintOperation {
                    unit: asset_id,
                    amount,
                });
            }
        }
    }

    // Parse fee
    let fee: u64 = webhook
        .fee
        .parse()
        .map_err(|e| format!("Failed to parse fee '{}': {}", webhook.fee, e))?;

    // Parse metadata from auxiliary if present
    let metadata = webhook
        .auxiliary
        .as_ref()
        .and_then(|aux| aux.get("metadata"))
        .cloned();

    let raw_tx_data = RawTxData {
        tx_hash,
        inputs,
        outputs,
        collateral_inputs,
        collateral_outputs,
        reference_inputs: Vec::new(), // Not available in webhook format
        mint: mint_ops,
        metadata,
        fee: Some(fee),
        block_height: None,
        timestamp: None,
        size: None,
        scripts: Vec::new(),
        redeemers: None,
    };

    Ok(raw_tx_data)
}

/// Convert base64 to hex string
fn base64_to_hex(base64_str: &str) -> Result<String, String> {
    use base64::prelude::*;

    let bytes = BASE64_STANDARD
        .decode(base64_str)
        .map_err(|e| format!("Failed to decode base64 '{base64_str}': {e}"))?;

    Ok(hex::encode(bytes))
}

/// Convert base64 encoded Cardano address to bech32 format
/// This is a simplified conversion - in practice, you'd need proper Cardano address decoding
fn base64_to_cardano_address(base64_addr: &str) -> Result<String, String> {
    use base64::prelude::*;

    let bytes = BASE64_STANDARD
        .decode(base64_addr)
        .map_err(|e| format!("Failed to decode address '{base64_addr}': {e}"))?;

    // For now, just return hex representation
    // TODO: Proper Cardano address decoding to bech32 would be needed for production
    Ok(hex::encode(bytes))
}

/// Helper function to create a mock Maestro format transaction from Oura data for testing
pub fn oura_to_maestro_format(
    webhook: &OuraWebhook,
) -> Result<maestro::CompleteTransactionDetails, String> {
    let tx_hash = base64_to_hex(&webhook.hash)?;

    // Convert outputs to Maestro format
    let mut maestro_outputs = Vec::new();
    for (idx, output) in webhook.outputs.iter().enumerate() {
        let address = base64_to_cardano_address(&output.address)?;
        let mut assets = Vec::new();

        // Add ADA
        assets.push(maestro::TransactionAsset {
            unit: "lovelace".to_string(),
            amount: output.coin.parse().unwrap_or(0),
        });

        // Add native assets
        if let Some(asset_groups) = &output.assets {
            for group in asset_groups {
                let policy_id = base64_to_hex(&group.policy_id)?;
                for asset in &group.assets {
                    let asset_name = base64_to_hex(&asset.name)?;
                    let asset_id = format!("{policy_id}{asset_name}");
                    assets.push(maestro::TransactionAsset {
                        unit: asset_id,
                        amount: asset.output_coin.parse().unwrap_or(0),
                    });
                }
            }
        }

        maestro_outputs.push(maestro::CompleteTransactionOutput {
            tx_hash: tx_hash.clone(),
            index: idx as u32,
            assets,
            address,
            datum: None,
            reference_script: None,
        });
    }

    // Create mock inputs (empty since we don't have UTXO data)
    let maestro_inputs = Vec::new();

    // Parse mint information as JSON Values
    let mut mint_assets = Vec::new();
    if let Some(mints) = &webhook.mint {
        for mint_group in mints {
            let policy_id = base64_to_hex(&mint_group.policy_id)?;
            for mint_asset in &mint_group.assets {
                let asset_name = base64_to_hex(&mint_asset.name)?;
                let asset_id = format!("{policy_id}{asset_name}");
                let amount: i64 = mint_asset.mint_coin.parse().unwrap_or(0);
                mint_assets.push(serde_json::json!({
                    "unit": asset_id,
                    "amount": amount
                }));
            }
        }
    }

    Ok(maestro::CompleteTransactionDetails {
        tx_hash,
        block_hash: "unknown".to_string(),
        block_tx_index: 0,
        block_height: 0,
        block_timestamp: 0,
        block_absolute_slot: 0,
        block_epoch: 0,
        inputs: maestro_inputs,
        outputs: maestro_outputs,
        reference_inputs: vec![],
        collateral_inputs: vec![],
        collateral_return: None,
        mint: mint_assets,
        invalid_before: None,
        invalid_hereafter: None,
        fee: webhook.fee.parse().unwrap_or(0),
        deposit: 0,
        certificates: serde_json::Value::Array(vec![]),
        withdrawals: vec![],
        additional_signers: vec![],
        scripts_executed: vec![],
        scripts_successful: webhook.successful,
        redeemers: serde_json::Value::Array(vec![]),
        metadata: webhook
            .auxiliary
            .as_ref()
            .and_then(|aux| aux.get("metadata"))
            .cloned(),
        size: 0,
    })
}
