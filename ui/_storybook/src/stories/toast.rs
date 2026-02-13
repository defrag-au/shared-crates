//! UI Toast stories - toast types and usage

use super::helpers::{ToastFnCard, ToastKindCard, TraitMethodCard};
use leptos::prelude::*;

// ============================================================================
// Toast Types Story
// ============================================================================

#[component]
pub fn ToastTypesStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Toast Types"</h2>
                <p>"The ui-toast crate provides four toast types for different notification severities."</p>
            </div>

            <div class="story-section">
                <h3>"ToastKind Variants"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ToastKindCard
                            name="Success"
                            css_class="toast--success"
                            icon="\u{2713}"
                            example="Operation completed successfully"
                        />
                        <ToastKindCard
                            name="Warning"
                            css_class="toast--warning"
                            icon="\u{26A0}"
                            example="Something needs attention"
                        />
                        <ToastKindCard
                            name="Error"
                            css_class="toast--error"
                            icon="\u{2715}"
                            example="An error occurred"
                        />
                        <ToastKindCard
                            name="Info"
                            css_class="toast--info"
                            icon="\u{2139}"
                            example="Informational message"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Convenience Functions"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ToastFnCard signature="success(msg)" description="Creates a success toast message" />
                        <ToastFnCard signature="warning(msg)" description="Creates a warning toast message" />
                        <ToastFnCard signature="error(msg)" description="Creates an error toast message" />
                        <ToastFnCard signature="info(msg)" description="Creates an info toast message" />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Creating Toast Messages"</h3>
                <pre class="code-block">{r#"use ui_toast::{success, error, warning, info, show, ToastKind};

// Using convenience functions
let msg = success("File saved successfully");
let msg = error("Failed to connect");
let msg = warning("Low disk space");
let msg = info("New version available");

// Using show() for more control
let msg = show("Custom message", ToastKind::Success);

// With custom icon
let msg = show_with_icon("Uploaded!", ToastKind::Success, "🚀");"#}</pre>
            </div>
        </div>
    }
}

// ============================================================================
// Toast Usage Story
// ============================================================================

#[component]
pub fn ToastUsageStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Toast Usage"</h2>
                <p>"Integrate toasts into your widget using the HasToasts trait."</p>
            </div>

            <div class="story-section">
                <h3>"HasToasts Trait"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <TraitMethodCard
                            signature="toasts()"
                            returns="&VecDeque<Toast>"
                            description="Get reference to toast queue"
                        />
                        <TraitMethodCard
                            signature="toasts_mut()"
                            returns="&mut VecDeque<Toast>"
                            description="Get mutable reference to toast queue"
                        />
                        <TraitMethodCard
                            signature="next_toast_id()"
                            returns="u32"
                            description="Get the next toast ID"
                        />
                        <TraitMethodCard
                            signature="set_next_toast_id(id)"
                            returns="()"
                            description="Set the next toast ID"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Provided Methods"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <TraitMethodCard
                            signature="add_toast(msg, kind)"
                            returns="u32"
                            description="Add a toast, returns its ID"
                        />
                        <TraitMethodCard
                            signature="add_toast_with_icon(...)"
                            returns="u32"
                            description="Add toast with custom icon"
                        />
                        <TraitMethodCard
                            signature="dismiss_toast(id)"
                            returns="()"
                            description="Remove a specific toast"
                        />
                        <TraitMethodCard
                            signature="cleanup_expired_toasts()"
                            returns="()"
                            description="Remove all expired toasts"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Implementation Example"</h3>
                <pre class="code-block">{r#"use ui_toast::{Toast, ToastKind, HasToasts};
use std::collections::VecDeque;

struct Model {
    toasts: VecDeque<Toast>,
    next_toast_id: u32,
}

impl HasToasts for Model {
    fn toasts(&self) -> &VecDeque<Toast> { &self.toasts }
    fn toasts_mut(&mut self) -> &mut VecDeque<Toast> { &mut self.toasts }
    fn next_toast_id(&self) -> u32 { self.next_toast_id }
    fn set_next_toast_id(&mut self, id: u32) { self.next_toast_id = id; }
}

// Then use the provided methods:
model.add_toast("Success!".to_string(), ToastKind::Success);
model.cleanup_expired_toasts();"#}</pre>
            </div>
        </div>
    }
}
