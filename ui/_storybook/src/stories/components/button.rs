//! Button component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Button, ButtonSize, ButtonVariant};

#[component]
pub fn ButtonStory() -> impl IntoView {
    let (click_count, set_click_count) = signal(0u32);
    let (loading, set_loading) = signal(false);

    view! {
        <div>
            <div class="story-header">
                <h2>"Button"</h2>
                <p>"A versatile button component with variants, sizes, loading state, and optional icon."</p>
            </div>

            // Variants
            <div class="story-section">
                <h3>"Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap; align-items: center;">
                        <Button on_click=|| {}>"Primary"</Button>
                        <Button variant=ButtonVariant::Secondary on_click=|| {}>"Secondary"</Button>
                        <Button variant=ButtonVariant::Danger on_click=|| {}>"Danger"</Button>
                        <Button variant=ButtonVariant::Warning on_click=|| {}>"Warning"</Button>
                        <Button variant=ButtonVariant::Success on_click=|| {}>"Success"</Button>
                        <Button variant=ButtonVariant::Ghost on_click=|| {}>"Ghost"</Button>
                    </div>
                </div>
            </div>

            // Sizes
            <div class="story-section">
                <h3>"Sizes"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap; align-items: center;">
                        <Button size=ButtonSize::Sm on_click=|| {}>"Small"</Button>
                        <Button size=ButtonSize::Md on_click=|| {}>"Medium"</Button>
                        <Button size=ButtonSize::Lg on_click=|| {}>"Large"</Button>
                    </div>
                </div>
            </div>

            // With icons
            <div class="story-section">
                <h3>"With Icons"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap; align-items: center;">
                        <Button icon="+" on_click=|| {}>"Add Item"</Button>
                        <Button icon="⚡" variant=ButtonVariant::Warning on_click=|| {}>"Power Up"</Button>
                        <Button icon="🚀" variant=ButtonVariant::Success on_click=|| {}>"Launch"</Button>
                        <Button icon="🗑️" variant=ButtonVariant::Danger on_click=|| {}>"Delete"</Button>
                    </div>
                </div>
            </div>

            // States
            <div class="story-section">
                <h3>"States"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap; align-items: center;">
                        <Button on_click=|| {}>"Normal"</Button>
                        <Button disabled=true on_click=|| {}>"Disabled"</Button>
                        <Button loading=true on_click=|| {}>"Loading"</Button>
                        <Button loading=true disabled=true on_click=|| {}>"Loading + Disabled"</Button>
                    </div>
                </div>
            </div>

            // Interactive demo
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; align-items: center;">
                        <Button
                            on_click=move || set_click_count.update(|c| *c += 1)
                        >
                            {move || format!("Clicked {} times", click_count.get())}
                        </Button>
                        <Button
                            variant=ButtonVariant::Secondary
                            on_click=move || set_click_count.set(0)
                        >
                            "Reset"
                        </Button>
                    </div>
                </div>
            </div>

            // Loading demo
            <div class="story-section">
                <h3>"Loading State Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; align-items: center;">
                        <Button
                            loading=loading
                            on_click=move || set_loading.update(|l| *l = !*l)
                        >
                            {move || if loading.get() { "Processing..." } else { "Submit" }}
                        </Button>
                        <span style="color: #888;">{move || if loading.get() { "Click again to stop loading" } else { "Click to toggle loading state" }}</span>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="variant"
                            values="ButtonVariant"
                            description="Primary, Secondary, Danger, Warning, Success, Ghost"
                        />
                        <AttributeCard
                            name="size"
                            values="ButtonSize (Sm|Md|Lg)"
                            description="Button size - default is Md"
                        />
                        <AttributeCard
                            name="disabled"
                            values="impl Into<Signal<bool>>"
                            description="Whether the button is disabled"
                        />
                        <AttributeCard
                            name="loading"
                            values="impl Into<Signal<bool>>"
                            description="Shows loading spinner and disables button"
                        />
                        <AttributeCard
                            name="icon"
                            values="String (optional)"
                            description="Icon shown before button text"
                        />
                        <AttributeCard
                            name="on_click"
                            values="Callback<()>"
                            description="Called when button is clicked"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Button, ButtonVariant, ButtonSize};

// Basic button
view! {
    <Button on_click=|| handle_click()>
        "Click Me"
    </Button>
}

// Danger button with icon
view! {
    <Button
        variant=ButtonVariant::Danger
        icon="🗑️"
        on_click=|| delete_item()
    >
        "Delete"
    </Button>
}

// Small secondary button
view! {
    <Button
        variant=ButtonVariant::Secondary
        size=ButtonSize::Sm
        on_click=|| cancel()
    >
        "Cancel"
    </Button>
}

// With loading state
let (loading, set_loading) = signal(false);
view! {
    <Button
        loading=loading
        on_click=move || {
            set_loading.set(true);
            // async operation...
        }
    >
        "Submit"
    </Button>
}"##}</pre>
            </div>
        </div>
    }
}
