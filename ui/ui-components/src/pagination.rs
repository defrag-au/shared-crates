//! Pagination Component
//!
//! A reusable pagination control with page numbers, navigation buttons,
//! and optional page jump input.
//!
//! ## Features
//!
//! - First/Last page buttons
//! - Previous/Next buttons
//! - Numbered page buttons with smart ellipsis
//! - Current page indicator
//! - Optional page jump input
//! - Configurable visible page range
//! - Adaptive page size based on grid container width
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::{Pagination, use_pagination, use_adaptive_pagination};
//!
//! // Fixed page size
//! let pagination = use_pagination(items.len(), 24);
//!
//! // Adaptive page size based on grid width
//! let grid_ref = NodeRef::<Div>::new();
//! let pagination = use_adaptive_pagination(items.len(), grid_ref, 3); // 3 rows per page
//!
//! <div node_ref=grid_ref class="my-grid">
//!     // grid items...
//! </div>
//! <Pagination state=pagination />
//! ```

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Pagination state - tracks current page and calculates derived values
#[derive(Clone, Copy)]
pub struct PaginationState {
    /// Current page (1-indexed)
    pub current_page: RwSignal<usize>,
    /// Total number of items
    pub total_items: usize,
    /// Items per page (can be reactive for adaptive pagination)
    page_size: RwSignal<usize>,
}

impl PaginationState {
    /// Create new pagination state
    pub fn new(total_items: usize, page_size: usize) -> Self {
        Self {
            current_page: RwSignal::new(1),
            total_items,
            page_size: RwSignal::new(page_size.max(1)),
        }
    }

    /// Get current page size
    pub fn page_size(&self) -> usize {
        self.page_size.get()
    }

    /// Set page size (used by adaptive pagination)
    pub fn set_page_size(&self, size: usize) {
        // Use get_untracked since we're in a non-reactive context (ResizeObserver callback)
        let old_size = self.page_size.get_untracked();
        let new_size = size.max(1);

        if old_size != new_size {
            // Try to keep roughly the same position in the list
            let current_page = self.current_page.get_untracked();
            let first_item_index = (current_page - 1) * old_size;
            let new_page = (first_item_index / new_size) + 1;

            self.page_size.set(new_size);
            // Calculate total_pages inline to avoid reactive read
            let total_pages = self.total_items.div_ceil(new_size);
            self.current_page.set(new_page.clamp(1, total_pages.max(1)));
        }
    }

    /// Total number of pages
    pub fn total_pages(&self) -> usize {
        let size = self.page_size.get();
        self.total_items.div_ceil(size)
    }

    /// Check if there's a previous page
    pub fn has_prev(&self) -> bool {
        self.current_page.get() > 1
    }

    /// Check if there's a next page
    pub fn has_next(&self) -> bool {
        self.current_page.get() < self.total_pages()
    }

    /// Go to a specific page (bounds checked)
    pub fn go_to(&self, page: usize) {
        let page = page.clamp(1, self.total_pages().max(1));
        self.current_page.set(page);
    }

    /// Go to first page
    pub fn go_first(&self) {
        self.go_to(1);
    }

    /// Go to last page
    pub fn go_last(&self) {
        self.go_to(self.total_pages());
    }

    /// Go to previous page
    pub fn go_prev(&self) {
        let current = self.current_page.get();
        if current > 1 {
            self.go_to(current - 1);
        }
    }

    /// Go to next page
    pub fn go_next(&self) {
        let current = self.current_page.get();
        if current < self.total_pages() {
            self.go_to(current + 1);
        }
    }

    /// Get the start index for current page (0-indexed)
    pub fn start_index(&self) -> usize {
        (self.current_page.get() - 1) * self.page_size.get()
    }

    /// Get the end index for current page (exclusive, 0-indexed)
    pub fn end_index(&self) -> usize {
        (self.start_index() + self.page_size.get()).min(self.total_items)
    }

    /// Get a slice of items for the current page
    pub fn slice<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.start_index();
        let end = self.end_index().min(items.len());
        &items[start..end]
    }

    /// Get items for the current page (cloned)
    pub fn page_items<T: Clone>(&self, items: &[T]) -> Vec<T> {
        self.slice(items).to_vec()
    }
}

/// Create pagination state for a collection with fixed page size
pub fn use_pagination(total_items: usize, page_size: usize) -> PaginationState {
    PaginationState::new(total_items, page_size)
}

/// Create adaptive pagination that adjusts page size based on grid container width
///
/// Uses ResizeObserver to monitor the grid container and getComputedStyle to
/// determine how many columns CSS Grid actually created. Page size is set to
/// columns * rows_per_page.
///
/// # Arguments
/// * `total_items` - Total number of items to paginate
/// * `grid_ref` - NodeRef to the grid container element
/// * `rows_per_page` - Desired number of rows per page (columns are detected automatically)
/// * `fallback_page_size` - Page size to use before grid is measured (default: 24)
pub fn use_adaptive_pagination(
    total_items: usize,
    grid_ref: NodeRef<leptos::html::Div>,
    rows_per_page: usize,
    fallback_page_size: Option<usize>,
) -> PaginationState {
    let fallback = fallback_page_size.unwrap_or(24);
    let state = PaginationState::new(total_items, fallback);

    // Set up ResizeObserver to detect grid size changes
    Effect::new(move |_| {
        let Some(grid_el) = grid_ref.get() else {
            return;
        };

        // Clone the element for the initial measurement
        let grid_element: web_sys::Element = grid_el.clone().into();

        // Create callback for ResizeObserver
        // Use the entry's target() instead of capturing element reference
        let state_clone = state;
        let rows = rows_per_page;
        let callback = Closure::wrap(Box::new(
            move |entries: js_sys::Array, _observer: web_sys::ResizeObserver| {
                // Get target from the ResizeObserverEntry to avoid stale reference
                if let Some(entry) = entries.get(0).dyn_ref::<web_sys::ResizeObserverEntry>() {
                    let target = entry.target();
                    if let Some(columns) = get_grid_column_count(&target) {
                        let new_page_size = columns * rows;
                        state_clone.set_page_size(new_page_size);
                    }
                }
            },
        )
            as Box<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>);

        // Create and start observing
        if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
            observer.observe(&grid_element);
            callback.forget(); // Keep callback alive

            // Do an initial measurement
            if let Some(columns) = get_grid_column_count(&grid_element) {
                let new_page_size = columns * rows_per_page;
                state.set_page_size(new_page_size);
            }
        }
    });

    state
}

/// Get the number of columns in a CSS Grid container by inspecting computed style
fn get_grid_column_count(element: &web_sys::Element) -> Option<usize> {
    let window = web_sys::window()?;
    let styles = window.get_computed_style(element).ok()??;
    let columns = styles.get_property_value("grid-template-columns").ok()?;

    // grid-template-columns returns something like "120px 120px 120px 120px"
    // or "none" if not a grid, or has named lines like "[start] 120px [end]"
    if columns.is_empty() || columns == "none" {
        return None;
    }

    // Count the number of pixel/fr values (filter out grid line names in brackets)
    let count = columns
        .split_whitespace()
        .filter(|part| !part.starts_with('[') && !part.ends_with(']') && !part.is_empty())
        .count();

    if count > 0 {
        Some(count)
    } else {
        None
    }
}

/// Pagination control component
#[component]
pub fn Pagination(
    /// Pagination state
    state: PaginationState,

    /// Number of page buttons to show on each side of current page
    #[prop(optional, default = 0)]
    sibling_count: usize,

    /// Show the page jump input
    #[prop(optional, default = true)]
    show_page_jump: bool,

    /// Show "X of Y" indicator
    #[prop(optional, default = true)]
    show_page_info: bool,

    /// Additional CSS class
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    // Page jump input state
    let (jump_input, set_jump_input) = signal(String::new());

    // Navigation handlers
    let go_first = move |_| state.go_first();
    let go_prev = move |_| state.go_prev();
    let go_next = move |_| state.go_next();
    let go_last = move |_| state.go_last();

    // Handle page jump input
    let handle_jump_input = move |ev: web_sys::Event| {
        let target = event_target::<web_sys::HtmlInputElement>(&ev);
        set_jump_input.set(target.value());
    };

    let handle_jump_submit = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" {
            if let Ok(page) = jump_input.get().parse::<usize>() {
                state.go_to(page);
                set_jump_input.set(String::new());
            }
        }
    };

    // Calculate which page numbers to show (reactive to page size changes)
    let page_numbers = move || {
        let current = state.current_page.get();
        let total = state.total_pages();

        if total <= 1 {
            return vec![];
        }

        let mut pages: Vec<PageItem> = Vec::new();

        // Always show first page
        pages.push(PageItem::Page {
            num: 1,
            is_sibling: false,
        });

        // Calculate range around current page
        let range_start = current.saturating_sub(sibling_count).max(2);
        let range_end = (current + sibling_count).min(total.saturating_sub(1));

        // Add ellipsis after first if needed
        if range_start > 2 {
            pages.push(PageItem::Ellipsis);
        }

        // Add pages in range (these are "siblings" - not first, last, or current)
        for page in range_start..=range_end {
            if page > 1 && page < total {
                let is_sibling = page != current;
                pages.push(PageItem::Page {
                    num: page,
                    is_sibling,
                });
            }
        }

        // Add ellipsis before last if needed
        if range_end < total.saturating_sub(1) {
            pages.push(PageItem::Ellipsis);
        }

        // Always show last page (if more than 1 page)
        if total > 1 {
            pages.push(PageItem::Page {
                num: total,
                is_sibling: false,
            });
        }

        pages
    };

    let wrapper_class = move || {
        let mut classes = vec!["ui-pagination"];
        if let Some(ref c) = class {
            classes.push(c);
        }
        classes.join(" ")
    };

    view! {
        <div class=wrapper_class>
            // First page button
            <button
                class="ui-pagination__btn ui-pagination__btn--nav ui-pagination__btn--first"
                on:click=go_first
                disabled=move || !state.has_prev()
                title="First page"
            >
                "«"
            </button>

            // Previous button
            <button
                class="ui-pagination__btn ui-pagination__btn--nav ui-pagination__btn--prev"
                on:click=go_prev
                disabled=move || !state.has_prev()
                title="Previous page"
            >
                "‹"
            </button>

            // Simple current page indicator (shown on mobile)
            <span class="ui-pagination__current">
                {move || state.current_page.get()}
            </span>

            // Page numbers (hidden on mobile)
            <div class="ui-pagination__pages">
                {move || {
                    page_numbers()
                        .into_iter()
                        
                        .map(|item| {
                            match item {
                                PageItem::Page { num, is_sibling } => {
                                    let is_current = move || state.current_page.get() == num;
                                    let btn_class = move || {
                                        let mut classes = String::from("ui-pagination__btn ui-pagination__btn--page");
                                        if is_current() {
                                            classes.push_str(" ui-pagination__btn--active");
                                        }
                                        if is_sibling {
                                            classes.push_str(" ui-pagination__btn--sibling");
                                        }
                                        classes
                                    };
                                    view! {
                                        <button
                                            class=btn_class
                                            on:click=move |_| state.go_to(num)
                                            disabled=is_current
                                        >
                                            {num}
                                        </button>
                                    }.into_any()
                                }
                                PageItem::Ellipsis => {
                                    view! {
                                        <span class="ui-pagination__ellipsis">
                                            "…"
                                        </span>
                                    }.into_any()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                }}
            </div>

            // Next button
            <button
                class="ui-pagination__btn ui-pagination__btn--nav ui-pagination__btn--next"
                on:click=go_next
                disabled=move || !state.has_next()
                title="Next page"
            >
                "›"
            </button>

            // Last page button
            <button
                class="ui-pagination__btn ui-pagination__btn--nav ui-pagination__btn--last"
                on:click=go_last
                disabled=move || !state.has_next()
                title="Last page"
            >
                "»"
            </button>

            // Page info
            <Show when=move || show_page_info fallback=|| ()>
                <span class="ui-pagination__info">
                    {move || format!("({} of {})", state.current_page.get(), state.total_pages())}
                </span>
            </Show>

            // Page jump input
            <Show when=move || show_page_jump fallback=|| ()>
                <input
                    type="number"
                    class="ui-pagination__jump"
                    placeholder="Go to"
                    min="1"
                    max=move || state.total_pages()
                    prop:value=move || jump_input.get()
                    on:input=handle_jump_input
                    on:keydown=handle_jump_submit
                />
            </Show>
        </div>
    }
}

/// Internal enum for page number display
#[derive(Clone, Copy)]
enum PageItem {
    Page { num: usize, is_sibling: bool },
    Ellipsis,
}
