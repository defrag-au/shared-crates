//! The ui-flow protocol for the gateway trigger-wiring admin surface.
//!
//! An admin surface subscribes to augminted-bots' `GatewayDO` over `/flow`
//! (snapshot + deltas) and sends edits back as actions — the same
//! server-authoritative model treasure-island uses. There is no REST admin
//! API; this is the one protocol.
//!
//! ## Why it lives in shared-crates
//!
//! There are **two renderers and one Durable Object**, and they are in
//! different repos: the standalone admin app ships with the gateway worker in
//! augminted-bots, and the client portal's pane ships from cnft.dev-workers.
//! Neither app repo depends on the other; they meet here.
//!
//! The wire and the widget are shared, the service stays home — the DO itself
//! remains in augminted-bots and is reached over HTTP/WS at runtime. Same
//! topology `augie-plugin` already uses. See
//! `augminted-bots/docs/ADMIN_SURFACE_CONSOLIDATION_DESIGN.md` §6.
//!
//! **Mechanism public, policy private.** This repo is public. "There is an
//! entitlement with role tiers and a daily allowance" belongs here; tier
//! values, real role ids, guild snowflakes and endpoint hostnames do not —
//! including in tests.

use std::collections::HashMap;

use gateway_wiring::{
    ActionTrace, AgentEntitlement, EventBinding, GatewayStatus, GuildInfo, GuildRole, GuildWiring,
    RecentActivity, MAX_RECENT_ACTIVITY,
};
use serde::{Deserialize, Serialize};

/// Who is looking at the admin surface, and therefore what they may change.
///
/// An enum rather than an `operator: bool`, because a boolean has to be read
/// alongside a name to mean anything and offers no place for a third audience
/// to land. This is the same shape the platform portal's `AuthPrincipal`
/// already uses, and it is the vocabulary three layers share: the worker
/// resolves it from verified claims, the DO enforces with it, and the
/// frontend renders from it.
///
/// **Ordered by authority, and deliberately not `Ord`-derived for
/// comparison** — ask the named predicate ([`may_set_entitlement`]) rather
/// than comparing variants, so adding an audience between these two can't
/// silently widen a `>=` somewhere.
///
/// [`may_set_entitlement`]: GatewayAudience::may_set_entitlement
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAudience {
    /// A guild's own admin. Their server, their trigger wiring — and nothing
    /// that decides what they pay.
    ///
    /// The default, and that direction matters: every place this is absent,
    /// unparseable or from an older encoding resolves to the audience with
    /// less authority.
    #[default]
    Client,
    /// Platform operator. Everything a client may do, plus the agent
    /// entitlement — who may ask, and how much.
    Operator,
}

impl GatewayAudience {
    /// Resolve from a session's entitlements.
    ///
    /// The single derivation, used by the worker (to tell the DO) and by the
    /// frontend (to decide what to render). Two implementations of "is this
    /// an operator" is how a screen ends up offering a control the backend
    /// refuses.
    pub fn from_entitlements(entitlements: &authorizations::EntitlementSet) -> Self {
        if entitlements.grants(&authorizations::features::GATEWAY_OPERATOR) {
            Self::Operator
        } else {
            Self::Client
        }
    }

    /// May this audience set a guild's agent entitlement?
    ///
    /// Named rather than a `== Operator` at each call site: the question is
    /// about the capability, and the set of audiences holding it is allowed
    /// to change without every caller being revisited.
    pub fn may_set_entitlement(self) -> bool {
        matches!(self, Self::Operator)
    }

    /// Wire spelling, for the header the worker sets on the DO upgrade.
    /// Same string serde writes, so a log line and a payload agree.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Operator => "operator",
        }
    }

    /// Parse the wire spelling. **Fails closed**: anything unrecognised is a
    /// [`Client`](Self::Client), never an operator. This parses a header that
    /// grants authority, so an unknown value must not be able to widen it.
    pub fn from_wire(value: &str) -> Self {
        match value {
            "operator" => Self::Operator,
            _ => Self::Client,
        }
    }
}

/// Everything the admin surface renders, in one snapshot. Small by
/// construction: a handful of guilds, each with a few rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayAdminState {
    pub status: GatewayStatus,
    /// Roster (rule counts filled), sorted by name.
    pub guilds: Vec<GuildInfo>,
    /// guild_id → wiring.
    pub wiring: HashMap<String, GuildWiring>,

    /// What the listener recently evaluated, oldest first.
    ///
    /// Its own field rather than a member of [`GatewayStatus`], because it is
    /// the one part of this snapshot that is **per-guild content** rather than
    /// global state — message previews, authors, and the agent's replies. That
    /// makes it the thing a client-facing pane must be filtered to, and it
    /// could not be filtered while it rode inside a type broadcast whole.
    #[serde(default)]
    pub activity: Vec<RecentActivity>,

    /// guild_id → that guild's roles, once fetched.
    ///
    /// **Not in the snapshot** — fetched per guild on demand (`RefreshRoles`),
    /// because it is one Discord round trip per guild and the operator edits
    /// one at a time. A missing entry means "not asked yet", which the picker
    /// renders differently from "asked, and this role is gone".
    #[serde(default)]
    pub roles: HashMap<String, Vec<GuildRole>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayAdminDelta {
    /// Connection status + counters — broadcast on every heartbeat while an
    /// admin is connected, so the dashboard ticks live.
    Status(GatewayStatus),
    /// A guild's wiring changed (any admin's save).
    WiringUpdated {
        guild_id: String,
        wiring: GuildWiring,
    },
    /// A guild's role roster arrived. Broadcast rather than sent to the asker,
    /// so a second admin editing the same guild gets it too — the roster is
    /// guild truth, not one session's view.
    RolesUpdated {
        guild_id: String,
        roles: Vec<GuildRole>,
    },

    /// The listener evaluated a message.
    ///
    /// One entry, carrying its own `guild_id`, so the fan-out can decide per
    /// recipient. This is the delta the `Status`/feed split existed to create:
    /// the feed is the only per-guild *content* on this surface, and a client
    /// must see only their own.
    ActivityAppended(Box<RecentActivity>),

    /// An executor reported what it did, for an entry already in the feed.
    ///
    /// Separate from the append because the trace arrives whenever augie
    /// finishes — seconds later, and out of order with other traffic. Matched
    /// on `message_id`; an entry that has already aged out is simply not
    /// found, which loses a trace rather than corrupting the feed.
    ActivityTraced {
        guild_id: String,
        message_id: String,
        trace: Box<ActionTrace>,
    },
}

impl GatewayAdminState {
    pub fn apply(&mut self, delta: &GatewayAdminDelta) {
        match delta {
            GatewayAdminDelta::Status(status) => self.status = status.clone(),
            GatewayAdminDelta::WiringUpdated { guild_id, wiring } => {
                self.wiring.insert(guild_id.clone(), wiring.clone());
                if let Some(guild) = self.guilds.iter_mut().find(|g| &g.id == guild_id) {
                    guild.binding_count = wiring.bindings.len();
                }
            }
            GatewayAdminDelta::RolesUpdated { guild_id, roles } => {
                self.roles.insert(guild_id.clone(), roles.clone());
            }
            GatewayAdminDelta::ActivityAppended(entry) => {
                self.activity.push(entry.as_ref().clone());
                // Bounded with the SAME constant the server rings, so a long
                // session cannot accumulate a feed the server never held.
                let excess = self.activity.len().saturating_sub(MAX_RECENT_ACTIVITY);
                self.activity.drain(..excess);
            }
            GatewayAdminDelta::ActivityTraced {
                message_id, trace, ..
            } => {
                // Newest first: a message id can repeat across a long enough
                // feed only if Discord reissued one, but searching backwards
                // matches the server's own rule and costs nothing.
                if let Some(entry) = self
                    .activity
                    .iter_mut()
                    .rev()
                    .find(|entry| &entry.message_id == message_id)
                {
                    entry.trace = Some(trace.as_ref().clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_wiring::RecentActivity;

    fn entry(message_id: &str) -> RecentActivity {
        RecentActivity {
            at_ms: 0.0,
            message_id: message_id.into(),
            guild_id: "g".into(),
            author: "a".into(),
            preview: "p".into(),
            matched_binding: None,
            note: None,
            trace: None,
        }
    }

    /// A client applying appends over a long session must ring at the SAME
    /// bound the server does, or it accumulates a feed the server no longer
    /// holds — and the next snapshot would silently shorten it.
    #[test]
    fn the_feed_rings_at_the_shared_bound() {
        let mut state = GatewayAdminState::default();
        for i in 0..(MAX_RECENT_ACTIVITY * 2) {
            state.apply(&GatewayAdminDelta::ActivityAppended(Box::new(entry(
                &i.to_string(),
            ))));
        }
        assert_eq!(state.activity.len(), MAX_RECENT_ACTIVITY);
        // Oldest dropped, newest kept — the ordering the UI reverses to show
        // the thing you just typed first.
        assert_eq!(
            state.activity.last().unwrap().message_id,
            (MAX_RECENT_ACTIVITY * 2 - 1).to_string()
        );
    }

    /// A trace arrives seconds after its entry, matched on message id. An
    /// entry that has aged out is a no-op, not a panic and not a new row.
    #[test]
    fn a_trace_finds_its_entry_or_is_dropped() {
        let mut state = GatewayAdminState::default();
        state.apply(&GatewayAdminDelta::ActivityAppended(Box::new(entry("m1"))));

        let traced = |id: &str| GatewayAdminDelta::ActivityTraced {
            guild_id: "g".into(),
            message_id: id.into(),
            trace: Box::new(gateway_wiring::ActionTrace {
                reply: Some("hello".into()),
                ..Default::default()
            }),
        };

        state.apply(&traced("m1"));
        assert!(state.activity[0].trace.is_some());

        state.apply(&traced("long-gone"));
        assert_eq!(state.activity.len(), 1, "a stray trace adds no row");
    }
}

/// One-shot notifications. None yet — the enum exists so the envelope type
/// is fixed and adding one later isn't a protocol change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayAdminEvent {}

/// Actions the admin surface can send.
///
/// **Split by audience, not by convenience.** Bindings and the agent
/// entitlement used to travel together in one `SetWiring` carrying the whole
/// [`GuildWiring`], which was safe only because the entire screen was
/// operator-only — `gateway.admin` came from a role in a server the customer
/// does not administer, so hiding the entitlement controls was the same thing
/// as denying them.
///
/// Once this surface becomes a pane in a client-facing portal that stops being
/// true, and a single action carrying both fields makes "hidden in the UI" the
/// only barrier: a client need only send the field the screen didn't show
/// them. Two actions let the DO refuse the operator one on its own merits.
/// See `docs/ADMIN_SURFACE_CONSOLIDATION_DESIGN.md` §6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatewayAdminAction {
    /// Replace a guild's event bindings — the client-editable half. Their
    /// server, their vocabulary. Validated server-side; rejected via
    /// `action_err` with a readable reason.
    ///
    /// Carries `Vec<EventBinding>` rather than a `GuildWiring` precisely so
    /// it *cannot* express an entitlement change. A struct with the field
    /// present-but-ignored would rely on the server remembering to drop it;
    /// this way there is nothing to drop.
    SetBindings {
        guild_id: String,
        bindings: Vec<EventBinding>,
    },

    /// Set a guild's agent entitlement — who may ask, and how much.
    ///
    /// **Operator only**, enforced in the DO. `None` un-entitles the guild
    /// (the agent goes silent), which is a distinct state from an entitlement
    /// with no tiers.
    SetAgentEntitlement {
        guild_id: String,
        entitlement: Option<AgentEntitlement>,
    },
    /// Fetch this guild's roles from Discord, so role fields can be chosen by
    /// name. On demand rather than in the snapshot: one round trip per guild,
    /// and the operator edits one guild at a time.
    RefreshRoles { guild_id: String },
    /// Wake the gateway connection, optionally with an auto-sleep TTL (the
    /// dev pattern). The flow connection itself proves `gateway.admin`, so
    /// lifecycle from the admin surface needs no operator token.
    Start { ttl_hours: Option<f64> },
    /// Put the gateway to sleep (persisted — survives eviction).
    Stop,
}

/// The full server→client message envelope.
pub type ServerMsg =
    ui_flow_protocol::ServerMessage<GatewayAdminState, GatewayAdminDelta, GatewayAdminEvent>;

/// The full client→server message envelope.
pub type ClientMsg = ui_flow_protocol::ClientMessage<GatewayAdminAction>;
