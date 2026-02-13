//! Asset Card component story

use crate::stories::helpers::AttributeCard;
use cardano_assets::AssetId;
use leptos::prelude::*;
use ui_components::{children_fn, AssetCard, AssetModal, Badge, CardSize, StatPill};

#[component]
pub fn AssetCardStory() -> impl IntoView {
    let (click_count, set_click_count) = signal(0u32);
    let (last_asset_id, set_last_asset_id) = signal(String::new());
    let (modal_asset, set_modal_asset) = signal(Option::<AssetId>::None);

    view! {
        <div>
            <div class="story-header">
                <h2>"Asset Card"</h2>
                <p>"Cardano NFT asset card with automatic IIIF URL generation. Wraps ImageCard and derives image URLs from asset IDs. IIIF image resolution is automatically selected based on card size."</p>
            </div>

            // Size Variants section
            <div class="story-section">
                <h3>"Size Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; align-items: flex-end; gap: 1rem; flex-wrap: wrap;">
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                            name="xs (80px)"
                            size=CardSize::Xs
                            show_name=true
                        />
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                            name="sm (120px)"
                            size=CardSize::Sm
                            show_name=true
                        />
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                            name="md (240px)"
                            size=CardSize::Md
                            show_name=true
                        />
                    </div>
                    <div style="display: flex; align-items: flex-end; gap: 1rem; margin-top: 1rem; flex-wrap: wrap;">
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465343434"
                            name="lg (400px) - 400px IIIF"
                            size=CardSize::Lg
                            show_name=true
                        />
                    </div>
                </div>
            </div>

            // Overlay Slots section
            <div class="story-section">
                <h3>"Overlay Slots"</h3>
                <p class="story-description">"AssetCard supports overlay slots at each corner and a footer slot for additional content like stats, badges, and actions."</p>
                <div class="story-canvas">
                    <div style="display: flex; align-items: flex-start; gap: 1.5rem; flex-wrap: wrap;">
                        // Card with power stat in top-right
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                            name="With Power"
                            size=CardSize::Md
                            show_name=true
                            top_right=children_fn(|| view! {
                                <StatPill value=Signal::derive(|| "285".to_string()) icon="⚡".to_string() />
                            })
                        />

                        // Card with badge in bottom-left
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                            name="With Badge"
                            size=CardSize::Md
                            show_name=true
                            bottom_left=children_fn(|| view! {
                                <Badge label="Deployed".to_string() />
                            })
                        />

                        // Card with multiple overlays
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                            name="Multiple Overlays"
                            size=CardSize::Md
                            show_name=true
                            top_right=children_fn(|| view! {
                                <StatPill value=Signal::derive(|| "350".to_string()) icon="⚡".to_string() />
                            })
                            bottom_left=children_fn(|| view! {
                                <Badge label="Captain".to_string() />
                            })
                        />
                    </div>
                </div>
            </div>

            // IIIF Info section
            <div class="story-section">
                <h3>"IIIF Image Selection"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="xs, sm, md, lg"
                            values="400px IIIF"
                            description="Card sizes up to 400px use the cached 400px IIIF image for fast loading"
                        />
                        <AttributeCard
                            name="xl"
                            values="1686px IIIF"
                            description="Card sizes above 400px use the high-resolution 1686px IIIF image"
                        />
                    </div>
                </div>
            </div>

            // Interactive Demo section
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 1rem;">
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465353535"
                            name="Click me!"
                            show_name=true
                            on_click=Callback::new(move |asset_id: String| {
                                set_click_count.update(|c| *c += 1);
                                set_last_asset_id.set(asset_id);
                            })
                        />
                        <span class="status-indicator status-indicator--connected">
                            {move || if click_count.get() == 0 {
                                "Click the card!".to_string()
                            } else {
                                format!("Clicks: {}", click_count.get())
                            }}
                        </span>
                        <span class="status-indicator">
                            {move || {
                                let id = last_asset_id.get();
                                if id.is_empty() {
                                    String::new()
                                } else {
                                    let truncated = if id.len() > 20 {
                                        format!("{}...{}", &id[..10], &id[id.len()-10..])
                                    } else {
                                        id
                                    };
                                    format!("ID: {truncated}")
                                }
                            }}
                        </span>
                    </div>
                </div>
            </div>

            // Attributes section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="asset_id"
                            values="String"
                            description="Cardano asset ID (policy_id + asset_name hex). Used for IIIF lookup and click events."
                        />
                        <AttributeCard
                            name="size"
                            values="CardSize (Xs|Sm|Md|Lg|Xl)"
                            description="Card size: 80px, 120px (default), 240px, 400px, 800px. IIIF resolution auto-selected."
                        />
                        <AttributeCard
                            name="name"
                            values="String"
                            description="Asset name (used for title tooltip and overlay)"
                        />
                        <AttributeCard
                            name="show_name"
                            values="bool"
                            description="Whether to show the name overlay at the bottom"
                        />
                        <AttributeCard
                            name="accent_color"
                            values="String (CSS color)"
                            description="Color for the accent bar at the top (e.g., tier color)"
                        />
                        <AttributeCard
                            name="is_static"
                            values="bool"
                            description="If true, disables hover effects and click events"
                        />
                    </div>
                </div>
            </div>

            // Asset Modal section
            <div class="story-section">
                <h3>"Asset Modal"</h3>
                <p class="story-description">"Click any card to open the AssetModal with progressive loading (blurred preview → sharp full image)."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 1rem;">
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                            name="Pirate 189"
                            size=CardSize::Auto
                            show_name=true
                            on_click=Callback::new(move |id: String| {
                                if let Ok(asset_id) = AssetId::parse_concatenated(&id) {
                                    set_modal_asset.set(Some(asset_id));
                                }
                            })
                        />
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                            name="Pirate 200"
                            size=CardSize::Auto
                            show_name=true
                            on_click=Callback::new(move |id: String| {
                                if let Ok(asset_id) = AssetId::parse_concatenated(&id) {
                                    set_modal_asset.set(Some(asset_id));
                                }
                            })
                        />
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                            name="Pirate 333"
                            size=CardSize::Auto
                            show_name=true
                            on_click=Callback::new(move |id: String| {
                                if let Ok(asset_id) = AssetId::parse_concatenated(&id) {
                                    set_modal_asset.set(Some(asset_id));
                                }
                            })
                        />
                        <AssetCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465343434"
                            name="Pirate 444"
                            size=CardSize::Auto
                            show_name=true
                            on_click=Callback::new(move |id: String| {
                                if let Ok(asset_id) = AssetId::parse_concatenated(&id) {
                                    set_modal_asset.set(Some(asset_id));
                                }
                            })
                        />
                    </div>
                </div>
            </div>

            // Asset Modal (rendered when an asset is selected)
            {move || modal_asset.get().map(|asset_id| view! {
                <AssetModal
                    asset_id=asset_id
                    on_close=Callback::new(move |_| set_modal_asset.set(None))
                />
            })}

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{AssetCard, CardSize};

// Small card (default) - uses 400px IIIF image
view! {
    <AssetCard
        asset_id="{policy_id}{asset_name_hex}"
        name="Pirate #189"
        show_name=true
    />
}

// Medium card - still uses 400px IIIF (cached, fast)
view! {
    <AssetCard
        asset_id="{policy_id}{asset_name_hex}"
        size=CardSize::Md
        name="Featured Pirate"
        show_name=true
    />
}

// Extra large card - auto-selects 1686px IIIF
view! {
    <AssetCard
        asset_id="{policy_id}{asset_name_hex}"
        size=CardSize::Xl
        name="Hero Display"
        show_name=true
    />
}

// With click handler
view! {
    <AssetCard
        asset_id=asset_id
        name=name
        size=CardSize::Md
        on_click=move |id: String| handle_asset_click(id)
    />
}"##}</pre>
            </div>
        </div>
    }
}
