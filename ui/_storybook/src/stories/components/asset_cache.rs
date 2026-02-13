//! Asset Cache component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{AssetCache, AssetCard, PreloadAsset};

/// Sample Pirate assets for testing
const TEST_ASSETS: &[(&str, &str)] = &[
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "506972617465313839",
    ),
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "506972617465323030",
    ),
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "506972617465333333",
    ),
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "506972617465343334",
    ),
    (
        "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        "506972617465353335",
    ),
];

#[component]
pub fn AssetCacheStory() -> impl IntoView {
    use leptos::prelude::CollectView;
    let (status, set_status) = signal("Ready to preload".to_string());
    let (cache_size, set_cache_size) = signal(0usize);
    let (cards_visible, set_cards_visible) = signal(false);
    let (preload_started, set_preload_started) = signal(false);
    let (assets_to_load, set_assets_to_load) = signal::<Vec<PreloadAsset>>(vec![]);

    let test_assets: Vec<PreloadAsset> = TEST_ASSETS
        .iter()
        .map(|(policy_id, asset_name_hex)| PreloadAsset {
            policy_id: policy_id.to_string(),
            asset_name_hex: asset_name_hex.to_string(),
        })
        .collect();

    view! {
        <div>
            <div class="story-header">
                <h2>"Asset Cache"</h2>
                <p>"Non-visual component that preloads NFT images into a global cache. Cached images are stored as blob URLs and used by AssetCard and ImageCard for instant display without network requests."</p>
            </div>

            // Demo section
            <div class="story-section">
                <h3>"Preload Demo"</h3>
                <p>"Click 'Start Preload' to cache 5 images. Watch the status update as images load. Then click 'Show Cards' to see them render instantly from cache."</p>

                <div class="story-canvas">
                    // Status display
                    <div style="display: flex; gap: 1rem; align-items: center; margin-bottom: 1rem;">
                        <span class="status-indicator">{status}</span>
                        <span class="status-indicator status-indicator--connected">
                            {move || format!("Cache: {} images", cache_size.get())}
                        </span>
                    </div>

                    // Buttons
                    <div style="display: flex; gap: 0.5rem; margin-bottom: 1rem;">
                        <button
                            class="btn"
                            style="padding: 0.5rem 1rem;"
                            on:click={
                                let test_assets = test_assets.clone();
                                move |_| {
                                    if !preload_started.get() {
                                        set_preload_started.set(true);
                                        set_status.set("Starting preload...".to_string());
                                        set_assets_to_load.set(test_assets.clone());
                                    }
                                }
                            }
                        >
                            "Start Preload"
                        </button>
                        <button
                            class="btn"
                            style="padding: 0.5rem 1rem;"
                            on:click=move |_| {
                                set_cards_visible.set(true);
                                set_cache_size.set(ui_components::image_cache::cache_size());
                            }
                        >
                            "Show Cards"
                        </button>
                        <button
                            class="btn"
                            style="padding: 0.5rem 1rem;"
                            on:click=move |_| {
                                set_cards_visible.set(false);
                                set_status.set("Ready to preload".to_string());
                                set_cache_size.set(0);
                                set_preload_started.set(false);
                                set_assets_to_load.set(vec![]);
                            }
                        >
                            "Reset"
                        </button>
                    </div>

                    // Asset cache component (non-visual)
                    <AssetCache
                        assets=assets_to_load
                        on_progress=move |loaded, total| {
                            set_status.set(format!("Loading: {loaded}/{total}"));
                        }
                        on_ready=move |loaded, failed| {
                            set_status.set(format!("Ready! {loaded} loaded, {failed} failed"));
                            set_cache_size.set(ui_components::image_cache::cache_size());
                        }
                    />

                    // Card display (conditionally visible)
                    <Show when=move || cards_visible.get()>
                        <div style="display: flex; gap: 1rem; flex-wrap: wrap; margin-top: 1rem;">
                            {TEST_ASSETS.iter().enumerate().map(|(i, (policy_id, asset_name_hex))| {
                                let asset_id = format!("{policy_id}{asset_name_hex}");
                                view! {
                                    <AssetCard
                                        asset_id=asset_id
                                        name=format!("Pirate #{}", i + 1)
                                        show_name=true
                                    />
                                }
                            }).collect_view()}
                        </div>
                    </Show>
                </div>
            </div>

            // How It Works section
            <div class="story-section">
                <h3>"How It Works"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="1. Preload"
                            values="AssetCache"
                            description="Pass asset list to AssetCache. The component fetches each image."
                        />
                        <AttributeCard
                            name="2. Store"
                            values="Blob URLs"
                            description="Images are stored in a global cache as blob URLs, keeping them in memory."
                        />
                        <AttributeCard
                            name="3. Use"
                            values="AssetCard / ImageCard"
                            description="When rendering, cards check the cache first. Cached images display instantly."
                        />
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="assets"
                            values="Signal<Vec<PreloadAsset>>"
                            description="List of assets to preload. Each has policy_id and asset_name_hex."
                        />
                        <AttributeCard
                            name="on_progress"
                            values="Callback<(u32, u32)>"
                            description="Called as images load: (loaded_count, total_count)"
                        />
                        <AttributeCard
                            name="on_ready"
                            values="Callback<(u32, u32)>"
                            description="Called when all done: (loaded_count, failed_count)"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{AssetCache, AssetCard, PreloadAsset};

let (assets, set_assets) = create_signal(vec![]);

// Start preloading
set_assets.set(vec![
    PreloadAsset {
        policy_id: "abc...".to_string(),
        asset_name_hex: "def...".to_string(),
    },
    // ...more assets
]);

view! {
    // Non-visual preloader
    <AssetCache
        assets=assets
        on_progress=move |loaded, total| {
            show_progress(loaded, total);
        }
        on_ready=move |loaded, _failed| {
            // Now safe to show cards - they'll load instantly
            show_game_board();
        }
    />

    // Cards will use cached blob URLs automatically
    <AssetCard
        asset_id="{policy_id}{asset_name_hex}"
        name="Pirate #1"
        show_name=true
    />
}"##}</pre>
            </div>

            // API section
            <div class="story-section">
                <h3>"Programmatic API"</h3>
                <pre class="code-block">{r##"use ui_components::image_cache;

// Check cache size
let count = image_cache::cache_size();

// Check if a specific URL is cached
if let Some(blob_url) = image_cache::get_cached_url(&image_url) {
    // Use the blob URL directly
}

// Manually preload an image (async)
let blob_url = image_cache::preload_image(url).await?;"##}</pre>
            </div>
        </div>
    }
}
