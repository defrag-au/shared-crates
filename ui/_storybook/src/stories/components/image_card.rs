//! Image Card component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{CardSize, ImageCard};

#[component]
pub fn ImageCardStory() -> impl IntoView {
    let (click_count, set_click_count) = signal(0u32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Image Card"</h2>
                <p>"A basic card component for displaying images with optional name overlay and accent color. Foundation for other card components."</p>
            </div>

            // Examples section
            <div class="story-section">
                <h3>"Examples"</h3>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(4, 120px); gap: 1rem;">
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465313839/full/400,/0/default.jpg"
                            name="Basic Card"
                        />
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465323030/full/400,/0/default.jpg"
                            name="With Name"
                            show_name=true
                        />
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465333333/full/400,/0/default.jpg"
                            name="Gold Accent"
                            show_name=true
                            accent_color="#FFD700"
                        />
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465343434/full/400,/0/default.jpg"
                            name="Static"
                            show_name=true
                            is_static=true
                        />
                    </div>
                </div>
            </div>

            // Size Variants section
            <div class="story-section">
                <h3>"Size Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; align-items: flex-end; gap: 1rem; flex-wrap: wrap;">
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465313839/full/400,/0/default.jpg"
                            name="xs (80px)"
                            size=CardSize::Xs
                            show_name=true
                        />
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465323030/full/400,/0/default.jpg"
                            name="sm (120px)"
                            size=CardSize::Sm
                            show_name=true
                        />
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465333333/full/400,/0/default.jpg"
                            name="md (240px)"
                            size=CardSize::Md
                            show_name=true
                        />
                    </div>
                    <div style="display: flex; align-items: flex-end; gap: 1rem; margin-top: 1rem; flex-wrap: wrap;">
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465343434/full/400,/0/default.jpg"
                            name="lg (400px)"
                            size=CardSize::Lg
                            show_name=true
                        />
                    </div>
                </div>
            </div>

            // Interactive Demo section
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 1rem;">
                        <ImageCard
                            image_url="https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465353535/full/400,/0/default.jpg"
                            name="Click me!"
                            show_name=true
                            on_click=move || set_click_count.update(|c| *c += 1)
                        />
                        <span class="status-indicator status-indicator--connected">
                            {move || if click_count.get() == 0 {
                                "Click the card!".to_string()
                            } else {
                                format!("Clicks: {}", click_count.get())
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
                            name="image_url"
                            values="String"
                            description="URL of the image to display"
                        />
                        <AttributeCard
                            name="name"
                            values="String"
                            description="Name shown in overlay and as tooltip"
                        />
                        <AttributeCard
                            name="size"
                            values="CardSize (Xs|Sm|Md|Lg|Xl)"
                            description="Card size: 80px, 120px (default), 240px, 400px, 800px"
                        />
                        <AttributeCard
                            name="show_name"
                            values="bool"
                            description="If true, shows the name overlay"
                        />
                        <AttributeCard
                            name="accent_color"
                            values="String (CSS color)"
                            description="Color for the accent bar at top"
                        />
                        <AttributeCard
                            name="is_static"
                            values="bool"
                            description="If true, disables hover effects and clicks"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{ImageCard, CardSize};

// Basic image card (default sm size)
view! {
    <ImageCard
        image_url="https://example.com/image.png"
        name="My Image"
    />
}

// Medium size with name overlay
view! {
    <ImageCard
        image_url="https://..."
        name="Featured"
        size=CardSize::Md
        show_name=true
    />
}

// Large card with accent color and click handler
view! {
    <ImageCard
        image_url="https://..."
        name="Hero"
        size=CardSize::Lg
        show_name=true
        accent_color="#FFD700"
        on_click=move || handle_click()
    />
}"##}</pre>
            </div>
        </div>
    }
}
