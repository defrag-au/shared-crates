//! What a plugin advertises: [`ServiceManifest`], fetched by Augie at
//! config-refresh time and merged into the guild's registered command set.

use serde::{Deserialize, Serialize};
// Re-exported deliberately. This crate is the facade a plugin author works
// against: they should be able to declare a tool's parameters, and render the
// schema for their own MCP surface, without needing to know `tool-schema`
// exists or adding a second dependency to say the same thing.
pub use tool_schema::{assert_flat, no_arguments, schema_for, schemars, JsonSchema};

/// A plugin's complete command surface.
///
/// Fetched from `GET /_augie/manifest` during the `RefreshBotConfigs` chain,
/// and — since the two halves of this type have different costs — revalidated
/// opportunistically off the agent path as well.
///
/// The distinction to hold onto:
///
/// - [`Self::commands`] **registers with Discord**, which is a deliberate,
///   rate-limited, guild-scoped write. That belongs to an explicit refresh and
///   must never happen on a request path.
/// - [`Self::tools`] are only ever handed to a model in-process. Nothing
///   external, nothing to rate-limit. Requiring a refresh to pick up a new
///   tool was an accident of sharing one cache with the commands, not a
///   property tools have.
///
/// So the manifest may be re-fetched and re-cached after answering a mention;
/// what that refresh cannot do is register anything.
/// `Default` so a caller names only the fields it means. This is a protocol
/// type: every field added to it has broken every literal that constructs one,
/// across three repos, and "the protocol grew" is a poor reason for a test
/// fixture to stop compiling.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

    /// Tools an agent may call on this plugin's behalf. See [`PluginTool`] for
    /// why this overlaps `commands` without being derived from it.
    ///
    /// Defaults to empty, so a plugin that predates agents advertises none and
    /// is simply never routed to by one.
    #[serde(default)]
    pub tools: Vec<PluginTool>,

    /// How this service's tools relate to each other, for the system prompt.
    ///
    /// A tool description answers *"should I call this one?"* and is read while
    /// choosing between siblings. It is the wrong place for a rule that spans
    /// them — and those rules exist: which tool is authoritative for a count,
    /// that a negative is an argument rather than a follow-up call, that a
    /// question should be mapped into one call rather than assembled from
    /// several.
    ///
    /// Without somewhere to put them they end up as clauses in whichever
    /// description seemed closest, where each is read only by a model already
    /// looking at that tool. `trait_count` ended up explaining `assets`, and a
    /// model asked for a filtered count read one description, believed the
    /// other tool could not do it, and gave up — with the counter-example
    /// sitting in that tool's own schema.
    ///
    /// The host puts this in the system prompt, so it is read once, before any
    /// tool is chosen. Keep it to relationships and rules; what a single tool
    /// does belongs on that tool.
    ///
    /// The same idea as MCP's `InitializeResult.instructions`, and deliberately
    /// so: a plugin that speaks both surfaces should be able to write this
    /// once.
    ///
    /// `None` from a plugin that has nothing cross-cutting to say, which is
    /// most of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// One tool an agent may call.
///
/// Advertised alongside — not instead of — [`PluginCommand`]. The two lists
/// overlap: `bargains` is worth being both a slash command and a tool, while
/// `resolve_traits` is only ever an agent's intermediate step and a
/// destructive command may be worth offering to humans and to no model at all.
///
/// Sent to `POST /_augie/tool` as a [`crate::ToolInvocation`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginTool {
    /// Stable identifier. Must be unique within this plugin; Augie namespaces
    /// across plugins itself, so two services may both offer `search`.
    pub name: String,

    /// What the tool does **and when to call it**.
    ///
    /// This is the single highest-leverage field in the manifest — it is the
    /// entire basis on which a model decides to reach for this tool rather
    /// than another, or rather than answering from its own knowledge. Say the
    /// trigger condition explicitly ("call this when the user asks about
    /// current listings or floor price"), not just the capability.
    pub description: String,

    /// The arguments it accepts, as JSON Schema.
    ///
    /// **Generate this with [`tool_schema::schema_for`], never by hand.** It
    /// is derived from the same type the tool parses its arguments into, so
    /// what is advertised and what is accepted are one definition rather than
    /// two that drift — which is how a filter came to be silently dropped and
    /// how a "1-5" in prose outlived a code limit of 60.
    ///
    /// Raw JSON here rather than a typed parameter list because this is what
    /// a function-calling API wants and what MCP already puts on the wire; a
    /// typed intermediate would only be re-serialised to exactly this. The
    /// hazard that argued for typing it — a hand-written `Value` nobody
    /// checked — is gone once the value is generated from a type.
    ///
    /// Missing or `null` means the tool takes no arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,

    /// Who may cause this tool to run. See [`PermissionClass`].
    ///
    /// Augie filters the offered list by the *asking user's* class before the
    /// model ever sees it, so a tool a caller may not use is not something the
    /// model can be talked into calling.
    #[serde(default)]
    pub permission: PermissionClass,
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

    /// The tools a caller of this class may use.
    ///
    /// Filtering happens here, before the list reaches the model, rather than
    /// at execution time. A model that is never told a tool exists cannot be
    /// argued into calling it — and since the question arrives from a public
    /// channel, "cannot" is worth more than "will be refused".
    pub fn tools_for(&self, caller: PermissionClass) -> impl Iterator<Item = &PluginTool> {
        self.tools
            .iter()
            .filter(move |tool| caller.satisfies(tool.permission))
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
            tools: vec![],
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

    fn tool(name: &str, permission: PermissionClass) -> PluginTool {
        PluginTool {
            name: name.to_string(),
            description: format!("call {name} when asked about {name}"),
            input_schema: None,
            permission,
        }
    }

    fn manifest_with_tools(tools: Vec<PluginTool>) -> ServiceManifest {
        ServiceManifest {
            service: "collection-ownership".to_string(),
            version: "1".to_string(),
            commands: vec![],
            tools,
        }
    }

    /// A tool the caller may not use is never offered to the model, so there is
    /// nothing to talk it into calling.
    #[test]
    fn tools_for_hides_what_the_caller_may_not_run() {
        let manifest = manifest_with_tools(vec![
            tool("find_assets", PermissionClass::Everyone),
            tool("purge_cache", PermissionClass::Admin),
        ]);

        let visible: Vec<&str> = manifest
            .tools_for(PermissionClass::Everyone)
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(visible, vec!["find_assets"]);

        let as_admin: Vec<&str> = manifest
            .tools_for(PermissionClass::Admin)
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(as_admin, vec!["find_assets", "purge_cache"]);
    }

    /// A manifest written before tools existed must still parse, and advertise
    /// none rather than failing the refresh.
    #[test]
    fn a_manifest_without_tools_still_parses() {
        let json = r#"{"service":"holder-map","version":"1","commands":[]}"#;
        let parsed: ServiceManifest = serde_json::from_str(json).unwrap();
        assert!(parsed.tools.is_empty());
        assert_eq!(parsed.tools_for(PermissionClass::Admin).count(), 0);
    }
}
