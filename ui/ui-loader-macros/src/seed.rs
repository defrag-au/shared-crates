//! Seed framework macros for widget loading orchestration

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Token};

/// Input structure for seed_main! macro
pub struct SeedMainInput {
    pub config: Expr,
    pub load: Expr,
    pub mount_point: Expr,
    pub init_fn: Expr,
    pub update_fn: Expr,
    pub view_fn: Expr,
    /// Optional realtime config: (EventType, MsgWrapper)
    pub realtime: Option<(Expr, Expr)>,
}

impl Parse for SeedMainInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut config: Option<Expr> = None;
        let mut load: Option<Expr> = None;
        let mut app: Option<(Expr, Expr, Expr, Expr)> = None;
        let mut realtime: Option<(Expr, Expr)> = None;

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
                    // Parse tuple: (mount_point, init, update, view)
                    let content;
                    syn::parenthesized!(content in input);
                    let mount_point: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let init_fn: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let update_fn: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let view_fn: Expr = content.parse()?;
                    app = Some((mount_point, init_fn, update_fn, view_fn));
                }
                "realtime" => {
                    // Parse tuple: (EventType, Msg::Wrapper)
                    let content;
                    syn::parenthesized!(content in input);
                    let event_type: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let msg_wrapper: Expr = content.parse()?;
                    realtime = Some((event_type, msg_wrapper));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown field `{other}`, expected `config`, `load`, `realtime`, or `app`"
                        ),
                    ));
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (mount_point, init_fn, update_fn, view_fn) =
            app.ok_or_else(|| input.error("missing `app` field"))?;

        Ok(SeedMainInput {
            config: config.ok_or_else(|| input.error("missing `config` field"))?,
            load: load.ok_or_else(|| input.error("missing `load` field"))?,
            mount_point,
            init_fn,
            update_fn,
            view_fn,
            realtime,
        })
    }
}

/// Generate Seed widget entry point
pub fn generate(input: SeedMainInput) -> TokenStream {
    let config = &input.config;
    let load = &input.load;
    let mount_point = &input.mount_point;
    let init_fn = &input.init_fn;
    let update_fn = &input.update_fn;
    let view_fn = &input.view_fn;

    let expanded = if let Some((event_type, msg_wrapper)) = &input.realtime {
        // With realtime: activate connection and pass to init
        // Note: realtime support requires widget_common from augminted-bots
        quote! {
            #[wasm_bindgen::prelude::wasm_bindgen(start)]
            pub async fn start() {
                use ui_loader::{LoadingOrchestrator, LoaderConfig, LoadResult};
                use widget_common::realtime_loader::ActivatedRealtime;
                use seed::prelude::*;

                let config: LoaderConfig = (#config).into();
                let load_fn = #load;

                let result = LoadingOrchestrator::run(config, load_fn).await;

                match result {
                    Ok(mut loaded) => {
                        App::start(
                            #mount_point,
                            move |_url, orders| {
                                // Activate realtime connection if present
                                let realtime: Option<ActivatedRealtime<#event_type>> =
                                    loaded.realtime.take().map(|conn| {
                                        let (client, url) = conn.activate(orders, #msg_wrapper);
                                        ActivatedRealtime::new(client, url)
                                    });

                                #init_fn(loaded, realtime, orders)
                            },
                            #update_fn,
                            #view_fn,
                        );
                    }
                    Err(_) => {
                        // Error screen already shown by orchestrator
                    }
                }
            }
        }
    } else {
        // Without realtime: simple passthrough
        quote! {
            #[wasm_bindgen::prelude::wasm_bindgen(start)]
            pub async fn start() {
                use ui_loader::{LoadingOrchestrator, LoaderConfig, LoadResult};
                use seed::prelude::*;

                let config: LoaderConfig = (#config).into();
                let load_fn = #load;

                let result = LoadingOrchestrator::run(config, load_fn).await;

                match result {
                    Ok(loaded) => {
                        App::start(
                            #mount_point,
                            move |_url, orders| #init_fn(loaded, orders),
                            #update_fn,
                            #view_fn,
                        );
                    }
                    Err(_) => {
                        // Error screen already shown by orchestrator
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}
