//! Draggable Stack Component
//!
//! A container for drag-and-drop reorderable items using mouse events.
//!
//! ## Features
//!
//! - Horizontal or vertical layout
//! - Smooth drag with actual element movement
//! - Items shift to show drop position
//! - Container-based mouse capture for reliable tracking
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::{DraggableStack, StackDirection};
//!
//! let (items, set_items) = signal(vec!["A", "B", "C", "D"]);
//!
//! view! {
//!     <DraggableStack
//!         items=items
//!         on_reorder=move |reorder| set_items.update(|i| reorder.apply(i))
//!         direction=StackDirection::Horizontal
//!         gap="0.5rem"
//!         render_item=move |item, _idx, drag_state| view! {
//!             <div class="my-item" class:dragging=drag_state.is_source>
//!                 {item}
//!             </div>
//!         }
//!     />
//! }
//! ```

use crate::Reorder;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Stack layout direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackDirection {
    #[default]
    Horizontal,
    Vertical,
}

impl StackDirection {
    fn flex_direction(&self) -> &'static str {
        match self {
            StackDirection::Horizontal => "row",
            StackDirection::Vertical => "column",
        }
    }
}

/// Drag state passed to render function
#[derive(Clone, Copy, Default)]
pub struct ItemDragState {
    /// True if this item is currently being dragged
    pub is_source: bool,
    /// True if drag is active (any item being dragged)
    pub drag_active: bool,
}

/// Captured item position at drag start
#[derive(Clone, Default)]
struct ItemRect {
    start: f64,
    end: f64,
}

impl ItemRect {
    fn mid(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// Internal drag state
#[derive(Clone, Default)]
struct DragState {
    /// Index of item being dragged
    source_index: Option<usize>,
    /// Current target position (where item would be inserted)
    target_position: Option<usize>,
    /// Current mouse offset from drag start
    offset_x: f64,
    offset_y: f64,
    /// Starting mouse position
    start_x: f64,
    start_y: f64,
    /// Original item positions captured at drag start (for stable hit testing)
    item_rects: Vec<ItemRect>,
}

/// Draggable stack component for reorderable lists
#[component]
pub fn DraggableStack<T, K, KeyFn, F, V>(
    /// Items to render
    #[prop(into)]
    items: Signal<Vec<T>>,
    /// Callback when items are reordered
    #[prop(into)]
    on_reorder: Callback<Reorder>,
    /// Function to extract a unique key from each item
    key_fn: KeyFn,
    /// Stack direction
    #[prop(optional, default = StackDirection::Horizontal)]
    direction: StackDirection,
    /// Gap between items (CSS value)
    #[prop(into, optional, default = "0.5rem".into())]
    gap: String,
    /// Render function for each item
    render_item: F,
    /// Additional class for the container
    #[prop(into, optional)]
    class: String,
    /// Disable drag reordering
    #[prop(into, optional)]
    disabled: Signal<bool>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
    K: Clone + PartialEq + Eq + std::hash::Hash + 'static,
    KeyFn: Fn(&T) -> K + Clone + Send + Sync + 'static,
    F: Fn(T, usize, ItemDragState) -> V + Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    let (drag_state, set_drag_state) = signal(DragState::default());
    let container_ref = NodeRef::<leptos::html::Div>::new();

    let container_class = if class.is_empty() {
        "ui-draggable-stack".to_string()
    } else {
        format!("ui-draggable-stack {class}")
    };

    let container_style = format!(
        "display: flex; flex-direction: {}; gap: {gap}; position: relative; user-select: none;",
        direction.flex_direction()
    );

    // Pointer down on an item - start drag and capture pointer
    let on_item_pointerdown = move |idx: usize, ev: web_sys::PointerEvent| {
        // Don't start drag if disabled
        if disabled.get_untracked() {
            return;
        }

        // Only handle primary button (left click / touch)
        if ev.button() != 0 {
            return;
        }

        ev.prevent_default();

        // Capture pointer to track movement even outside container
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(ev.pointer_id());
            }
        }

        // Capture original item positions for stable hit testing during drag
        let item_rects = if let Some(container) = container_ref.get() {
            let container_el: &web_sys::HtmlElement = &container;
            let children = container_el.children();
            let mut rects = Vec::with_capacity(children.length() as usize);

            for i in 0..children.length() {
                if let Some(child) = children.item(i) {
                    if let Ok(el) = child.dyn_into::<web_sys::HtmlElement>() {
                        let rect = el.get_bounding_client_rect();
                        let (start, end) = match direction {
                            StackDirection::Horizontal => (rect.left(), rect.right()),
                            StackDirection::Vertical => (rect.top(), rect.bottom()),
                        };
                        rects.push(ItemRect { start, end });
                    }
                }
            }
            rects
        } else {
            vec![]
        };

        set_drag_state.set(DragState {
            source_index: Some(idx),
            target_position: Some(idx),
            offset_x: 0.0,
            offset_y: 0.0,
            start_x: ev.client_x() as f64,
            start_y: ev.client_y() as f64,
            item_rects,
        });
    };

    // Pointer move - update drag position (works even outside container due to capture)
    let on_pointermove = move |ev: web_sys::PointerEvent| {
        let state = drag_state.get();
        if state.source_index.is_none() {
            return;
        }

        let source_idx = state.source_index.unwrap();
        let offset_x = ev.client_x() as f64 - state.start_x;
        let offset_y = ev.client_y() as f64 - state.start_y;

        // Calculate target position based on pointer position using ORIGINAL item positions
        // This ensures stable hit testing even as items visually shift during drag
        let pointer_pos = match direction {
            StackDirection::Horizontal => ev.client_x() as f64,
            StackDirection::Vertical => ev.client_y() as f64,
        };

        let item_count = state.item_rects.len();

        // Find which position the pointer is at based on original midpoints
        let mut visual_position = item_count; // Default to end

        for (i, rect) in state.item_rects.iter().enumerate() {
            if pointer_pos < rect.mid() {
                visual_position = i;
                break;
            }
        }

        // For forward dragging to feel natural, we need to check if the pointer
        // has actually crossed into the NEXT item's space (past the source item's end).
        // Without this, clicking anywhere on an item immediately triggers the skip logic.
        let source_rect = state.item_rects.get(source_idx);
        let has_left_source = source_rect
            .map(|r| pointer_pos > r.end || pointer_pos < r.start)
            .unwrap_or(false);

        // Convert visual position to insertion position
        let insertion_pos = if visual_position <= source_idx {
            // Dragging backward or staying put - use visual position directly
            visual_position
        } else if visual_position == source_idx + 1 && has_left_source {
            // Pointer has left the source item AND is in the no-op zone
            // Skip to the next position to make the drag feel responsive
            source_idx + 2
        } else if visual_position == source_idx + 1 {
            // Still within source item bounds - keep at source position (no shift)
            source_idx
        } else {
            // Dragging further forward - use visual position
            visual_position
        };

        let target_pos = insertion_pos.min(item_count);

        set_drag_state.set(DragState {
            source_index: Some(source_idx),
            target_position: Some(target_pos),
            offset_x,
            offset_y,
            start_x: state.start_x,
            start_y: state.start_y,
            item_rects: state.item_rects.clone(),
        });
    };

    // Pointer up - complete drag
    let on_pointerup = move |ev: web_sys::PointerEvent| {
        // Release pointer capture
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.release_pointer_capture(ev.pointer_id());
            }
        }

        let state = drag_state.get();
        if let (Some(source), Some(target)) = (state.source_index, state.target_position) {
            if source != target && source + 1 != target {
                on_reorder.run(Reorder::new(source, target));
            }
        }
        set_drag_state.set(DragState::default());
    };

    // Pointer cancel - abort drag (e.g., if another touch starts, or system interrupts)
    let on_pointercancel = move |ev: web_sys::PointerEvent| {
        // Release pointer capture
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.release_pointer_capture(ev.pointer_id());
            }
        }
        set_drag_state.set(DragState::default());
    };

    view! {
        <div
            class=container_class
            style=container_style
            node_ref=container_ref
        >
            <For
                each={move || items.get().into_iter().enumerate().collect::<Vec<_>>()}
                key={
                    let key_fn = key_fn.clone();
                    move |(_, item)| key_fn(item)
                }
                children=move |(initial_idx, item)| {
                    let render_item = render_item.clone();
                    let item_for_style = item.clone();
                    let item_for_mousedown = item.clone();
                    let item_for_drag_state = item.clone();
                    let item_for_display_idx = item.clone();
                    let item_for_render = item.clone();

                    // Calculate transform and styles for this item
                    let item_style = move || {
                        let is_disabled = disabled.get();
                        let idx = items.get().iter().position(|i| i == &item_for_style).unwrap_or(initial_idx);
                        let state = drag_state.get();
                        let source = state.source_index;
                        let target = state.target_position;

                        let cursor = if is_disabled { "default" } else { "grab" };
                        let cursor_active = if is_disabled { "default" } else { "grabbing" };

                        if source == Some(idx) {
                            // This is the dragged item - apply offset transform
                            let (tx, ty) = match direction {
                                StackDirection::Horizontal => (state.offset_x, 0.0),
                                StackDirection::Vertical => (0.0, state.offset_y),
                            };
                            format!(
                                "transform: translate({tx}px, {ty}px); z-index: 100; position: relative; \
                                 transition: none; cursor: {cursor_active};"
                            )
                        } else if let (Some(src), Some(tgt)) = (source, target) {
                            // Other items may need to shift
                            let shift = calculate_shift(idx, src, tgt, &state.item_rects);
                            if shift != 0.0 {
                                let (tx, ty) = match direction {
                                    StackDirection::Horizontal => (shift, 0.0),
                                    StackDirection::Vertical => (0.0, shift),
                                };
                                format!(
                                    "transform: translate({tx}px, {ty}px); \
                                     transition: transform 0.15s ease; cursor: {cursor};"
                                )
                            } else {
                                format!("transition: transform 0.15s ease; cursor: {cursor};")
                            }
                        } else {
                            format!("cursor: {cursor};")
                        }
                    };

                    let drag_state_for_render = move || {
                        let idx = items.get().iter().position(|i| i == &item_for_drag_state).unwrap_or(initial_idx);
                        let state = drag_state.get();
                        ItemDragState {
                            is_source: state.source_index == Some(idx),
                            drag_active: state.source_index.is_some(),
                        }
                    };

                    let on_pointerdown_item = move |ev: web_sys::PointerEvent| {
                        let idx = items.get().iter().position(|i| i == &item_for_mousedown).unwrap_or(initial_idx);
                        on_item_pointerdown(idx, ev);
                    };

                    // For render_item, we need a stable index for display purposes
                    let display_idx = items.get().iter().position(|i| i == &item_for_display_idx).unwrap_or(initial_idx);

                    view! {
                        <div
                            class="ui-draggable-stack__item-wrapper"
                            style=item_style
                            on:pointerdown=on_pointerdown_item
                            on:pointermove=on_pointermove
                            on:pointerup=on_pointerup
                            on:pointercancel=on_pointercancel
                        >
                            {render_item(item_for_render.clone(), display_idx, drag_state_for_render())}
                        </div>
                    }
                }
            />
        </div>
    }
}

/// Calculate the shift amount for an item based on drag state
fn calculate_shift(idx: usize, source: usize, target: usize, item_rects: &[ItemRect]) -> f64 {
    // Get the source item's size (width or height depending on direction)
    let source_size = item_rects
        .get(source)
        .map(|r| r.end - r.start)
        .unwrap_or(0.0);

    // Include gap between items (estimate from the difference between items)
    let gap = if item_rects.len() >= 2 {
        item_rects
            .get(1)
            .and_then(|r1| item_rects.first().map(|r0| r1.start - r0.end))
            .unwrap_or(0.0)
            .max(0.0)
    } else {
        0.0
    };

    let shift_amount = source_size + gap;

    if source < target {
        // Dragging forward: items between source+1 and target-1 shift backward
        // Note: target is already adjusted to skip no-op positions, so target >= source + 2
        if idx > source && idx < target {
            return -shift_amount;
        }
    } else if source > target {
        // Dragging backward: items between target and source-1 shift forward
        if idx >= target && idx < source {
            return shift_amount;
        }
    }
    0.0
}
