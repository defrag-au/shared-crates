//! Hooks for accessing wallet context

use crate::context::WalletContext;
use leptos::prelude::*;

/// Get the wallet context from the current scope
///
/// # Panics
///
/// Panics if called outside of a `WalletProvider`
///
/// # Example
///
/// ```ignore
/// use wallet_leptos::use_wallet;
///
/// #[component]
/// fn MyComponent() -> impl IntoView {
///     let wallet = use_wallet();
///
///     view! {
///         <p>"Connected: " {move || wallet.is_connected()}</p>
///     }
/// }
/// ```
pub fn use_wallet() -> WalletContext {
    expect_context::<WalletContext>()
}

/// Try to get the wallet context, returning None if not in a WalletProvider
///
/// Use this when wallet functionality is optional.
///
/// # Example
///
/// ```ignore
/// use wallet_leptos::try_use_wallet;
///
/// #[component]
/// fn MyComponent() -> impl IntoView {
///     let wallet = try_use_wallet();
///
///     view! {
///         {wallet.map(|w| view! {
///             <p>"Wallet available"</p>
///         })}
///     }
/// }
/// ```
pub fn try_use_wallet() -> Option<WalletContext> {
    use_context::<WalletContext>()
}
