//! Asset Modal Component
//!
//! A modal for displaying NFT assets in high resolution.
//!
//! ## Features
//!
//! - Fixed 1:1 aspect ratio container (no layout shift)
//! - Loading spinner while image loads
//! - High-res IIIF image (1686px)
//! - Displays asset name and ID
//! - Click backdrop or close button to dismiss
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::AssetModal;
//! use cardano_assets::AssetId;
//!
//! let (selected, set_selected) = signal(Option::<AssetId>::None);
//!
//! // When user clicks an asset
//! set_selected.set(Some(asset_id));
//!
//! // Render modal when selected
//! {move || selected.get().map(|asset_id| view! {
//!     <AssetModal
//!         asset_id=asset_id
//!         on_close=Callback::new(move |_| set_selected.set(None))
//!     />
//! })}
//! ```

use cardano_assets::AssetId;
use leptos::prelude::*;

/// Default large image size for IIIF requests
const LARGE_IMAGE_SIZE: u16 = 1686;
/// Preview image size (loaded first with blur)
const PREVIEW_IMAGE_SIZE: u16 = 400;

/// Modal for displaying an NFT asset in high resolution
#[component]
pub fn AssetModal(
    /// The asset to display
    asset_id: AssetId,

    /// Optional display name override (defaults to derived name from asset_id)
    #[prop(into, optional)]
    name: Option<String>,

    /// Callback when modal is closed
    #[prop(into)]
    on_close: Callback<()>,

    /// Image size in pixels (default 1686)
    #[prop(optional, default = LARGE_IMAGE_SIZE)]
    size: u16,
) -> impl IntoView {
    let (preview_loaded, set_preview_loaded) = signal(false);
    let (full_loaded, set_full_loaded) = signal(false);

    // Build IIIF URLs for both sizes
    let base_url = format!(
        "https://iiif.hodlcroft.com/iiif/3/{}:{}/full",
        asset_id.policy_id(),
        asset_id.asset_name_hex(),
    );
    let preview_url = format!("{}/{},/0/default.jpg", base_url, PREVIEW_IMAGE_SIZE);
    let full_url = format!("{}/{},/0/default.jpg", base_url, size);

    // Derive display name - strip CIP-67 and split PascalCase
    let display_name = name.unwrap_or_else(|| {
        let stripped = asset_id.strip_cip67();
        let raw_name = stripped.asset_name();
        split_pascal_case(&raw_name)
    });

    // Short asset ID for display
    let asset_id_str = asset_id.concatenated();
    let short_id = if asset_id_str.len() > 20 {
        format!(
            "{}...{}",
            &asset_id_str[..12],
            &asset_id_str[asset_id_str.len() - 8..]
        )
    } else {
        asset_id_str
    };

    let on_close_backdrop = on_close;
    let on_close_button = on_close;

    view! {
        <div class="ui-asset-modal-backdrop" on:click=move |_| on_close_backdrop.run(())>
            <div class="ui-asset-modal" on:click=|e| e.stop_propagation()>
                <button
                    class="ui-asset-modal__close"
                    on:click=move |_| on_close_button.run(())
                    aria-label="Close"
                >
                    "×"
                </button>

                <div class="ui-asset-modal__image-container">
                    // Preview image (blurred while full image loads)
                    <img
                        class="ui-asset-modal__preview"
                        class:ui-asset-modal__preview--visible=move || preview_loaded.get() && !full_loaded.get()
                        src=preview_url
                        alt=""
                        on:load=move |_| set_preview_loaded.set(true)
                    />

                    // Full resolution image
                    <img
                        class="ui-asset-modal__image"
                        class:ui-asset-modal__image--loaded=move || full_loaded.get()
                        src=full_url
                        alt=display_name.clone()
                        on:load=move |_| set_full_loaded.set(true)
                    />
                </div>

                <div class="ui-asset-modal__info">
                    // Indeterminate progress bar (absolute positioned at top of info)
                    <div
                        class="ui-asset-modal__progress"
                        class:ui-asset-modal__progress--hidden=move || full_loaded.get()
                    ></div>
                    <h3 class="ui-asset-modal__name">{display_name}</h3>
                    <code class="ui-asset-modal__id">{short_id}</code>
                </div>
            </div>
        </div>
    }
}

/// Split a PascalCase or camelCase string into space-separated words
fn split_pascal_case(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }

    let mut result = String::with_capacity(s.len() + 10);
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            let is_boundary = (prev.is_lowercase() && c.is_uppercase())
                || (prev.is_alphabetic()
                    && c.is_ascii_digit()
                    && (i + 1 >= chars.len()
                        || !chars[i + 1].is_ascii_digit()
                        || !prev.is_ascii_digit()))
                || (prev.is_ascii_digit() && c.is_alphabetic())
                || (prev.is_uppercase()
                    && c.is_uppercase()
                    && i + 1 < chars.len()
                    && chars[i + 1].is_lowercase());

            if is_boundary && !prev.is_whitespace() {
                result.push(' ');
            }
        }
        result.push(c);
    }

    result
}
