//! Asset Picker Component
//!
//! A modal for selecting NFT assets from a grid. Supports:
//! - Displaying assets with optional power/status overlays
//! - Disabled state for unavailable assets (already assigned, etc.)
//! - Click to select (no extra button needed)
//! - Optional header content slot (for wage selectors, filters, etc.)
//! - Loading and empty states

use leptos::children::ChildrenFn;
use leptos::prelude::*;

use crate::{AssetGrid, Modal};

/// Represents an asset item in the picker
#[derive(Clone, Debug)]
pub struct PickerAsset {
    /// Unique identifier (typically the asset_id string)
    pub id: String,
    /// Display name
    pub name: String,
    /// Optional power value
    pub power: Option<u32>,
    /// Whether this asset is available for selection
    pub available: bool,
    /// Optional reason why unavailable (shown as badge)
    pub unavailable_reason: Option<String>,
}

/// Asset picker modal component
///
/// Generic modal for selecting NFT assets from a grid.
/// Pass a render function to customize how each asset is displayed.
#[component]
pub fn AssetPicker<F, V>(
    /// Whether the modal is open
    #[prop(into)]
    open: Signal<bool>,
    /// Modal title
    #[prop(into)]
    title: String,
    /// Assets to display
    #[prop(into)]
    assets: Signal<Vec<PickerAsset>>,
    /// Loading state
    #[prop(into, optional)]
    loading: Signal<bool>,
    /// Empty state message
    #[prop(into, optional)]
    empty_message: String,
    /// Callback when an asset is selected
    on_select: Callback<String>,
    /// Callback to close the modal
    on_close: Callback<()>,
    /// Optional header content (e.g., wage tier buttons, filters)
    #[prop(into, optional)]
    header: Option<ChildrenFn>,
    /// Render function for each asset card
    /// Receives (asset, on_click callback)
    render_asset: F,
) -> impl IntoView
where
    F: Fn(PickerAsset, Callback<()>) -> V + Clone + Send + 'static,
    V: IntoView + 'static,
{
    let empty_msg = if empty_message.is_empty() {
        "No assets available".to_string()
    } else {
        empty_message
    };

    let header_content = header.map(|h| h());

    let is_empty = Signal::derive(move || assets.get().is_empty());

    view! {
        <Modal
            open=open
            title=title
            on_close=on_close
        >
            <div class="asset-picker">
                // Optional header content
                {header_content}

                // Asset grid
                <AssetGrid
                    loading=loading
                    is_empty=is_empty
                    empty_message=empty_msg
                    min_column_width="120px"
                    gap="0.75rem"
                >
                    <For
                        each=move || assets.get()
                        key=|a| a.id.clone()
                        let:asset
                    >
                        {
                            let asset_id = asset.id.clone();
                            let is_available = asset.available;
                            let render = render_asset.clone();

                            let on_click = Callback::new(move |()| {
                                if is_available {
                                    on_select.run(asset_id.clone());
                                }
                            });

                            view! {
                                <div
                                    class="asset-picker__item"
                                    class:asset-picker__item--disabled=!is_available
                                >
                                    {render(asset, on_click)}
                                </div>
                            }
                        }
                    </For>
                </AssetGrid>
            </div>
        </Modal>
    }
}
