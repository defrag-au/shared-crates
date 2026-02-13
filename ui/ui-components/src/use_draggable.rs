//! Draggable List Hook
//!
//! A reusable hook for drag-and-drop reordering of list items.
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::use_draggable;
//!
//! let (items, set_items) = signal(vec!["A", "B", "C"]);
//!
//! let draggable = use_draggable(move |reorder| {
//!     set_items.update(|items| reorder(items));
//! });
//!
//! view! {
//!     <For each=move || items.get().into_iter().enumerate() key=|(i, _)| *i let:item>
//!         {
//!             let (idx, value) = item;
//!             let attrs = draggable.attrs(idx);
//!             view! {
//!                 <div
//!                     class:dragging=move || draggable.is_dragging(idx)
//!                     class:drag-over=move || draggable.is_drag_over(idx)
//!                     draggable="true"
//!                     on:dragstart=attrs.on_drag_start
//!                     on:dragend=attrs.on_drag_end
//!                     on:dragover=attrs.on_drag_over
//!                     on:drop=attrs.on_drop
//!                 >
//!                     {value}
//!                 </div>
//!             }
//!         }
//!     </For>
//! }
//! ```

use leptos::prelude::*;

/// State for a drag operation
#[derive(Clone, Default, PartialEq)]
pub struct DragState {
    /// Index of the item being dragged
    pub source_index: Option<usize>,
    /// Index of the item currently being dragged over
    pub target_index: Option<usize>,
}

impl DragState {
    /// Check if a drag operation is in progress
    pub fn is_active(&self) -> bool {
        self.source_index.is_some()
    }

    /// Check if a specific index is being dragged
    pub fn is_dragging(&self, index: usize) -> bool {
        self.source_index == Some(index)
    }

    /// Check if a specific index is being dragged over
    pub fn is_drag_over(&self, index: usize) -> bool {
        self.target_index == Some(index) && self.source_index != Some(index)
    }
}

/// Event handler type (Arc for cloneability)
pub type DragHandler = std::sync::Arc<dyn Fn(web_sys::DragEvent) + Send + Sync>;

/// Event handlers for a draggable item.
///
/// These are Arc-wrapped so they can be cloned. To use with Leptos `on:` handlers,
/// wrap in a closure: `on:dragstart=move |ev| attrs.on_drag_start(ev)`
#[derive(Clone)]
pub struct DragAttrs {
    inner_drag_start: DragHandler,
    inner_drag_end: DragHandler,
    inner_drag_over: DragHandler,
    inner_drag_leave: DragHandler,
    inner_drop: DragHandler,
}

impl DragAttrs {
    /// Call on dragstart event
    pub fn on_drag_start(&self, ev: web_sys::DragEvent) {
        (self.inner_drag_start)(ev);
    }

    /// Call on dragend event
    pub fn on_drag_end(&self, ev: web_sys::DragEvent) {
        (self.inner_drag_end)(ev);
    }

    /// Call on dragover event
    pub fn on_drag_over(&self, ev: web_sys::DragEvent) {
        (self.inner_drag_over)(ev);
    }

    /// Call on dragleave event
    pub fn on_drag_leave(&self, ev: web_sys::DragEvent) {
        (self.inner_drag_leave)(ev);
    }

    /// Call on drop event
    pub fn on_drop(&self, ev: web_sys::DragEvent) {
        (self.inner_drop)(ev);
    }
}

/// A reorder operation that can be applied to a Vec
pub struct Reorder {
    source: usize,
    target: usize,
}

impl Reorder {
    /// Create a new reorder operation
    ///
    /// - `source`: index of the item being dragged
    /// - `target`: position to insert the item (can be 0..=len)
    pub fn new(source: usize, target: usize) -> Self {
        Self { source, target }
    }

    /// Apply this reorder operation to a vector
    pub fn apply<T>(&self, items: &mut Vec<T>) {
        if self.source == self.target || self.source >= items.len() {
            return;
        }
        let item = items.remove(self.source);
        // Adjust target index if we removed before it
        let adjusted_target = if self.source < self.target {
            self.target - 1
        } else {
            self.target
        };
        let insert_at = adjusted_target.min(items.len());
        items.insert(insert_at, item);
    }

    /// Get the source index
    pub fn source(&self) -> usize {
        self.source
    }

    /// Get the target index
    pub fn target(&self) -> usize {
        self.target
    }
}

/// Callback type for reorder operations
pub type ReorderCallback = std::sync::Arc<dyn Fn(Reorder) + Send + Sync>;

/// Handle returned by use_draggable
#[derive(Clone)]
pub struct Draggable {
    state: Signal<DragState>,
    set_state: WriteSignal<DragState>,
    on_reorder: StoredValue<ReorderCallback>,
}

impl Draggable {
    /// Check if a drag operation is in progress
    pub fn is_active(&self) -> bool {
        self.state.get().is_active()
    }

    /// Check if a specific index is being dragged
    pub fn is_dragging(&self, index: usize) -> bool {
        self.state.get().is_dragging(index)
    }

    /// Check if a specific index is being dragged over
    pub fn is_drag_over(&self, index: usize) -> bool {
        self.state.get().is_drag_over(index)
    }

    /// Get the current drag state signal (for reactive access in views)
    pub fn state(&self) -> Signal<DragState> {
        self.state
    }

    /// Get drag event handlers for a specific index
    pub fn attrs(&self, index: usize) -> DragAttrs {
        use std::sync::Arc;

        let set_state = self.set_state;
        let state = self.state;
        let on_reorder = self.on_reorder;

        let set_state_start = set_state;
        let on_drag_start: DragHandler = Arc::new(move |ev: web_sys::DragEvent| {
            if let Some(dt) = ev.data_transfer() {
                dt.set_effect_allowed("move");
            }
            set_state_start.set(DragState {
                source_index: Some(index),
                target_index: None,
            });
        });

        let set_state_end = set_state;
        let on_drag_end: DragHandler = Arc::new(move |_: web_sys::DragEvent| {
            set_state_end.set(DragState::default());
        });

        let set_state_over = set_state;
        let state_over = state;
        let on_drag_over: DragHandler = Arc::new(move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            let current = state_over.get_untracked();
            if current.source_index.is_some() && current.target_index != Some(index) {
                set_state_over.set(DragState {
                    source_index: current.source_index,
                    target_index: Some(index),
                });
            }
        });

        let set_state_leave = set_state;
        let state_leave = state;
        let on_drag_leave: DragHandler = Arc::new(move |_: web_sys::DragEvent| {
            let current = state_leave.get_untracked();
            if current.target_index == Some(index) {
                set_state_leave.set(DragState {
                    source_index: current.source_index,
                    target_index: None,
                });
            }
        });

        let set_state_drop = set_state;
        let state_drop = state;
        let on_drop: DragHandler = Arc::new(move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            let current = state_drop.get_untracked();
            if let Some(source) = current.source_index {
                if source != index {
                    let callback = on_reorder.get_value();
                    callback(Reorder {
                        source,
                        target: index,
                    });
                }
            }
            set_state_drop.set(DragState::default());
        });

        DragAttrs {
            inner_drag_start: on_drag_start,
            inner_drag_end: on_drag_end,
            inner_drag_over: on_drag_over,
            inner_drag_leave: on_drag_leave,
            inner_drop: on_drop,
        }
    }
}

/// Create a draggable list hook
///
/// The callback receives a `Reorder` struct that can be applied to your items vector.
///
/// ## Example
///
/// ```ignore
/// let (items, set_items) = signal(vec!["A", "B", "C"]);
///
/// let draggable = use_draggable(move |reorder| {
///     set_items.update(|items| reorder.apply(items));
/// });
/// ```
pub fn use_draggable<F>(on_reorder: F) -> Draggable
where
    F: Fn(Reorder) + Send + Sync + 'static,
{
    let (state, set_state) = signal(DragState::default());
    let on_reorder = StoredValue::new(std::sync::Arc::new(on_reorder) as ReorderCallback);

    Draggable {
        state: state.into(),
        set_state,
        on_reorder,
    }
}
