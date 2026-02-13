//! Asset Detail Card Component
//!
//! A detailed view for displaying NFT asset information including traits.
//! Designed for use in modals, detail panels, or full-page views.
//!
//! ## Features
//!
//! - Large image display with IIIF URL generation
//! - Asset name and optional rarity rank
//! - Traits displayed as label/value pairs
//! - Optional accent color (e.g., tier/rarity color)
//! - Action slot for buttons (buy, sell, equip, etc.)
//!
//! ## Props
//!
//! - `asset_id` - Cardano asset ID for IIIF URL generation
//! - `image_url` - Direct image URL (fallback)
//! - `name` - Asset display name
//! - `traits` - Map of trait name to values
//! - `rarity_rank` - Optional rarity ranking
//! - `accent_color` - Optional accent color for header
//! - `actions` - Optional action buttons slot
//! - `on_close` - Optional close callback (shows X button when set)
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::AssetDetailCard;
//! use std::collections::HashMap;
//!
//! let traits = HashMap::from([
//!     ("Background".to_string(), vec!["Ocean".to_string()]),
//!     ("Hat".to_string(), vec!["Pirate".to_string()]),
//!     ("Weapon".to_string(), vec!["Cutlass".to_string()]),
//! ]);
//!
//! view! {
//!     <AssetDetailCard
//!         asset_id="b3dab69f7e..."
//!         name="Pirate #189"
//!         traits=traits
//!         rarity_rank=Some(42)
//!         on_close=move |()| set_selected.set(None)
//!     />
//! }
//! ```

use crate::asset_card::{generate_iiif_url, IiifSize};
use leptos::prelude::*;
use std::collections::HashMap;

/// Asset detail card for displaying full asset information with traits
#[component]
pub fn AssetDetailCard(
    /// Cardano asset ID (policy_id + asset_name hex)
    #[prop(into, optional)]
    asset_id: Option<Signal<String>>,
    /// Direct image URL (fallback when asset_id not available)
    #[prop(into, optional)]
    image_url: Option<Signal<String>>,
    /// Thumbnail/placeholder URL shown blurred while high-res loads
    #[prop(into, optional)]
    thumbnail_url: Option<Signal<String>>,
    /// Asset display name
    #[prop(into)]
    name: Signal<String>,
    /// Trait map: trait_name -> values
    #[prop(into, optional)]
    traits: Signal<HashMap<String, Vec<String>>>,
    /// Rarity rank (lower = rarer)
    #[prop(into, optional)]
    rarity_rank: Signal<Option<u32>>,
    /// Accent color for header bar
    #[prop(into, optional)]
    accent_color: Option<Signal<String>>,
    /// Footer content slot (power stats, action buttons, etc.)
    #[prop(optional)]
    children: Option<Children>,
    /// Close callback - shows X button when set
    #[prop(into, optional)]
    on_close: Option<Callback<()>>,
) -> impl IntoView {
    // Track whether high-res image has loaded
    let (high_res_loaded, set_high_res_loaded) = signal(false);

    // Resolve high-res image URL - prefer direct URL, fall back to IIIF generation
    let high_res_url = Memo::new(move |_| {
        // Direct image URL takes precedence
        if let Some(ref url_signal) = image_url {
            let url = url_signal.get();
            if !url.is_empty() {
                return url;
            }
        }

        // Fall back to IIIF URL generation from asset-id (use large size for detail view)
        if let Some(ref id_signal) = asset_id {
            let id = id_signal.get();
            if !id.is_empty() {
                if let Some(url) = generate_iiif_url(&id, IiifSize::Large) {
                    return url;
                }
            }
        }

        String::new()
    });

    // Resolve thumbnail URL for blurred placeholder
    let placeholder_url = Memo::new(move |_| {
        // Use provided thumbnail if available
        if let Some(ref thumb_signal) = thumbnail_url {
            let url = thumb_signal.get();
            if !url.is_empty() {
                return Some(url);
            }
        }

        // Generate small IIIF thumbnail from asset_id
        if let Some(ref id_signal) = asset_id {
            let id = id_signal.get();
            if !id.is_empty() {
                return generate_iiif_url(&id, IiifSize::Thumb);
            }
        }

        None
    });

    // Reset loaded state when asset changes
    Effect::new(move |_| {
        let _ = high_res_url.get();
        set_high_res_loaded.set(false);
    });

    // Accent bar style
    let accent_style = move || {
        accent_color
            .as_ref()
            .map(|c| c.get())
            .filter(|c| !c.is_empty())
            .map(|c| format!("background-color: {c};"))
    };

    // Render children slot (footer content)
    let footer_content = children.map(|c| c());

    view! {
        <div class="asset-detail-card">
            // Accent bar (optional)
            {move || accent_style().map(|style| view! {
                <div class="asset-detail-card__accent" style=style></div>
            })}

            // Close button (if on_close provided)
            {on_close.map(|cb| view! {
                <button
                    class="asset-detail-card__close"
                    on:click=move |_| cb.run(())
                    aria-label="Close"
                >
                    "×"
                </button>
            })}

            // Image section with progressive loading
            <div class="asset-detail-card__image">
                // Blurred placeholder (thumbnail) - hidden once high-res loads
                {move || placeholder_url.get().map(|url| view! {
                    <img
                        class="asset-detail-card__placeholder"
                        class:loaded=move || high_res_loaded.get()
                        src=url
                        alt=""
                        aria-hidden="true"
                    />
                })}

                // High-res image - fades in when loaded
                <img
                    class="asset-detail-card__highres"
                    class:loaded=move || high_res_loaded.get()
                    src=move || high_res_url.get()
                    alt=move || name.get()
                    on:load=move |_| set_high_res_loaded.set(true)
                />

                // Rarity rank pill overlay
                {move || rarity_rank.get().map(|rank| view! {
                    <span class="asset-detail-card__rarity">
                        {"Rank #"}{rank}
                    </span>
                })}
            </div>

            // Info section
            <div class="asset-detail-card__info">
                // Header with name
                <div class="asset-detail-card__header">
                    <h2 class="asset-detail-card__name">{move || name.get()}</h2>
                </div>

                // Traits section
                <div class="asset-detail-card__traits">
                    <For
                        each=move || {
                            let mut traits_vec: Vec<_> = traits.get().into_iter().collect();
                            traits_vec.sort_by(|a, b| a.0.cmp(&b.0));
                            traits_vec
                        }
                        key=|(name, _)| name.clone()
                        let:trait_entry
                    >
                        <TraitRow
                            name=trait_entry.0
                            values=trait_entry.1
                        />
                    </For>
                </div>

                // Footer (always present for visual balance, contains children if provided)
                <div class="asset-detail-card__footer">
                    {footer_content}
                </div>
            </div>
        </div>
    }
}

/// Single trait row component
#[component]
fn TraitRow(name: String, values: Vec<String>) -> impl IntoView {
    let display_value = values.join(", ");

    view! {
        <div class="asset-detail-card__trait">
            <span class="asset-detail-card__trait-name">{name}</span>
            <span class="asset-detail-card__trait-value">{display_value}</span>
        </div>
    }
}
