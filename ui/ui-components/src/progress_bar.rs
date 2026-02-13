//! ProgressBar Leptos Component
//!
//! A horizontal progress indicator.
//!
//! ## Props
//!
//! - `value` - Progress value (0.0 to 1.0)
//! - `label` - Optional label text
//! - `color` - Optional CSS color for the bar
//! - `show_percentage` - Whether to show percentage text
//!
//! ## Usage
//!
//! ```ignore
//! <ProgressBar value=Signal::derive(|| 0.75) />
//!
//! <ProgressBar
//!     value=progress
//!     label="Loading..."
//!     color="#28a745"
//!     show_percentage=true
//! />
//! ```

use leptos::prelude::*;

/// Progress bar component
#[component]
pub fn ProgressBar(
    /// Progress value (0.0 to 1.0)
    #[prop(into)]
    value: Signal<f32>,
    /// Optional label text
    #[prop(into, optional)]
    label: Option<String>,
    /// Optional CSS color for the bar
    #[prop(into, optional)]
    color: Option<String>,
    /// Whether to show percentage text
    #[prop(optional)]
    show_percentage: bool,
) -> impl IntoView {
    let bar_style = move || {
        let pct = (value.get().clamp(0.0, 1.0) * 100.0) as u32;
        let color_style = color
            .as_ref()
            .map(|c| format!(" background-color: {c};"))
            .unwrap_or_default();
        format!("width: {pct}%;{color_style}")
    };

    let percentage_text = move || {
        let pct = (value.get().clamp(0.0, 1.0) * 100.0) as u32;
        format!("{pct}%")
    };

    let has_label = label.is_some();

    view! {
        <div class="ui-progress">
            {label.map(|l| view! {
                <div class="ui-progress__header">
                    <span class="ui-progress__label">{l}</span>
                    {show_percentage.then(|| view! {
                        <span class="ui-progress__percentage">{percentage_text}</span>
                    })}
                </div>
            })}
            <div class="ui-progress__track">
                <div class="ui-progress__bar" style=bar_style></div>
            </div>
            {(!has_label && show_percentage).then(|| view! {
                <span class="ui-progress__percentage">{percentage_text}</span>
            })}
        </div>
    }
}
