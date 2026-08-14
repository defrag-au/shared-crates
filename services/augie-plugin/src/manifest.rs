//! What a plugin advertises: [`ServiceManifest`], fetched by Augie at
//! config-refresh time and merged into the guild's registered command set.

use serde::{Deserialize, Serialize};

/// A plugin's complete command surface.
///
/// Fetched from `GET /_augie/manifest` during the `RefreshBotConfigs` chain —
/// **not** at interaction time. Discord command registration is a deliberate,
/// rate-limited, guild-scoped operation; it belongs to an explicit refresh, not
/// to a request path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceManifest {
    /// Stable identifier for this plugin, e.g. `"holder-map"`. Must match the
    /// key a guild opts in under, and must not change once registered.
    pub service: String,

    /// Bumped whenever `commands` changes. Augie records the version it last
    /// registered and warns when a live manifest differs, so "I changed the
    /// command but nothing happened" surfaces as a log line rather than a
    /// mystery. It is not a semver constraint — any change is just a change.
    pub version: String,

    /// Top-level commands. Names must be unique across *all* sources for a
    /// guild (TOML config and every other plugin); a collision fails the
    /// refresh rather than picking a winner.
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
}

/// One command, or one subcommand of a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Discord command name. Lowercase, no spaces.
    pub name: String,

    /// Shown in the Discord command picker.
    pub description: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<PluginParameter>,

    /// Nested subcommands (`/comp create`). A command with subcommands should
    /// not also declare parameters — Discord does not allow both.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subcommands: Vec<PluginCommand>,

    /// Who may run this. See [`PermissionClass`] for why this isn't role IDs.
    #[serde(default)]
    pub permission: PermissionClass,

    /// Whether Augie should defer before dispatching. Set this for anything
    /// that can't reliably answer inside Discord's 3-second window — which
    /// includes essentially every cross-repo plugin call.
    #[serde(default)]
    pub needs_defer: bool,

    /// Whether replies are visible only to the invoking user.
    ///
    /// **This has to be advertised, not just set on the response.** When
    /// `needs_defer` is true Augie must ACK before it has called the plugin,
    /// and Discord fixes ephemerality at that ACK — a later `CommandResponse`
    /// with `ephemeral: true` cannot demote an already-public message. So a
    /// deferred command that only set the response flag would leak into the
    /// channel, silently and only in production.
    ///
    /// Defaults to `false` to match [`crate::CommandResponse`]'s default and
    /// to keep existing manifests behaving exactly as before: an
    /// accidentally-public message is the milder failure in general. Commands
    /// answering about the caller's own private data should set it.
    #[serde(default)]
    pub ephemeral: bool,

    /// Ask Augie to mint an identity token for the invoking user and pass it on
    /// [`crate::CommandInvocation::identity_token`].
    ///
    /// **Opt-in, and default off.** A bearer credential handed to a service
    /// that has no use for it is pure downside — one more place it can be
    /// logged, forwarded or leaked — so a command asks only when it genuinely
    /// needs to hand the user onward already authenticated.
    ///
    /// The usual reason is a link button: without a token the destination has
    /// to re-authenticate the user itself, and an OAuth bounce from a Discord
    /// button someone just clicked is friction they'll read as brokenness.
    #[serde(default)]
    pub needs_identity: bool,

    /// Per-user cooldown. Deliberately a flat number rather than mirroring
    /// `bot_config::CooldownPolicy`: the richer rate-limit shapes are Augie's
    /// own vocabulary, and a plugin wanting one can be given it in guild
    /// config instead of advertising it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<u64>,
}

/// A command parameter. Mirrors the subset of Discord's option types that
/// `bot_config::CommandParameter` already supports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginParameter {
    pub name: String,
    pub description: String,
    pub kind: ParameterKind,

    #[serde(default)]
    pub required: bool,

    /// Fixed choices. Empty means free input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<ParameterChoice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    String,
    Integer,
    Number,
    Boolean,
    /// Resolves to a Discord user ID (as a string — see the crate docs).
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterChoice {
    pub name: String,
    pub value: String,
}

/// Who may run a command, expressed as intent rather than identity.
///
/// A plugin **cannot** advertise role IDs. Roles are per-guild, and a service
/// that may be installed in several guilds has no way to know — or any business
/// knowing — that "admin" means `1320158903262122115` in one of them. So the
/// manifest declares a class and the guild's config maps classes to role IDs,
/// which Augie resolves into `CommandPermissions::Restricted([...])` at
/// registration.
///
/// Augie resolves the caller's class *before* dispatch and passes it on the
/// invocation. A plugin should still check the class it was handed for anything
/// destructive — defence in depth, not a re-derivation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionClass {
    #[default]
    Everyone,
    Moderator,
    Admin,
}

impl PermissionClass {
    /// Whether this class satisfies `required`.
    ///
    /// Ordering is `Everyone < Moderator < Admin`, so an Admin passes a
    /// Moderator gate.
    pub fn satisfies(self, required: PermissionClass) -> bool {
        self.rank() >= required.rank()
    }

    fn rank(self) -> u8 {
        match self {
            PermissionClass::Everyone => 0,
            PermissionClass::Moderator => 1,
            PermissionClass::Admin => 2,
        }
    }
}

impl ServiceManifest {
    /// Every command name advertised, including subcommand paths
    /// (`"comp"`, `"comp create"`, …). Used by Augie to detect collisions
    /// across sources during a refresh.
    pub fn command_paths(&self) -> Vec<String> {
        fn walk(prefix: &str, cmd: &PluginCommand, out: &mut Vec<String>) {
            let path = if prefix.is_empty() {
                cmd.name.clone()
            } else {
                format!("{prefix} {}", cmd.name)
            };
            out.push(path.clone());
            for sub in &cmd.subcommands {
                walk(&path, sub, out);
            }
        }

        let mut out = Vec::new();
        for cmd in &self.commands {
            walk("", cmd, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(name: &str, subs: Vec<PluginCommand>) -> PluginCommand {
        PluginCommand {
            name: name.to_string(),
            description: format!("{name} description"),
            parameters: vec![],
            subcommands: subs,
            permission: PermissionClass::Everyone,
            needs_defer: true,
            ephemeral: false,
            needs_identity: false,
            cooldown_seconds: None,
        }
    }

    #[test]
    fn command_paths_includes_subcommands() {
        let manifest = ServiceManifest {
            service: "holder-map".to_string(),
            version: "1".to_string(),
            commands: vec![cmd(
                "comp",
                vec![cmd("create", vec![]), cmd("draw", vec![])],
            )],
        };

        assert_eq!(
            manifest.command_paths(),
            vec!["comp", "comp create", "comp draw"]
        );
    }

    #[test]
    fn admin_satisfies_moderator_gate() {
        assert!(PermissionClass::Admin.satisfies(PermissionClass::Moderator));
        assert!(PermissionClass::Admin.satisfies(PermissionClass::Everyone));
        assert!(!PermissionClass::Everyone.satisfies(PermissionClass::Admin));
        assert!(!PermissionClass::Moderator.satisfies(PermissionClass::Admin));
    }

    #[test]
    fn permission_class_defaults_to_everyone() {
        let json = r#"{"name":"status","description":"d"}"#;
        let parsed: PluginCommand = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.permission, PermissionClass::Everyone);
        assert!(!parsed.needs_defer);
    }
}
