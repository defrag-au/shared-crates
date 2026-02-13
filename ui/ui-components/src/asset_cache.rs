//! Asset Cache Leptos Component
//!
//! A non-visual component that preloads NFT asset images in the background.
//! Images are stored in a global cache and can be retrieved by other components.
//!
//! ## Props
//!
//! - `assets` - List of (policy_id, asset_name_hex) tuples to preload
//! - `on_ready` - Callback when all images have been preloaded (receives loaded, failed counts)
//! - `on_progress` - Optional callback as each image loads (receives loaded, total counts)
//!
//! ## Usage
//!
//! ```ignore
//! <AssetCache
//!     assets=assets_to_preload
//!     on_ready=move |(loaded, failed)| {
//!         log!("Cache ready: {} loaded, {} failed", loaded, failed);
//!     }
//! />
//! ```

use crate::image_cache;
use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

/// Asset to preload - contains policy_id and asset_name_hex
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreloadAsset {
    pub policy_id: String,
    pub asset_name_hex: String,
}

impl PreloadAsset {
    pub fn new(policy_id: impl Into<String>, asset_name_hex: impl Into<String>) -> Self {
        Self {
            policy_id: policy_id.into(),
            asset_name_hex: asset_name_hex.into(),
        }
    }

    /// Build the IIIF thumbnail URL (400px thumb)
    pub fn to_url(&self) -> String {
        format!(
            "https://iiif.hodlcroft.com/iiif/3/{}:{}/full/400,/0/default.jpg",
            self.policy_id, self.asset_name_hex
        )
    }
}

/// Asset cache component - preloads images in background
#[component]
pub fn AssetCache(
    /// Assets to preload
    #[prop(into)]
    assets: Signal<Vec<PreloadAsset>>,
    /// Callback when all images have been loaded (loaded_count, failed_count)
    #[prop(into)]
    on_ready: Callback<(u32, u32)>,
    /// Optional progress callback (loaded_count, total_count)
    #[prop(into, optional)]
    on_progress: Option<Callback<(u32, u32)>>,
) -> impl IntoView {
    // Track assets we've already started loading to avoid duplicates
    let loading_started = Rc::new(RefCell::new(Vec::<String>::new()));

    Effect::new(move |_| {
        let current_assets = assets.get();

        if current_assets.is_empty() {
            on_ready.run((0, 0));
            return;
        }

        // Filter out assets we've already started loading
        let urls: Vec<String> = current_assets
            .iter()
            .map(|a| a.to_url())
            .filter(|url| {
                let mut started = loading_started.borrow_mut();
                if started.contains(url) {
                    false
                } else {
                    started.push(url.clone());
                    true
                }
            })
            .collect();

        if urls.is_empty() {
            return;
        }

        let total = urls.len() as u32;
        tracing::info!("Preloading {} images into cache", total);

        let loaded = Rc::new(RefCell::new(0u32));
        let failed = Rc::new(RefCell::new(0u32));

        for url in urls {
            let loaded = Rc::clone(&loaded);
            let failed = Rc::clone(&failed);
            let on_ready = on_ready;
            let on_progress = on_progress;

            spawn_local(async move {
                let success = match image_cache::preload_image(url.clone()).await {
                    Ok(_blob_url) => {
                        tracing::debug!("Cached image: {}", url);
                        true
                    }
                    Err(e) => {
                        tracing::warn!("Failed to cache image {}: {:?}", url, e);
                        false
                    }
                };

                if success {
                    *loaded.borrow_mut() += 1;
                } else {
                    *failed.borrow_mut() += 1;
                }

                let current_loaded = *loaded.borrow();
                let current_failed = *failed.borrow();

                // Emit progress
                if let Some(cb) = &on_progress {
                    cb.run((current_loaded, total));
                }

                // Check if all done
                if current_loaded + current_failed >= total {
                    tracing::info!(
                        "Image cache ready: {} loaded, {} failed",
                        current_loaded,
                        current_failed
                    );
                    on_ready.run((current_loaded, current_failed));
                }
            });
        }
    });

    // Non-visual component - renders nothing
}
