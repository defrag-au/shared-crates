//! CIP-30 wallet API bindings
//!
//! Provides JavaScript bindings for the CIP-30 wallet connector standard.

use wasm_bindgen::prelude::*;

use crate::types::{DataSignature, WalletInfo, WalletProvider};
use crate::WalletError;

// JavaScript bindings for wallet detection and connection
#[wasm_bindgen(inline_js = r#"
export function detectWallets() {
    const wallets = [];
    if (typeof window !== 'undefined' && window.cardano) {
        const knownWallets = ['nami', 'eternl', 'lace', 'flint', 'typhon', 'vespr', 'nufi', 'gerowallet', 'yoroi'];
        for (const name of knownWallets) {
            if (window.cardano[name]) {
                wallets.push(name);
            }
        }
    }
    return wallets;
}

export function getWalletInfo(name) {
    if (typeof window === 'undefined' || !window.cardano || !window.cardano[name]) {
        return null;
    }
    const wallet = window.cardano[name];
    return {
        apiName: name,
        name: wallet.name || name,
        icon: wallet.icon || null
    };
}

export function detectWalletsWithInfo() {
    const wallets = [];
    if (typeof window !== 'undefined' && window.cardano) {
        const knownWallets = ['nami', 'eternl', 'lace', 'flint', 'typhon', 'vespr', 'nufi', 'gerowallet', 'yoroi'];
        for (const apiName of knownWallets) {
            const wallet = window.cardano[apiName];
            if (wallet) {
                wallets.push({
                    apiName: apiName,
                    name: wallet.name || apiName,
                    icon: wallet.icon || null
                });
            }
        }
    }
    return wallets;
}

export async function enableWallet(name) {
    if (typeof window === 'undefined' || !window.cardano || !window.cardano[name]) {
        throw new Error(`Wallet ${name} not found`);
    }
    return await window.cardano[name].enable();
}

export async function getNetworkId(api) {
    return await api.getNetworkId();
}

export async function getUsedAddresses(api) {
    return await api.getUsedAddresses();
}

export async function getChangeAddress(api) {
    return await api.getChangeAddress();
}

export async function getBalance(api) {
    return await api.getBalance();
}

export async function getUtxos(api, amount, paginate) {
    // amount and paginate are optional
    return await api.getUtxos(amount, paginate);
}

export async function signTx(api, txHex, partialSign) {
    return await api.signTx(txHex, partialSign);
}

export async function signData(api, address, payload) {
    // CIP-8 signData returns { signature: string, key: string }
    const result = await api.signData(address, payload);
    return result;
}

export function extractSignature(dataSignature) {
    return dataSignature.signature || null;
}

export function extractKey(dataSignature) {
    return dataSignature.key || null;
}

export async function submitTx(api, txHex) {
    return await api.submitTx(txHex);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = detectWallets)]
    pub fn detect_wallets_js() -> Vec<String>;

    #[wasm_bindgen(js_name = detectWalletsWithInfo)]
    pub fn detect_wallets_with_info_js() -> JsValue;

    #[wasm_bindgen(js_name = getWalletInfo)]
    pub fn get_wallet_info_js(name: &str) -> JsValue;

    #[wasm_bindgen(js_name = enableWallet, catch)]
    pub async fn enable_wallet_js(name: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = getNetworkId, catch)]
    pub async fn get_network_id_js(api: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = getUsedAddresses, catch)]
    pub async fn get_used_addresses_js(api: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = getChangeAddress, catch)]
    pub async fn get_change_address_js(api: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = getBalance, catch)]
    pub async fn get_balance_js(api: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = getUtxos, catch)]
    pub async fn get_utxos_js(
        api: &JsValue,
        amount: &JsValue,
        paginate: &JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = signTx, catch)]
    pub async fn sign_tx_js(
        api: &JsValue,
        tx_hex: &str,
        partial_sign: bool,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = signData, catch)]
    pub async fn sign_data_js(
        api: &JsValue,
        address: &str,
        payload: &str,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = extractSignature)]
    pub fn extract_signature_js(data_signature: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = extractKey)]
    pub fn extract_key_js(data_signature: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = submitTx, catch)]
    pub async fn submit_tx_js(api: &JsValue, tx_hex: &str) -> Result<JsValue, JsValue>;
}

/// Detect available wallet extensions
pub fn detect_wallets() -> Vec<WalletProvider> {
    detect_wallets_js()
        .into_iter()
        .filter_map(|name| match name.as_str() {
            "nami" => Some(WalletProvider::Nami),
            "eternl" => Some(WalletProvider::Eternl),
            "lace" => Some(WalletProvider::Lace),
            "flint" => Some(WalletProvider::Flint),
            "typhon" => Some(WalletProvider::Typhon),
            "vespr" => Some(WalletProvider::Vespr),
            "nufi" => Some(WalletProvider::NuFi),
            "gerowallet" => Some(WalletProvider::Gero),
            "yoroi" => Some(WalletProvider::Yoroi),
            _ => None,
        })
        .collect()
}

/// Detect available wallet extensions with full info (name, icon)
pub fn detect_wallets_with_info() -> Vec<WalletInfo> {
    let js_value = detect_wallets_with_info_js();
    serde_wasm_bindgen::from_value(js_value).unwrap_or_default()
}

/// Get info for a specific wallet
pub fn get_wallet_info(provider: WalletProvider) -> Option<WalletInfo> {
    let js_value = get_wallet_info_js(provider.api_name());
    serde_wasm_bindgen::from_value(js_value).ok()
}

/// Connected wallet API handle
#[derive(Clone)]
pub struct WalletApi {
    provider: WalletProvider,
    api: JsValue,
}

impl WalletApi {
    /// Enable a wallet and get the API handle
    pub async fn connect(provider: WalletProvider) -> Result<Self, WalletError> {
        let api = enable_wallet_js(provider.api_name()).await?;
        Ok(Self { provider, api })
    }

    /// Get the wallet provider
    pub fn provider(&self) -> WalletProvider {
        self.provider
    }

    /// Get the network ID (0 = testnet, 1 = mainnet)
    pub async fn network_id(&self) -> Result<u8, WalletError> {
        let result = get_network_id_js(&self.api).await?;
        result
            .as_f64()
            .map(|n| n as u8)
            .ok_or_else(|| WalletError::ApiError("Invalid network ID".into()))
    }

    /// Get used addresses (hex-encoded)
    pub async fn used_addresses(&self) -> Result<Vec<String>, WalletError> {
        let result = get_used_addresses_js(&self.api).await?;
        let array = js_sys::Array::from(&result);
        Ok(array.iter().filter_map(|v| v.as_string()).collect())
    }

    /// Get change address (hex-encoded)
    pub async fn change_address(&self) -> Result<String, WalletError> {
        let result = get_change_address_js(&self.api).await?;
        result
            .as_string()
            .ok_or_else(|| WalletError::ApiError("Invalid change address".into()))
    }

    /// Get wallet balance (CBOR-encoded hex string)
    ///
    /// Returns the total balance as CBOR-encoded Value.
    /// Use `wallet_pallas::decode_value` to parse the result.
    pub async fn balance(&self) -> Result<String, WalletError> {
        let result = get_balance_js(&self.api).await?;
        result
            .as_string()
            .ok_or_else(|| WalletError::ApiError("Invalid balance".into()))
    }

    /// Get UTxOs from the wallet (CBOR-encoded hex strings)
    ///
    /// Returns a list of CBOR-encoded TransactionUnspentOutput values.
    /// Use `wallet_pallas::decode_utxo` to parse each result.
    pub async fn utxos(&self) -> Result<Vec<String>, WalletError> {
        let result = get_utxos_js(&self.api, &JsValue::UNDEFINED, &JsValue::UNDEFINED).await?;

        // Result can be null if wallet has no UTxOs
        if result.is_null() || result.is_undefined() {
            return Ok(vec![]);
        }

        let array = js_sys::Array::from(&result);
        Ok(array.iter().filter_map(|v| v.as_string()).collect())
    }

    /// Sign a transaction (returns witness set hex)
    pub async fn sign_tx(&self, tx_hex: &str, partial_sign: bool) -> Result<String, WalletError> {
        let result = sign_tx_js(&self.api, tx_hex, partial_sign).await?;
        result
            .as_string()
            .ok_or_else(|| WalletError::SigningFailed("Invalid signature response".into()))
    }

    /// Sign arbitrary data (CIP-8)
    /// Returns a DataSignature containing the COSE signature and public key
    pub async fn sign_data(
        &self,
        address: &str,
        payload: &str,
    ) -> Result<DataSignature, WalletError> {
        let result = sign_data_js(&self.api, address, payload).await?;

        // CIP-8 signData returns { signature: string, key: string }
        let signature = extract_signature_js(&result)
            .as_string()
            .ok_or_else(|| WalletError::SigningFailed("Missing signature in response".into()))?;

        let key = extract_key_js(&result)
            .as_string()
            .ok_or_else(|| WalletError::SigningFailed("Missing key in response".into()))?;

        Ok(DataSignature { signature, key })
    }

    /// Submit a signed transaction
    pub async fn submit_tx(&self, tx_hex: &str) -> Result<String, WalletError> {
        let result = submit_tx_js(&self.api, tx_hex).await?;
        result
            .as_string()
            .ok_or_else(|| WalletError::SubmitFailed("Invalid submit response".into()))
    }
}
