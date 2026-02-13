//! Modal Stack Component
//!
//! A modal with view swapping support for nested interactions.
//! Views slide in/out horizontally with breadcrumb navigation.
//!
//! ## Design
//!
//! Instead of stacking multiple modals, this component maintains a stack of views
//! within a single modal container. Child views can push new views onto the stack,
//! and the user can navigate back via breadcrumbs or a back button.
//!
//! ## Nested Modals
//!
//! When a `<Modal>` is rendered inside a ModalStack, it automatically detects
//! the `ModalNavigation` context and coordinates with the stack. The Modal renders
//! its content inline but the stack manages breadcrumbs and navigation.
//!
//! ## Usage
//!
//! ```ignore
//! #[derive(Clone, PartialEq)]
//! enum MyView {
//!     Main,
//!     AddItem,
//!     SelectAsset { filter: String },
//! }
//!
//! <ModalStack
//!     open=show_modal
//!     initial_view=MyView::Main
//!     on_close=Callback::new(move |()| set_show_modal.set(false))
//!     view_title=|view: &MyView| match view {
//!         MyView::Main => "My Modal".into(),
//!         MyView::AddItem => "Add Item".into(),
//!         MyView::SelectAsset { .. } => "Select Asset".into(),
//!     }
//!     view_content=|view: MyView, ctx: ModalStackContext<MyView>| match view {
//!         MyView::Main => view! {
//!             <button on:click={
//!                 let ctx = ctx.clone();
//!                 move |_| ctx.push(MyView::AddItem)
//!             }>"Add"</button>
//!         }.into_any(),
//!         // ... etc
//!     }
//! />
//! ```

use crate::modal_context::{ModalNavigation, ModalViewId, MountedView};
use leptos::prelude::*;
use std::sync::Arc;

/// Context provided to view content for navigation
#[derive(Clone)]
pub struct ModalStackContext<V: Clone + 'static> {
    push_fn: Arc<dyn Fn(V) + Send + Sync>,
    pop_fn: Arc<dyn Fn() + Send + Sync>,
    close_fn: Arc<dyn Fn() + Send + Sync>,
}

impl<V: Clone + 'static> ModalStackContext<V> {
    /// Push a new view onto the stack
    pub fn push(&self, view: V) {
        (self.push_fn)(view);
    }

    /// Pop the current view and go back
    pub fn pop(&self) {
        (self.pop_fn)();
    }

    /// Close the entire modal
    pub fn close(&self) {
        (self.close_fn)();
    }
}

/// Direction of the current slide animation
#[derive(Clone, Copy, PartialEq, Default)]
enum SlideDirection {
    #[default]
    None,
    Forward,
    Back,
}

/// Combined state for the modal stack - enables atomic updates
#[derive(Clone)]
struct StackState<V: Clone> {
    /// Typed views pushed via ctx.push()
    typed_views: Vec<V>,
    /// Mounted modals from nested Modal components (overlay, don't hide parent)
    mounted_modals: Vec<MountedView>,
    /// Current slide animation direction
    slide_direction: SlideDirection,
    /// Whether an animation is in progress
    is_animating: bool,
}

impl<V: Clone> StackState<V> {
    fn new(initial_view: V) -> Self {
        Self {
            typed_views: vec![initial_view],
            mounted_modals: vec![],
            slide_direction: SlideDirection::None,
            is_animating: false,
        }
    }

    fn reset(&mut self, initial_view: V) {
        self.typed_views = vec![initial_view];
        self.mounted_modals.clear();
        self.slide_direction = SlideDirection::None;
        self.is_animating = false;
    }

    fn can_go_back(&self) -> bool {
        self.typed_views.len() > 1 || !self.mounted_modals.is_empty()
    }

    fn total_breadcrumb_count(&self) -> usize {
        self.typed_views.len() + self.mounted_modals.len()
    }
}

/// Modal with view stack support
///
/// Generic over `V` - the view enum type that defines possible views.
#[component]
pub fn ModalStack<V, TitleFn, ContentFn>(
    /// Signal controlling modal visibility
    #[prop(into)]
    open: Signal<bool>,
    /// Initial view to display
    initial_view: V,
    /// Callback when modal should close
    #[prop(into, optional)]
    on_close: Option<Callback<()>>,
    /// Function to get title for a view
    view_title: TitleFn,
    /// Remove body padding (for full-bleed content)
    #[prop(optional)]
    flush: bool,
    /// Render function for view content - receives current view and navigation context
    view_content: ContentFn,
) -> impl IntoView
where
    V: Clone + PartialEq + Send + Sync + 'static,
    TitleFn: Fn(&V) -> String + Send + Sync + Clone + 'static,
    ContentFn: Fn(V, ModalStackContext<V>) -> AnyView + Send + Sync + Clone + 'static,
{
    // Single state signal for atomic updates
    let (state, set_state) = signal(StackState::new(initial_view.clone()));

    // Reset state when modal closes and reopens
    let initial_for_effect = initial_view.clone();
    Effect::new(move |prev_open: Option<bool>| {
        let currently_open = open.get();
        if currently_open && prev_open == Some(false) {
            set_state.update(|s| s.reset(initial_for_effect.clone()));
        }
        currently_open
    });

    // Create the context for child views (typed navigation)
    let push_typed_fn = Arc::new(move |view: V| {
        // Collect callbacks from mounted modals before clearing
        let callbacks: Vec<_> = {
            let s = state.get_untracked();
            if s.is_animating {
                return;
            }
            s.mounted_modals
                .iter()
                .filter_map(|m| m.on_external_close.clone())
                .collect()
        };

        set_state.update(|s| {
            if s.is_animating {
                return;
            }
            s.mounted_modals.clear();
            s.slide_direction = SlideDirection::Forward;
            s.is_animating = true;
            s.typed_views.push(view);
        });

        // Call all external close callbacks
        for cb in callbacks {
            cb();
        }
    });

    let pop_fn = Arc::new(move || {
        // Get the callback before updating state (to avoid borrow issues)
        let callback_to_call = {
            let s = state.get_untracked();
            if s.is_animating {
                return;
            }
            if let Some(modal) = s.mounted_modals.last() {
                modal.on_external_close.clone()
            } else {
                None
            }
        };

        set_state.update(|s| {
            if s.is_animating {
                return;
            }
            // First pop any mounted modals, then typed views
            if !s.mounted_modals.is_empty() {
                s.slide_direction = SlideDirection::Back;
                s.is_animating = true;
                s.mounted_modals.pop();
            } else if s.typed_views.len() > 1 {
                s.slide_direction = SlideDirection::Back;
                s.is_animating = true;
                s.typed_views.pop();
            }
        });

        // Call the external close callback after state update
        if let Some(cb) = callback_to_call {
            cb();
        }
    });

    let close_fn = Arc::new(move || {
        if let Some(cb) = on_close {
            cb.run(());
        }
    });

    let ctx = ModalStackContext {
        push_fn: push_typed_fn,
        pop_fn: pop_fn.clone(),
        close_fn: close_fn.clone(),
    };

    // Create ModalNavigation context for nested Modals
    let mount_fn = Arc::new(
        move |title: String, on_external_close: Option<crate::modal_context::OnExternalClose>| {
            let id = ModalViewId::new();
            let mounted = MountedView {
                id,
                title: title.clone(),
                on_external_close,
            };
            set_state.update(|s| {
                s.mounted_modals.push(mounted);
            });
            id
        },
    );

    let unmount_fn = Arc::new(move |id: ModalViewId| {
        set_state.update(|s| {
            s.mounted_modals.retain(|m| m.id != id);
        });
    });

    let modal_nav = ModalNavigation::new(
        move |title, on_close| (mount_fn)(title, on_close),
        move |id| (unmount_fn)(id),
    );

    // Provide the context immediately
    provide_context(modal_nav);

    // Clone for use in view
    let view_title_for_breadcrumbs = view_title.clone();
    let content_fn = view_content.clone();
    let ctx_for_content = ctx.clone();

    // Handle animation end
    let on_animation_end = move |_| {
        set_state.update(|s| {
            s.is_animating = false;
            s.slide_direction = SlideDirection::None;
        });
    };

    // Close handlers
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

    // Animation class
    let animation_class = move || match state.get().slide_direction {
        SlideDirection::None => "",
        SlideDirection::Forward => "ui-modal-stack__content--slide-in",
        SlideDirection::Back => "ui-modal-stack__content--slide-out",
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
                class="ui-modal ui-modal-stack"
                on:click=stop_propagation
                role="dialog"
                aria-modal="true"
            >
                // Header with breadcrumbs
                <div class="ui-modal__header ui-modal-stack__header">
                    // Back button (only show when there's something to go back to)
                    {move || {
                        let s = state.get();
                        let ctx = ctx.clone();
                        s.can_go_back().then(|| view! {
                            <button
                                class="ui-modal-stack__back"
                                on:click=move |_| ctx.pop()
                                aria-label="Go back"
                            >
                                "\u{2190}"
                            </button>
                        })
                    }}

                    // Breadcrumbs - show typed views + mounted modals
                    <nav class="ui-modal-stack__breadcrumbs">
                        {move || {
                            let s = state.get();
                            let view_title = view_title_for_breadcrumbs.clone();
                            let total_items = s.total_breadcrumb_count();

                            // Build breadcrumb items: typed views first, then mounted modals
                            let mut items = Vec::new();

                            // Add typed views
                            for (i, v) in s.typed_views.iter().enumerate() {
                                let title = view_title(v);
                                let is_last = i == total_items - 1;
                                let target_depth = i + 1;
                                items.push((title, is_last, Some(target_depth)));
                            }

                            // Add mounted modals
                            for (i, m) in s.mounted_modals.iter().enumerate() {
                                let title = m.title.clone();
                                let is_last = s.typed_views.len() + i == total_items - 1;
                                items.push((title, is_last, None));
                            }

                            items.into_iter().map(|(title, is_last, typed_depth)| {
                                view! {
                                    <span class="ui-modal-stack__breadcrumb">
                                        {if is_last {
                                            view! {
                                                <span class="ui-modal-stack__breadcrumb-current">{title}</span>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <button
                                                    class="ui-modal-stack__breadcrumb-link"
                                                    on:click={
                                                        move |_| {
                                                            if let Some(depth) = typed_depth {
                                                                // Collect callbacks from mounted modals before clearing
                                                                let callbacks: Vec<_> = {
                                                                    let s = state.get_untracked();
                                                                    s.mounted_modals.iter()
                                                                        .filter_map(|m| m.on_external_close.clone())
                                                                        .collect()
                                                                };

                                                                set_state.update(|s| {
                                                                    s.typed_views.truncate(depth);
                                                                    s.mounted_modals.clear();
                                                                });

                                                                // Call all external close callbacks
                                                                for cb in callbacks {
                                                                    cb();
                                                                }
                                                            }
                                                        }
                                                    }
                                                >
                                                    {title}
                                                </button>
                                                <span class="ui-modal-stack__breadcrumb-sep">" / "</span>
                                            }.into_any()
                                        }}
                                    </span>
                                }
                            }).collect_view()
                        }}
                    </nav>

                    // Close button
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

                // Content area with slide animation
                <div
                    class="ui-modal__body ui-modal-stack__body"
                    class:flush=flush
                >
                    <div
                        class=move || format!("ui-modal-stack__content {}", animation_class())
                        on:animationend=on_animation_end
                    >
                        // Render typed views only - mounted modals overlay via nested Modal component
                        <For
                            each=move || {
                                state.get().typed_views.into_iter().enumerate().collect::<Vec<_>>()
                            }
                            key=|(i, _)| format!("typed-{i}")
                            let:indexed_entry
                        >
                            {
                                let content_fn = content_fn.clone();
                                let ctx = ctx_for_content.clone();
                                let (i, view) = indexed_entry;

                                let content = content_fn(view, ctx.clone());

                                // Make is_current reactive
                                let is_current = move || {
                                    let s = state.get();
                                    i == s.typed_views.len() - 1
                                };

                                view! {
                                    <div
                                        class="ui-modal-stack__view"
                                        class:ui-modal-stack__view--current=is_current
                                        class:ui-modal-stack__view--hidden=move || !is_current()
                                    >
                                        {content}
                                    </div>
                                }
                            }
                        </For>
                    </div>
                </div>
            </div>
        </div>
    }
}
