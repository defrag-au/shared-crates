//! Reactive wallet context for Leptos applications

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::rc::Rc;
use wallet_core::{ConnectionState, Network, WalletApi, WalletError, WalletInfo, WalletProvider};
use wallet_pallas::WalletBalance;
use wasm_bindgen_futures::spawn_local;

/// Reactive wallet context providing signals for wallet state
///
/// Use with `WalletProvider` component and `use_wallet()` hook.
#[derive(Clone)]
pub struct WalletContext {
    /// Current connection state
    pub connection_state: RwSignal<ConnectionState>,

    /// Available wallet extensions detected in browser
    pub available_wallets: RwSignal<Vec<WalletInfo>>,

    /// Current connected address (hex-encoded)
    pub address: RwSignal<Option<String>>,

    /// Current network
    pub network: RwSignal<Option<Network>>,

    /// Wallet balance (opt-in, call fetch_balance to populate)
    pub balance: RwSignal<Option<WalletBalance>>,

    /// Derived stake address (bech32)
    pub stake_address: Memo<Option<String>>,

    /// Loading state for async operations
    pub loading: RwSignal<bool>,

    /// Last error message
    pub error: RwSignal<Option<String>>,

    /// Internal: connected wallet API handle
    api: RwSignal<Option<SendWrapper<Rc<RefCell<WalletApi>>>>>,
}

impl WalletContext {
    /// Create a new wallet context with default state
    pub fn new() -> Self {
        let address: RwSignal<Option<String>> = RwSignal::new(None);

        // Derive stake address from payment address
        // Returns the stake address in bech32 format (stake1... or stake_test1...)
        let stake_address = Memo::new(move |_| {
            address.get().and_then(|addr| {
                wallet_pallas::Address::from_hex(&addr)
                    .ok()
                    .and_then(|a| a.stake_address_bech32())
            })
        });

        Self {
            connection_state: RwSignal::new(ConnectionState::Disconnected),
            available_wallets: RwSignal::new(vec![]),
            address,
            network: RwSignal::new(None),
            balance: RwSignal::new(None),
            stake_address,
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
            api: RwSignal::new(None),
        }
    }

    /// Connect to a wallet provider
    pub fn connect(&self, provider: WalletProvider) {
        let ctx = self.clone();
        ctx.connection_state.set(ConnectionState::Connecting);
        ctx.loading.set(true);
        ctx.error.set(None);

        spawn_local(async move {
            match WalletApi::connect(provider).await {
                Ok(api) => {
                    // Get network and address
                    let network_id = api.network_id().await.unwrap_or(1);
                    let network = match network_id {
                        1 => Network::Mainnet,
                        _ => Network::Preprod,
                    };

                    let address = api.change_address().await.ok();

                    // Store API handle
                    ctx.api
                        .set(Some(SendWrapper::new(Rc::new(RefCell::new(api)))));

                    // Update state
                    ctx.network.set(Some(network));
                    ctx.address.set(address.clone());
                    ctx.connection_state.set(ConnectionState::Connected {
                        provider,
                        address: address.unwrap_or_default(),
                        network,
                    });

                    // Save to localStorage for auto-reconnect
                    wallet_core::save_last_wallet(provider);
                }
                Err(e) => {
                    ctx.error.set(Some(e.to_string()));
                    ctx.connection_state
                        .set(ConnectionState::Error(e.to_string()));
                }
            }
            ctx.loading.set(false);
        });
    }

    /// Disconnect from current wallet
    pub fn disconnect(&self) {
        self.api.set(None);
        self.address.set(None);
        self.network.set(None);
        self.balance.set(None);
        self.connection_state.set(ConnectionState::Disconnected);
        wallet_core::clear_last_wallet();
    }

    /// Fetch balance from connected wallet
    ///
    /// This is opt-in - call this method to populate the `balance` signal.
    pub fn fetch_balance(&self) {
        let ctx = self.clone();

        spawn_local(async move {
            if let Some(api_wrapper) = ctx.api.get() {
                // Clone the api handle to avoid holding RefCell borrow across await
                let api = api_wrapper.borrow().clone();
                match api.balance().await {
                    Ok(balance_hex) => {
                        if let Ok(decoded) = wallet_pallas::decode_balance(&balance_hex) {
                            ctx.balance.set(Some(decoded));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch balance: {e}");
                    }
                }
            }
        });
    }

    /// Attempt auto-reconnect from localStorage
    pub fn try_reconnect(&self) {
        if let Some(provider) = wallet_core::load_last_wallet() {
            // Check if wallet is still available
            let available = wallet_core::detect_wallets();
            if available.contains(&provider) {
                self.connect(provider);
            }
        }
    }

    /// Refresh available wallets
    pub fn detect_wallets(&self) {
        let wallets = wallet_core::detect_wallets_with_info();
        self.available_wallets.set(wallets);
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(
            self.connection_state.get(),
            ConnectionState::Connected { .. }
        )
    }

    /// Get the current provider if connected
    pub fn current_provider(&self) -> Option<WalletProvider> {
        match self.connection_state.get() {
            ConnectionState::Connected { provider, .. } => Some(provider),
            _ => None,
        }
    }

    /// Sign arbitrary data using CIP-8
    ///
    /// Returns the signature and public key.
    pub async fn sign_data(
        &self,
        payload_hex: &str,
    ) -> Result<wallet_core::DataSignature, WalletError> {
        let api_wrapper = self
            .api
            .get()
            .ok_or_else(|| WalletError::NotEnabled("Not connected".into()))?;

        let address = self
            .address
            .get()
            .ok_or_else(|| WalletError::NotEnabled("No address".into()))?;

        // Clone to avoid holding RefCell borrow across await
        let api = api_wrapper.borrow().clone();
        api.sign_data(&address, payload_hex).await
    }

    /// Sign a transaction
    ///
    /// Returns the witness set hex.
    pub async fn sign_tx(&self, tx_hex: &str, partial_sign: bool) -> Result<String, WalletError> {
        let api_wrapper = self
            .api
            .get()
            .ok_or_else(|| WalletError::NotEnabled("Not connected".into()))?;

        // Clone to avoid holding RefCell borrow across await
        let api = api_wrapper.borrow().clone();
        api.sign_tx(tx_hex, partial_sign).await
    }

    /// Submit a signed transaction
    ///
    /// Returns the transaction hash.
    pub async fn submit_tx(&self, tx_hex: &str) -> Result<String, WalletError> {
        let api_wrapper = self
            .api
            .get()
            .ok_or_else(|| WalletError::NotEnabled("Not connected".into()))?;

        // Clone to avoid holding RefCell borrow across await
        let api = api_wrapper.borrow().clone();
        api.submit_tx(tx_hex).await
    }
}

impl Default for WalletContext {
    fn default() -> Self {
        Self::new()
    }
}
