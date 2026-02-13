//! Helper utilities for Leptos components
//!
//! This module provides utilities for working with Leptos component patterns,
//! particularly around children/slots which can be tricky in Leptos 0.8.

use leptos::children::ChildrenFn;
use leptos::prelude::*;
use std::sync::Arc;

/// Create a `ChildrenFn` from a closure that returns a view.
///
/// In Leptos 0.8, `ChildrenFn` is `Arc<dyn Fn() -> AnyView + Send + Sync>`.
/// This helper makes it ergonomic to create slot content for components
/// that accept `Option<ChildrenFn>` props.
///
/// ## Usage
///
/// ```ignore
/// use ui_components::{AssetCard, StatPill, children_fn};
///
/// <AssetCard
///     asset_id="..."
///     name="My Asset"
///     top_right=children_fn(|| view! {
///         <StatPill value=Signal::derive(|| "285".to_string()) icon="⚡".to_string() />
///     })
/// />
/// ```
///
/// ## Why this is needed
///
/// When passing slot content to component props of type `Option<ChildrenFn>`,
/// a bare closure `|| view! { ... }` won't compile because:
/// 1. Closures don't automatically coerce to `Arc<dyn Fn() -> AnyView + ...>`
/// 2. The `#[prop(into)]` attribute doesn't trigger `ToChildren` conversion for `Option` types
///
/// This helper wraps the closure in an `Arc` and calls `.into_any()` on the view output.
pub fn children_fn<F, V>(f: F) -> ChildrenFn
where
    F: Fn() -> V + Send + Sync + 'static,
    V: IntoView + 'static,
{
    Arc::new(move || f().into_any())
}
