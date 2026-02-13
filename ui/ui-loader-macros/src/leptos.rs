//! Leptos framework macros for widget loading orchestration
//!
//! Provides `leptos_main!` macro for Leptos widgets with automatic loading,
//! authentication, and error handling.
//!
//! ## Usage
//!
//! ```ignore
//! widget_loader::leptos_main! {
//!     config: LoaderConfig::new()
//!         .auth_required(true)
//!         .initial_message("Loading reports..."),
//!
//!     load: |auth: AuthState, _loader: LoadingHandle| async move {
//!         // Pre-load any data needed before rendering
//!         Ok(MyInitialData { ... })
//!     },
//!
//!     app: App,  // Your root Leptos component
//! }
//! ```
//!
//! The root component receives `LoadResult<T>` as a prop:
//!
//! ```ignore
//! #[component]
//! pub fn App(result: LoadResult<MyInitialData>) -> impl IntoView {
//!     let LoadResult { auth, data, .. } = result;
//!     // ...
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Token};

/// Input structure for leptos_main! macro
pub struct LeptosMainInput {
    pub config: Expr,
    pub load: Expr,
    pub app: Expr,
}

impl Parse for LeptosMainInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut config: Option<Expr> = None;
        let mut load: Option<Expr> = None;
        let mut app: Option<Expr> = None;

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
                "app" => {
                    app = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown field `{other}`, expected `config`, `load`, or `app`"),
                    ));
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(LeptosMainInput {
            config: config.ok_or_else(|| input.error("missing `config` field"))?,
            load: load.ok_or_else(|| input.error("missing `load` field"))?,
            app: app.ok_or_else(|| input.error("missing `app` field"))?,
        })
    }
}

/// Generate Leptos widget entry point
pub fn generate(input: LeptosMainInput) -> TokenStream {
    let config = &input.config;
    let load = &input.load;
    let app = &input.app;

    let expanded = quote! {
        #[wasm_bindgen::prelude::wasm_bindgen(start)]
        pub async fn start() {
            use ui_loader::{LoadingOrchestrator, LoaderConfig, LoadResult};

            let config: LoaderConfig = (#config).into();
            let load_fn = #load;

            let result = LoadingOrchestrator::run(config, load_fn).await;

            match result {
                Ok(loaded) => {
                    use wasm_bindgen::JsCast;

                    // Get the #app element and cast to HtmlElement
                    let mount_point = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id("app"))
                        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                        .expect("Could not find #app element");

                    // Mount Leptos app to #app element (forget handle to keep mounted)
                    leptos::mount::mount_to(mount_point, move || {
                        leptos::view! { <#app result=loaded /> }
                    }).forget();
                }
                Err(_) => {
                    // Error screen already shown by orchestrator
                }
            }
        }
    };

    TokenStream::from(expanded)
}
