//! TextInput component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{InputType, TextInput};

#[component]
pub fn TextInputStory() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (email, set_email) = signal("user@example.com".to_string());
    let (password, set_password) = signal(String::new());
    let (search, set_search) = signal(String::new());
    let (disabled_value, _) = signal("Cannot edit this".to_string());

    view! {
        <div>
            <div class="story-header">
                <h2>"TextInput"</h2>
                <p>"A styled text input field with optional label and various input types."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <div class="story-canvas">
                    <div style="max-width: 300px;">
                        <TextInput
                            value=username
                            on_change=Callback::new(move |v| set_username.set(v))
                            label="Username"
                            placeholder="Enter username..."
                        />
                        <p style="margin-top: 0.5rem; color: #888; font-size: 0.875rem;">
                            "Value: " {move || username.get()}
                        </p>
                    </div>
                </div>
            </div>

            // Input types
            <div class="story-section">
                <h3>"Input Types"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 300px;">
                        <TextInput
                            value=email
                            on_change=Callback::new(move |v| set_email.set(v))
                            label="Email"
                            input_type=InputType::Email
                            placeholder="email@example.com"
                        />
                        <TextInput
                            value=password
                            on_change=Callback::new(move |v| set_password.set(v))
                            label="Password"
                            input_type=InputType::Password
                            placeholder="Enter password..."
                        />
                        <TextInput
                            value=search
                            on_change=Callback::new(move |v| set_search.set(v))
                            label="Search"
                            input_type=InputType::Search
                            placeholder="Search..."
                        />
                    </div>
                </div>
            </div>

            // States
            <div class="story-section">
                <h3>"States"</h3>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 300px;">
                        <TextInput
                            value=disabled_value
                            on_change=Callback::new(|_| {})
                            label="Disabled Input"
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
                            description="Current input value"
                        />
                        <AttributeCard
                            name="on_change"
                            values="Callback<String>"
                            description="Callback when value changes"
                        />
                        <AttributeCard
                            name="label"
                            values="String (optional)"
                            description="Label text above the input"
                        />
                        <AttributeCard
                            name="placeholder"
                            values="String (optional)"
                            description="Placeholder text"
                        />
                        <AttributeCard
                            name="disabled"
                            values="Signal<bool> (optional)"
                            description="Whether input is disabled"
                        />
                        <AttributeCard
                            name="input_type"
                            values="InputType (Text|Email|Password|Number|Search|Tel|Url)"
                            description="HTML input type - default is Text"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{TextInput, InputType};

let (name, set_name) = signal(String::new());

// Basic text input
view! {
    <TextInput
        value=name
        on_change=Callback::new(move |v| set_name.set(v))
        label="Name"
        placeholder="Enter your name..."
    />
}

// Password input
view! {
    <TextInput
        value=password
        on_change=set_password
        label="Password"
        input_type=InputType::Password
    />
}"##}</pre>
            </div>
        </div>
    }
}
