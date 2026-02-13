//! ButtonGroup component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Button, ButtonGroup, ButtonVariant};

#[component]
pub fn ButtonGroupStory() -> impl IntoView {
    let (selected, set_selected) = signal("option1".to_string());

    view! {
        <div>
            <div class="story-header">
                <h2>"Button Group"</h2>
                <p>"A container that visually groups related buttons together with connected styling."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <div class="story-canvas">
                    <ButtonGroup>
                        <Button on_click=|| {}>"Left"</Button>
                        <Button on_click=|| {}>"Center"</Button>
                        <Button on_click=|| {}>"Right"</Button>
                    </ButtonGroup>
                </div>
            </div>

            // Mixed variants
            <div class="story-section">
                <h3>"Mixed Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem;">
                        <ButtonGroup>
                            <Button variant=ButtonVariant::Secondary on_click=|| {}>"Cancel"</Button>
                            <Button variant=ButtonVariant::Primary on_click=|| {}>"Save"</Button>
                        </ButtonGroup>
                        <ButtonGroup>
                            <Button variant=ButtonVariant::Ghost on_click=|| {}>"Undo"</Button>
                            <Button variant=ButtonVariant::Ghost on_click=|| {}>"Redo"</Button>
                            <Button variant=ButtonVariant::Danger on_click=|| {}>"Delete"</Button>
                        </ButtonGroup>
                    </div>
                </div>
            </div>

            // Toggle group pattern
            <div class="story-section">
                <h3>"Toggle Group Pattern"</h3>
                <p style="color: #888; margin-bottom: 1rem;">"Selected: "<code>{move || selected.get()}</code></p>
                <div class="story-canvas">
                    {move || {
                        let current = selected.get();
                        view! {
                            <ButtonGroup>
                                <Button
                                    variant=if current == "option1" { ButtonVariant::Primary } else { ButtonVariant::Secondary }
                                    on_click=move || set_selected.set("option1".into())
                                >
                                    "Option 1"
                                </Button>
                                <Button
                                    variant=if current == "option2" { ButtonVariant::Primary } else { ButtonVariant::Secondary }
                                    on_click=move || set_selected.set("option2".into())
                                >
                                    "Option 2"
                                </Button>
                                <Button
                                    variant=if current == "option3" { ButtonVariant::Primary } else { ButtonVariant::Secondary }
                                    on_click=move || set_selected.set("option3".into())
                                >
                                    "Option 3"
                                </Button>
                            </ButtonGroup>
                        }
                    }}
                </div>
            </div>

            // With icons
            <div class="story-section">
                <h3>"With Icons"</h3>
                <div class="story-canvas">
                    <ButtonGroup>
                        <Button icon="⬅️" variant=ButtonVariant::Secondary on_click=|| {}>"Prev"</Button>
                        <Button icon="🔄" variant=ButtonVariant::Secondary on_click=|| {}>"Refresh"</Button>
                        <Button icon="➡️" variant=ButtonVariant::Secondary on_click=|| {}>"Next"</Button>
                    </ButtonGroup>
                </div>
            </div>

            // Custom class
            <div class="story-section">
                <h3>"Custom Class"</h3>
                <div class="story-canvas">
                    <ButtonGroup class="my-custom-group">
                        <Button on_click=|| {}>"A"</Button>
                        <Button on_click=|| {}>"B"</Button>
                        <Button on_click=|| {}>"C"</Button>
                    </ButtonGroup>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="class"
                            values="String (optional)"
                            description="Additional CSS classes to apply"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Button components to group together"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Button, ButtonGroup, ButtonVariant};

// Basic group
view! {
    <ButtonGroup>
        <Button on_click=|| {}>"One"</Button>
        <Button on_click=|| {}>"Two"</Button>
        <Button on_click=|| {}>"Three"</Button>
    </ButtonGroup>
}

// Toggle pattern - use reactive variant
let (selected, set_selected) = signal("a");
view! {
    <ButtonGroup>
        <Button
            variant=if selected.get() == "a" {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            }
            on_click=move || set_selected.set("a")
        >
            "A"
        </Button>
        // ... more buttons
    </ButtonGroup>
}"##}</pre>
            </div>
        </div>
    }
}
