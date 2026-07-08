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
///   role_id = "222222222222222222"   # Holder
///   features = ["tools.visual-search"]
///   [[server.grant]]
///   role_id = "333333333333333333"   # OG
///   features = ["tools.visual-search", "tools.pricing"]
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GuildRoleConfig {
    #[serde(default, rename = "server")]
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub guild_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, rename = "grant")]
    pub grants: Vec<RoleGrant>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleGrant {
    pub role_id: String,
    /// Entitlement ids this role grants (must match `authorizations::Feature`
    /// ids, e.g. `"tools.visual-search"`).
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

    /// Resolve the user's granted entitlement ids from their role ids per
    /// guild. Deduped and sorted (stable `ent` claim). A guild the user
    /// isn't in, or has no qualifying role in, contributes nothing.
    pub fn resolve(&self, roles_by_guild: &HashMap<String, Vec<String>>) -> Vec<String> {
        let mut ents: BTreeSet<String> = BTreeSet::new();
        for server in &self.servers {
            let Some(user_roles) = roles_by_guild.get(&server.guild_id) else {
                continue;
            };
            for grant in &server.grants {
                if user_roles.iter().any(|r| r == &grant.role_id) {
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
  [[server.grant]]
  role_id = "holder"
  features = ["tools.visual-search"]
  [[server.grant]]
  role_id = "og"
  features = ["tools.visual-search", "tools.pricing"]

[[server]]
guild_id = "g2"
  [[server.grant]]
  role_id = "member"
  features = ["tools.visual-search"]
"#;

    fn config() -> GuildRoleConfig {
        GuildRoleConfig::from_toml(CFG).unwrap()
    }

    #[test]
    fn parses_and_lists_guilds() {
        let c = config();
        assert_eq!(c.servers.len(), 2);
        assert_eq!(c.guild_ids(), vec!["g1", "g2"]);
    }

    #[test]
    fn resolves_union_across_guilds_and_roles() {
        let c = config();
        let mut roles = HashMap::new();
        roles.insert("g1".to_string(), vec!["og".to_string()]);
        roles.insert("g2".to_string(), vec!["member".to_string()]);
        // og grants visual-search + pricing; g2 member grants visual-search.
        assert_eq!(
            c.resolve(&roles),
            vec![
                "tools.pricing".to_string(),
                "tools.visual-search".to_string()
            ]
        );
    }

    #[test]
    fn non_qualifying_roles_grant_nothing() {
        let c = config();
        let mut roles = HashMap::new();
        roles.insert("g1".to_string(), vec!["random-role".to_string()]);
        assert!(c.resolve(&roles).is_empty());
        // A guild not in config is ignored even with matching-looking roles.
        let mut other = HashMap::new();
        other.insert("g99".to_string(), vec!["holder".to_string()]);
        assert!(c.resolve(&other).is_empty());
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
