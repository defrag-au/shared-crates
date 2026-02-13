//! ModalStack component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Button, ButtonVariant, Modal, ModalStack, ModalStackContext};

/// View enum for the demo modal
#[derive(Clone, PartialEq)]
enum DemoView {
    Main,
    Settings,
    Profile,
    EditName,
}

/// View enum for the nested example
#[derive(Clone, PartialEq)]
enum NestedView {
    Step1,
    Step2,
    Step3,
    Step4,
}

fn demo_title(view: &DemoView) -> String {
    match view {
        DemoView::Main => "Dashboard".to_string(),
        DemoView::Settings => "Settings".to_string(),
        DemoView::Profile => "Profile".to_string(),
        DemoView::EditName => "Edit Name".to_string(),
    }
}

fn demo_content(view: DemoView, ctx: ModalStackContext<DemoView>) -> AnyView {
    match view {
        DemoView::Main => view! {
            <div style="padding: 1rem;">
                <p style="margin-bottom: 1rem;">"This is the main view. Click a button to navigate to a sub-view."</p>
                <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                    <button class="btn btn--secondary" on:click={
                        let ctx = ctx.clone();
                        move |_| ctx.push(DemoView::Settings)
                    }>"Settings"</button>
                    <button class="btn btn--secondary" on:click={
                        let ctx = ctx.clone();
                        move |_| ctx.push(DemoView::Profile)
                    }>"Profile"</button>
                </div>
            </div>
        }.into_any(),
        DemoView::Settings => view! {
            <div style="padding: 1rem;">
                <p>"Settings page content."</p>
                <p style="color: #888; margin-top: 0.5rem;">"Use the breadcrumb or back arrow to return."</p>
            </div>
        }.into_any(),
        DemoView::Profile => view! {
            <div style="padding: 1rem;">
                <p style="margin-bottom: 1rem;">"Profile page. You can go deeper:"</p>
                <button class="btn btn--secondary" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.push(DemoView::EditName)
                }>"Edit Name"</button>
            </div>
        }.into_any(),
        DemoView::EditName => view! {
            <div style="padding: 1rem;">
                <p>"Edit your name here."</p>
                <input type="text" placeholder="Enter name" style="margin-top: 0.5rem; padding: 0.5rem; border-radius: 4px; border: 1px solid #444; background: #1a1a2e; color: #e0e0e0;" />
            </div>
        }.into_any(),
    }
}

fn nested_title(view: &NestedView) -> String {
    match view {
        NestedView::Step1 => "Step 1".to_string(),
        NestedView::Step2 => "Step 2".to_string(),
        NestedView::Step3 => "Step 3".to_string(),
        NestedView::Step4 => "Step 4".to_string(),
    }
}

fn nested_content(view: NestedView, ctx: ModalStackContext<NestedView>) -> AnyView {
    match view {
        NestedView::Step1 => view! {
            <div style="padding: 1rem;">
                <p>"Step 1 of 4"</p>
                <button class="btn btn--primary" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.push(NestedView::Step2)
                }>"Next"</button>
            </div>
        }
        .into_any(),
        NestedView::Step2 => view! {
            <div style="padding: 1rem;">
                <p>"Step 2 of 4"</p>
                <button class="btn btn--primary" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.push(NestedView::Step3)
                }>"Next"</button>
            </div>
        }
        .into_any(),
        NestedView::Step3 => view! {
            <div style="padding: 1rem;">
                <p>"Step 3 of 4"</p>
                <button class="btn btn--primary" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.push(NestedView::Step4)
                }>"Next"</button>
            </div>
        }
        .into_any(),
        NestedView::Step4 => view! {
            <div style="padding: 1rem;">
                <p>"Step 4 - Final step!"</p>
                <p style="color: #888; margin-top: 0.5rem;">"Click any breadcrumb to jump back."</p>
                <button class="btn btn--success" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.close()
                }>"Done"</button>
            </div>
        }
        .into_any(),
    }
}

fn form_title(view: &DemoView) -> String {
    match view {
        DemoView::Main => "Form".to_string(),
        DemoView::Settings => "Sub-form".to_string(),
        _ => "View".to_string(),
    }
}

fn form_content(view: DemoView, ctx: ModalStackContext<DemoView>) -> AnyView {
    match view {
        DemoView::Main => view! {
            <div style="padding: 1rem;">
                <p style="margin-bottom: 1rem;">"Enter some text, then navigate away and back. Your input is preserved!"</p>
                <input type="text" placeholder="Type something..." style="width: 100%; padding: 0.5rem; border-radius: 4px; border: 1px solid #444; background: #1a1a2e; color: #e0e0e0;" />
                <button class="btn btn--secondary" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.push(DemoView::Settings)
                }>"Go to Sub-form"</button>
            </div>
        }.into_any(),
        DemoView::Settings => view! {
            <div style="padding: 1rem;">
                <p style="margin-bottom: 1rem;">"This is a sub-form. When you go back, the main form state is preserved."</p>
                <input type="text" placeholder="More input..." style="width: 100%; padding: 0.5rem; border-radius: 4px; border: 1px solid #444; background: #1a1a2e; color: #e0e0e0;" />
                <button class="btn btn--secondary" style="margin-top: 1rem;" on:click={
                    let ctx = ctx.clone();
                    move |_| ctx.pop()
                }>"Back"</button>
            </div>
        }.into_any(),
        _ => view! { <div></div> }.into_any(),
    }
}

/// View enum for context-aware Modal demo
#[derive(Clone, PartialEq)]
enum ContextAwareView {
    Main,
}

fn context_aware_title(view: &ContextAwareView) -> String {
    match view {
        ContextAwareView::Main => "Main View".to_string(),
    }
}

fn context_aware_content(
    view: ContextAwareView,
    _ctx: ModalStackContext<ContextAwareView>,
) -> AnyView {
    match view {
        ContextAwareView::Main => {
            // This nested Modal will detect the ModalNavigation context
            // and render as a view in the stack instead of its own overlay
            let (show_nested, set_show_nested) = signal(false);

            view! {
                <div style="padding: 1rem;">
                    <p style="margin-bottom: 1rem;">
                        "This view contains a regular " <code>"<Modal>"</code> " component."
                    </p>
                    <p style="margin-bottom: 1rem; color: #888;">
                        "When opened, it automatically coordinates with the ModalStack - "
                        "no special setup required!"
                    </p>
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move || set_show_nested.set(true))
                    >
                        "Open Nested Modal"
                    </Button>

                    // This Modal will detect the context and render as a stack view
                    <Modal
                        open=show_nested
                        title="Nested Modal".to_string()
                        on_close=Callback::new(move || set_show_nested.set(false))
                    >
                        <p>"This is a nested Modal that automatically became a stack view!"</p>
                        <p style="color: #888; margin-top: 0.5rem;">
                            "Notice it appears in the breadcrumbs above."
                        </p>
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move || set_show_nested.set(false))
                        >
                            "Close"
                        </Button>
                    </Modal>
                </div>
            }
            .into_any()
        }
    }
}

#[component]
pub fn ModalStackStory() -> impl IntoView {
    let (show_basic, set_show_basic) = signal(false);
    let (show_nested, set_show_nested) = signal(false);
    let (show_preserved, set_show_preserved) = signal(false);
    let (show_context_aware, set_show_context_aware) = signal(false);

    view! {
        <div>
            <div class="story-header">
                <h2>"ModalStack"</h2>
                <p>"A modal with view swapping support. Instead of stacking multiple modals, views slide in/out within a single container with breadcrumb navigation."</p>
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
                            "Basic Navigation"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_nested.set(true)
                        >
                            "Deep Nesting (4 levels)"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_preserved.set(true)
                        >
                            "State Preservation"
                        </button>
                        <button
                            class="btn btn--primary"
                            on:click=move |_| set_show_context_aware.set(true)
                        >
                            "Context-Aware Modal"
                        </button>
                    </div>
                </div>
            </div>

            // Basic Navigation Demo
            <ModalStack
                open=show_basic
                initial_view=DemoView::Main
                on_close=Callback::new(move || set_show_basic.set(false))
                view_title=demo_title
                view_content=demo_content
            />

            // Deep Nesting Demo
            <ModalStack
                open=show_nested
                initial_view=NestedView::Step1
                on_close=Callback::new(move || set_show_nested.set(false))
                view_title=nested_title
                view_content=nested_content
            />

            // State Preservation Demo
            <ModalStack
                open=show_preserved
                initial_view=DemoView::Main
                on_close=Callback::new(move || set_show_preserved.set(false))
                view_title=form_title
                view_content=form_content
            />

            // Context-Aware Modal Demo
            <ModalStack
                open=show_context_aware
                initial_view=ContextAwareView::Main
                on_close=Callback::new(move || set_show_context_aware.set(false))
                view_title=context_aware_title
                view_content=context_aware_content
            />

            // Key Differences section
            <div class="story-section">
                <h3>"Key Differences from Modal"</h3>
                <div class="story-canvas">
                    <table style="width: 100%; border-collapse: collapse;">
                        <thead>
                            <tr style="border-bottom: 1px solid #444;">
                                <th style="text-align: left; padding: 0.5rem;">"Feature"</th>
                                <th style="text-align: left; padding: 0.5rem;">"Modal"</th>
                                <th style="text-align: left; padding: 0.5rem;">"ModalStack"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr style="border-bottom: 1px solid #333;">
                                <td style="padding: 0.5rem;">"Nested interactions"</td>
                                <td style="padding: 0.5rem; color: #888;">"Stack multiple modals"</td>
                                <td style="padding: 0.5rem; color: #4caf50;">"Single modal, swap views"</td>
                            </tr>
                            <tr style="border-bottom: 1px solid #333;">
                                <td style="padding: 0.5rem;">"Navigation"</td>
                                <td style="padding: 0.5rem; color: #888;">"Close buttons only"</td>
                                <td style="padding: 0.5rem; color: #4caf50;">"Breadcrumbs + back button"</td>
                            </tr>
                            <tr style="border-bottom: 1px solid #333;">
                                <td style="padding: 0.5rem;">"State"</td>
                                <td style="padding: 0.5rem; color: #888;">"Lost when closed"</td>
                                <td style="padding: 0.5rem; color: #4caf50;">"Preserved in stack"</td>
                            </tr>
                            <tr>
                                <td style="padding: 0.5rem;">"Type safety"</td>
                                <td style="padding: 0.5rem; color: #888;">"Children only"</td>
                                <td style="padding: 0.5rem; color: #4caf50;">"Strongly-typed view enum"</td>
                            </tr>
                        </tbody>
                    </table>
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
                            name="initial_view"
                            values="V"
                            description="The starting view when modal opens"
                        />
                        <AttributeCard
                            name="on_close"
                            values="Callback<()> (optional)"
                            description="Called when backdrop clicked, Escape pressed, or ctx.close() called"
                        />
                        <AttributeCard
                            name="view_title"
                            values="Fn(&V) -> String"
                            description="Returns the title for each view (shown in breadcrumbs)"
                        />
                        <AttributeCard
                            name="view_content"
                            values="Fn(V, ModalStackContext<V>) -> AnyView"
                            description="Render function receiving current view and navigation context"
                        />
                        <AttributeCard
                            name="flush"
                            values="bool (default: false)"
                            description="Remove body padding for full-bleed content"
                        />
                    </div>
                </div>
            </div>

            // Context Methods section
            <div class="story-section">
                <h3>"ModalStackContext Methods"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="ctx.push(view)"
                            values="fn(V)"
                            description="Push a new view onto the stack (slides in from right)"
                        />
                        <AttributeCard
                            name="ctx.pop()"
                            values="fn()"
                            description="Pop current view and go back (slides out to right)"
                        />
                        <AttributeCard
                            name="ctx.close()"
                            values="fn()"
                            description="Close the entire modal"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{ModalStack, ModalStackContext};

#[derive(Clone, PartialEq)]
enum MyView {
    Main,
    AddItem,
    SelectAsset { filter: String },
}

fn my_title(view: &MyView) -> String {
    match view {
        MyView::Main => "My Modal".into(),
        MyView::AddItem => "Add Item".into(),
        MyView::SelectAsset { .. } => "Select Asset".into(),
    }
}

fn my_content(view: MyView, ctx: ModalStackContext<MyView>) -> AnyView {
    match view {
        MyView::Main => view! {
            <button on:click={
                let ctx = ctx.clone();
                move |_| ctx.push(MyView::AddItem)
            }>"Add"</button>
        }.into_any(),
        MyView::AddItem => view! {
            <button on:click={
                let ctx = ctx.clone();
                move |_| ctx.push(MyView::SelectAsset {
                    filter: "nft".into()
                })
            }>"Select Asset"</button>
        }.into_any(),
        MyView::SelectAsset { filter } => view! {
            <p>"Filtering by: " {filter}</p>
            <button on:click={
                let ctx = ctx.clone();
                move |_| ctx.pop()
            }>"Back"</button>
        }.into_any(),
    }
}

let (show_modal, set_show_modal) = signal(false);

<ModalStack
    open=show_modal
    initial_view=MyView::Main
    on_close=Callback::new(move || set_show_modal.set(false))
    view_title=my_title
    view_content=my_content
/>"##}</pre>
            </div>
        </div>
    }
}
