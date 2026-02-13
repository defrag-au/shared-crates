//! Asset Picker component story

use crate::api::pfp_city::{fetch_collection_assets, KNOWN_COLLECTIONS};
use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{AssetCard, AssetPicker, Badge, BadgeSize, CardSize, PickerAsset};

#[component]
pub fn AssetPickerStory() -> impl IntoView {
    let (show_basic, set_show_basic) = signal(false);
    let (show_with_disabled, set_show_with_disabled) = signal(false);
    let (show_live, set_show_live) = signal(false);
    let (selected_asset, set_selected_asset) = signal(None::<String>);

    // Static sample assets for basic demos
    let sample_assets = Signal::derive(|| {
        vec![
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                    .to_string(),
                name: "Pirate #189".to_string(),
                power: Some(285),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                    .to_string(),
                name: "Pirate #200".to_string(),
                power: Some(312),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                    .to_string(),
                name: "Pirate #333".to_string(),
                power: Some(245),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465343434"
                    .to_string(),
                name: "Pirate #434".to_string(),
                power: Some(198),
                available: true,
                unavailable_reason: None,
            },
        ]
    });

    // Assets with some disabled
    let mixed_assets = Signal::derive(|| {
        vec![
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                    .to_string(),
                name: "Pirate #189".to_string(),
                power: Some(285),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                    .to_string(),
                name: "Pirate #200".to_string(),
                power: Some(312),
                available: false,
                unavailable_reason: Some("Assigned".to_string()),
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                    .to_string(),
                name: "Pirate #333".to_string(),
                power: Some(245),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465343434"
                    .to_string(),
                name: "Pirate #434".to_string(),
                power: Some(198),
                available: false,
                unavailable_reason: Some("Deployed".to_string()),
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465353535"
                    .to_string(),
                name: "Pirate #535".to_string(),
                power: Some(367),
                available: true,
                unavailable_reason: None,
            },
            PickerAsset {
                id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465363636"
                    .to_string(),
                name: "Pirate #636".to_string(),
                power: Some(421),
                available: false,
                unavailable_reason: Some("In Combat".to_string()),
            },
        ]
    });

    // Render function for basic picker
    let render_basic = move |asset: PickerAsset, on_click: Callback<()>| {
        let id = asset.id.clone();
        view! {
            <AssetCard
                asset_id=Signal::derive(move || id.clone())
                name=Signal::derive({
                    let name = asset.name.clone();
                    move || name.clone()
                })
                size=CardSize::Sm
                show_name=true
                on_click=Callback::new(move |_| on_click.run(()))
            />
        }
    };

    // Render function with badges for unavailable reasons
    let render_with_badges = move |asset: PickerAsset, on_click: Callback<()>| {
        let id = asset.id.clone();
        let is_available = asset.available;
        let reason = asset.unavailable_reason.clone();

        view! {
            <div style="position: relative;">
                <AssetCard
                    asset_id=Signal::derive(move || id.clone())
                    name=Signal::derive({
                        let name = asset.name.clone();
                        move || name.clone()
                    })
                    size=CardSize::Sm
                    show_name=true
                    is_static=!is_available
                    on_click=Callback::new(move |_| if is_available { on_click.run(()) })
                />
                {reason.map(|r| view! {
                    <div style="position: absolute; top: 4px; left: 4px;">
                        <Badge label=r size=BadgeSize::Xs />
                    </div>
                })}
            </div>
        }
    };

    let on_select_basic = {
        let set_selected = set_selected_asset;
        Callback::new(move |id: String| {
            set_selected.set(Some(id));
            set_show_basic.set(false);
        })
    };

    let on_select_mixed = {
        let set_selected = set_selected_asset;
        Callback::new(move |id: String| {
            set_selected.set(Some(id));
            set_show_with_disabled.set(false);
        })
    };

    view! {
        <div>
            <div class="story-header">
                <h2>"Asset Picker"</h2>
                <p>"A modal for selecting NFT assets from a grid. Supports disabled states for unavailable assets, click-to-select, and custom rendering."</p>
            </div>

            // Selected asset display
            {move || selected_asset.get().map(|id| view! {
                <div class="story-section">
                    <div class="selected-asset-banner">
                        "Selected: " <code>{id}</code>
                    </div>
                </div>
            })}

            // Interactive Demos
            <div class="story-section">
                <h3>"Interactive Demos"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap;">
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_basic.set(true)
                        >
                            "Open Basic Picker"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_with_disabled.set(true)
                        >
                            "Open With Disabled Items"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_live.set(true)
                        >
                            "Open Live API Demo (200 assets)"
                        </button>
                    </div>
                </div>
            </div>

            // Basic Picker Modal
            <AssetPicker
                open=Signal::derive(move || show_basic.get())
                title="Select Crew Member"
                assets=sample_assets
                on_select=on_select_basic
                on_close=Callback::new(move |_| set_show_basic.set(false))
                render_asset=render_basic
            />

            // Picker with disabled items
            <AssetPicker
                open=Signal::derive(move || show_with_disabled.get())
                title="Select Available Crew"
                assets=mixed_assets
                empty_message="No crew members available"
                on_select=on_select_mixed
                on_close=Callback::new(move |_| set_show_with_disabled.set(false))
                render_asset=render_with_badges
            />

            // Live API Demo Modal
            <LiveApiPickerDemo
                open=Signal::derive(move || show_live.get())
                on_close=Callback::new(move |_| set_show_live.set(false))
                on_select=Callback::new({
                    let set_selected = set_selected_asset;
                    move |id: String| {
                        set_selected.set(Some(id));
                        set_show_live.set(false);
                    }
                })
            />

            // Features section
            <div class="story-section">
                <h3>"Features"</h3>
                <div class="story-canvas">
                    <ul style="margin: 0; padding-left: 1.5rem; line-height: 1.8;">
                        <li>"Click to select - no separate button needed"</li>
                        <li>"Disabled items are greyed out and non-clickable"</li>
                        <li>"Custom render function for full control over card display"</li>
                        <li>"Built-in loading and empty states"</li>
                        <li>"Optional header slot for filters or other controls"</li>
                        <li>"Uses AssetGrid internally for responsive layout"</li>
                    </ul>
                </div>
            </div>

            // Props Reference
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="open"
                            values="Signal<bool>"
                            description="Controls whether the modal is visible"
                        />
                        <AttributeCard
                            name="title"
                            values="String"
                            description="Modal title text"
                        />
                        <AttributeCard
                            name="assets"
                            values="Signal<Vec<PickerAsset>>"
                            description="List of assets to display in the grid"
                        />
                        <AttributeCard
                            name="loading"
                            values="Signal<bool> (optional)"
                            description="Shows loading spinner when true"
                        />
                        <AttributeCard
                            name="empty_message"
                            values="String (optional)"
                            description="Message shown when assets list is empty"
                        />
                        <AttributeCard
                            name="on_select"
                            values="Callback<String>"
                            description="Called with asset ID when an available asset is clicked"
                        />
                        <AttributeCard
                            name="on_close"
                            values="Callback<()>"
                            description="Called when modal should close"
                        />
                        <AttributeCard
                            name="header"
                            values="Option<ChildrenFn>"
                            description="Optional header content (filters, buttons, etc.)"
                        />
                        <AttributeCard
                            name="render_asset"
                            values="Fn(PickerAsset, Callback<()>) -> impl IntoView"
                            description="Render function for each asset card"
                        />
                    </div>
                </div>
            </div>

            // PickerAsset struct
            <div class="story-section">
                <h3>"PickerAsset Struct"</h3>
                <pre class="code-block">{r##"pub struct PickerAsset {
    /// Unique identifier (typically asset_id string)
    pub id: String,
    /// Display name
    pub name: String,
    /// Optional power value
    pub power: Option<u32>,
    /// Whether this asset is available for selection
    pub available: bool,
    /// Optional reason why unavailable (e.g., "Assigned", "In Combat")
    pub unavailable_reason: Option<String>,
}"##}</pre>
            </div>

            // Usage Example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{AssetPicker, AssetCard, PickerAsset, CardSize};

let (show_picker, set_show_picker) = signal(false);

// Build picker assets from your data
let picker_assets = Signal::derive(move || {
    my_assets.get().into_iter().map(|asset| {
        PickerAsset {
            id: asset.id.to_string(),
            name: asset.name,
            power: asset.power,
            available: !assigned_ids.contains(&asset.id),
            unavailable_reason: if assigned_ids.contains(&asset.id) {
                Some("Assigned".to_string())
            } else {
                None
            },
        }
    }).collect()
});

// Custom render function
let render_asset = |asset: PickerAsset, on_click: Callback<()>| {
    let id = asset.id.clone();
    view! {
        <AssetCard
            asset_id=Signal::derive(move || id.clone())
            name=asset.name.clone()
            size=CardSize::Sm
            show_name=true
            is_static=!asset.available
            on_click=move |_| on_click.run(())
        />
    }
};

view! {
    <button on:click=move |_| set_show_picker.set(true)>
        "Select Asset"
    </button>

    <AssetPicker
        open=show_picker
        title="Select Crew Member"
        assets=picker_assets
        on_select=Callback::new(|id| { /* handle selection */ })
        on_close=Callback::new(move || set_show_picker.set(false))
        render_asset=render_asset
    />
}"##}</pre>
            </div>

            // Inline styles
            <style>{r#"
                .selected-asset-banner {
                    padding: 0.75rem 1rem;
                    background: rgba(74, 158, 255, 0.1);
                    border: 1px solid rgba(74, 158, 255, 0.3);
                    border-radius: 4px;
                    font-size: 0.875rem;
                }
                .selected-asset-banner code {
                    background: rgba(0, 0, 0, 0.2);
                    padding: 0.125rem 0.375rem;
                    border-radius: 3px;
                    font-size: 0.75rem;
                }
            "#}</style>
        </div>
    }
}

/// Live API picker demo - loads 200 assets with loading state
#[component]
fn LiveApiPickerDemo(
    #[prop(into)] open: Signal<bool>,
    on_close: Callback<()>,
    on_select: Callback<String>,
) -> impl IntoView {
    let (assets, set_assets) = signal(Vec::<PickerAsset>::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(None::<String>);

    // Fetch assets when modal opens
    Effect::new(move |prev_open: Option<bool>| {
        let is_open = open.get();
        let was_open = prev_open.unwrap_or(false);

        // Only fetch when opening (not when already open)
        if is_open && !was_open {
            set_loading.set(true);
            set_error.set(None);
            set_assets.set(Vec::new());

            wasm_bindgen_futures::spawn_local(async move {
                // Fetch 200 assets from Black Flag Pirates
                let policy_id = KNOWN_COLLECTIONS[0].0;

                match fetch_collection_assets(policy_id, 200, 0).await {
                    Ok((fetched, _pagination)) => {
                        // Convert to PickerAsset, marking some as unavailable for demo
                        let picker_assets: Vec<PickerAsset> = fetched
                            .into_iter()
                            .enumerate()
                            .map(|(i, asset)| {
                                // Mark every 5th asset as unavailable for demo purposes
                                let unavailable = i % 5 == 3;
                                PickerAsset {
                                    id: asset.id.concatenated(),
                                    name: asset.name,
                                    power: Some(150 + (i as u32 * 7) % 300), // Fake power for demo
                                    available: !unavailable,
                                    unavailable_reason: if unavailable {
                                        Some("Assigned".to_string())
                                    } else {
                                        None
                                    },
                                }
                            })
                            .collect();

                        set_assets.set(picker_assets);
                        set_error.set(None);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                    }
                }
                set_loading.set(false);
            });
        }

        is_open
    });

    let render_asset = move |asset: PickerAsset, on_click: Callback<()>| {
        let id = asset.id.clone();
        let is_available = asset.available;
        let reason = asset.unavailable_reason.clone();

        view! {
            <div style="position: relative;">
                <AssetCard
                    asset_id=Signal::derive(move || id.clone())
                    name=Signal::derive({
                        let name = asset.name.clone();
                        move || name.clone()
                    })
                    size=CardSize::Sm
                    show_name=true
                    is_static=!is_available
                    on_click=Callback::new(move |_| if is_available { on_click.run(()) })
                />
                {reason.map(|r| view! {
                    <div style="position: absolute; top: 4px; left: 4px;">
                        <Badge label=r size=BadgeSize::Xs />
                    </div>
                })}
            </div>
        }
    };

    // Build empty message from error or default
    let empty_msg =
        Signal::derive(move || error.get().unwrap_or_else(|| "No assets found".to_string()));

    view! {
        <AssetPicker
            open=open
            title="Select Pirate (Live API - 200 assets)"
            assets=Signal::derive(move || assets.get())
            loading=Signal::derive(move || loading.get())
            empty_message=empty_msg.get()
            on_select=on_select
            on_close=on_close
            render_asset=render_asset
        />
    }
}
