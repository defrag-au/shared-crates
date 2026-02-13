//! WalletProvider component for providing wallet context

use crate::context::WalletContext;
use leptos::prelude::*;

/// Provides wallet context to child components
///
/// Wrap your app (or a section of it) with this component to enable
/// wallet functionality via `use_wallet()`.
///
/// # Example
///
/// ```ignore
/// use wallet_leptos::{WalletProvider, use_wallet};
///
/// #[component]
/// fn App() -> impl IntoView {
///     view! {
///         <WalletProvider>
///             <MyApp />
///         </WalletProvider>
///     }
/// }
/// ```
#[component]
pub fn WalletProvider(
    /// Whether to auto-detect wallets on mount
    #[prop(optional, default = true)]
    auto_detect: bool,

    /// Whether to attempt auto-reconnect on mount
    #[prop(optional, default = true)]
    auto_reconnect: bool,

    children: Children,
) -> impl IntoView {
    let ctx = WalletContext::new();
    provide_context(ctx.clone());

    // Auto-detect and reconnect on mount
    Effect::new(move |_| {
        if auto_detect {
            ctx.detect_wallets();
        }
        if auto_reconnect {
            ctx.try_reconnect();
        }
    });

    children()
}
