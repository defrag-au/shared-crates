//! Alert component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Alert, AlertVariant};

#[component]
pub fn AlertStory() -> impl IntoView {
    let (show_dismissible, set_show_dismissible) = signal(true);

    view! {
        <div>
            <div class="story-header">
                <h2>"Alert"</h2>
                <p>"A dismissible alert/message box for user feedback."</p>
            </div>

            // Variants
            <div class="story-section">
                <h3>"Variants"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 500px;">
                        <Alert variant=AlertVariant::Info>
                            "This is an informational message."
                        </Alert>
                        <Alert variant=AlertVariant::Success>
                            "Operation completed successfully!"
                        </Alert>
                        <Alert variant=AlertVariant::Warning>
                            "Please review your changes before proceeding."
                        </Alert>
                        <Alert variant=AlertVariant::Error>
                            "An error occurred. Please try again."
                        </Alert>
                    </div>
                </div>
            </div>

            // With title
            <div class="story-section">
                <h3>"With Title"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 500px;">
                        <Alert variant=AlertVariant::Success title="Success!".to_string()>
                            "Your profile has been updated."
                        </Alert>
                        <Alert variant=AlertVariant::Error title="Error".to_string()>
                            "Could not save changes. Please check your connection and try again."
                        </Alert>
                    </div>
                </div>
            </div>

            // Dismissible
            <div class="story-section">
                <h3>"Dismissible"</h3>
                <div class="story-canvas">
                    <div style="max-width: 500px;">
                        {move || if show_dismissible.get() {
                            view! {
                                <Alert
                                    variant=AlertVariant::Info
                                    title="Tip".to_string()
                                    dismissible=true
                                    on_dismiss=Callback::new(move |_| set_show_dismissible.set(false))
                                >
                                    "Click the X button to dismiss this alert."
                                </Alert>
                            }.into_any()
                        } else {
                            view! {
                                <button
                                    style="padding: 0.5rem 1rem; background: #2a2a4e; border: 1px solid #3a3a5e; border-radius: 4px; color: #e0e0e0; cursor: pointer;"
                                    on:click=move |_| set_show_dismissible.set(true)
                                >
                                    "Show Alert Again"
                                </button>
                            }.into_any()
                        }}
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
                            values="AlertVariant (Info|Success|Warning|Error)"
                            description="Visual style - default is Info"
                        />
                        <AttributeCard
                            name="title"
                            values="String (optional)"
                            description="Bold title above the message"
                        />
                        <AttributeCard
                            name="dismissible"
                            values="bool (default: false)"
                            description="Show close button"
                        />
                        <AttributeCard
                            name="on_dismiss"
                            values="Callback<()> (optional)"
                            description="Called when dismiss button clicked"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Alert message content"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Alert, AlertVariant};

// Basic alert
view! {
    <Alert variant=AlertVariant::Success>
        "Changes saved!"
    </Alert>
}

// With title and dismissible
view! {
    <Alert
        variant=AlertVariant::Warning
        title="Warning".to_string()
        dismissible=true
        on_dismiss=Callback::new(|_| hide_alert())
    >
        "This action cannot be undone."
    </Alert>
}"##}</pre>
            </div>
        </div>
    }
}
