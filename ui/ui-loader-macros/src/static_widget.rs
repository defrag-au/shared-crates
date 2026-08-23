//! Static widget macro for read-only widgets
//!
//! For widgets that load data once and render without interactivity,
//! this macro skips the reactive framework entirely and just renders
//! HTML directly to the DOM.
//!
//! ## Usage
//!
//! ```ignore
//! widget_loader::static_widget! {
//!     config: LoaderConfig::new()
//!         .auth_required(true)
//!         .initial_message("Loading..."),
//!
//!     load: |auth: AuthState, _loader: LoadingHandle| async move {
//!         let data = http::get_json("/api/data", Some(&auth)).await?;
//!         Ok(data)
//!     },
//!
//!     render: |data: &MyData, auth: &AuthState| {
//!         maud::html! {
//!             div.container {
//!                 h1 { "My Widget" }
//!                 p { (data.some_field) }
//!             }
//!         }
//!     },
//! }
//! ```
//!
//! The `render` function receives the loaded data and auth state,
//! and returns a `maud::Markup` that gets set as innerHTML of the
//! mount point.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Token};

/// Input structure for static_widget! macro
pub struct StaticWidgetInput {
    pub config: Expr,
    pub load: Expr,
    pub render: Expr,
    /// Optional mount point (defaults to "app")
    pub mount: Option<Expr>,
}

impl Parse for StaticWidgetInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut config: Option<Expr> = None;
        let mut load: Option<Expr> = None;
        let mut render: Option<Expr> = None;
        let mut mount: Option<Expr> = None;

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
                "render" => {
                    render = Some(input.parse()?);
                }
                "mount" => {
                    mount = Some(input.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown field `{other}`, expected `config`, `load`, `render`, or `mount`"
                        ),
                    ));
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(StaticWidgetInput {
            config: config.ok_or_else(|| input.error("missing `config` field"))?,
            load: load.ok_or_else(|| input.error("missing `load` field"))?,
            render: render.ok_or_else(|| input.error("missing `render` field"))?,
            mount,
        })
    }
}

/// Generate static widget entry point
pub fn generate(input: StaticWidgetInput) -> TokenStream {
    let config = &input.config;
    let load = &input.load;
    let render = &input.render;
    let mount_point = input
        .mount
        .as_ref()
        .map(|m| quote! { #m })
        .unwrap_or_else(|| quote! { "app" });

    let expanded = quote! {
        #[wasm_bindgen::prelude::wasm_bindgen(start)]
        pub async fn start() {
            use ui_loader::{LoadingOrchestrator, LoaderConfig, LoadResult};

            let config: LoaderConfig = (#config).into();
            let load_fn = #load;
            let render_fn = #render;

            let result = LoadingOrchestrator::run(config, load_fn).await;

            match result {
                Ok(loaded) => {
                    // Get the mount point element
                    let window = web_sys::window().expect("no window");
                    let document = window.document().expect("no document");
                    let mount = document
                        .get_element_by_id(#mount_point)
                        .expect("mount point not found");

                    // Render the static HTML.
                    //
                    // The `allow` is generated deliberately. Expanded macro code
                    // lands in the CALLER's crate, so a consumer that bans
                    // `set_inner_html` (augminted-bots does, via clippy.toml)
                    // reports this line against the `static_widget!` invocation —
                    // a lint the caller cannot act on, in code they did not
                    // write. The waiver is sound here: `render_fn` returns maud
                    // `Markup`, which escapes text and attribute values by
                    // construction, and installing it is the whole point of a
                    // static widget.
                    let html = render_fn(&loaded.data, &loaded.auth);
                    #[allow(clippy::disallowed_methods)]
                    mount.set_inner_html(&html.into_string());
                }
                Err(_) => {
                    // Error screen already shown by orchestrator
                }
            }
        }
    };

    TokenStream::from(expanded)
}
