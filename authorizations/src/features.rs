//! Ecosystem feature registry — the single place gated features are
//! declared. Workers enforce and frontends render from these same consts,
//! so an entitlement id, its display name, and its locked-state copy can
//! never drift apart.
//!
//! Adding a gated feature = one entry here + a `require()` at the route +
//! a `gated()` wrapper at the widget.

crate::features! {
    /// Base entitlement — access to the tool at all. Gates the whole app;
    /// every qualifying partner role grants it.
    pub const APP_ACCESS = {
        id: "app.access",
        name: "Collection Explorer",
        locked_hint: "Access is granted through partner communities — hold a qualifying role to unlock",
    };
    /// Perceptual-hash reverse image search over indexed collections.
    pub const VISUAL_SEARCH = {
        id: "tools.visual-search",
        name: "Visual Search",
        locked_hint: "Hold a qualifying role in a partner Discord — sign in to unlock",
    };
}
