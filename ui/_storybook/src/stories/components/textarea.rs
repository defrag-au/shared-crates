//! Textarea component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::Textarea;

#[component]
pub fn TextareaStory() -> impl IntoView {
    let (bio, set_bio) = signal(String::new());
    let (notes, set_notes) = signal("Some existing notes...".to_string());
    let (disabled_value, _) = signal("This content is read-only.".to_string());

    view! {
        <div>
            <div class="story-header">
                <h2>"Textarea"</h2>
                <p>"A styled multi-line text input for longer content."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <div class="story-canvas">
                    <div style="max-width: 400px;">
                        <Textarea
                            value=bio
                            on_change=Callback::new(move |v| set_bio.set(v))
                            label="Bio"
                            placeholder="Tell us about yourself..."
                        />
                        <p style="margin-top: 0.5rem; color: #888; font-size: 0.875rem;">
                            "Characters: " {move || bio.get().len()}
                        </p>
                    </div>
                </div>
            </div>

            // Different row counts
            <div class="story-section">
                <h3>"Row Counts"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 400px;">
                        <Textarea
                            value=notes
                            on_change=Callback::new(move |v| set_notes.set(v))
                            label="Small (3 rows)"
                            rows=3
                        />
                        <Textarea
                            value=notes
                            on_change=Callback::new(move |v| set_notes.set(v))
                            label="Large (8 rows)"
                            rows=8
                        />
                    </div>
                </div>
            </div>

            // Disabled state
            <div class="story-section">
                <h3>"Disabled State"</h3>
                <div class="story-canvas">
                    <div style="max-width: 400px;">
                        <Textarea
                            value=disabled_value
                            on_change=Callback::new(|_| {})
                            label="Disabled Textarea"
                            disabled=Signal::derive(|| true)
                        />
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="value"
                            values="Signal<String>"
                            description="Current textarea value"
                        />
                        <AttributeCard
                            name="on_change"
                            values="Callback<String>"
                            description="Callback when value changes"
                        />
                        <AttributeCard
                            name="label"
                            values="String (optional)"
                            description="Label text above the textarea"
                        />
                        <AttributeCard
                            name="placeholder"
                            values="String (optional)"
                            description="Placeholder text"
                        />
                        <AttributeCard
                            name="rows"
                            values="u32 (default: 4)"
                            description="Number of visible text rows"
                        />
                        <AttributeCard
                            name="disabled"
                            values="Signal<bool> (optional)"
                            description="Whether textarea is disabled"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::Textarea;

let (content, set_content) = signal(String::new());

view! {
    <Textarea
        value=content
        on_change=Callback::new(move |v| set_content.set(v))
        label="Description"
        placeholder="Enter description..."
        rows=6
    />
}"##}</pre>
            </div>
        </div>
    }
}
