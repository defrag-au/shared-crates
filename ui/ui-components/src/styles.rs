//! UI Components Styles
//!
//! Combined stylesheet for all ui-components. Include this once in your app.
//!
//! ## Usage
//!
//! In your Leptos app's root component:
//!
//! ```ignore
//! use ui_components::STYLES;
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     view! {
//!         <style>{STYLES}</style>
//!         // ... rest of app
//!     }
//! }
//! ```
//!
//! Or import in your app's SCSS (if using trunk with SCSS):
//!
//! ```scss
//! @use "path/to/ui-components/src/styles/mod" as ui;
//! ```

use scss_macros::scss;

/// Combined CSS for all ui-components.
/// Include this once at the app root level.
// Touch to force SCSS recompile
pub const STYLES: &str = scss!("src/styles/mod.scss");
