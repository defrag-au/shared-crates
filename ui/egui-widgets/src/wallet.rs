//! Framework-agnostic wallet connector for egui frontends.
//!
//! Wraps `wallet-core` CIP-30 bindings and `wallet-pallas` address/balance
//! utilities into a single state struct. Consumers drive async operations
//! through their own message channels — this module never spawns tasks.

pub use wallet_core::{on_window_focus, ConnectionState, WalletApi, WalletInfo, WalletProvider};
pub use wallet_pallas::{decode_balance, Address, WalletBalance};

/// Result of a successful wallet connection (produced by async connect task).
pub struct WalletConnectResult {
    pub provider: WalletProvider,
    pub address_hex: String,
    pub stake_address: Option<String>,
    pub network_id: u8,
}

/// Framework-agnostic wallet state.
///
/// Holds connection state, derived addresses, and balance data.
/// All async operations (connect, fetch balance) must be driven
/// externally via the consuming app's message channel.
pub struct WalletConnector {
    pub available_wallets: Vec<WalletInfo>,
    pub connection_state: ConnectionState,
    pub address_hex: Option<String>,
    pub stake_address: Option<String>,
    pub handle: Option<String>,
    pub balance: Option<WalletBalance>,
    /// CIP-30 API handle — stored after connect for later use (sign_tx, utxos, etc.)
    pub api: Option<WalletApi>,
    /// Wallet icon data URL (base64 from CIP-30), set on connect for display.
    pub connected_icon: Option<String>,
}

impl WalletConnector {
    /// Create a new connector, detecting available wallets.
    pub fn new() -> Self {
        let available_wallets = wallet_core::detect_wallets_with_info();
        for w in &available_wallets {
            let icon_info = match &w.icon {
                Some(url) => format!("len={}, prefix={}", url.len(), &url[..url.len().min(80)]),
                None => "None".to_string(),
            };
            log::info!(
                "[wallet] detected: {} ({}), icon: {icon_info}",
                w.name,
                w.api_name
            );
        }
        Self {
            available_wallets,
            connection_state: ConnectionState::Disconnected,
            address_hex: None,
            stake_address: None,
            handle: None,
            balance: None,
            api: None,
            connected_icon: None,
        }
    }

    /// Re-detect available wallet extensions.
    pub fn detect(&mut self) {
        self.available_wallets = wallet_core::detect_wallets_with_info();
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection_state, ConnectionState::Connected { .. })
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.connection_state, ConnectionState::Connecting)
    }

    pub fn provider_name(&self) -> Option<&str> {
        match &self.connection_state {
            ConnectionState::Connected { provider, .. } => Some(provider.display_name()),
            _ => None,
        }
    }

    /// Load the last connected wallet from localStorage (for auto-reconnect).
    pub fn last_wallet() -> Option<WalletProvider> {
        wallet_core::load_last_wallet()
    }

    /// Apply a successful connection result. Call after the async connect task completes.
    pub fn apply_connect_result(&mut self, result: WalletConnectResult) {
        let network = match result.network_id {
            1 => wallet_core::Network::Mainnet,
            _ => wallet_core::Network::Preprod,
        };
        // Look up the wallet icon from the available wallets list
        self.connected_icon = self
            .available_wallets
            .iter()
            .find(|w| WalletProvider::from_api_name(&w.api_name) == Some(result.provider))
            .and_then(|w| w.icon.clone());
        self.connection_state = ConnectionState::Connected {
            provider: result.provider,
            address: result.address_hex.clone(),
            network,
        };
        self.address_hex = Some(result.address_hex);
        self.stake_address = result.stake_address;
        wallet_core::save_last_wallet(result.provider);
    }

    /// Apply a fetched balance. Derives $handle automatically.
    pub fn apply_balance(&mut self, balance: WalletBalance) {
        self.handle = balance.ada_handle();
        self.balance = Some(balance);
    }

    /// Whether the CIP-30 API handle is available for signing/UTxO queries.
    pub fn has_api(&self) -> bool {
        self.api.is_some()
    }

    /// Disconnect and clear all state.
    pub fn disconnect(&mut self) {
        self.connection_state = ConnectionState::Disconnected;
        self.address_hex = None;
        self.stake_address = None;
        self.handle = None;
        self.balance = None;
        self.api = None;
        self.connected_icon = None;
        wallet_core::clear_last_wallet();
    }

    /// Set connecting state (call before spawning the async connect task).
    pub fn set_connecting(&mut self) {
        self.connection_state = ConnectionState::Connecting;
    }

    /// Set error state.
    pub fn set_error(&mut self, error: String) {
        self.connection_state = ConnectionState::Error(error);
    }
}

impl Default for WalletConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// Async helper: connect to a wallet and derive addresses.
///
/// Call from `wasm_bindgen_futures::spawn_local` and send the result
/// through your app's message channel.
pub async fn connect_wallet(
    provider: WalletProvider,
) -> Result<(WalletConnectResult, WalletApi), String> {
    let api = WalletApi::connect(provider)
        .await
        .map_err(|e| format!("Connection failed: {e}"))?;

    let network_id = api
        .network_id()
        .await
        .map_err(|e| format!("Failed to get network: {e}"))?;

    let address_hex = api
        .change_address()
        .await
        .map_err(|e| format!("Failed to get address: {e}"))?;

    let stake_address = Address::from_hex(&address_hex)
        .ok()
        .and_then(|addr| addr.stake_address_bech32());

    let result = WalletConnectResult {
        provider,
        address_hex,
        stake_address,
        network_id,
    };

    Ok((result, api))
}

/// Async helper: fetch and decode wallet balance.
///
/// Call from `wasm_bindgen_futures::spawn_local` and send the result
/// through your app's message channel.
pub async fn fetch_wallet_balance(api: &WalletApi) -> Result<WalletBalance, String> {
    let balance_hex = api
        .balance()
        .await
        .map_err(|e| format!("Failed to get balance: {e}"))?;

    decode_balance(&balance_hex).map_err(|e| format!("Failed to decode balance: {e}"))
}

/// Async helper: fetch and decode wallet UTxOs.
///
/// Call from `wasm_bindgen_futures::spawn_local` and send the result
/// through your app's message channel.
pub async fn fetch_wallet_utxos(
    api: &WalletApi,
) -> Result<Vec<cardano_assets::utxo::UtxoApi>, String> {
    let cbor_hexes = api
        .utxos()
        .await
        .map_err(|e| format!("Failed to get UTxOs: {e}"))?;

    wallet_pallas::decode_utxos(&cbor_hexes).map_err(|e| format!("Failed to decode UTxOs: {e}"))
}
