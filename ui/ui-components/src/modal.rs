//! Modal Leptos Component
//!
//! A modal dialog with backdrop using fixed positioning.
//!
//! ## Context-Aware Behavior
//!
//! When rendered inside a `ModalStack`, the Modal automatically detects the
//! `ModalNavigation` context and coordinates with the stack to show its content
//! as a new view. This enables seamless view swapping for nested modals.
//!
//! ## Props
//!
//! - `open` - Signal controlling modal visibility
//! - `title` - Optional modal title
//! - `on_close` - Callback when modal should close (backdrop click or Escape key)
//! - `children` - Modal body content
//!
//! ## Usage
//!
//! ```ignore
//! let (show_modal, set_show_modal) = signal(false);
//!
//! <Button on_click=move |_| set_show_modal.set(true)>"Open Modal"</Button>
//!
//! <Modal
//!     open=show_modal
//!     title="Confirm Action".to_string()
//!     on_close=Callback::new(move |_| set_show_modal.set(false))
//! >
//!     <p>"Are you sure you want to proceed?"</p>
//!     <Button on_click=move |_| set_show_modal.set(false)>"Close"</Button>
//! </Modal>
//! ```

use crate::modal_context::{use_modal_navigation, ModalViewId};
use leptos::prelude::*;

/// Modal dialog component
///
/// When inside a ModalStack, automatically mounts as a view in the stack.
/// When standalone, renders its own overlay.
#[component]
pub fn Modal(
    /// Signal controlling modal visibility
    #[prop(into)]
    open: Signal<bool>,
    /// Optional modal title
    #[prop(into, optional)]
    title: Option<String>,
    /// Callback when modal should close
    #[prop(into, optional)]
    on_close: Option<Callback<()>>,
    /// Remove body padding (for full-bleed content like cards)
    #[prop(optional)]
    flush: bool,
    /// Modal body content
    children: Children,
) -> impl IntoView {
    // Check if we're inside a ModalStack
    let modal_nav = use_modal_navigation();

    // Eagerly render children - we need this regardless of mode
    let body_content = children();

    if let Some(nav) = modal_nav {
        // We're inside a ModalStack - coordinate with it
        let (view_id, set_view_id) = signal(Option::<ModalViewId>::None);
        let title_str = title.clone().unwrap_or_default();

        // Clone nav for different closures
        let nav_for_mount = nav.clone();
        let nav_for_unmount = nav.clone();
        let nav_for_cleanup = nav.clone();
        let title_for_effect = title_str.clone();

        // Create callback for external close (e.g., back button in ModalStack)
        let external_close_callback: Option<std::sync::Arc<dyn Fn() + Send + Sync>> =
            on_close.map(|cb| {
                std::sync::Arc::new(move || cb.run(())) as std::sync::Arc<dyn Fn() + Send + Sync>
            });

        // Sync open state with mount/unmount
        Effect::new(move |prev_open: Option<bool>| {
            let is_open = open.get();

            match (is_open, prev_open) {
                (true, Some(false)) | (true, None) => {
                    // Opening - register with the stack, passing the close callback
                    let id = nav_for_mount
                        .mount(title_for_effect.clone(), external_close_callback.clone());
                    set_view_id.set(Some(id));
                }
                (false, Some(true)) => {
                    // Closing - unregister from the stack
                    if let Some(id) = view_id.get_untracked() {
                        nav_for_unmount.unmount(id);
                        set_view_id.set(None);
                    }
                }
                _ => {}
            }

            is_open
        });

        // Cleanup on component unmount
        on_cleanup({
            let nav_for_cleanup = nav_for_cleanup.clone();
            move || {
                if let Some(id) = view_id.get_untracked() {
                    nav_for_cleanup.unmount(id);
                }
            }
        });

        view! {
            // Render into the portal target when open
            // Use a simple div that gets shown/hidden
            <div
                class="ui-modal-nested"
                class:ui-modal-nested--open=move || open.get()
                data-modal-view-id=move || view_id.get().map(|id| format!("{:?}", id)).unwrap_or_default()
            >
                <div class="ui-modal__body" class:flush=flush>
                    {body_content}
                </div>
            </div>
        }
        .into_any()
    } else {
        // Standalone modal - render our own overlay
        let header_content = title.map(|t| {
            view! {
                <div class="ui-modal__header">
                    <h2 class="ui-modal__title">{t}</h2>
                    {on_close.map(|cb| view! {
                        <button
                            class="ui-modal__close"
                            on:click=move |_| cb.run(())
                            aria-label="Close"
                        >
                            "\u{00D7}"
                        </button>
                    })}
                </div>
            }
        });

        let handle_keydown = move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" {
                if let Some(cb) = on_close {
                    cb.run(());
                }
            }
        };

        let handle_backdrop_click = move |_| {
            if let Some(cb) = on_close {
                cb.run(());
            }
        };

        let stop_propagation = move |ev: web_sys::MouseEvent| {
            ev.stop_propagation();
        };

        let backdrop_style = move || {
            if open.get() {
                "display: flex;"
            } else {
                "display: none;"
            }
        };

        view! {
            <div
                class="ui-modal-backdrop"
                style=backdrop_style
                on:click=handle_backdrop_click
                on:keydown=handle_keydown
                tabindex="-1"
            >
                <div
                    class="ui-modal"
                    on:click=stop_propagation
                    role="dialog"
                    aria-modal="true"
                    aria-hidden=move || (!open.get()).to_string()
                >
                    {header_content}
                    <div class="ui-modal__body" class:flush=flush>
                        {body_content}
                    </div>
                </div>
            </div>
        }
        .into_any()
    }
}
