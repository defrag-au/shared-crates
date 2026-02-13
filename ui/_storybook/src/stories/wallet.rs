//! Wallet stories - wallet providers, detection, connection flow, balance, and NFT display

use cardano_assets::AssetId;
use leptos::prelude::*;
use ui_components::{AssetModal, WalletNftGallery};
use wallet_core::{
    detect_wallets, detect_wallets_with_info, ConnectionState, Network, WalletApi, WalletInfo,
    WalletProvider,
};
use wallet_leptos::{use_wallet, WalletProvider as WalletProviderComponent};
use wallet_pallas::{decode_balance, PolicyGroup, WalletBalance};
use wasm_bindgen_futures::spawn_local;

// ============================================================================
// Wallet Providers Story (Static Reference)
// ============================================================================

#[component]
pub fn WalletProvidersStory() -> impl IntoView {
    use leptos::prelude::CollectView;
    view! {
        <div>
            <div class="story-header">
                <h2>"Wallet Providers"</h2>
                <p>"Supported Cardano wallet providers for CIP-30 integration."</p>
            </div>

            <div class="story-section">
                <h3>"Supported Wallets"</h3>
                <p class="story-description">"These are all the wallet providers that wallet-core can detect and connect to."</p>
                <div class="story-canvas">
                    <div class="story-grid">
                        {WalletProvider::all().iter().map(|provider| {
                            view! {
                                <div class="wallet-card">
                                    <div class="wallet-card__header">
                                        <div class="wallet-card__icon">{wallet_icon(*provider)}</div>
                                        <span class="wallet-card__name">{provider.display_name()}</span>
                                    </div>
                                    <div class="wallet-card__body">
                                        <div class="wallet-card__row">
                                            <span class="wallet-card__label">"API Name"</span>
                                            <span class="wallet-card__value">{provider.api_name()}</span>
                                        </div>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use wallet_core::WalletProvider;

// Get all providers
for provider in WalletProvider::all() {
    println!("{}: {}", provider.display_name(), provider.api_name());
}

// Check specific provider
let nami = WalletProvider::Nami;
assert_eq!(nami.api_name(), "nami");
assert_eq!(nami.display_name(), "Nami");"#}</pre>
            </div>
        </div>
    }
}

fn wallet_icon(provider: WalletProvider) -> &'static str {
    match provider {
        WalletProvider::Nami => "N",
        WalletProvider::Eternl => "E",
        WalletProvider::Lace => "L",
        WalletProvider::Flint => "F",
        WalletProvider::Typhon => "T",
        WalletProvider::Vespr => "V",
        WalletProvider::NuFi => "Nu",
        WalletProvider::Gero => "G",
        WalletProvider::Yoroi => "Y",
    }
}

// ============================================================================
// Connection States Story (Static Reference)
// ============================================================================

#[component]
pub fn ConnectionStatesStory() -> impl IntoView {
    use leptos::prelude::CollectView;
    let states = vec![
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected {
            provider: WalletProvider::Eternl,
            address: "addr1qx...abc123".to_string(),
            network: Network::Mainnet,
        },
        ConnectionState::Error("User rejected connection".to_string()),
    ];

    view! {
        <div>
            <div class="story-header">
                <h2>"Connection States"</h2>
                <p>"Visual representation of wallet connection states."</p>
            </div>

            <div class="story-section">
                <h3>"State Indicators"</h3>
                <div class="story-canvas">
                    <div class="story-inline">
                        <span class="status-indicator status-indicator--disconnected">"Disconnected"</span>
                        <span class="status-indicator status-indicator--connecting">"Connecting..."</span>
                        <span class="status-indicator status-indicator--connected">"Connected"</span>
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Connection State Examples"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        {states.into_iter().map(|state| {
                            view! { <ConnectionCard state=state /> }
                        }).collect_view()}
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use wallet_core::ConnectionState;

let state = ConnectionState::Disconnected;

match state {
    ConnectionState::Disconnected => { /* Show connect button */ }
    ConnectionState::Connecting => { /* Show spinner */ }
    ConnectionState::Connected { provider, address, network } => {
        // Show connected wallet info
    }
    ConnectionState::Error(msg) => { /* Show error */ }
}"#}</pre>
            </div>
        </div>
    }
}

#[component]
fn ConnectionCard(state: ConnectionState) -> impl IntoView {
    let (status_class, status_text) = match &state {
        ConnectionState::Disconnected => ("status-indicator--disconnected", "Disconnected"),
        ConnectionState::Connecting => ("status-indicator--connecting", "Connecting"),
        ConnectionState::Connected { .. } => ("status-indicator--connected", "Connected"),
        ConnectionState::Error(_) => ("status-indicator--error", "Error"),
    };

    view! {
        <div class="wallet-card">
            <div class="wallet-card__header">
                <span class=format!("status-indicator {status_class}")>{status_text}</span>
            </div>
            <div class="wallet-card__body">
                {match state {
                    ConnectionState::Connected { provider, address, network } => {
                        let network_name = match network {
                            Network::Mainnet => "Mainnet",
                            Network::Preprod => "Preprod",
                            Network::Preview => "Preview",
                        };
                        view! {
                            <>
                                <div class="wallet-card__row">
                                    <span class="wallet-card__label">"Provider"</span>
                                    <span class="wallet-card__value">{provider.display_name()}</span>
                                </div>
                                <div class="wallet-card__row">
                                    <span class="wallet-card__label">"Address"</span>
                                    <span class="wallet-card__value">{address}</span>
                                </div>
                                <div class="wallet-card__row">
                                    <span class="wallet-card__label">"Network"</span>
                                    <span class="wallet-card__value">{network_name}</span>
                                </div>
                            </>
                        }.into_any()
                    }
                    ConnectionState::Error(msg) => {
                        view! {
                            <div class="wallet-card__row">
                                <span class="wallet-card__label">"Error"</span>
                                <span class="wallet-card__value wallet-card__value--error">{msg}</span>
                            </div>
                        }.into_any()
                    }
                    _ => {
                        view! {
                            <p class="wallet-card__empty">"No additional details"</p>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

// ============================================================================
// Live Wallet Detection Story
// ============================================================================

#[component]
pub fn WalletDetectionStory() -> impl IntoView {
    let (detected_wallets, set_detected_wallets) = signal(Vec::<WalletInfo>::new());
    let (is_scanning, set_is_scanning) = signal(false);
    let (last_scan, set_last_scan) = signal(Option::<String>::None);

    // Scan for wallets
    let scan_wallets = move || {
        set_is_scanning.set(true);

        // Use the sync detection function
        let wallets = detect_wallets_with_info();
        set_detected_wallets.set(wallets);
        set_is_scanning.set(false);

        // Record scan time
        let now = js_sys::Date::new_0();
        let time_str = format!(
            "{:02}:{:02}:{:02}",
            now.get_hours(),
            now.get_minutes(),
            now.get_seconds()
        );
        set_last_scan.set(Some(time_str));
    };

    // Auto-scan on mount
    Effect::new(move |_| {
        scan_wallets();
    });

    view! {
        <div>
            <div class="story-header">
                <h2>"Live Wallet Detection"</h2>
                <p>"Detect Cardano wallets installed in your browser using CIP-30."</p>
            </div>

            <div class="story-section">
                <h3>"Detected Wallets"</h3>
                <div class="story-toolbar">
                    <button
                        class="btn btn--primary"
                        on:click=move |_| scan_wallets()
                        disabled=move || is_scanning.get()
                    >
                        {move || if is_scanning.get() { "Scanning..." } else { "Rescan" }}
                    </button>
                    {move || last_scan.get().map(|time| view! {
                        <span class="story-toolbar__info">"Last scan: " {time}</span>
                    })}
                </div>

                <div class="story-canvas">
                    {move || {
                        let wallets = detected_wallets.get();
                        if wallets.is_empty() {
                            view! {
                                <div class="wallet-empty-state">
                                    <p class="wallet-empty-state__title">"No wallets detected"</p>
                                    <p class="wallet-empty-state__hint">
                                        "Install a Cardano wallet extension like "
                                        <a href="https://eternl.io" target="_blank">"Eternl"</a>
                                        ", "
                                        <a href="https://namiwallet.io" target="_blank">"Nami"</a>
                                        ", or "
                                        <a href="https://www.lace.io" target="_blank">"Lace"</a>
                                        " to test wallet integration."
                                    </p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="story-grid">
                                    {wallets.into_iter().map(|wallet| {
                                        view! { <DetectedWalletCard wallet=wallet /> }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }
                    }}
                </div>
            </div>

            <div class="story-section">
                <h3>"Detection API"</h3>
                <pre class="code-block">{r#"use wallet_core::{detect_wallets, detect_wallets_with_info};

// Simple detection (just provider types)
let providers: Vec<WalletProvider> = detect_wallets();

// Detection with full info (name, icon from extension)
let wallets: Vec<WalletInfo> = detect_wallets_with_info();

for wallet in wallets {
    println!("Found: {} ({})", wallet.name, wallet.api_name);
    if let Some(icon) = wallet.icon {
        // icon is a base64 data URL
    }
}"#}</pre>
            </div>
        </div>
    }
}

#[component]
fn DetectedWalletCard(wallet: WalletInfo) -> impl IntoView {
    view! {
        <div class="wallet-card wallet-card--detected">
            <div class="wallet-card__header">
                {wallet.icon.as_ref().map(|icon| view! {
                    <img class="wallet-card__icon-img" src=icon.clone() alt=wallet.name.clone() />
                })}
                <span class="wallet-card__name">{wallet.name.clone()}</span>
                <span class="wallet-card__badge wallet-card__badge--success">"Installed"</span>
            </div>
            <div class="wallet-card__body">
                <div class="wallet-card__row">
                    <span class="wallet-card__label">"API Name"</span>
                    <span class="wallet-card__value">{wallet.api_name.clone()}</span>
                </div>
            </div>
        </div>
    }
}

// ============================================================================
// Wallet Connection Flow Story
// ============================================================================

#[component]
pub fn WalletConnectionStory() -> impl IntoView {
    let (connection_state, set_connection_state) = signal(ConnectionState::Disconnected);
    let (available_wallets, set_available_wallets) = signal(Vec::<WalletProvider>::new());
    let (connection_log, set_connection_log) = signal(Vec::<String>::new());

    // Helper to add log entry
    let add_log = move |msg: String| {
        set_connection_log.update(|log| {
            let now = js_sys::Date::new_0();
            let time_str = format!(
                "{:02}:{:02}:{:02}",
                now.get_hours(),
                now.get_minutes(),
                now.get_seconds()
            );
            log.push(format!("[{time_str}] {msg}"));
            // Keep last 20 entries
            if log.len() > 20 {
                log.remove(0);
            }
        });
    };

    // Detect wallets on mount
    Effect::new(move |_| {
        let wallets = detect_wallets();
        set_available_wallets.set(wallets.clone());
        add_log(format!("Detected {} wallet(s)", wallets.len()));
    });

    // Connect to a wallet
    let connect_to_wallet = move |provider: WalletProvider| {
        set_connection_state.set(ConnectionState::Connecting);
        add_log(format!("Connecting to {}...", provider.display_name()));

        spawn_local(async move {
            match WalletApi::connect(provider).await {
                Ok(api) => {
                    add_log(format!("Connected to {}", provider.display_name()));

                    // Get network ID
                    match api.network_id().await {
                        Ok(network_id) => {
                            let network = match network_id {
                                1 => Network::Mainnet,
                                _ => Network::Preprod,
                            };
                            add_log(format!("Network: {:?} (id={})", network, network_id));

                            // Get change address
                            match api.change_address().await {
                                Ok(address) => {
                                    let short_addr = if address.len() > 20 {
                                        format!(
                                            "{}...{}",
                                            &address[..10],
                                            &address[address.len() - 10..]
                                        )
                                    } else {
                                        address.clone()
                                    };
                                    add_log(format!("Address: {short_addr}"));

                                    set_connection_state.set(ConnectionState::Connected {
                                        provider,
                                        address,
                                        network,
                                    });
                                }
                                Err(e) => {
                                    add_log(format!("Failed to get address: {e}"));
                                    set_connection_state.set(ConnectionState::Error(e.to_string()));
                                }
                            }
                        }
                        Err(e) => {
                            add_log(format!("Failed to get network: {e}"));
                            set_connection_state.set(ConnectionState::Error(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    add_log(format!("Connection failed: {e}"));
                    set_connection_state.set(ConnectionState::Error(e.to_string()));
                }
            }
        });
    };

    // Disconnect
    let disconnect = move || {
        add_log("Disconnected".to_string());
        set_connection_state.set(ConnectionState::Disconnected);
    };

    view! {
        <div>
            <div class="story-header">
                <h2>"Wallet Connection Flow"</h2>
                <p>"Interactive demo of the full wallet connection lifecycle."</p>
            </div>

            <div class="story-section">
                <h3>"Current State"</h3>
                <div class="story-canvas">
                    <div class="connection-demo">
                        <div class="connection-demo__state">
                            {move || {
                                let state = connection_state.get();
                                view! { <ConnectionCard state=state /> }
                            }}
                        </div>

                        <div class="connection-demo__actions">
                            {move || {
                                let state = connection_state.get();
                                match state {
                                    ConnectionState::Disconnected => {
                                        let wallets = available_wallets.get();
                                        if wallets.is_empty() {
                                            view! {
                                                <p class="connection-demo__hint">"No wallets available. Install a Cardano wallet extension to test."</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div class="connection-demo__wallet-list">
                                                    <p class="connection-demo__label">"Select a wallet to connect:"</p>
                                                    <div class="connection-demo__buttons">
                                                        {wallets.into_iter().map(|provider| {
                                                            view! {
                                                                <button
                                                                    class="btn btn--outline"
                                                                    on:click=move |_| connect_to_wallet(provider)
                                                                >
                                                                    {provider.display_name()}
                                                                </button>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                    ConnectionState::Connecting => {
                                        view! {
                                            <div class="connection-demo__connecting">
                                                <div class="spinner"></div>
                                                <p>"Waiting for wallet approval..."</p>
                                            </div>
                                        }.into_any()
                                    }
                                    ConnectionState::Connected { .. } => {
                                        view! {
                                            <button
                                                class="btn btn--secondary"
                                                on:click=move |_| disconnect()
                                            >
                                                "Disconnect"
                                            </button>
                                        }.into_any()
                                    }
                                    ConnectionState::Error(_) => {
                                        view! {
                                            <button
                                                class="btn btn--primary"
                                                on:click=move |_| {
                                                    set_connection_state.set(ConnectionState::Disconnected);
                                                }
                                            >
                                                "Try Again"
                                            </button>
                                        }.into_any()
                                    }
                                }
                            }}
                        </div>
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Connection Log"</h3>
                <div class="story-canvas">
                    <div class="connection-log">
                        {move || {
                            let log = connection_log.get();
                            if log.is_empty() {
                                view! { <p class="connection-log__empty">"No events yet"</p> }.into_any()
                            } else {
                                view! {
                                    <ul class="connection-log__list">
                                        {log.into_iter().map(|entry| {
                                            view! { <li>{entry}</li> }
                                        }).collect::<Vec<_>>()}
                                    </ul>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Connection API"</h3>
                <pre class="code-block">{r#"use wallet_core::{WalletApi, WalletProvider, ConnectionState};

// Connect to a specific wallet
let api = WalletApi::connect(WalletProvider::Eternl).await?;

// Get wallet info
let network_id = api.network_id().await?;  // 1 = mainnet, 0 = testnet
let address = api.change_address().await?;  // Hex-encoded address
let balance = api.balance().await?;         // CBOR-encoded value

// Sign a transaction
let witness = api.sign_tx(tx_hex, partial_sign).await?;

// Sign arbitrary data (CIP-8)
let signature = api.sign_data(address, payload_hex).await?;
// Returns DataSignature { signature, key }

// Submit a transaction
let tx_hash = api.submit_tx(signed_tx_hex).await?;"#}</pre>
            </div>
        </div>
    }
}

// ============================================================================
// Wallet Balance Story
// ============================================================================

#[component]
pub fn WalletBalanceStory() -> impl IntoView {
    let (available_wallets, set_available_wallets) = signal(Vec::<WalletProvider>::new());
    let (is_loading, set_is_loading) = signal(false);
    let (balance, set_balance) = signal(Option::<WalletBalance>::None);
    let (error, set_error) = signal(Option::<String>::None);
    let (connected_provider, set_connected_provider) = signal(Option::<WalletProvider>::None);

    // Detect wallets on mount
    Effect::new(move |_| {
        let wallets = detect_wallets();
        set_available_wallets.set(wallets);
    });

    // Fetch balance from a wallet
    let fetch_balance = move |provider: WalletProvider| {
        set_is_loading.set(true);
        set_error.set(None);
        set_balance.set(None);

        spawn_local(async move {
            match WalletApi::connect(provider).await {
                Ok(api) => {
                    set_connected_provider.set(Some(provider));

                    match api.balance().await {
                        Ok(balance_hex) => match decode_balance(&balance_hex) {
                            Ok(decoded) => {
                                set_balance.set(Some(decoded));
                            }
                            Err(e) => {
                                set_error.set(Some(format!("Failed to decode balance: {e}")));
                            }
                        },
                        Err(e) => {
                            set_error.set(Some(format!("Failed to get balance: {e}")));
                        }
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Connection failed: {e}")));
                }
            }
            set_is_loading.set(false);
        });
    };

    view! {
        <div>
            <div class="story-header">
                <h2>"Wallet Balance"</h2>
                <p>"Query and display wallet balance including ADA and native tokens."</p>
            </div>

            <div class="story-section">
                <h3>"Connect and Query Balance"</h3>
                <div class="story-canvas">
                    <div class="balance-demo">
                        // Wallet selection
                        <div class="balance-demo__controls">
                            {move || {
                                let wallets = available_wallets.get();
                                let loading = is_loading.get();

                                if wallets.is_empty() {
                                    view! {
                                        <p class="balance-demo__hint">"No wallets detected. Install a Cardano wallet extension."</p>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="balance-demo__buttons">
                                            {wallets.into_iter().map(|provider| {
                                                let is_connected = connected_provider.get() == Some(provider);
                                                view! {
                                                    <button
                                                        class="btn"
                                                        class:btn--primary=is_connected
                                                        class:btn--outline=!is_connected
                                                        on:click=move |_| fetch_balance(provider)
                                                        disabled=loading
                                                    >
                                                        {provider.display_name()}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>

                        // Loading state
                        {move || is_loading.get().then(|| view! {
                            <div class="balance-demo__loading">
                                <div class="spinner"></div>
                                <p>"Fetching balance..."</p>
                            </div>
                        })}

                        // Error state
                        {move || error.get().map(|e| view! {
                            <div class="balance-demo__error">
                                <p>{e}</p>
                            </div>
                        })}

                        // Balance display
                        {move || balance.get().map(|b| view! {
                            <BalanceDisplay balance=b />
                        })}
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Balance API"</h3>
                <pre class="code-block">{r#"use wallet_core::WalletApi;
use wallet_pallas::{decode_balance, WalletBalance};

// Connect and get raw CBOR balance
let api = WalletApi::connect(WalletProvider::Eternl).await?;
let balance_hex = api.balance().await?;

// Decode the CBOR into structured data
let balance: WalletBalance = decode_balance(&balance_hex)?;

// Access balance data
println!("Lovelace: {}", balance.lovelace);
println!("ADA: {:.6}", balance.ada());
println!("Token count: {}", balance.token_count());

// Iterate over native tokens
for token in balance.tokens() {
    println!("{}.{}: {}",
        token.policy_id,
        token.asset_name.unwrap_or(token.asset_name_hex),
        token.quantity
    );
}"#}</pre>
            </div>
        </div>
    }
}

#[component]
fn BalanceDisplay(balance: WalletBalance) -> impl IntoView {
    let tokens = balance.tokens();
    let ada = balance.ada();

    view! {
        <div class="balance-display">
            // ADA Balance
            <div class="balance-display__ada">
                <span class="balance-display__ada-amount">{format!("{:.6}", ada)}</span>
                <span class="balance-display__ada-label">"ADA"</span>
            </div>

            <div class="balance-display__details">
                <div class="balance-display__row">
                    <span class="balance-display__label">"Lovelace"</span>
                    <span class="balance-display__value">{format!("{}", balance.lovelace)}</span>
                </div>
                <div class="balance-display__row">
                    <span class="balance-display__label">"Policies"</span>
                    <span class="balance-display__value">{format!("{}", balance.policy_count())}</span>
                </div>
                <div class="balance-display__row">
                    <span class="balance-display__label">"Tokens"</span>
                    <span class="balance-display__value">{format!("{}", balance.token_count())}</span>
                </div>
            </div>

            // Native tokens list
            {(!tokens.is_empty()).then(|| view! {
                <div class="balance-display__tokens">
                    <h4 class="balance-display__tokens-title">"Native Tokens"</h4>
                    <div class="balance-display__tokens-list">
                        {tokens.into_iter().take(20).map(|token| {
                            let display_name = token.asset_name.clone()
                                .unwrap_or_else(|| {
                                    if token.asset_name_hex.len() > 16 {
                                        format!("{}...", &token.asset_name_hex[..16])
                                    } else {
                                        token.asset_name_hex.clone()
                                    }
                                });
                            let short_policy = format!("{}...", &token.policy_id[..8]);

                            view! {
                                <div class="token-row">
                                    <div class="token-row__info">
                                        <span class="token-row__name">{display_name}</span>
                                        <span class="token-row__policy">{short_policy}</span>
                                    </div>
                                    <span class="token-row__quantity">{format!("{}", token.quantity)}</span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            })}
        </div>
    }
}

// ============================================================================
// Wallet NFTs Story - Using WalletNftGallery component
// ============================================================================

#[component]
pub fn WalletNftsStory() -> impl IntoView {
    let (available_wallets, set_available_wallets) = signal(Vec::<WalletProvider>::new());
    let (is_loading, set_is_loading) = signal(false);
    let (policy_groups, set_policy_groups) = signal(Vec::<PolicyGroup>::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (connected_provider, set_connected_provider) = signal(Option::<WalletProvider>::None);

    // Modal state for asset preview
    let (modal_asset, set_modal_asset) = signal(Option::<AssetId>::None);

    // Detect wallets on mount
    Effect::new(move |_| {
        let wallets = detect_wallets();
        set_available_wallets.set(wallets);
    });

    // Fetch NFTs from a wallet
    let fetch_nfts = move |provider: WalletProvider| {
        set_is_loading.set(true);
        set_error.set(None);
        set_policy_groups.set(vec![]);

        spawn_local(async move {
            match WalletApi::connect(provider).await {
                Ok(api) => {
                    set_connected_provider.set(Some(provider));

                    match api.balance().await {
                        Ok(balance_hex) => match decode_balance(&balance_hex) {
                            Ok(decoded) => {
                                let groups = decoded.nft_policy_groups();
                                set_policy_groups.set(groups);
                            }
                            Err(e) => {
                                set_error.set(Some(format!("Failed to decode balance: {e}")));
                            }
                        },
                        Err(e) => {
                            set_error.set(Some(format!("Failed to get balance: {e}")));
                        }
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Connection failed: {e}")));
                }
            }
            set_is_loading.set(false);
        });
    };

    view! {
        <div>
            <div class="story-header">
                <h2>"Wallet NFTs"</h2>
                <p>"Display wallet NFTs grouped by policy using the WalletNftGallery component."</p>
            </div>

            <div class="story-section">
                <h3>"Connect and View NFTs"</h3>
                <div class="story-canvas">
                    <div class="nft-demo">
                        // Wallet selection
                        <div class="nft-demo__controls">
                            {move || {
                                let wallets = available_wallets.get();
                                let loading = is_loading.get();

                                if wallets.is_empty() {
                                    view! {
                                        <p class="nft-demo__hint">"No wallets detected. Install a Cardano wallet extension."</p>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="nft-demo__buttons">
                                            {wallets.into_iter().map(|provider| {
                                                let is_connected = connected_provider.get() == Some(provider);
                                                view! {
                                                    <button
                                                        class="btn"
                                                        class:btn--primary=is_connected
                                                        class:btn--outline=!is_connected
                                                        on:click=move |_| fetch_nfts(provider)
                                                        disabled=loading
                                                    >
                                                        {provider.display_name()}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>

                        // Error state
                        {move || error.get().map(|e| view! {
                            <div class="nft-demo__error">
                                <p>{e}</p>
                            </div>
                        })}

                        // NFT Gallery component with click handler
                        <WalletNftGallery
                            groups=Signal::derive(move || policy_groups.get())
                            loading=Signal::derive(move || is_loading.get())
                            on_asset_click=Callback::new(move |(asset_id_str, _name): (String, String)| {
                                // Parse the asset_id string into AssetId
                                if let Ok(asset_id) = AssetId::parse_concatenated(&asset_id_str) {
                                    set_modal_asset.set(Some(asset_id));
                                }
                            })
                        />
                    </div>
                </div>
            </div>

            // Asset preview modal using AssetModal component
            {move || modal_asset.get().map(|asset_id| {
                view! {
                    <AssetModal
                        asset_id=asset_id
                        on_close=Callback::new(move |_| set_modal_asset.set(None))
                    />
                }
            })}

            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use ui_components::{AssetModal, WalletNftGallery};
use cardano_assets::AssetId;
use wallet_pallas::PolicyGroup;

// Get NFT groups from wallet balance
let balance = decode_balance(&balance_hex)?;
let groups: Vec<PolicyGroup> = balance.nft_policy_groups();

// State for modal
let (selected, set_selected) = signal(Option::<AssetId>::None);

// Gallery with click handler
<WalletNftGallery
    groups=Signal::derive(move || groups.clone())
    loading=Signal::derive(move || is_loading.get())
    on_asset_click=Callback::new(move |(asset_id_str, _name)| {
        if let Ok(id) = AssetId::parse_concatenated(&asset_id_str) {
            set_selected.set(Some(id));
        }
    })
/>

// Render modal when asset is selected
{move || selected.get().map(|asset_id| view! {
    <AssetModal
        asset_id=asset_id
        on_close=Callback::new(move |_| set_selected.set(None))
    />
})}"#}</pre>
            </div>
        </div>
    }
}

// ============================================================================
// Wallet Leptos Story - Demonstrating the wallet-leptos context
// ============================================================================

#[component]
pub fn WalletLeptosStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Wallet Leptos Context"</h2>
                <p>"Reactive wallet state management using wallet-leptos crate."</p>
            </div>

            <div class="story-section">
                <h3>"Live Demo"</h3>
                <div class="story-canvas">
                    <WalletProviderComponent>
                        <WalletLeptosDemo />
                    </WalletProviderComponent>
                </div>
            </div>

            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use wallet_leptos::{WalletProvider, use_wallet, WalletProviderEnum};

// Wrap your app with WalletProvider
#[component]
fn App() -> impl IntoView {
    view! {
        <WalletProvider auto_detect=true auto_reconnect=true>
            <MyApp />
        </WalletProvider>
    }
}

// Use the wallet context in any child component
#[component]
fn MyApp() -> impl IntoView {
    let wallet = use_wallet();

    view! {
        // Available wallets
        <For each=move || wallet.available_wallets.get() key=|w| w.api_name.clone() let:info>
            <button on:click=move |_| {
                if let Some(p) = WalletProviderEnum::from_api_name(&info.api_name) {
                    wallet.connect(p);
                }
            }>
                {info.name.clone()}
            </button>
        </For>

        // Connection state
        {move || wallet.is_connected().then(|| view! {
            <p>"Address: " {wallet.address.get()}</p>
            <p>"Stake: " {wallet.stake_address.get()}</p>
            <button on:click=move |_| wallet.fetch_balance()>
                "Fetch Balance"
            </button>
        })}

        // Balance
        {move || wallet.balance.get().map(|b| view! {
            <p>{format!("{:.6} ADA", b.ada())}</p>
        })}
    }
}"#}</pre>
            </div>
        </div>
    }
}

/// Demo component showing wallet-leptos in action
#[component]
fn WalletLeptosDemo() -> impl IntoView {
    let wallet = use_wallet();

    // Clone wallet context for closures that need to move it
    let wallet_for_wallets = wallet.clone();
    let wallet_for_info = wallet.clone();

    view! {
        <div class="wallet-leptos-demo">
            // Available wallets
            <div class="wallet-leptos-demo__section">
                <h4>"Available Wallets"</h4>
                {move || {
                    let wallets = wallet_for_wallets.available_wallets.get();
                    let wallet_inner = wallet_for_wallets.clone();
                    if wallets.is_empty() {
                        view! { <p class="text-muted">"No wallets detected"</p> }.into_any()
                    } else {
                        view! {
                            <div class="wallet-leptos-demo__wallets">
                                {wallets.into_iter().map(|info| {
                                    let api_name = info.api_name.clone();
                                    let display_name = info.name.clone();
                                    let wallet_btn = wallet_inner.clone();
                                    view! {
                                        <button
                                            class="btn btn--outline"
                                            on:click=move |_| {
                                                if let Some(provider) = WalletProvider::from_api_name(&api_name) {
                                                    wallet_btn.connect(provider);
                                                }
                                            }
                                        >
                                            {display_name}
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Connection state
            <div class="wallet-leptos-demo__section">
                <h4>"Connection State"</h4>
                {move || {
                    let state = wallet.connection_state.get();
                    let (status_class, status_text) = match &state {
                        ConnectionState::Disconnected => ("status-indicator--disconnected", "Disconnected"),
                        ConnectionState::Connecting => ("status-indicator--connecting", "Connecting..."),
                        ConnectionState::Connected { .. } => ("status-indicator--connected", "Connected"),
                        ConnectionState::Error(_) => ("status-indicator--error", "Error"),
                    };
                    view! {
                        <span class=format!("status-indicator {status_class}")>{status_text}</span>
                    }
                }}
            </div>

            // Connected wallet info
            {move || {
                let wallet_inner = wallet_for_info.clone();
                wallet_for_info.is_connected().then(move || {
                    let address = wallet_inner.address.get().unwrap_or_default();
                    let short_addr = if address.len() > 20 {
                        format!("{}...{}", &address[..10], &address[address.len()-10..])
                    } else {
                        address
                    };

                    let wallet_fetch = wallet_inner.clone();
                    let wallet_disconnect = wallet_inner.clone();

                    view! {
                        <div class="wallet-leptos-demo__section">
                            <h4>"Wallet Info"</h4>
                            <div class="wallet-leptos-demo__info">
                                <div class="wallet-leptos-demo__row">
                                    <span class="label">"Address:"</span>
                                    <code>{short_addr}</code>
                                </div>
                                {wallet_inner.stake_address.get().map(|sa| {
                                    let short_stake = if sa.len() > 24 {
                                        format!("{}...{}", &sa[..12], &sa[sa.len()-8..])
                                    } else {
                                        sa.clone()
                                    };
                                    view! {
                                        <div class="wallet-leptos-demo__row">
                                            <span class="label">"Stake:"</span>
                                            <code>{short_stake}</code>
                                        </div>
                                    }
                                })}
                                {wallet_inner.network.get().map(|n| view! {
                                    <div class="wallet-leptos-demo__row">
                                        <span class="label">"Network:"</span>
                                        <span>{format!("{:?}", n)}</span>
                                    </div>
                                })}
                            </div>

                            <div class="wallet-leptos-demo__actions">
                                <button class="btn btn--outline" on:click=move |_| wallet_fetch.fetch_balance()>
                                    "Fetch Balance"
                                </button>
                                <button class="btn btn--secondary" on:click=move |_| wallet_disconnect.disconnect()>
                                    "Disconnect"
                                </button>
                            </div>
                        </div>
                    }
                })
            }}

            // Balance
            {move || wallet.balance.get().map(|b| view! {
                <div class="wallet-leptos-demo__section">
                    <h4>"Balance"</h4>
                    <div class="wallet-leptos-demo__balance">
                        <span class="wallet-leptos-demo__ada">{format!("{:.6}", b.ada())}</span>
                        <span class="wallet-leptos-demo__ada-label">"ADA"</span>
                    </div>
                    <p class="text-muted">{b.token_count()} " native tokens"</p>
                </div>
            })}

            // Error display
            {move || wallet.error.get().map(|e| view! {
                <div class="wallet-leptos-demo__error">
                    <p>{e}</p>
                </div>
            })}
        </div>
    }
}
