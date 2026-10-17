//! Framework-agnostic wallet connector for egui frontends.
//!
//! Wraps `wallet-core` CIP-30 bindings and `wallet-pallas` address/balance
//! utilities into a single state struct. Consumers drive async operations
//! through their own message channels — this module never spawns tasks.

pub use wallet_core::{
    CONNECTABLE, CardanoConnection, ChainConnection, ChainFamily, ConnectedIdentity,
    ConnectionPhase, EvmConnection, EvmWalletId, EvmWalletInfo, Network, WalletApi, WalletChoice,
    WalletConnections, WalletInfo, WalletProvider, collect_announced_wallets, on_window_focus,
};
pub use wallet_pallas::{Address, WalletBalance, decode_balance};

/// Result of a successful wallet connection (produced by async connect task).
pub struct WalletConnectResult {
    pub provider: WalletProvider,
    pub address_hex: String,
    pub stake_address: Option<String>,
    pub network_id: u8,
}

/// A connected Cardano wallet travelling as one value: the connect result
/// and the CIP-30 handle it signs with.
///
/// The connector is told about both at once —
/// [`WalletConnector::apply_connect_result`] — because a result with no handle
/// is not a connection. This is therefore the payload an app's message channel
/// carries back from its connect task; the handle is a JS value, wrapped so
/// the pair can cross a channel that asks for `Send`.
pub type CardanoSession = (WalletConnectResult, send_wrapper::SendWrapper<WalletApi>);

/// The bech32 stake address a CIP-30 reward address (hex, `e1…`) names, when it
/// parses. `None` for anything that is not a stake address — a payment address
/// carries a stake credential but is not one, and `stake_address_bech32`
/// refuses it.
///
/// The conversion every caller that compares a wallet's reward addresses
/// against a bech32 claim needs: CIP-30 speaks hex throughout, and encoding is
/// not identity.
pub fn stake_address_bech32(address_hex: &str) -> Option<String> {
    Address::from_hex(address_hex)
        .ok()
        .and_then(|addr| addr.stake_address_bech32())
}

/// Framework-agnostic wallet state.
///
/// The connected side is [`WalletConnections`]: a set of active connections,
/// one per chain, keyed by [`ChainFamily`]. A reader can have Cardano and EVM
/// connected at the same time, and every read here takes the chain it is about
/// — `is_connected(ChainFamily::Cardano)` — so a surface cannot accidentally
/// treat one chain's answer as another's.
///
/// All async operations (connect, fetch balance) must be driven externally via
/// the consuming app's message channel.
pub struct WalletConnector {
    /// The Cardano wallets detected on `window.cardano` — the CIP-30 half of
    /// what the connect flow can offer.
    pub available_wallets: Vec<WalletInfo>,
    /// The EVM wallets this browser announced — the other half.
    ///
    /// Emptied nowhere and filled by [`Self::set_evm_wallets`], because EIP-6963
    /// discovery is a *request* followed by events, and nothing in this crate
    /// spawns tasks or waits for them. Until a host pushes announcements in, a
    /// surface looks exactly as it did before there was a second chain.
    pub available_evm_wallets: Vec<EvmWalletInfo>,
    /// The set of active connections — one per chain, keyed by family.
    pub connections: WalletConnections,
    /// The last balance read for the Cardano connection, and the `$handle` it
    /// derives.
    ///
    /// A *reading about* a connection, not part of one: it is fetched after
    /// connecting and changes as the chain does. Only Cardano has one — the
    /// estate reads EVM ownership from chain state, never from the wallet — so
    /// this stays a named field rather than a second map, and it is cleared
    /// with the Cardano connection.
    pub balance: Option<WalletBalance>,
    pub handle: Option<String>,
}

impl WalletConnector {
    /// Create a new connector, detecting what the browser offers.
    pub fn new() -> Self {
        let mut connector = Self {
            available_wallets: Vec::new(),
            available_evm_wallets: Vec::new(),
            connections: WalletConnections::new(),
            balance: None,
            handle: None,
        };
        connector.detect();
        connector
    }

    /// Hand in the EVM wallets the browser announced.
    ///
    /// [`Self::detect`] already collects EIP-6963 announcements, so this is the
    /// escape hatch rather than the way in: for a host that keeps its own
    /// announcement store (a wallet that announces after the first tick, an app
    /// that collects once and shares the result between surfaces). An app that
    /// never calls it still offers every wallet that announced at startup.
    pub fn set_evm_wallets(&mut self, wallets: Vec<EvmWalletInfo>) {
        self.available_evm_wallets = wallets;
    }

    /// Re-detect what the browser offers.
    ///
    /// Both halves come from here — the CIP-30 namespaces on `window.cardano`,
    /// and the wallets that answer EIP-6963 on the EVM side — because a surface
    /// that detects one chain and not the other is a surface whose EVM half
    /// looks like a browser with nothing installed.
    pub fn detect(&mut self) {
        self.available_wallets = wallet_core::detect_wallets_with_info();
        for w in &self.available_wallets {
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
        self.available_evm_wallets = wallet_core::collect_announced_wallets();
        for w in &self.available_evm_wallets {
            log::info!("[wallet] announced: {} ({})", w.name, w.id);
        }
    }

    // ========================================================================
    // Reads — every one of them names the chain it is about
    // ========================================================================

    /// Whether `family` has an active connection.
    pub fn is_connected(&self, family: ChainFamily) -> bool {
        self.connections.is_connected(family)
    }

    /// Whether `family` is mid-connect.
    pub fn is_connecting(&self, family: ChainFamily) -> bool {
        self.connections.is_connecting(family)
    }

    /// Whether any chain at all is connected — the question a header asks
    /// before it decides to draw a connect button.
    pub fn any_connected(&self) -> bool {
        self.connections.any_connected()
    }

    /// The connection on `family`, when that chain has one.
    pub fn connection(&self, family: ChainFamily) -> Option<&ChainConnection> {
        self.connections.connection(family)
    }

    /// The Cardano connection, narrowed. The accessor most call sites want;
    /// signing paths match on [`ChainConnection`] instead.
    pub fn cardano(&self) -> Option<&CardanoConnection> {
        self.connections
            .connection(ChainFamily::Cardano)
            .and_then(ChainConnection::cardano)
    }

    /// The EVM connection, narrowed.
    pub fn evm(&self) -> Option<&EvmConnection> {
        self.connections
            .connection(ChainFamily::Evm)
            .and_then(ChainConnection::evm)
    }

    /// One line per connected chain, in `ChainFamily::ALL` order — what a
    /// cluster shows.
    pub fn identities(&self) -> impl Iterator<Item = ConnectedIdentity<'_>> {
        self.connections.identities()
    }

    /// The Cardano session's handle, for the signing and UTxO paths.
    pub fn api(&self) -> Option<WalletApi> {
        self.cardano().and_then(|connection| connection.api.clone())
    }

    /// Whether the CIP-30 handle is available for signing/UTxO queries.
    pub fn has_api(&self) -> bool {
        self.api().is_some()
    }

    /// The connected Cardano network, if there is one.
    pub fn connected_network(&self) -> Option<Network> {
        self.cardano().map(|connection| connection.network)
    }

    /// Load the last connected wallet from localStorage (for auto-reconnect).
    pub fn last_wallet() -> Option<WalletProvider> {
        wallet_core::load_last_wallet()
    }

    // ========================================================================
    // Writes
    // ========================================================================

    /// Apply a successful Cardano connection. Call after the async connect task
    /// completes.
    pub fn apply_connect_result(&mut self, result: WalletConnectResult, api: WalletApi) {
        let network = match result.network_id {
            1 => wallet_core::Network::Mainnet,
            _ => wallet_core::Network::Preprod,
        };
        // The icon the extension advertised for itself, at connect time; the
        // announcements are the only place it exists.
        let icon = self
            .available_wallets
            .iter()
            .find(|w| WalletProvider::from_api_name(&w.api_name) == Some(result.provider))
            .and_then(|w| w.icon.clone());
        self.connections
            .set_connected(ChainConnection::Cardano(CardanoConnection {
                provider: result.provider,
                address_hex: result.address_hex,
                stake_address: result.stake_address,
                network,
                api: Some(api),
                icon,
            }));
        wallet_core::save_last_wallet(result.provider);
    }

    /// Apply an EVM connection the host established — `eth_requestAccounts`
    /// answered, and this is the wallet that answered it.
    ///
    /// The host owns the connect (it is the one that can await the dialog); the
    /// connector is told the result, the same way it is told about Cardano.
    pub fn apply_evm_connection(&mut self, wallet: EvmWalletInfo, address: String) {
        self.connections
            .set_connected(ChainConnection::Evm(EvmConnection { wallet, address }));
    }

    /// Apply a fetched balance. Derives $handle automatically.
    ///
    /// A reading about the Cardano connection: see [`Self::balance`].
    pub fn apply_balance(&mut self, balance: WalletBalance) {
        self.handle = balance.ada_handle();
        self.balance = Some(balance);
    }

    /// Disconnect one chain and clear what was read about it.
    ///
    /// Disconnecting Cardano says nothing about EVM, which is the whole point of
    /// the set.
    pub fn disconnect(&mut self, family: ChainFamily) {
        self.connections.remove(family);
        if family == ChainFamily::Cardano {
            self.balance = None;
            self.handle = None;
            wallet_core::clear_last_wallet();
        }
    }

    /// Set the connecting phase (call before spawning the async connect task).
    pub fn set_connecting(&mut self, family: ChainFamily) {
        self.connections.set_connecting(family);
    }

    /// Set the error phase for a chain.
    pub fn set_error(&mut self, family: ChainFamily, error: impl Into<String>) {
        self.connections.set_error(family, error);
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

    let stake_address = stake_address_bech32(&address_hex);

    let result = WalletConnectResult {
        provider,
        address_hex,
        stake_address,
        network_id,
    };

    Ok((result, api))
}

/// Async helper: connect a chosen EVM wallet and return the account it gave.
///
/// The provider is resolved from the wallet's EIP-6963 id, not from
/// `window.ethereum`: several wallets install at once and only one of them owns
/// that global, so reading it would connect whichever won rather than the row
/// the reader picked.
///
/// Call from `wasm_bindgen_futures::spawn_local` and send the account through
/// your app's message channel; then hand the wallet and the account to
/// [`WalletConnector::apply_evm_connection`].
///
/// The first account is the answer. EIP-1193 returns the accounts the reader
/// granted, in the wallet's own order, and the active account is the first of
/// them — the same rule `wallet_links::link_evm` follows.
pub async fn connect_evm(wallet: &EvmWalletInfo) -> Result<String, String> {
    let provider = wallet_core::EvmProvider::for_wallet(wallet).map_err(|e| e.to_string())?;
    let accounts = provider
        .request_accounts()
        .await
        .map_err(|e| e.to_string())?;
    accounts.into_iter().next().ok_or_else(|| {
        format!(
            "{name} returned no account — unlock it and try again",
            name = wallet.name
        )
    })
}

/// Async helper: the account the connected EVM wallet currently has selected.
///
/// `eth_accounts` never prompts: it answers what the wallet is willing to name
/// for this origin, and nothing while it is locked or the permission has been
/// withdrawn. That is what makes it right for a window-focus refresh, where a
/// dialog the reader did not ask for would be an ambush.
///
/// `None` when the wallet cannot be reached or reports no account. A caller
/// refreshing a connection should then KEEP it: a locked wallet is transient,
/// and the next signature reports its own failure rather than the app silently
/// dropping the reader's account.
pub async fn connected_evm_account(wallet: &EvmWalletInfo) -> Option<String> {
    let provider = wallet_core::EvmProvider::for_wallet(wallet).ok()?;
    provider.accounts().await.ok()?.into_iter().next()
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

/// Pseudo-wallet demo profiles — see [`crate::wallet_mock`]. Re-exported here
/// so consumers reach it as `wallet::mock`.
pub use crate::wallet_mock as mock;

/// Infer the expected Cardano network from the current page hostname.
///
/// Returns `Preprod` if the hostname contains `.dev.` (e.g. `hire.dev.hodlcroft.com`)
/// or is `localhost`, otherwise `Mainnet`.
pub fn expected_network_from_hostname() -> Network {
    let hostname = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_default();

    if hostname.contains(".dev.")
        || hostname.contains(".preprod.")
        || hostname == "localhost"
        || hostname.starts_with("127.")
    {
        Network::Preprod
    } else {
        Network::Mainnet
    }
}
