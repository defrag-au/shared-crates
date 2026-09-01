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
    /// Market scenario modelling — re-price a collection's listing book
    /// against a hypothetical floor to surface what becomes under-priced if
    /// the floor moves.
    ///
    /// **Deliberately granted to nobody yet.** The capability is built and
    /// enforced, but which holding earns it is an open decision, so today only
    /// wildcard (`*`) operator tokens see it. Naming the entitlement now means
    /// turning it on later is a gate-config change, not a code change — and
    /// nothing ships to general users in the meantime.
    pub const MARKET_SCENARIOS = {
        id: "tools.market-scenarios",
        name: "Market Scenarios",
        locked_hint: "Advanced market tooling — access tier not yet assigned",
    };
    /// Operator control surface — add/edit/delete tracked collections,
    /// trigger syncs, and the visual-analysis tooling. Granted to specific
    /// Discord accounts via the gate config; the operator `X-Debug-Token`
    /// bypasses it for shell/CLI ops.
    pub const ADMIN = {
        id: "admin.access",
        name: "Admin",
        locked_hint: "Operator access — granted to specific Discord accounts",
    };
    /// Augminted gateway-listener admin surface — live editing of chat
    /// trigger wiring (which utterances invoke which bot commands, with
    /// which reactions) per guild. Granted via a Gateway Admin role in
    /// HODLCroft; enforced by the augminted-bots gateway worker.
    pub const GATEWAY_ADMIN = {
        id: "gateway.admin",
        name: "Gateway Admin",
        locked_hint: "Gateway operator access — hold the Gateway Admin role in HODLCroft",
    };
    /// Platform-operator authority over the gateway: setting a guild's agent
    /// **entitlement** — who may ask, and how much.
    ///
    /// Deliberately separate from [`GATEWAY_ADMIN`], which is now the
    /// *client* tier: their server, their trigger wiring. The entitlement is
    /// what we sold them, so a paying guild's own admins must not be able to
    /// grant themselves more of it.
    ///
    /// This split exists because the gate used to be the SCREEN — the whole
    /// admin app was operator-only, so hiding controls was sufficient. Once
    /// the surface is a pane in a client-facing portal that stops being true,
    /// and "only operators can see this section" is a much weaker guarantee
    /// than "the DO refuses it". See `ADMIN_SURFACE_CONSOLIDATION_DESIGN.md`
    /// §6.
    pub const GATEWAY_OPERATOR = {
        id: "gateway.operator",
        name: "Gateway Operator",
        locked_hint: "Platform-operator access — agent entitlements are set by Augminted",
    };
}
