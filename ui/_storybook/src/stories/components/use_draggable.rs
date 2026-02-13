//! use_draggable hook and DraggableStack component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{use_draggable, DraggableStack, Reorder, StackDirection};

#[component]
pub fn UseDraggableStory() -> impl IntoView {
    // Basic example with colored boxes
    let (items, set_items) = signal(vec![
        ("A", "#e74c3c"),
        ("B", "#3498db"),
        ("C", "#2ecc71"),
        ("D", "#f39c12"),
        ("E", "#9b59b6"),
    ]);

    let draggable = use_draggable(move |reorder| {
        set_items.update(|items| reorder.apply(items));
    });

    // Task list example
    let (tasks, set_tasks) = signal(vec![
        "Review pull request",
        "Fix bug in login flow",
        "Write unit tests",
        "Update documentation",
        "Deploy to staging",
    ]);

    let task_draggable = use_draggable(move |reorder| {
        set_tasks.update(|tasks| reorder.apply(tasks));
    });

    view! {
        <div>
            <div class="story-header">
                <h2>"use_draggable"</h2>
                <p>"A reusable hook for drag-and-drop reordering of list items. Provides drag state tracking and event handlers."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Example"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "Drag the colored boxes to reorder them."
                </p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.5rem;">
                        <For
                            each={move || items.get().into_iter().enumerate().collect::<Vec<_>>()}
                            key=|(_, (label, _))| *label
                            let:item
                        >
                            {
                                let (idx, (label, color)) = item;
                                let attrs = draggable.attrs(idx);
                                let draggable_clone = draggable.clone();

                                {
                                    let attrs_start = attrs.clone();
                                    let attrs_end = attrs.clone();
                                    let attrs_over = attrs.clone();
                                    let attrs_leave = attrs.clone();
                                    let attrs_drop = attrs;

                                    view! {
                                        <div
                                            style=move || format!(
                                                "width: 60px; height: 60px; background: {}; border-radius: 8px; \
                                                display: flex; align-items: center; justify-content: center; \
                                                font-weight: bold; font-size: 1.5rem; color: white; cursor: grab; \
                                                transition: transform 0.15s, opacity 0.15s; \
                                                transform: {}; opacity: {};",
                                                color,
                                                if draggable_clone.is_drag_over(idx) { "scale(1.1)" } else { "scale(1)" },
                                                if draggable_clone.is_dragging(idx) { "0.5" } else { "1" }
                                            )
                                            draggable="true"
                                            on:dragstart=move |ev| attrs_start.on_drag_start(ev)
                                            on:dragend=move |ev| attrs_end.on_drag_end(ev)
                                            on:dragover=move |ev| attrs_over.on_drag_over(ev)
                                            on:dragleave=move |ev| attrs_leave.on_drag_leave(ev)
                                            on:drop=move |ev| attrs_drop.on_drop(ev)
                                        >
                                            {label}
                                        </div>
                                    }
                                }
                            }
                        </For>
                    </div>
                </div>
                <div style="margin-top: 1rem; padding: 0.75rem; background: #1a1a2e; border-radius: 4px;">
                    <p style="font-size: 0.75rem; color: #888; margin-bottom: 0.5rem;">"Current order:"</p>
                    <code style="font-size: 0.875rem; color: #fff;">
                        {move || items.get().iter().map(|(l, _)| *l).collect::<Vec<_>>().join(" → ")}
                    </code>
                </div>
            </div>

            // Task list example
            <div class="story-section">
                <h3>"Task List Example"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "A more realistic example with a sortable task list."
                </p>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 0.5rem; max-width: 400px;">
                        <For
                            each={move || tasks.get().into_iter().enumerate().collect::<Vec<_>>()}
                            key=|(_, task)| task.to_string()
                            let:item
                        >
                            {
                                let (idx, task) = item;
                                let attrs = task_draggable.attrs(idx);
                                let draggable_clone = task_draggable.clone();

                                {
                                    let attrs_start = attrs.clone();
                                    let attrs_end = attrs.clone();
                                    let attrs_over = attrs.clone();
                                    let attrs_leave = attrs.clone();
                                    let attrs_drop = attrs;

                                    view! {
                                        <div
                                            style=move || format!(
                                                "padding: 0.75rem 1rem; background: {}; border-radius: 6px; \
                                                cursor: grab; display: flex; align-items: center; gap: 0.75rem; \
                                                transition: background 0.15s, transform 0.15s; \
                                                transform: {};",
                                                if draggable_clone.is_drag_over(idx) { "#3a3a5e" }
                                                else if draggable_clone.is_dragging(idx) { "#2a2a4e" }
                                                else { "#252538" },
                                                if draggable_clone.is_drag_over(idx) { "translateX(4px)" } else { "translateX(0)" }
                                            )
                                            draggable="true"
                                            on:dragstart=move |ev| attrs_start.on_drag_start(ev)
                                            on:dragend=move |ev| attrs_end.on_drag_end(ev)
                                            on:dragover=move |ev| attrs_over.on_drag_over(ev)
                                            on:dragleave=move |ev| attrs_leave.on_drag_leave(ev)
                                            on:drop=move |ev| attrs_drop.on_drop(ev)
                                        >
                                            <span style="color: #666; font-size: 0.875rem; min-width: 1.5rem;">
                                                {idx + 1}"."
                                            </span>
                                            <span style="color: #fff;">{task}</span>
                                            <span style="margin-left: auto; color: #444; font-size: 1.25rem;">"⠿"</span>
                                        </div>
                                    }
                                }
                            }
                        </For>
                    </div>
                </div>
            </div>

            // DraggableStack examples
            <div class="story-section">
                <h3>"DraggableStack Component"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "A higher-level component that handles all the wiring automatically. Shows drop position indicators between items."
                </p>

                // Horizontal DraggableStack
                <div class="story-canvas" style="margin-bottom: 1rem;">
                    <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">"Horizontal (default)"</p>
                    {
                        let (stack_items, set_stack_items) = signal(vec![
                            ("1", "#e74c3c"),
                            ("2", "#3498db"),
                            ("3", "#2ecc71"),
                            ("4", "#f39c12"),
                        ]);

                        view! {
                            <DraggableStack
                                items=stack_items
                                on_reorder=move |reorder: Reorder| set_stack_items.update(|i| reorder.apply(i))
                                key_fn=|(label, _)| *label
                                direction=StackDirection::Horizontal
                                gap="0.5rem"
                                render_item=move |(label, color), _idx, _drag_state| view! {
                                    <div style=format!(
                                        "width: 60px; height: 60px; background: {}; border-radius: 8px; \
                                        display: flex; align-items: center; justify-content: center; \
                                        font-weight: bold; font-size: 1.5rem; color: white;",
                                        color
                                    )>
                                        {label}
                                    </div>
                                }
                            />
                        }
                    }
                </div>

                // Vertical DraggableStack
                <div class="story-canvas">
                    <p style="margin: 0 0 0.5rem; font-size: 0.75rem; color: #888;">"Vertical"</p>
                    {
                        let (vertical_items, set_vertical_items) = signal(vec![
                            "First item",
                            "Second item",
                            "Third item",
                            "Fourth item",
                        ]);

                        view! {
                            <DraggableStack
                                items=vertical_items
                                on_reorder=move |reorder: Reorder| set_vertical_items.update(|i| reorder.apply(i))
                                key_fn=|item| *item
                                direction=StackDirection::Vertical
                                gap="0.5rem"
                                render_item=move |item, idx, drag_state| view! {
                                    <div style=format!(
                                        "padding: 0.75rem 1rem; background: {}; border-radius: 6px; \
                                        display: flex; align-items: center; gap: 0.75rem; min-width: 250px;",
                                        if drag_state.is_source { "#2a2a4e" } else { "#252538" }
                                    )>
                                        <span style="color: #666; font-size: 0.875rem; min-width: 1.5rem;">
                                            {idx + 1}"."
                                        </span>
                                        <span style="color: #fff;">{item}</span>
                                        <span style="margin-left: auto; color: #444; font-size: 1.25rem;">"⠿"</span>
                                    </div>
                                }
                            />
                        }
                    }
                </div>
            </div>

            // API section
            <div class="story-section">
                <h3>"use_draggable Hook Return Value"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="draggable.attrs(index)"
                            values="DragAttrs"
                            description="Returns event handlers for a specific item index"
                        />
                        <AttributeCard
                            name="draggable.is_dragging(index)"
                            values="bool"
                            description="Check if a specific item is being dragged"
                        />
                        <AttributeCard
                            name="draggable.is_drag_over(index)"
                            values="bool"
                            description="Check if an item is being dragged over"
                        />
                        <AttributeCard
                            name="draggable.is_active()"
                            values="bool"
                            description="Check if any drag operation is in progress"
                        />
                        <AttributeCard
                            name="draggable.state()"
                            values="Signal<DragState>"
                            description="Access the full drag state signal"
                        />
                    </div>
                </div>
            </div>

            // DragAttrs section
            <div class="story-section">
                <h3>"DragAttrs Events"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="on_drag_start"
                            values="Fn(DragEvent)"
                            description="Call on dragstart event"
                        />
                        <AttributeCard
                            name="on_drag_end"
                            values="Fn(DragEvent)"
                            description="Call on dragend event"
                        />
                        <AttributeCard
                            name="on_drag_over"
                            values="Fn(DragEvent)"
                            description="Call on dragover event"
                        />
                        <AttributeCard
                            name="on_drag_leave"
                            values="Fn(DragEvent)"
                            description="Call on dragleave event"
                        />
                        <AttributeCard
                            name="on_drop"
                            values="Fn(DragEvent)"
                            description="Call on drop event"
                        />
                    </div>
                </div>
            </div>

            // Reorder section
            <div class="story-section">
                <h3>"Reorder Struct"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "The callback receives a Reorder struct that can be applied to your items."
                </p>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="reorder.apply(&mut vec)"
                            values="()"
                            description="Apply the reorder operation to a mutable Vec"
                        />
                        <AttributeCard
                            name="reorder.source()"
                            values="usize"
                            description="The original index of the dragged item"
                        />
                        <AttributeCard
                            name="reorder.target()"
                            values="usize"
                            description="The target index where the item was dropped"
                        />
                    </div>
                </div>
            </div>

            // DraggableStack Props
            <div class="story-section">
                <h3>"DraggableStack Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="items"
                            values="Signal<Vec<T>>"
                            description="Items to render in the stack"
                        />
                        <AttributeCard
                            name="on_reorder"
                            values="Callback<Reorder>"
                            description="Called when items are reordered"
                        />
                        <AttributeCard
                            name="direction"
                            values="StackDirection"
                            description="Horizontal (default) or Vertical"
                        />
                        <AttributeCard
                            name="gap"
                            values="String"
                            description="CSS gap between items (default: 0.5rem)"
                        />
                        <AttributeCard
                            name="render_item"
                            values="Fn(T, usize, ItemDragState) -> V"
                            description="Render function for each item"
                        />
                        <AttributeCard
                            name="class"
                            values="String (optional)"
                            description="Additional CSS class for container"
                        />
                    </div>
                </div>
            </div>

            // DraggableStack Usage code
            <div class="story-section">
                <h3>"DraggableStack Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{DraggableStack, StackDirection};

let (items, set_items) = signal(vec!["A", "B", "C", "D"]);

view! {
    <DraggableStack
        items=items
        on_reorder=move |reorder| set_items.update(|i| reorder.apply(i))
        direction=StackDirection::Horizontal
        gap="0.5rem"
        render_item=move |item, idx, drag_state| view! {
            <div
                class="my-item"
                class:dragging=drag_state.is_source
            >
                {item}
            </div>
        }
    />
}"##}</pre>
            </div>

            // Low-level hook usage code
            <div class="story-section">
                <h3>"Low-Level use_draggable Hook"</h3>
                <p style="color: #888; margin-bottom: 1rem; font-size: 0.875rem;">
                    "For custom implementations where DraggableStack doesn't fit your needs."
                </p>
                <pre class="code-block">{r##"use ui_components::use_draggable;

let (items, set_items) = signal(vec!["A", "B", "C", "D"]);

let draggable = use_draggable(move |reorder| {
    set_items.update(|items| reorder.apply(items));
});

view! {
    <For each=move || items.get().into_iter().enumerate() ...>
        {
            let (idx, value) = item;
            let attrs = draggable.attrs(idx);
            // Clone attrs for each handler since they need to be moved
            let attrs_start = attrs.clone();
            let attrs_end = attrs.clone();
            // ... etc

            view! {
                <div
                    draggable="true"
                    on:dragstart=move |ev| attrs_start.on_drag_start(ev)
                    on:dragend=move |ev| attrs_end.on_drag_end(ev)
                    // ... etc
                >
                    {value}
                </div>
            }
        }
    </For>
}"##}</pre>
            </div>

            // Features
            <div class="story-section">
                <h3>"Features"</h3>
                <div class="story-canvas">
                    <ul style="margin: 0; padding-left: 1.5rem; color: #888; font-size: 0.875rem;">
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Index-based"</strong>
                            " - Works with any Vec type, no trait requirements"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Automatic index adjustment"</strong>
                            " - Correctly handles the shift when dragging items forward"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Visual state tracking"</strong>
                            " - is_dragging() and is_drag_over() for styling feedback"
                        </li>
                        <li style="margin-bottom: 0.5rem;">
                            <strong style="color: #fff;">"Reusable"</strong>
                            " - Same hook works for any sortable list"
                        </li>
                        <li>
                            <strong style="color: #fff;">"Leptos 0.8 compatible"</strong>
                            " - Uses signals and StoredValue for proper cleanup"
                        </li>
                    </ul>
                </div>
            </div>
        </div>
    }
}
