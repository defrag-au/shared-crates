//! Modal component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::Modal;

#[component]
pub fn ModalStory() -> impl IntoView {
    let (show_basic, set_show_basic) = signal(false);
    let (show_titled, set_show_titled) = signal(false);
    let (show_complex, set_show_complex) = signal(false);

    view! {
        <div>
            <div class="story-header">
                <h2>"Modal"</h2>
                <p>"A modal dialog with backdrop overlay. Supports title, close callback, and escape key to dismiss."</p>
            </div>

            // Interactive Demo section
            <div class="story-section">
                <h3>"Interactive Demo"</h3>
                <div class="story-canvas">
                    <div style="display: flex; gap: 1rem; flex-wrap: wrap;">
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_basic.set(true)
                        >
                            "Open Basic Modal"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_titled.set(true)
                        >
                            "Open Titled Modal"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_complex.set(true)
                        >
                            "Open Complex Modal"
                        </button>
                    </div>
                </div>
            </div>

            // Basic Modal
            <Modal
                open=show_basic
                on_close=Callback::new(move |_| set_show_basic.set(false))
            >
                <div style="padding: 1rem;">
                    <p>"This is a basic modal without a title."</p>
                    <p>"Click outside or press Escape to close."</p>
                </div>
            </Modal>

            // Titled Modal
            <Modal
                open=show_titled
                title="Modal Title"
                on_close=Callback::new(move |_| set_show_titled.set(false))
            >
                <div style="padding: 1rem;">
                    <p>"This modal has a title and close button in the header."</p>
                </div>
            </Modal>

            // Complex Modal
            <Modal
                open=show_complex
                title="Confirm Action"
                on_close=Callback::new(move |_| set_show_complex.set(false))
            >
                <div style="padding: 1rem;">
                    <p style="margin-bottom: 1rem;">"Are you sure you want to proceed? This action cannot be undone."</p>
                    <div style="display: flex; gap: 0.5rem; justify-content: flex-end;">
                        <button
                            class="btn btn--secondary"
                            on:click=move |_| set_show_complex.set(false)
                        >
                            "Cancel"
                        </button>
                        <button
                            class="btn btn--danger"
                            on:click=move |_| set_show_complex.set(false)
                        >
                            "Confirm"
                        </button>
                    </div>
                </div>
            </Modal>

            // Features section
            <div class="story-section">
                <h3>"Features"</h3>
                <div class="story-canvas">
                    <ul style="margin: 0; padding-left: 1.5rem; line-height: 1.8;">
                        <li>"Click backdrop to close (if on_close provided)"</li>
                        <li>"Press Escape key to close"</li>
                        <li>"Optional title with close button"</li>
                        <li>"Centered on screen with dark backdrop"</li>
                        <li>"Click inside modal does not close it"</li>
                    </ul>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="open"
                            values="Signal<bool>"
                            description="Controls whether the modal is visible"
                        />
                        <AttributeCard
                            name="title"
                            values="String (optional)"
                            description="Title text shown in the modal header"
                        />
                        <AttributeCard
                            name="on_close"
                            values="Callback<()> (optional)"
                            description="Called when backdrop clicked or Escape pressed"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Content to render inside the modal body"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::Modal;

let (show_modal, set_show_modal) = signal(false);

view! {
    <button on:click=move |_| set_show_modal.set(true)>
        "Open Modal"
    </button>

    <Modal
        open=show_modal
        title="My Modal"
        on_close=Callback::new(move || set_show_modal.set(false))
    >
        <div style="padding: 1rem;">
            <p>"Modal content here"</p>
            <button on:click=move |_| set_show_modal.set(false)>
                "Close"
            </button>
        </div>
    </Modal>
}"##}</pre>
            </div>
        </div>
    }
}
