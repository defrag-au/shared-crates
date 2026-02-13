//! Leptos bindings for wallet-core
//!
//! Provides reactive wallet state management for Leptos applications.
//!
//! ## Quick Start
//!
//! 1. Wrap your app with `WalletProvider`
//! 2. Use `use_wallet()` to access wallet state and methods
//!
//! ## Example
//!
//! ```ignore
//! use leptos::prelude::*;
//! use wallet_leptos::{WalletProvider, use_wallet, WalletProviderEnum, ConnectionState};
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     view! {
//!         <WalletProvider>
//!             <WalletDemo />
//!         </WalletProvider>
//!     }
//! }
//!
//! #[component]
//! fn WalletDemo() -> impl IntoView {
//!     let wallet = use_wallet();
//!
//!     view! {
//!         <div>
//!             // Show available wallets
//!             <For
//!                 each=move || wallet.available_wallets.get()
//!                 key=|w| w.api_name.clone()
//!                 let:info
//!             >
//!                 <button on:click=move |_| {
//!                     if let Some(provider) = WalletProviderEnum::from_api_name(&info.api_name) {
//!                         wallet.connect(provider);
//!                     }
//!                 }>
//!                     {info.name.clone()}
//!                 </button>
//!             </For>
//!
//!             // Show connection state
//!             {move || match wallet.connection_state.get() {
//!                 ConnectionState::Disconnected => view! {
//!                     <p>"Not connected"</p>
//!                 }.into_any(),
//!                 ConnectionState::Connecting => view! {
//!                     <p>"Connecting..."</p>
//!                 }.into_any(),
//!                 ConnectionState::Connected { address, .. } => view! {
//!                     <p>"Connected: " {address}</p>
//!                     <button on:click=move |_| wallet.disconnect()>
//!                         "Disconnect"
//!                     </button>
//!                 }.into_any(),
//!                 ConnectionState::Error(e) => view! {
//!                     <p>"Error: " {e}</p>
//!                 }.into_any(),
//!             }}
//!
//!             // Show balance (opt-in)
//!             <button on:click=move |_| wallet.fetch_balance()>
//!                 "Fetch Balance"
//!             </button>
//!             {move || wallet.balance.get().map(|b| view! {
//!                 <p>{format!("{:.6} ADA", b.ada())}</p>
//!             })}
//!
//!             // Show stake address (derived automatically)
//!             {move || wallet.stake_address.get().map(|sa| view! {
//!                 <p>"Stake: " {sa}</p>
//!             })}
//!         </div>
//!     }
//! }
//! ```
//!
//! ## Features
//!
//! - **Reactive signals** for all wallet state (connection, address, network, balance)
//! - **Auto-detection** of installed wallet extensions
//! - **Auto-reconnect** via localStorage persistence
//! - **Derived stake address** computed from payment address
//! - **Opt-in balance fetching** to avoid unnecessary API calls
//! - **Signing methods** for CIP-8 data signing and transaction signing

mod context;
mod hooks;
mod provider;

pub use context::WalletContext;
pub use hooks::{try_use_wallet, use_wallet};
pub use provider::WalletProvider;

// Re-export commonly used types from wallet-core
pub use wallet_core::{
    ConnectionState, DataSignature, Network, WalletApi, WalletError, WalletInfo,
    WalletProvider as WalletProviderEnum,
};

// Re-export balance types from wallet-pallas
pub use wallet_pallas::{decode_balance, NativeToken, PolicyGroup, WalletBalance};
