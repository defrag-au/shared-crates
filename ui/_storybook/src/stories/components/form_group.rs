//! FormGroup component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{FormGroup, TextInput, Textarea};

#[component]
pub fn FormGroupStory() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (email, set_email) = signal(String::new());
    let (bio, set_bio) = signal(String::new());
    let (email_error, set_email_error) = signal(Option::<String>::None);

    // Validate email on change
    let validate_email = move |v: String| {
        set_email.set(v.clone());
        if !v.is_empty() && !v.contains('@') {
            set_email_error.set(Some("Please enter a valid email address".to_string()));
        } else {
            set_email_error.set(None);
        }
    };

    view! {
        <div>
            <div class="story-header">
                <h2>"FormGroup"</h2>
                <p>"A wrapper for form fields with label, error, and hint support."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <div class="story-canvas">
                    <div style="max-width: 350px; display: flex; flex-direction: column; gap: 1rem;">
                        <FormGroup label="Name" required=true>
                            <TextInput
                                value=name
                                on_change=Callback::new(move |v| set_name.set(v))
                                placeholder="Enter your name..."
                            />
                        </FormGroup>
                        <FormGroup label="Bio" hint="Optional - tell us about yourself">
                            <Textarea
                                value=bio
                                on_change=Callback::new(move |v| set_bio.set(v))
                                placeholder="Your bio..."
                                rows=3
                            />
                        </FormGroup>
                    </div>
                </div>
            </div>

            // With validation
            <div class="story-section">
                <h3>"With Validation Error"</h3>
                <div class="story-canvas">
                    <div style="max-width: 350px;">
                        <FormGroup
                            label="Email"
                            required=true
                            error=email_error
                        >
                            <TextInput
                                value=email
                                on_change=Callback::new(validate_email)
                                placeholder="email@example.com"
                            />
                        </FormGroup>
                        <p style="margin-top: 0.5rem; color: #888; font-size: 0.75rem;">
                            "Try typing without an @ symbol to see the error state"
                        </p>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="label"
                            values="String"
                            description="Field label text"
                        />
                        <AttributeCard
                            name="required"
                            values="bool (default: false)"
                            description="Shows required indicator (*)"
                        />
                        <AttributeCard
                            name="error"
                            values="Signal<Option<String>> (optional)"
                            description="Error message to display"
                        />
                        <AttributeCard
                            name="hint"
                            values="String (optional)"
                            description="Help text shown below the field"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Form control (TextInput, Textarea, Select, etc.)"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{FormGroup, TextInput};

let (email, set_email) = signal(String::new());
let (error, set_error) = signal(None::<String>);

view! {
    <FormGroup
        label="Email"
        required=true
        error=error
        hint="We'll never share your email"
    >
        <TextInput
            value=email
            on_change=Callback::new(move |v| {
                set_email.set(v.clone());
                if !v.contains('@') {
                    set_error.set(Some("Invalid email".into()));
                } else {
                    set_error.set(None);
                }
            })
        />
    </FormGroup>
}"##}</pre>
            </div>
        </div>
    }
}
