//! Ecosystem feature registry — the single place gated features are
//! declared. Workers enforce and frontends render from these same consts,
//! so an entitlement id, its display name, and its locked-state copy can
//! never drift apart.
//!
//! Adding a gated feature = one entry here + a `require()` at the route +
//! a `gated()` wrapper at the widget.

crate::features! {
    /// Perceptual-hash reverse image search over indexed collections.
    pub const VISUAL_SEARCH = {
        id: "tools.visual-search",
        name: "Visual Search",
        locked_hint: "Collector-gated — run /collector in a partner Discord to unlock",
    };
}
