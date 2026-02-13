//! Memory Card component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::MemoryCard;

#[component]
pub fn MemoryCardStory() -> impl IntoView {
    let (click_count, set_click_count) = signal(0u32);
    let (is_flipped, set_is_flipped) = signal(false);

    view! {
        <div>
            <div class="story-header">
                <h2>"Memory Card"</h2>
                <p>"A flippable card component for the memory matching game. Wraps AssetCard internally. Shows card back when face-down, asset image when flipped."</p>
            </div>

            // Card States section
            <div class="story-section">
                <h3>"Card States"</h3>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(4, 100px); gap: 1rem;">
                        <MemoryCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465313839"
                            name="Face Down"
                            flipped=false
                        />
                        <MemoryCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465323030"
                            name="Flipped Card"
                            flipped=true
                        />
                        <MemoryCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465333333"
                            name="Matched Card"
                            flipped=true
                            matched=true
                            matched_by="Player1"
                        />
                        <MemoryCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465343434"
                            name="Disabled"
                            flipped=false
                            disabled=true
                        />
                    </div>
                </div>
            </div>

            // Interactive Demo section
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 1rem;">
                        <MemoryCard
                            asset_id="b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6506972617465353535"
                            name="Click to flip!"
                            flipped=is_flipped
                            on_click=move || {
                                set_click_count.update(|c| *c += 1);
                                set_is_flipped.update(|f| *f = !*f);
                            }
                        />
                        <span class="status-indicator status-indicator--connected">
                            {move || format!("Clicks: {}", click_count.get())}
                        </span>
                        <button
                            class="demo-btn demo-btn--primary"
                            on:click=move |_| set_is_flipped.update(|f| *f = !*f)
                        >
                            "Toggle Flip"
                        </button>
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
                            description="Cardano asset ID (policy_id + asset_name hex) for IIIF image lookup"
                        />
                        <AttributeCard
                            name="name"
                            values="String"
                            description="Asset name shown as overlay on flipped card"
                        />
                        <AttributeCard
                            name="flipped"
                            values="bool | Signal<bool>"
                            description="Whether the card is face-up"
                        />
                        <AttributeCard
                            name="matched"
                            values="bool"
                            description="Whether the card has been matched (shows glow)"
                        />
                        <AttributeCard
                            name="matched_by"
                            values="String"
                            description="Name of player who matched this card"
                        />
                        <AttributeCard
                            name="disabled"
                            values="bool"
                            description="Whether the card can be clicked"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r#"use ui_components::MemoryCard;

// Basic card (face down)
view! {
    <MemoryCard
        asset_id="{policy_id}{asset_name_hex}"
        name="Asset Name"
    />
}

// Flipped card (face up)
view! {
    <MemoryCard
        asset_id="{policy_id}{asset_name_hex}"
        name="Asset Name"
        flipped=true
    />
}

// Matched card with player attribution
view! {
    <MemoryCard
        asset_id="{policy_id}{asset_name_hex}"
        name="Asset Name"
        flipped=true
        matched=true
        matched_by="Player1"
    />
}

// With reactive flipped state and click handler
let (flipped, set_flipped) = create_signal(false);
view! {
    <MemoryCard
        asset_id=asset_id
        name=name
        flipped=flipped
        on_click=move || set_flipped.update(|f| *f = !*f)
    />
}"#}</pre>
            </div>
        </div>
    }
}
