//! Asset Grid Component
//!
//! A responsive grid layout for displaying asset cards.
//! Handles empty state and provides configurable column/gap settings.
//!
//! ## Features
//!
//! - Responsive auto-fit grid layout
//! - Configurable min column width and gap
//! - Empty state with customizable message
//! - Loading state support
//!
//! ## Props
//!
//! - `children` - Grid content (typically `AssetCard` components)
//! - `empty_message` - Message shown when grid is empty (default: "No assets")
//! - `min_column_width` - Minimum column width for auto-fit (default: "120px")
//! - `gap` - Gap between grid items (default: "1rem")
//! - `columns` - Fixed number of columns (overrides auto-fit if set)
//! - `is_empty` - Signal indicating if grid is empty (for reliable empty state)
//! - `loading` - Show loading state instead of content
//! - `class` - Additional CSS class
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::{AssetGrid, AssetCard};
//!
//! // Basic usage - empty detection via is_empty signal
//! let assets = signal(vec![...]);
//! let is_empty = Memo::new(move |_| assets.get().is_empty());
//!
//! <AssetGrid is_empty=is_empty>
//!     <For each=move || assets.get() key=|a| a.id.clone() let:asset>
//!         <AssetCard asset_id=asset.id name=asset.name />
//!     </For>
//! </AssetGrid>
//!
//! // With custom settings
//! <AssetGrid
//!     empty_message="No crew members found"
//!     min_column_width="150px"
//!     gap="0.5rem"
//!     is_empty=is_empty
//! >
//!     {children}
//! </AssetGrid>
//!
//! // Fixed columns
//! <AssetGrid columns=4 is_empty=is_empty>
//!     {children}
//! </AssetGrid>
//!
//! // Loading state
//! <AssetGrid loading=is_loading is_empty=is_empty>
//!     {children}
//! </AssetGrid>
//! ```

use leptos::prelude::*;

/// Asset grid component for displaying collections of asset cards
#[component]
pub fn AssetGrid(
    /// Grid content (AssetCard components)
    children: Children,
    /// Signal indicating if grid is empty
    #[prop(into, optional)]
    is_empty: Signal<bool>,
    /// Message shown when grid is empty
    #[prop(into, optional, default = "No assets".into())]
    empty_message: String,
    /// Minimum column width for auto-fit layout
    #[prop(into, optional, default = "120px".into())]
    min_column_width: String,
    /// Gap between grid items
    #[prop(into, optional, default = "1rem".into())]
    gap: String,
    /// Fixed number of columns (overrides auto-fit)
    #[prop(optional)]
    columns: Option<u32>,
    /// Show loading state
    #[prop(into, optional)]
    loading: Signal<bool>,
    /// Additional CSS class
    #[prop(into, optional)]
    class: String,
) -> impl IntoView {
    // Build grid template style
    let grid_style = if let Some(cols) = columns {
        format!("grid-template-columns: repeat({cols}, 1fr); gap: {gap};")
    } else {
        format!(
            "grid-template-columns: repeat(auto-fill, minmax({min_column_width}, 1fr)); gap: {gap};"
        )
    };

    let wrapper_class = if class.is_empty() {
        "asset-grid".to_string()
    } else {
        format!("asset-grid {class}")
    };

    // Render children once upfront
    let rendered_children = children();

    view! {
        <div class=wrapper_class>
            // Loading state
            <div
                class="asset-grid__loading"
                style:display=move || if loading.get() { "flex" } else { "none" }
            >
                <div class="asset-grid__spinner"></div>
                <span>"Loading..."</span>
            </div>

            // Empty state
            <div
                class="asset-grid__empty"
                style:display=move || if !loading.get() && is_empty.get() { "block" } else { "none" }
            >
                {empty_message}
            </div>

            // Grid content
            <div
                class="asset-grid__grid"
                style=move || {
                    if loading.get() || is_empty.get() {
                        "display: none;".to_string()
                    } else {
                        grid_style.clone()
                    }
                }
            >
                {rendered_children}
            </div>
        </div>
    }
}
