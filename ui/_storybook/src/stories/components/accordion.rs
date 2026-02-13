//! Accordion component story

use crate::stories::helpers::{resolve_iiif_image, AttributeCard};
use cardano_assets::AssetId;
use leptos::prelude::*;
use ui_components::{Accordion, AccordionItem};

// Black Flag Pirates policy ID
const BFP_POLICY: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

#[component]
pub fn AccordionStory() -> impl IntoView {
    // For controlled single-expand demo
    let (expanded_index, set_expanded_index) = signal(Some(0usize));

    // Generate IIIF URLs for image icon demos
    let pirate_1 = AssetId {
        policy_id: BFP_POLICY.to_string(),
        asset_name_hex: "506972617465313839".to_string(),
    };
    let pirate_2 = AssetId {
        policy_id: BFP_POLICY.to_string(),
        asset_name_hex: "506972617465323030".to_string(),
    };
    let icon_url_1 = resolve_iiif_image(&pirate_1, 32);
    let icon_url_2 = resolve_iiif_image(&pirate_2, 32);

    view! {
        <div>
            <div class="story-header">
                <h2>"Accordion"</h2>
                <p>"A collapsible accordion container with expandable items. Supports both uncontrolled (self-managed) and controlled (parent-managed) state."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Accordion"</h3>
                <p class="story-description">"Each item manages its own expanded state independently."</p>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        <Accordion>
                            <AccordionItem title="Getting Started" icon="🚀">
                                <p>"Welcome to the accordion component! Click on any header to expand or collapse the content."</p>
                            </AccordionItem>
                            <AccordionItem title="Features" icon="✨">
                                <p>"This accordion supports icons, badges, and rich content. Each item can be expanded independently."</p>
                            </AccordionItem>
                            <AccordionItem title="Customization" icon="🎨">
                                <p>"You can customize the appearance with CSS classes and control the expanded state programmatically."</p>
                            </AccordionItem>
                        </Accordion>
                    </div>
                </div>
            </div>

            // With badges
            <div class="story-section">
                <h3>"With Badges"</h3>
                <p class="story-description">"Accordion items can display badges for counts or status indicators."</p>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        <Accordion>
                            <AccordionItem title="Inbox" icon="📥" badge="12">
                                <p>"You have 12 unread messages in your inbox."</p>
                            </AccordionItem>
                            <AccordionItem title="Drafts" icon="📝" badge="3">
                                <p>"3 draft messages waiting to be sent."</p>
                            </AccordionItem>
                            <AccordionItem title="Sent" icon="📤" badge="156">
                                <p>"156 messages sent this month."</p>
                            </AccordionItem>
                        </Accordion>
                    </div>
                </div>
            </div>

            // With image icons
            <div class="story-section">
                <h3>"With Image Icons"</h3>
                <p class="story-description">"Use icon_url for custom image icons instead of emoji."</p>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        <Accordion>
                            <AccordionItem
                                title="Pirate Ship"
                                icon_url=icon_url_1.clone()
                            >
                                <p>"A mighty vessel ready for adventure on the high seas."</p>
                            </AccordionItem>
                            <AccordionItem
                                title="Treasure Map"
                                icon_url=icon_url_2.clone()
                            >
                                <p>"X marks the spot where the treasure lies buried."</p>
                            </AccordionItem>
                        </Accordion>
                    </div>
                </div>
            </div>

            // Controlled single-expand
            <div class="story-section">
                <h3>"Controlled Single-Expand"</h3>
                <p class="story-description">"Only one item can be expanded at a time. Parent controls the state."</p>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        <Accordion>
                            <AccordionItem
                                title="Section One"
                                icon="1️⃣"
                                expanded=Signal::derive(move || expanded_index.get() == Some(0))
                                on_toggle=Callback::new(move |open| set_expanded_index.set(if open { Some(0) } else { None }))
                            >
                                <p>"This is the first section. When you open another section, this one will close."</p>
                            </AccordionItem>
                            <AccordionItem
                                title="Section Two"
                                icon="2️⃣"
                                expanded=Signal::derive(move || expanded_index.get() == Some(1))
                                on_toggle=Callback::new(move |open| set_expanded_index.set(if open { Some(1) } else { None }))
                            >
                                <p>"This is the second section. Only one section can be open at a time."</p>
                            </AccordionItem>
                            <AccordionItem
                                title="Section Three"
                                icon="3️⃣"
                                expanded=Signal::derive(move || expanded_index.get() == Some(2))
                                on_toggle=Callback::new(move |open| set_expanded_index.set(if open { Some(2) } else { None }))
                            >
                                <p>"This is the third section. The parent component controls which item is expanded."</p>
                            </AccordionItem>
                        </Accordion>
                    </div>
                </div>
            </div>

            // Rich content
            <div class="story-section">
                <h3>"Rich Content"</h3>
                <p class="story-description">"Accordion items can contain any HTML content."</p>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        <Accordion>
                            <AccordionItem title="Ship Statistics" icon="📊">
                                <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 0.5rem;">
                                    <div style="padding: 0.5rem; background: rgba(255,255,255,0.05); border-radius: 4px;">
                                        <div style="font-size: 0.75rem; color: #888;">"Speed"</div>
                                        <div style="font-size: 1.25rem; font-weight: bold;">"24 knots"</div>
                                    </div>
                                    <div style="padding: 0.5rem; background: rgba(255,255,255,0.05); border-radius: 4px;">
                                        <div style="font-size: 0.75rem; color: #888;">"Crew"</div>
                                        <div style="font-size: 1.25rem; font-weight: bold;">"156"</div>
                                    </div>
                                    <div style="padding: 0.5rem; background: rgba(255,255,255,0.05); border-radius: 4px;">
                                        <div style="font-size: 0.75rem; color: #888;">"Cargo"</div>
                                        <div style="font-size: 1.25rem; font-weight: bold;">"2,400 tons"</div>
                                    </div>
                                    <div style="padding: 0.5rem; background: rgba(255,255,255,0.05); border-radius: 4px;">
                                        <div style="font-size: 0.75rem; color: #888;">"Cannons"</div>
                                        <div style="font-size: 1.25rem; font-weight: bold;">"32"</div>
                                    </div>
                                </div>
                            </AccordionItem>
                        </Accordion>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Accordion Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="class"
                            values="String (optional)"
                            description="Additional CSS classes for the accordion container"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="AccordionItem components"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"AccordionItem Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="title"
                            values="String"
                            description="Header title text"
                        />
                        <AttributeCard
                            name="icon"
                            values="String (optional)"
                            description="Emoji or text icon displayed before the title"
                        />
                        <AttributeCard
                            name="icon_url"
                            values="String (optional)"
                            description="URL for an image icon (takes precedence over icon)"
                        />
                        <AttributeCard
                            name="badge"
                            values="String (optional)"
                            description="Badge text displayed after the title"
                        />
                        <AttributeCard
                            name="expanded"
                            values="Signal<bool> (optional)"
                            description="Controlled expanded state. If not provided, uses internal state."
                        />
                        <AttributeCard
                            name="on_toggle"
                            values="Callback<bool> (optional)"
                            description="Called when item is toggled. Receives new expanded state."
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Content to show when expanded"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Accordion, AccordionItem};

// Basic uncontrolled accordion
view! {
    <Accordion>
        <AccordionItem title="Section 1" icon="🚢">
            <p>"Content for section 1"</p>
        </AccordionItem>
        <AccordionItem title="Section 2" icon="⚔️" badge="New">
            <p>"Content for section 2"</p>
        </AccordionItem>
    </Accordion>
}

// Controlled single-expand accordion
let (expanded, set_expanded) = signal(Some(0usize));

view! {
    <Accordion>
        <AccordionItem
            title="Section 1"
            expanded=Signal::derive(move || expanded.get() == Some(0))
            on_toggle=move |open| set_expanded.set(if open { Some(0) } else { None })
        >
            <p>"Content"</p>
        </AccordionItem>
        <AccordionItem
            title="Section 2"
            expanded=Signal::derive(move || expanded.get() == Some(1))
            on_toggle=move |open| set_expanded.set(if open { Some(1) } else { None })
        >
            <p>"Content"</p>
        </AccordionItem>
    </Accordion>
}"##}</pre>
            </div>
        </div>
    }
}
