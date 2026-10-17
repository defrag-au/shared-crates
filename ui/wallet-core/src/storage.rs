//! Browser localStorage persistence for wallet state

use crate::evm::EvmWalletInfo;
use crate::types::WalletProvider;

const STORAGE_KEY: &str = "shared_ui_wallet";
const EVM_STORAGE_KEY: &str = "shared_ui_evm_wallet";

/// Save the last connected wallet provider
pub fn save_last_wallet(provider: WalletProvider) {
    if let Some(storage) = get_storage() {
        let _ = storage.set_item(STORAGE_KEY, provider.api_name());
    }
}

/// Load the last connected wallet provider
///
/// Goes through [`WalletProvider::from_api_name`] rather than repeating the
/// name→variant map: this used to be a second hand-written copy, which meant a
/// newly supported wallet could be saved and never loaded again — an
/// auto-reconnect that silently does nothing.
pub fn load_last_wallet() -> Option<WalletProvider> {
    let storage = get_storage()?;
    let name = storage.get_item(STORAGE_KEY).ok()??;

    WalletProvider::from_api_name(&name)
}

/// Clear the saved wallet
pub fn clear_last_wallet() {
    if let Some(storage) = get_storage() {
        let _ = storage.remove_item(STORAGE_KEY);
    }
}

/// Save the last connected EVM wallet
///
/// The whole announcement, not just its id: a restore needs a name and an icon
/// to offer, and the id alone would have to be looked up in a list the wallet
/// may have left.
pub fn save_last_evm_wallet(wallet: &EvmWalletInfo) {
    if let Some(storage) = get_storage()
        && let Ok(json) = serde_json::to_string(wallet)
    {
        let _ = storage.set_item(EVM_STORAGE_KEY, &json);
    }
}

/// Load the last connected EVM wallet, if one was saved
///
/// Absent and unreadable both mean "nothing to restore": a payload that no
/// longer parses is a schema change, and declining to restore is the safe end of
/// that. The wallet still has to be present in the browser before it can be
/// connected — this only answers which one to try.
pub fn load_last_evm_wallet() -> Option<EvmWalletInfo> {
    let storage = get_storage()?;
    let json = storage.get_item(EVM_STORAGE_KEY).ok()??;
    serde_json::from_str(&json).ok()
}

/// Clear the saved EVM wallet
pub fn clear_last_evm_wallet() {
    if let Some(storage) = get_storage() {
        let _ = storage.remove_item(EVM_STORAGE_KEY);
    }
}

fn get_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
