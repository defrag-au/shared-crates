use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use transactions::{MintOperation, RawTxData, TxDatum, TxInput, TxOutput};

/// Blockfrost webhook payload structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostWebhook {
    pub id: String,
    pub webhook_id: String,
    pub created: u64,
    pub api_version: u32,
    #[serde(rename = "type")]
    pub webhook_type: String,
    pub payload: Vec<BlockfrostTransactionPayload>,
}

/// Single transaction payload from Blockfrost webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostTransactionPayload {
    pub tx: BlockfrostTransaction,
    pub inputs: Vec<BlockfrostInput>,
    pub outputs: Vec<BlockfrostOutput>,
}

/// Blockfrost transaction details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostTransaction {
    pub hash: String,
    pub block: String,
    pub block_height: u64,
    pub block_time: u64,
    pub slot: u64,
    pub index: u32,
    pub output_amount: Vec<BlockfrostAmount>,
    pub fees: String,
    pub deposit: String,
    pub size: u32,
    pub invalid_before: Option<String>,
    pub invalid_hereafter: Option<String>,
    pub utxo_count: u32,
    pub withdrawal_count: u32,
    pub mir_cert_count: u32,
    pub delegation_count: u32,
    pub stake_cert_count: u32,
    pub pool_update_count: u32,
    pub pool_retire_count: u32,
    pub asset_mint_or_burn_count: u32,
    pub redeemer_count: u32,
    pub valid_contract: bool,
}

/// Blockfrost input UTXO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostInput {
    pub address: String,
    pub amount: Vec<BlockfrostAmount>,
    pub tx_hash: String,
    pub output_index: u32,
    pub data_hash: Option<String>,
    pub inline_datum: Option<String>,
    pub reference_script_hash: Option<String>,
    pub collateral: bool,
    pub reference: bool,
}

/// Blockfrost output UTXO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostOutput {
    pub address: String,
    pub amount: Vec<BlockfrostAmount>,
    pub output_index: u32,
    pub data_hash: Option<String>,
    pub inline_datum: Option<String>,
    pub reference_script_hash: Option<String>,
    pub collateral: bool,
    pub consumed_by_tx: Option<String>,
}

/// Blockfrost amount (lovelace or native token)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockfrostAmount {
    pub unit: String,
    pub quantity: String,
}

/// Conversion error types
#[derive(Debug)]
pub enum BlockfrostConversionError {
    NoTransactionPayload,
    ParseError(String),
    InvalidAmount(String),
    DatumDecodeError(String),
}

impl std::fmt::Display for BlockfrostConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTransactionPayload => write!(f, "No transaction payload in webhook"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
            Self::InvalidAmount(amount) => write!(f, "Invalid amount: {amount}"),
            Self::DatumDecodeError(msg) => write!(f, "Datum decode error: {msg}"),
        }
    }
}

impl std::error::Error for BlockfrostConversionError {}

impl From<std::num::ParseIntError> for BlockfrostConversionError {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::ParseError(e.to_string())
    }
}

/// Convert Blockfrost webhook to RawTxData
pub fn convert_blockfrost_webhook_to_raw_tx_data(
    webhook: &BlockfrostWebhook,
) -> Result<RawTxData, BlockfrostConversionError> {
    let tx_payload = webhook
        .payload
        .first()
        .ok_or(BlockfrostConversionError::NoTransactionPayload)?;

    // Convert inputs
    let inputs = tx_payload
        .inputs
        .iter()
        .filter(|input| !input.collateral)
        .map(convert_blockfrost_utxo_to_input)
        .collect::<Result<Vec<_>, _>>()?;

    // Convert outputs
    let outputs = tx_payload
        .outputs
        .iter()
        .map(convert_blockfrost_utxo_to_output)
        .collect::<Result<Vec<_>, _>>()?;

    // Convert collateral inputs
    let collateral_inputs = tx_payload
        .inputs
        .iter()
        .filter(|input| input.collateral)
        .map(convert_blockfrost_utxo_to_input)
        .collect::<Result<Vec<_>, _>>()?;

    // Extract mint operations from output_amount
    let mint = extract_mint_operations_from_output_amount(&tx_payload.tx.output_amount)?;

    let raw_tx_data = RawTxData {
        tx_hash: tx_payload.tx.hash.clone(),
        inputs,
        outputs,
        collateral_inputs,
        collateral_outputs: Vec::new(), // Blockfrost doesn't separate collateral outputs in this format
        reference_inputs: Vec::new(),   // Not available in webhook format
        mint,
        metadata: None, // Would need additional Blockfrost API call
        fee: Some(tx_payload.tx.fees.parse()?),
        block_height: Some(tx_payload.tx.block_height),
        timestamp: Some(tx_payload.tx.block_time),
        size: Some(tx_payload.tx.size),
        scripts: Vec::new(), // Would need to extract from reference_script_hash
        redeemers: None,     // Not available in webhook format
    };

    Ok(raw_tx_data)
}

/// Convert Blockfrost input to TxInput
fn convert_blockfrost_utxo_to_input(
    input: &BlockfrostInput,
) -> Result<TxInput, BlockfrostConversionError> {
    let (amount_lovelace, assets) = convert_blockfrost_amounts(&input.amount)?;
    let datum = convert_blockfrost_input_datum(input)?;

    Ok(TxInput {
        address: input.address.clone(),
        tx_hash: input.tx_hash.clone(),
        output_index: input.output_index,
        amount_lovelace,
        assets,
        datum,
    })
}

/// Convert Blockfrost output to TxOutput
fn convert_blockfrost_utxo_to_output(
    output: &BlockfrostOutput,
) -> Result<TxOutput, BlockfrostConversionError> {
    let (amount_lovelace, assets) = convert_blockfrost_amounts(&output.amount)?;
    let datum = convert_blockfrost_output_datum(output)?;

    Ok(TxOutput {
        address: output.address.clone(),
        amount_lovelace,
        assets,
        datum,
        script_ref: output.reference_script_hash.clone(),
    })
}

/// Convert Blockfrost amounts to lovelace + assets map
fn convert_blockfrost_amounts(
    amounts: &[BlockfrostAmount],
) -> Result<(u64, HashMap<String, u64>), BlockfrostConversionError> {
    let mut lovelace = 0u64;
    let mut assets = HashMap::new();

    for amount in amounts {
        let quantity = amount.quantity.parse::<u64>().map_err(|_| {
            BlockfrostConversionError::InvalidAmount(format!("{}:{}", amount.unit, amount.quantity))
        })?;

        if amount.unit == "lovelace" {
            lovelace = quantity;
        } else {
            assets.insert(amount.unit.clone(), quantity);
        }
    }

    Ok((lovelace, assets))
}

/// Convert Blockfrost input datum information to TxDatum
fn convert_blockfrost_input_datum(
    input: &BlockfrostInput,
) -> Result<Option<TxDatum>, BlockfrostConversionError> {
    convert_blockfrost_datum_common(&input.data_hash, &input.inline_datum)
}

/// Convert Blockfrost output datum information to TxDatum
fn convert_blockfrost_output_datum(
    output: &BlockfrostOutput,
) -> Result<Option<TxDatum>, BlockfrostConversionError> {
    convert_blockfrost_datum_common(&output.data_hash, &output.inline_datum)
}

/// Common datum conversion logic
fn convert_blockfrost_datum_common(
    data_hash: &Option<String>,
    inline_datum: &Option<String>,
) -> Result<Option<TxDatum>, BlockfrostConversionError> {
    match (data_hash, inline_datum) {
        (Some(hash), None) => {
            // Hash datum
            Ok(Some(TxDatum::Hash { hash: hash.clone() }))
        }
        (_, Some(inline_datum)) => {
            // Inline datum
            let hash = compute_datum_hash(inline_datum)?;
            let decoded_json = decode_cbor_to_json(inline_datum).ok();

            Ok(Some(match decoded_json {
                Some(json) => TxDatum::Json {
                    hash,
                    json,
                    bytes: Some(inline_datum.clone()),
                },
                None => TxDatum::Bytes {
                    hash,
                    bytes: inline_datum.clone(),
                },
            }))
        }
        (None, None) => Ok(None),
    }
}

/// Extract mint operations from Blockfrost output_amount
/// Note: This is a simplified approach - ideally we'd get mint data directly from Blockfrost
fn extract_mint_operations_from_output_amount(
    _output_amounts: &[BlockfrostAmount],
) -> Result<Vec<MintOperation>, BlockfrostConversionError> {
    // This is a placeholder - in reality, mint/burn data should come from a separate field
    // Blockfrost webhooks might not include mint data directly in output_amount
    Ok(Vec::new())
}

/// Compute SHA256 hash of CBOR datum (simplified)
fn compute_datum_hash(cbor_hex: &str) -> Result<String, BlockfrostConversionError> {
    use sha2::{Digest, Sha256};

    let bytes = hex::decode(cbor_hex)
        .map_err(|e| BlockfrostConversionError::DatumDecodeError(format!("Invalid hex: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();

    Ok(hex::encode(hash))
}

/// Decode CBOR hex to JSON (simplified)
fn decode_cbor_to_json(cbor_hex: &str) -> Result<serde_json::Value, BlockfrostConversionError> {
    use pallas_codec::minicbor::{Decode, Decoder};

    let bytes = hex::decode(cbor_hex)
        .map_err(|e| BlockfrostConversionError::DatumDecodeError(format!("Invalid hex: {e}")))?;

    let mut decoder = Decoder::new(&bytes);
    let plutus_data = pallas_primitives::alonzo::PlutusData::decode(&mut decoder, &mut ())
        .map_err(|e| {
            BlockfrostConversionError::DatumDecodeError(format!("CBOR decode error: {e}"))
        })?;

    // Convert PlutusData to serde_json::Value
    convert_cbor_to_json_value(&plutus_data)
}

/// Convert PlutusData to serde_json::Value
fn convert_cbor_to_json_value(
    plutus_data: &pallas_primitives::alonzo::PlutusData,
) -> Result<serde_json::Value, BlockfrostConversionError> {
    match plutus_data {
        pallas_primitives::alonzo::PlutusData::BigInt(big_int) => {
            match big_int {
                pallas_primitives::alonzo::BigInt::Int(int) => {
                    let val: i128 = (*int).into();
                    if val >= i64::MIN as i128 && val <= i64::MAX as i128 {
                        Ok(serde_json::Value::Number(serde_json::Number::from(
                            val as i64,
                        )))
                    } else {
                        Ok(serde_json::Value::String(val.to_string()))
                    }
                }
                pallas_primitives::alonzo::BigInt::BigUInt(bytes) => {
                    // Convert big uint bytes to string representation
                    let hex_str = hex::encode(bytes.as_slice());
                    Ok(serde_json::Value::String(format!("0x{hex_str}")))
                }
                pallas_primitives::alonzo::BigInt::BigNInt(bytes) => {
                    // Convert big negative int bytes to string representation
                    let hex_str = hex::encode(bytes.as_slice());
                    Ok(serde_json::Value::String(format!("-0x{hex_str}")))
                }
            }
        }
        pallas_primitives::alonzo::PlutusData::BoundedBytes(bytes) => {
            Ok(serde_json::Value::String(hex::encode(bytes.as_slice())))
        }
        pallas_primitives::alonzo::PlutusData::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.iter().map(convert_cbor_to_json_value).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        pallas_primitives::alonzo::PlutusData::Map(map) => {
            let mut json_obj = serde_json::Map::new();
            for (k, v) in map.iter() {
                let key = match k {
                    pallas_primitives::alonzo::PlutusData::BigInt(big_int) => match big_int {
                        pallas_primitives::alonzo::BigInt::Int(int) => {
                            let val: i128 = (*int).into();
                            val.to_string()
                        }
                        _ => format!("{big_int:?}"),
                    },
                    pallas_primitives::alonzo::PlutusData::BoundedBytes(bytes) => {
                        hex::encode(bytes.as_slice())
                    }
                    _ => format!("{k:?}"),
                };
                json_obj.insert(key, convert_cbor_to_json_value(v)?);
            }
            Ok(serde_json::Value::Object(json_obj))
        }
        pallas_primitives::alonzo::PlutusData::Constr(constr) => {
            // For Plutus constructors, include the constructor tag and fields
            let mut obj = serde_json::Map::new();
            obj.insert(
                "constructor".to_string(),
                serde_json::Value::Number(serde_json::Number::from(constr.tag)),
            );
            let fields: Result<Vec<_>, _> = constr
                .fields
                .iter()
                .map(convert_cbor_to_json_value)
                .collect();
            obj.insert("fields".to_string(), serde_json::Value::Array(fields?));
            Ok(serde_json::Value::Object(obj))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockfrost_amounts_conversion() {
        let amounts = vec![
            BlockfrostAmount {
                unit: "lovelace".to_string(),
                quantity: "1000000".to_string(),
            },
            BlockfrostAmount {
                unit: "3966cf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b49154686520416e636573746f72202331313736".to_string(),
                quantity: "1".to_string(),
            },
        ];

        let (lovelace, assets) = convert_blockfrost_amounts(&amounts).unwrap();

        assert_eq!(lovelace, 1_000_000);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets.get("3966cf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b49154686520416e636573746f72202331313736"), Some(&1));
    }

    #[test]
    fn test_datum_hash_computation() {
        // Test with a simple CBOR hex string (even number of characters)
        let cbor_hex = "d8799f581c801fa7e81f256b3e97be0602c91531abf235d47d4a103f5999bd6d4eff";
        let hash = compute_datum_hash(cbor_hex).unwrap();

        // Hash should be 64 characters (32 bytes in hex)
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
