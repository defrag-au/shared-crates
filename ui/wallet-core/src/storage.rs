//! Browser localStorage persistence for wallet state

use crate::types::WalletProvider;

const STORAGE_KEY: &str = "shared_ui_wallet";

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

fn get_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
