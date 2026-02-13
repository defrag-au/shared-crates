//! Proc-macro for declarative widget loading orchestration
//!
//! This crate provides macros for creating framework-specific widget entry points
//! with automatic loading orchestration, authentication, and error handling.
//!
//! ## Seed Framework
//!
//! Use `seed_main!` for Seed-based widgets:
//!
//! ```ignore
//! widget_loader::seed_main! {
//!     config: LoaderConfig::new()
//!         .auth_required(true)
//!         .initial_message("Loading..."),
//!
//!     load: |auth: AuthState, loader: LoadingHandle| async move {
//!         loader.set_message("Fetching data...");
//!         let data = fetch_data(&auth).await?;
//!         Ok(MyData { data })
//!     },
//!
//!     app: ("app", model::init_with_data, update::update, view::view),
//! }
//! ```
//!
//! With realtime WebSocket support:
//!
//! ```ignore
//! widget_loader::seed_main! {
//!     config: LoaderConfig::new()
//!         .realtime(RealtimeConfig::widget_bridge()),
//!
//!     load: |auth, loader| async move { Ok(MyData {}) },
//!
//!     realtime: (WidgetBridgeEvent, Msg::Realtime),
//!     app: ("app", init_with_data, update, view),
//! }
//! ```
//!
//! ## Leptos Framework
//!
//! Use `leptos_main!` for Leptos-based widgets:
//!
//! ```ignore
//! widget_loader::leptos_main! {
//!     config: LoaderConfig::new()
//!         .auth_required(true)
//!         .initial_message("Loading reports..."),
//!
//!     load: |auth: AuthState, _loader: LoadingHandle| async move {
//!         Ok(MyInitialData { ... })
//!     },
//!
//!     app: App,  // Your root Leptos component
//! }
//! ```
//!
//! The root Leptos component receives `LoadResult<T>` as a prop:
//!
//! ```ignore
//! #[component]
//! pub fn App(result: LoadResult<MyInitialData>) -> impl IntoView {
//!     let LoadResult { auth, data, .. } = result;
//!     // Use auth and pre-loaded data
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Ident, Token};

mod leptos;
mod seed;
mod static_widget;

/// Input structure for the widget_main! macro (generic)
struct WidgetMainInput {
    config: Expr,
    load: Expr,
    start: Expr,
}

impl Parse for WidgetMainInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut config: Option<Expr> = None;
        let mut load: Option<Expr> = None;
        let mut start: Option<Expr> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "config" => {
                    config = Some(input.parse()?);
                }
                "load" => {
                    load = Some(input.parse()?);
                }
                "start" => {
                    start = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown field `{other}`, expected `config`, `load`, or `start`"),
                    ));
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(WidgetMainInput {
            config: config.ok_or_else(|| input.error("missing `config` field"))?,
            load: load.ok_or_else(|| input.error("missing `load` field"))?,
            start: start.ok_or_else(|| input.error("missing `start` field"))?,
        })
    }
}

/// Generate a widget entry point with loading orchestration (generic/framework-agnostic)
///
/// For framework-specific macros, use `seed_main!` or `leptos_main!` instead.
#[proc_macro]
pub fn widget_main(input: TokenStream) -> TokenStream {
    let WidgetMainInput {
        config,
        load,
        start,
    } = parse_macro_input!(input as WidgetMainInput);

    let expanded = quote! {
        #[wasm_bindgen::prelude::wasm_bindgen(start)]
        pub async fn start() {
            use ui_loader::{LoadingOrchestrator, LoaderConfig, LoadResult};

            let config = #config;
            let load_fn = #load;
            let start_fn = #start;

            let result = LoadingOrchestrator::run(config, load_fn).await;

            match result {
                Ok(loaded) => {
                    start_fn(loaded);
                }
                Err(_) => {
                    // Error screen already shown by orchestrator
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Seed framework widget entry point with loading orchestration
///
/// See crate-level documentation for usage examples.
#[proc_macro]
pub fn seed_main(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as seed::SeedMainInput);
    seed::generate(parsed)
}

/// Leptos framework widget entry point with loading orchestration
///
/// See crate-level documentation for usage examples.
#[proc_macro]
pub fn leptos_main(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as leptos::LeptosMainInput);
    leptos::generate(parsed)
}

/// Static widget entry point for read-only widgets
///
/// For widgets that load data once and render without interactivity.
/// Uses maud for HTML templating and skips reactive framework overhead.
///
/// See `static_widget` module documentation for usage examples.
#[proc_macro]
pub fn static_widget(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as static_widget::StaticWidgetInput);
    static_widget::generate(parsed)
}
