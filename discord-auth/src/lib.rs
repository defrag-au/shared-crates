//! Reusable Discord-OAuth role gating.
//!
//! Adds "sign in with Discord, entitlements come from your server roles" to
//! any egui app + worker in the ecosystem. Three parts, each a separate
//! feature so an app pulls only what it needs:
//!
//! - **core** (always): the operator-owned config — which `(guild, role)`
//!   grants which feature entitlements — plus the pure resolution logic and
//!   the OAuth authorize-URL builder. This is the entire control surface:
//!   a server grants nothing unless it's in the config.
//! - **`worker`**: [`worker::handle_callback`] — the OAuth callback mixin.
//!   A worker mounts one route; the handler exchanges the code, reads the
//!   user's roles in each configured guild, resolves entitlements, mints an
//!   `authorizations` session JWT, and redirects back to the app.
//! - **`web`**: [`web::Session`] — the frontend half: begin-login redirect,
//!   token load from URL fragment / localStorage, and the parsed
//!   entitlement set for gating UI.
//!
//! The resolved entitlements ride in the session JWT's `ent` claim, which
//! the `authorizations` crate consumes unchanged — so gating a feature is
//! the same `require()` / `gated()` call regardless of how the session was
//! obtained.
//!
//! ## The role-read scope
//!
//! Roles are read per-guild via `GET /users/@me/guilds/{guild}/member`
//! using the **user's** OAuth token (`guilds.members.read` scope) — the
//! design intent being that no bot need be present in the server. Discord's
//! historical behaviour here has been ambiguous in community docs; if a
//! given server requires the app's bot to be a member for that endpoint to
//! return data, the effect is only that the server's grants don't resolve
//! until the bot is invited — an onboarding step, not a code change. The
//! resolution logic is identical either way.

use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

pub mod scopes {
    //! OAuth2 scope constants.
    pub const IDENTIFY: &str = "identify";
    pub const GUILDS: &str = "guilds";
    pub const GUILDS_MEMBERS_READ: &str = "guilds.members.read";

    /// The scope set this crate's role gating needs.
    pub const REQUIRED: &[&str] = &[IDENTIFY, GUILDS, GUILDS_MEMBERS_READ];
}

// ============================================================================
// Config — the operator's control surface
// ============================================================================

/// The guild → role → feature grant map. Checked into the consuming worker
/// (e.g. via `include_str!` of a TOML file) so onboarding a server is a
/// reviewed, versioned edit.
///
/// ```toml
/// [[server]]
/// guild_id = "111111111111111111"
/// label = "SpaceBudz"
///   [[server.grant]]
///   roles = ["222222222222222222", "333333333333333333"]  # any of these
///   features = ["tools.visual-search"]
///   [[server.grant]]
///   roles = ["444444444444444444"]        # OG
///   match = "all"                         # require every listed role
///   features = ["tools.visual-search", "tools.pricing"]
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GuildRoleConfig {
    #[serde(default, rename = "server")]
    pub servers: Vec<ServerConfig>,
    /// Direct per-user grants, matched by Discord user id independent of any
    /// server role. The operator/admin surface: a specific account gets a
    /// feature (e.g. `admin.access`) without needing a role in a guild.
    #[serde(default, rename = "admin")]
    pub admins: Vec<AdminGrant>,
}

/// A grant keyed to a specific Discord account rather than a server role —
/// the way operators/admins are named. Checked into the config alongside the
/// server grants, so who is an admin is reviewed + versioned + shipped.
///
/// ```toml
/// [[admin]]
/// user_id = "179744071361757184"
/// features = ["admin.access"]
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct AdminGrant {
    /// The Discord user id (the JWT/OAuth `sub`).
    pub user_id: String,
    /// Entitlement ids this account is granted (must match
    /// `authorizations::Feature` ids).
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub guild_id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Optional Discord invite URL, surfaced on the requirements screen so
    /// a logged-in-but-unqualified user knows where to go.
    #[serde(default)]
    pub invite_url: Option<String>,
    #[serde(default, rename = "grant")]
    pub grants: Vec<RoleGrant>,
}

/// A community a user could join to gain a feature — shown on the
/// requirements screen. Derived from the config; safe to expose publicly
/// (only display names, no role ids, just where to go).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessProvider {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite_url: Option<String>,
    /// Human-readable prompts describing what to do to qualify (from the
    /// grants' `requirement`). One per feature-granting grant, deduped.
    /// Empty when the config didn't describe them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
}

/// Response shape for a "what unlocks this feature" endpoint — shared by
/// the worker (serialize) and the frontend requirements screen
/// (deserialize).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthRequirements {
    /// The feature id the providers unlock.
    pub feature: String,
    /// Communities that grant it.
    pub providers: Vec<AccessProvider>,
}

/// How a grant's `roles` are matched against the user's roles.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    /// Holding ANY of the listed roles qualifies (the default).
    #[default]
    Any,
    /// The user must hold EVERY listed role.
    All,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleGrant {
    /// Role ids that satisfy this grant. Matched any-of by default (see
    /// `match_mode`) — a partner typically lists the set of roles it wants
    /// to grant access, growing it over time without touching code.
    pub roles: Vec<String>,
    /// Optional human-readable prompt describing what to do to qualify —
    /// shown on the requirements screen (role ids are opaque). Can explain
    /// *how* to earn the role, not just its name, e.g. "Hold the Deckhand
    /// role — verify an OG NFT in #verify". Purely cosmetic; matching is
    /// always by id.
    #[serde(default)]
    pub requirement: Option<String>,
    /// Match semantics for `roles`. TOML key `match`; defaults to `any`.
    #[serde(default, rename = "match")]
    pub match_mode: MatchMode,
    /// Entitlement ids this grant confers (must match
    /// `authorizations::Feature` ids, e.g. `"tools.visual-search"`).
    pub features: Vec<String>,
}

impl GuildRoleConfig {
    /// Parse from TOML. Consuming workers typically
    /// `GuildRoleConfig::from_toml(include_str!("discord_gate.toml"))`.
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// The guild ids to check at login — the config IS the allowlist, so
    /// only these servers are ever queried.
    pub fn guild_ids(&self) -> Vec<&str> {
        self.servers.iter().map(|s| s.guild_id.as_str()).collect()
    }

    /// Communities whose roles grant `feature_id` — for the requirements
    /// screen ("join one of these to gain access"). A server with no label
    /// falls back to its guild id. Exposes only label + invite (no role
    /// ids), so it's safe to serve publicly.
    pub fn access_providers(&self, feature_id: &str) -> Vec<AccessProvider> {
        self.servers
            .iter()
            .filter(|s| {
                s.grants
                    .iter()
                    .any(|g| g.features.iter().any(|f| f == feature_id))
            })
            .map(|s| {
                // Collect the requirement prompt of every grant that confers
                // this feature, deduped in first-seen order.
                let mut requirements: Vec<String> = Vec::new();
                for g in &s.grants {
                    if g.features.iter().any(|f| f == feature_id) {
                        if let Some(req) = &g.requirement {
                            if !requirements.contains(req) {
                                requirements.push(req.clone());
                            }
                        }
                    }
                }
                AccessProvider {
                    label: s.label.clone().unwrap_or_else(|| s.guild_id.clone()),
                    invite_url: s.invite_url.clone(),
                    requirements,
                }
            })
            .collect()
    }

    /// Resolve the user's granted entitlement ids from their Discord id +
    /// their role ids per guild. Deduped and sorted (stable `ent` claim).
    /// A guild the user isn't in, or has no qualifying role in, contributes
    /// nothing; `[[admin]]` grants matching `user_id` contribute regardless
    /// of guild membership.
    pub fn resolve(
        &self,
        user_id: &str,
        roles_by_guild: &HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        let mut ents: BTreeSet<String> = BTreeSet::new();
        // Direct per-user (admin) grants.
        for admin in &self.admins {
            if admin.user_id == user_id {
                ents.extend(admin.features.iter().cloned());
            }
        }
        // Server role grants.
        for server in &self.servers {
            let Some(user_roles) = roles_by_guild.get(&server.guild_id) else {
                continue;
            };
            for grant in &server.grants {
                let qualifies = match grant.match_mode {
                    MatchMode::Any => grant.roles.iter().any(|r| user_roles.contains(r)),
                    MatchMode::All => grant.roles.iter().all(|r| user_roles.contains(r)),
                };
                if qualifies {
                    ents.extend(grant.features.iter().cloned());
                }
            }
        }
        ents.into_iter().collect()
    }
}

// ============================================================================
// OAuth authorize URL
// ============================================================================

/// Build the Discord authorize URL to redirect the user to. `state` is the
/// caller's CSRF/return token (echoed back to the callback).
pub fn authorize_url(client_id: &str, redirect_uri: &str, scopes: &[&str], state: &str) -> String {
    format!(
        "https://discord.com/oauth2/authorize?response_type=code\
         &client_id={client_id}\
         &scope={}\
         &redirect_uri={}\
         &state={}\
         &prompt=none",
        percent_encode(&scopes.join(" ")),
        percent_encode(redirect_uri),
        percent_encode(state),
    )
}

/// Minimal RFC 3986 percent-encoding for query components (encodes
/// everything outside the unreserved set). Keeps the crate dep-light.
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(feature = "worker")]
pub mod worker;

#[cfg(feature = "web")]
pub mod web;

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: &str = r#"
[[server]]
guild_id = "g1"
label = "Alpha"
  # Any of several roles (default match) grants visual-search.
  [[server.grant]]
  roles = ["holder", "deckhand"]
  features = ["tools.visual-search"]
  # A distinct grant with extra features.
  [[server.grant]]
  roles = ["og"]
  features = ["tools.visual-search", "tools.pricing"]
  # All-of: requires BOTH roles.
  [[server.grant]]
  roles = ["founder", "verified"]
  match = "all"
  features = ["tools.pricing"]

[[server]]
guild_id = "g2"
  [[server.grant]]
  roles = ["member"]
  features = ["tools.visual-search"]

[[admin]]
user_id = "admin-uid"
features = ["admin.access"]
"#;

    fn config() -> GuildRoleConfig {
        GuildRoleConfig::from_toml(CFG).unwrap()
    }

    #[test]
    fn parses_and_lists_guilds() {
        let c = config();
        assert_eq!(c.servers.len(), 2);
        assert_eq!(c.guild_ids(), vec!["g1", "g2"]);
        // Default match mode is Any.
        assert_eq!(c.servers[0].grants[0].match_mode, MatchMode::Any);
        assert_eq!(c.servers[0].grants[2].match_mode, MatchMode::All);
    }

    #[test]
    fn any_of_matches_on_a_single_listed_role() {
        let c = config();
        let mut roles = HashMap::new();
        // Holding just "deckhand" (one of the any-of set) qualifies.
        roles.insert("g1".to_string(), vec!["deckhand".to_string()]);
        assert_eq!(
            c.resolve("u1", &roles),
            vec!["tools.visual-search".to_string()]
        );
    }

    #[test]
    fn resolves_union_across_guilds_and_roles() {
        let c = config();
        let mut roles = HashMap::new();
        roles.insert("g1".to_string(), vec!["og".to_string()]);
        roles.insert("g2".to_string(), vec!["member".to_string()]);
        // og grants visual-search + pricing; g2 member grants visual-search.
        assert_eq!(
            c.resolve("u1", &roles),
            vec![
                "tools.pricing".to_string(),
                "tools.visual-search".to_string()
            ]
        );
    }

    #[test]
    fn admin_grant_resolves_by_user_id_without_any_roles() {
        let c = config();
        let empty = HashMap::new();
        // The named admin id gets admin.access with no guild roles at all.
        assert_eq!(
            c.resolve("admin-uid", &empty),
            vec!["admin.access".to_string()]
        );
        // A different id gets nothing from the admin table.
        assert!(c.resolve("someone-else", &empty).is_empty());
    }

    #[test]
    fn all_of_requires_every_role() {
        let c = config();
        // Only one of the two required roles → the all-of grant misses.
        let mut one = HashMap::new();
        one.insert("g1".to_string(), vec!["founder".to_string()]);
        assert!(c.resolve("u1", &one).is_empty());
        // Both → grant applies.
        let mut both = HashMap::new();
        both.insert(
            "g1".to_string(),
            vec!["founder".to_string(), "verified".to_string()],
        );
        assert_eq!(c.resolve("u1", &both), vec!["tools.pricing".to_string()]);
    }

    #[test]
    fn non_qualifying_roles_grant_nothing() {
        let c = config();
        let mut roles = HashMap::new();
        roles.insert("g1".to_string(), vec!["random-role".to_string()]);
        assert!(c.resolve("u1", &roles).is_empty());
        // A guild not in config is ignored even with matching-looking roles.
        let mut other = HashMap::new();
        other.insert("g99".to_string(), vec!["holder".to_string()]);
        assert!(c.resolve("u1", &other).is_empty());
    }

    #[test]
    fn authorize_url_encodes_scope_and_redirect() {
        let u = authorize_url(
            "client123",
            "https://ownership.cnft.dev/auth/discord/callback",
            scopes::REQUIRED,
            "nonce",
        );
        assert!(u.contains("client_id=client123"));
        assert!(u.contains("scope=identify%20guilds%20guilds.members.read"));
        assert!(
            u.contains("redirect_uri=https%3A%2F%2Fownership.cnft.dev%2Fauth%2Fdiscord%2Fcallback")
        );
        assert!(u.contains("state=nonce"));
    }
}
