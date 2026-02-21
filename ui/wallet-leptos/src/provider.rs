//! WalletProvider component for providing wallet context

use crate::context::WalletContext;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

/// Provides wallet context to child components
///
/// Wrap your app (or a section of it) with this component to enable
/// wallet functionality via `use_wallet()`.
///
/// Automatically refreshes wallet state (address + balance) when the page
/// regains visibility, detecting account switches in wallet extensions.
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

    // Clone before the Effect moves ctx
    let ctx_for_visibility = ctx.clone();

    // Auto-detect and reconnect on mount
    Effect::new(move |_| {
        if auto_detect {
            ctx.detect_wallets();
        }
        if auto_reconnect {
            ctx.try_reconnect();
        }
    });

    // Refresh wallet state when the page becomes visible again.
    // This detects account switches in extensions like Eternl.
    let closure = Closure::<dyn Fn()>::new(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let state = doc.visibility_state();
            tracing::info!(?state, "visibilitychange event fired");
            if state == web_sys::VisibilityState::Visible {
                ctx_for_visibility.refresh();
            }
        }
    });

    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        let _ = doc
            .add_event_listener_with_callback("visibilitychange", closure.as_ref().unchecked_ref());
    }

    // Leak the closure so it lives for the lifetime of the app
    closure.forget();

    children()
}
