//! Select component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Select, SelectOption};

#[component]
pub fn SelectStory() -> impl IntoView {
    let (selected_string, set_selected_string) = signal("".to_string());

    let string_options = Signal::derive(move || {
        vec![
            SelectOption::new("apple", "Apple"),
            SelectOption::new("banana", "Banana"),
            SelectOption::new("cherry", "Cherry"),
            SelectOption::new("date", "Date"),
        ]
    });

    view! {
        <div>
            <div class="story-header">
                <h2>"Select"</h2>
                <p>"A dropdown select component for choosing from a list of options."</p>
            </div>

            // String options
            <div class="story-section">
                <h3>"Basic Select"</h3>
                <p style="color: #888; margin-bottom: 1rem;">"Selected: "<code>{move || if selected_string.get().is_empty() { "(none)".to_string() } else { selected_string.get() }}</code></p>
                <div class="story-canvas">
                    <Select
                        value=selected_string
                        options=string_options
                        on_change=Callback::new(move |v| set_selected_string.set(v))
                        placeholder="Choose a fruit..."
                    />
                </div>
            </div>

            // Pre-selected value
            <div class="story-section">
                <h3>"Pre-selected Value"</h3>
                <div class="story-canvas">
                    {
                        let (preset, set_preset) = signal("banana".to_string());
                        let options = Signal::derive(move || vec![
                            SelectOption::new("apple", "Apple"),
                            SelectOption::new("banana", "Banana"),
                            SelectOption::new("cherry", "Cherry"),
                        ]);
                        view! {
                            <Select
                                value=preset
                                options=options
                                on_change=Callback::new(move |v| set_preset.set(v))
                            />
                        }
                    }
                </div>
            </div>

            // Disabled options
            <div class="story-section">
                <h3>"Disabled Options"</h3>
                <div class="story-canvas">
                    {
                        let (val, set_val) = signal("".to_string());
                        let options = Signal::derive(move || vec![
                            SelectOption::new("active", "Active Item"),
                            SelectOption::disabled("disabled", "Disabled Item"),
                            SelectOption::new("another", "Another Active"),
                        ]);
                        view! {
                            <Select
                                value=val
                                options=options
                                on_change=Callback::new(move |v| set_val.set(v))
                                placeholder="Select an option..."
                            />
                        }
                    }
                </div>
            </div>

            // Reactive options
            <div class="story-section">
                <h3>"Reactive Options"</h3>
                <div class="story-canvas">
                    {
                        let (category, set_category) = signal("fruits".to_string());
                        let (item, set_item) = signal("".to_string());

                        let categories = Signal::derive(move || vec![
                            SelectOption::new("fruits", "Fruits"),
                            SelectOption::new("vegetables", "Vegetables"),
                        ]);

                        let items = Signal::derive(move || {
                            match category.get().as_str() {
                                "fruits" => vec![
                                    SelectOption::new("apple", "Apple"),
                                    SelectOption::new("orange", "Orange"),
                                ],
                                "vegetables" => vec![
                                    SelectOption::new("carrot", "Carrot"),
                                    SelectOption::new("broccoli", "Broccoli"),
                                ],
                                _ => vec![],
                            }
                        });

                        view! {
                            <div style="display: flex; gap: 1rem; align-items: center;">
                                <Select
                                    value=category
                                    options=categories
                                    on_change=Callback::new(move |v| {
                                        set_category.set(v);
                                        set_item.set("".to_string());
                                    })
                                />
                                <Select
                                    value=item
                                    options=items
                                    on_change=Callback::new(move |v| set_item.set(v))
                                    placeholder="Select item..."
                                />
                                <span style="color: #888;">{move || {
                                    let cat = category.get();
                                    let it = item.get();
                                    format!("Category: {}, Item: {}", cat, if it.is_empty() { "(none)".to_string() } else { it })
                                }}</span>
                            </div>
                        }
                    }
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
                            description="The currently selected value"
                        />
                        <AttributeCard
                            name="options"
                            values="Signal<Vec<SelectOption>>"
                            description="List of options (use SelectOption::new)"
                        />
                        <AttributeCard
                            name="on_change"
                            values="Callback<String>"
                            description="Called when selection changes"
                        />
                        <AttributeCard
                            name="placeholder"
                            values="String (optional)"
                            description="Placeholder text when no selection"
                        />
                        <AttributeCard
                            name="disabled"
                            values="Signal<bool> (optional)"
                            description="Whether the select is disabled"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Select, SelectOption};

let (selected, set_selected) = signal("".to_string());

let options = Signal::derive(|| vec![
    SelectOption::new("a", "Option A"),
    SelectOption::new("b", "Option B"),
    SelectOption::disabled("c", "Option C (disabled)"),
]);

view! {
    <Select
        value=selected
        options=options
        on_change=Callback::new(move |v| set_selected.set(v))
        placeholder="Choose an option..."
    />
}"##}</pre>
            </div>
        </div>
    }
}
