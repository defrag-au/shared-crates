//! Policy Folder Component
//!
//! A collapsible folder for displaying NFTs grouped by policy ID.
//! Supports pagination for large collections.
//!
//! ## Features
//!
//! - Collapsible with expand/collapse toggle
//! - Pagination with page controls
//! - Optional custom title (for known collection names)
//! - Controlled or uncontrolled expansion state
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::PolicyFolder;
//! use wallet_pallas::PolicyGroup;
//!
//! <PolicyFolder
//!     group=policy_group
//!     title="Unsigned Algorithms"
//!     page_size=15
//! />
//! ```

use crate::asset_card::AssetCard;
use crate::image_card::CardSize;
use crate::pagination::{use_adaptive_pagination, Pagination};
use leptos::prelude::*;
use wallet_pallas::{NativeToken, PolicyGroup};

/// Collapsible folder for displaying NFTs from a single policy
#[component]
pub fn PolicyFolder(
    /// Policy group data (contains policy_id, tokens, nft_count)
    group: PolicyGroup,

    /// Optional title override (for when we know the collection name)
    #[prop(into, optional)]
    title: Option<String>,

    /// Number of rows per page (columns are detected automatically)
    #[prop(optional, default = 3)]
    rows_per_page: usize,

    /// Controlled expanded state (optional - uses internal state if not provided)
    #[prop(into, optional)]
    expanded: Option<Signal<bool>>,

    /// Callback when folder is toggled
    #[prop(into, optional)]
    on_toggle: Option<Callback<bool>>,

    /// Callback when an asset is clicked (receives asset_id and display_name)
    #[prop(into, optional)]
    on_asset_click: Option<Callback<(String, String)>>,

    /// Additional CSS class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    // Internal expansion state (used if not controlled)
    let (internal_expanded, set_internal_expanded) = signal(false);

    // Use controlled or internal state
    let is_expanded = move || {
        expanded
            .map(|s| s.get())
            .unwrap_or_else(|| internal_expanded.get())
    };

    // Toggle handler
    let toggle = move |_| {
        let new_state = !is_expanded();
        if expanded.is_none() {
            set_internal_expanded.set(new_state);
        }
        if let Some(cb) = on_toggle {
            cb.run(new_state);
        }
    };

    // Extract data from group
    let policy_id = group.policy_id.clone();
    let policy_id_short = group.policy_id_short.clone();
    let display_title = title.unwrap_or_else(|| policy_id_short.clone());
    let nfts: Vec<NativeToken> = group.nfts().into_iter().cloned().collect();
    let total_count = nfts.len();

    // Grid ref for adaptive pagination
    let grid_ref = NodeRef::<leptos::html::Div>::new();

    // Adaptive pagination - detects grid columns and adjusts page size
    let pagination = use_adaptive_pagination(total_count, grid_ref, rows_per_page, None);

    let wrapper_class = move || {
        let mut classes = vec!["ui-policy-folder"];
        if is_expanded() {
            classes.push("ui-policy-folder--expanded");
        }
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    view! {
        <div class=wrapper_class>
            <button class="ui-policy-folder__header" on:click=toggle>
                <span class="ui-policy-folder__icon">
                    {move || if is_expanded() { "▼" } else { "▶" }}
                </span>
                <span class="ui-policy-folder__title">{display_title.clone()}</span>
                <span class="ui-policy-folder__count">{total_count} " NFTs"</span>
            </button>

            <Show when=move || is_expanded()>
                {
                    let policy_for_display = policy_id.clone();
                    let has_multiple_pages = pagination.total_pages() > 1;

                    view! {
                        <div class="ui-policy-folder__content">
                            // Pagination at top (if more than one page)
                            <Show when=move || has_multiple_pages fallback=|| ()>
                                <div class="ui-policy-folder__pagination">
                                    <Pagination
                                        state=pagination
                                        show_page_jump=false
                                    />
                                </div>
                            </Show>

                            <div class="ui-policy-folder__grid" node_ref=grid_ref>
                                <For
                                    each={
                                        let nfts = nfts.clone();
                                        move || pagination.page_items(&nfts)
                                    }
                                    key=|nft| nft.asset_id()
                                    let:nft
                                >
                                    {
                                        if let Some(cb) = on_asset_click {
                                            view! { <NftCard nft=nft on_click=cb /> }.into_any()
                                        } else {
                                            view! { <NftCard nft=nft /> }.into_any()
                                        }
                                    }
                                </For>
                            </div>

                            // Pagination at bottom (if more than one page)
                            <Show when=move || has_multiple_pages fallback=|| ()>
                                <div class="ui-policy-folder__pagination">
                                    <Pagination state=pagination />
                                </div>
                            </Show>

                            // Full policy ID for reference
                            <div class="ui-policy-folder__policy-id">
                                <span class="ui-policy-folder__policy-label">"Policy ID:"</span>
                                <code class="ui-policy-folder__policy-value">{policy_for_display.clone()}</code>
                            </div>
                        </div>
                    }
                }
            </Show>
        </div>
    }
}

/// Internal helper component for rendering a single NFT card
/// Extracted to simplify the For loop and ensure proper component lifecycle
#[component]
fn NftCard(
    nft: NativeToken,
    #[prop(optional)] on_click: Option<Callback<(String, String)>>,
) -> impl IntoView {
    let asset_id = nft.asset_id();
    let name = nft.display_name();

    if let Some(cb) = on_click {
        let id = asset_id.clone();
        let n = name.clone();
        let click_handler = Callback::new(move |_: String| {
            cb.run((id.clone(), n.clone()));
        });

        view! {
            <AssetCard
                asset_id=asset_id
                name=name
                size=CardSize::Auto
                show_name=true
                is_static=false
                on_click=click_handler
            />
        }
        .into_any()
    } else {
        view! {
            <AssetCard
                asset_id=asset_id
                name=name
                size=CardSize::Auto
                show_name=true
                is_static=true
            />
        }
        .into_any()
    }
}
