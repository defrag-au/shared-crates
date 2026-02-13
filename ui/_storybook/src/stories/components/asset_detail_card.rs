//! Asset Detail Card component story

use crate::api::pfp_city::{
    fetch_asset_details, fetch_collection_assets, AssetDetails, CollectionAsset, KNOWN_COLLECTIONS,
};
use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use std::collections::HashMap;
use ui_components::{
    generate_iiif_url, AssetCard, AssetDetailCard, AssetGrid, CardSize, IiifSize, Modal,
};

#[component]
pub fn AssetDetailCardStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Asset Detail Card"</h2>
                <p>"A detailed view for NFT assets showing traits, rarity, and actions. Click an asset in the collection viewer to see it in action."</p>
            </div>

            // Collection Viewer Demo
            <div class="story-section">
                <h3>"Collection Viewer Demo"</h3>
                <p class="story-description">"Click any asset to view its details. Fetches real trait data from PFP City API."</p>
                <div class="story-canvas">
                    <CollectionViewer />
                </div>
            </div>

            // Grid Example
            <div class="story-section">
                <h3>"Grid Layout"</h3>
                <p class="story-description">"Multiple AssetDetailCards in a grid layout."</p>
                <div class="story-canvas">
                    <GridExample />
                </div>
            </div>

            // Static Example
            <div class="story-section">
                <h3>"Static Example"</h3>
                <p class="story-description">"Single AssetDetailCard with sample trait data."</p>
                <div class="story-canvas">
                    <StaticDetailExample />
                </div>
            </div>

            // Props Reference
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="asset_id"
                            values="Signal<String>"
                            description="Cardano asset ID for IIIF URL generation (policy_id + asset_name_hex)"
                        />
                        <AttributeCard
                            name="image_url"
                            values="Signal<String>"
                            description="Direct image URL (fallback when asset_id not available)"
                        />
                        <AttributeCard
                            name="thumbnail_url"
                            values="Signal<String>"
                            description="Thumbnail URL shown blurred while high-res loads (e.g., the cached grid thumbnail)"
                        />
                        <AttributeCard
                            name="name"
                            values="Signal<String>"
                            description="Asset display name"
                        />
                        <AttributeCard
                            name="traits"
                            values="Signal<HashMap<String, Vec<String>>>"
                            description="Trait map: trait_name -> list of values"
                        />
                        <AttributeCard
                            name="rarity_rank"
                            values="Signal<Option<u32>>"
                            description="Rarity ranking (lower = rarer)"
                        />
                        <AttributeCard
                            name="accent_color"
                            values="Signal<String>"
                            description="Accent color for header bar (CSS color)"
                        />
                        <AttributeCard
                            name="actions"
                            values="Children"
                            description="Slot for action buttons"
                        />
                        <AttributeCard
                            name="on_close"
                            values="Callback<()>"
                            description="Close callback - shows X button when set"
                        />
                    </div>
                </div>
            </div>

            // Usage
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{AssetDetailCard, Button};
use std::collections::HashMap;

// Basic usage with traits
let traits = HashMap::from([
    ("Background".to_string(), vec!["Ocean".to_string()]),
    ("Hat".to_string(), vec!["Pirate".to_string()]),
]);

view! {
    <AssetDetailCard
        asset_id="b3dab69f7e..."
        name="Pirate #189"
        traits=Signal::derive(move || traits.clone())
        rarity_rank=Signal::derive(|| Some(42))
    />
}

// With close button and actions
view! {
    <AssetDetailCard
        asset_id=asset_id
        name=name
        traits=traits
        on_close=move || set_selected.set(None)
        actions=view! {
            <Button variant=ButtonVariant::Primary on_click=|| buy()>
                "Buy Now"
            </Button>
        }
    />
}

// In a modal
view! {
    <Modal open=show_detail on_close=move || set_show_detail.set(false)>
        <AssetDetailCard
            asset_id=selected_asset_id
            name=selected_name
            traits=traits
            on_close=move || set_show_detail.set(false)
        />
    </Modal>
}"##}</pre>
            </div>
        </div>
    }
}

/// Interactive collection viewer with click-to-detail
#[component]
fn CollectionViewer() -> impl IntoView {
    let (selected_collection, set_selected_collection) = signal(0usize);
    let (assets, set_assets) = signal(Vec::<CollectionAsset>::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(None::<String>);

    // Selected asset for detail view: (policy_id, asset_name_hex, concatenated_id, thumbnail_url)
    let (selected_asset, set_selected_asset) = signal(None::<(String, String, String, String)>);
    let (asset_details, set_asset_details) = signal(None::<AssetDetails>);
    let (loading_details, set_loading_details) = signal(false);

    // Fetch collection assets
    let fetch_assets = move || {
        let (policy_id, _name) = KNOWN_COLLECTIONS[selected_collection.get()];
        set_loading.set(true);
        set_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match fetch_collection_assets(policy_id, 12, 0).await {
                Ok((fetched, _pagination)) => {
                    set_assets.set(fetched);
                    set_error.set(None);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_assets.set(Vec::new());
                }
            }
            set_loading.set(false);
        });
    };

    // Initial fetch
    Effect::new(move |_| {
        fetch_assets();
    });

    // Fetch asset details when selected
    Effect::new(move |_| {
        if let Some((policy_id, asset_name_hex, _, _)) = selected_asset.get() {
            set_loading_details.set(true);
            set_asset_details.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match fetch_asset_details(&policy_id, &asset_name_hex).await {
                    Ok(details) => {
                        set_asset_details.set(Some(details));
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch asset details: {}", e);
                    }
                }
                set_loading_details.set(false);
            });
        }
    });

    let is_empty = Signal::derive(move || assets.get().is_empty());
    let show_modal = Signal::derive(move || selected_asset.get().is_some());

    let close_modal = Callback::new(move |_| {
        set_selected_asset.set(None);
        set_asset_details.set(None);
    });

    view! {
        <div>
            <div class="collection-viewer-controls">
                <select on:change=move |ev| {
                    use wasm_bindgen::JsCast;
                    let target = ev.target().unwrap();
                    let select = target.dyn_ref::<web_sys::HtmlSelectElement>().unwrap();
                    set_selected_collection.set(select.selected_index() as usize);
                    set_selected_asset.set(None);
                    fetch_assets();
                }>
                    {KNOWN_COLLECTIONS.iter().enumerate().map(|(i, (_, name))| {
                        view! {
                            <option value=i.to_string() selected=move || selected_collection.get() == i>
                                {*name}
                            </option>
                        }
                    }).collect_view()}
                </select>
                <span class="collection-viewer-status">
                    "Click an asset to view details"
                </span>
            </div>

            {move || error.get().map(|e| view! {
                <div class="collection-viewer-error">{e}</div>
            })}

            <AssetGrid
                loading=Signal::derive(move || loading.get())
                is_empty=is_empty
                empty_message="No assets found"
                min_column_width="120px"
            >
                <For
                    each=move || assets.get()
                    key=|a| a.id.concatenated()
                    let:asset
                >
                    {
                        let policy_id = asset.id.policy_id.clone();
                        let asset_name_hex = asset.id.asset_name_hex.clone();
                        let concatenated = asset.id.concatenated();
                        let name = asset.name.clone();
                        // Generate thumbnail URL (same as what AssetCard uses)
                        let thumb_url = generate_iiif_url(&concatenated, IiifSize::Thumb)
                            .unwrap_or_default();

                        view! {
                            <AssetCard
                                asset_id=concatenated.clone()
                                name=Signal::derive({
                                    let name = name.clone();
                                    move || name.clone()
                                })
                                size=CardSize::Sm
                                show_name=true
                                on_click=Callback::new({
                                    let thumb_url = thumb_url.clone();
                                    move |_id: String| {
                                        set_selected_asset.set(Some((
                                            policy_id.clone(),
                                            asset_name_hex.clone(),
                                            concatenated.clone(),
                                            thumb_url.clone(),
                                        )));
                                    }
                                })
                            />
                        }
                    }
                </For>
            </AssetGrid>

            // Detail Modal
            <Modal
                open=show_modal
                on_close=close_modal
                flush=true
            >
                {move || {
                    if loading_details.get() {
                        view! {
                            <div class="detail-loading">
                                <div class="detail-loading-spinner"></div>
                                <span>"Loading details..."</span>
                            </div>
                        }.into_any()
                    } else if let Some(details) = asset_details.get() {
                        let (_, _, concatenated_id, thumb_url) = selected_asset.get().unwrap_or_default();
                        view! {
                            <AssetDetailCard
                                asset_id=Signal::derive(move || concatenated_id.clone())
                                thumbnail_url=Signal::derive({
                                    let thumb_url = thumb_url.clone();
                                    move || thumb_url.clone()
                                })
                                name=Signal::derive({
                                    let name = details.name.clone();
                                    move || name.clone()
                                })
                                traits=Signal::derive({
                                    let traits = details.traits.clone();
                                    move || traits.clone()
                                })
                                rarity_rank=Signal::derive({
                                    let rank = details.rarity_rank;
                                    move || rank
                                })
                                on_close=close_modal
                            />
                        }.into_any()
                    } else {
                        view! {
                            <div class="detail-loading">"No details available"</div>
                        }.into_any()
                    }
                }}
            </Modal>

            <style>{r#"
                .collection-viewer-controls {
                    display: flex;
                    gap: 1rem;
                    align-items: center;
                    margin-bottom: 1rem;
                }
                .collection-viewer-controls select {
                    padding: 0.5rem;
                    border-radius: 4px;
                    background: var(--bg-secondary, #333);
                    color: var(--text-primary, #fff);
                    border: 1px solid var(--border-color, #555);
                }
                .collection-viewer-status {
                    font-size: 0.875rem;
                    color: var(--text-muted, #888);
                }
                .collection-viewer-error {
                    color: var(--danger, #dc3545);
                    padding: 0.5rem;
                    background: rgba(220, 53, 69, 0.1);
                    border-radius: 4px;
                    margin-bottom: 1rem;
                }
                .detail-loading {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    justify-content: center;
                    padding: 3rem;
                    gap: 1rem;
                    color: var(--text-muted, #888);
                }
                .detail-loading-spinner {
                    width: 2rem;
                    height: 2rem;
                    border: 3px solid var(--border-color, rgba(255, 255, 255, 0.1));
                    border-top-color: var(--primary, #0d6efd);
                    border-radius: 50%;
                    animation: detail-spin 0.8s linear infinite;
                }
                @keyframes detail-spin {
                    to { transform: rotate(360deg); }
                }
            "#}</style>
        </div>
    }
}

/// Grid example fetching one asset from each collection
#[component]
fn GridExample() -> impl IntoView {
    let (assets, set_assets) = signal(Vec::<(
        String,
        String,
        Option<u32>,
        HashMap<String, Vec<String>>,
    )>::new());
    let (loading, set_loading) = signal(true);

    // Accent colors for each collection
    let colors = ["#4a9eff", "#8b5cf6", "#ef4444"];

    // Fetch one asset from each collection
    Effect::new(move |_| {
        set_loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let mut fetched = Vec::new();

            for (policy_id, _name) in KNOWN_COLLECTIONS.iter() {
                // Get first asset from collection
                if let Ok((collection_assets, _)) = fetch_collection_assets(policy_id, 1, 0).await {
                    if let Some(asset) = collection_assets.first() {
                        // Fetch full details with traits
                        if let Ok(details) =
                            fetch_asset_details(policy_id, &asset.id.asset_name_hex).await
                        {
                            fetched.push((
                                asset.id.concatenated(),
                                details.name,
                                details.rarity_rank,
                                details.traits,
                            ));
                        }
                    }
                }
            }

            set_assets.set(fetched);
            set_loading.set(false);
        });
    });

    view! {
        <div class="detail-card-grid">
            {move || if loading.get() {
                view! {
                    <div class="grid-loading">"Loading assets from collections..."</div>
                }.into_any()
            } else {
                view! {
                    <For
                        each=move || assets.get().into_iter().enumerate()
                        key=|(_, (id, _, _, _))| id.clone()
                        let:item
                    >
                        {
                            let (idx, (asset_id, name, rank, traits)) = item;
                            let color = colors.get(idx).copied().unwrap_or("#666");
                            view! {
                                <AssetDetailCard
                                    asset_id=Signal::derive(move || asset_id.clone())
                                    name=Signal::derive(move || name.clone())
                                    traits=Signal::derive(move || traits.clone())
                                    rarity_rank=Signal::derive(move || rank)
                                    accent_color=Signal::derive(move || color.to_string())
                                />
                            }
                        }
                    </For>
                }.into_any()
            }}
        </div>
        <style>{r#"
            .detail-card-grid {
                display: grid;
                grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
                gap: 1.5rem;
            }
            .grid-loading {
                grid-column: 1 / -1;
                text-align: center;
                padding: 2rem;
                color: var(--text-muted, #888);
            }
        "#}</style>
    }
}

/// Static example with sample data
#[component]
fn StaticDetailExample() -> impl IntoView {
    let sample_traits = HashMap::from([
        ("Background".to_string(), vec!["Ocean Storm".to_string()]),
        ("Body".to_string(), vec!["Scarred".to_string()]),
        ("Hat".to_string(), vec!["Captain's Tricorn".to_string()]),
        ("Weapon".to_string(), vec!["Flintlock Pistol".to_string()]),
        ("Accessory".to_string(), vec!["Gold Earring".to_string()]),
    ]);

    view! {
        <AssetDetailCard
            asset_id=Signal::derive(|| "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839".to_string())
            name=Signal::derive(|| "Pirate #189".to_string())
            traits=Signal::derive(move || sample_traits.clone())
            rarity_rank=Signal::derive(|| Some(42))
            accent_color=Signal::derive(|| "#f4a460".to_string())
        />
    }
}
