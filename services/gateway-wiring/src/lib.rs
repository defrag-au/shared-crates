//! Per-guild chat trigger wiring and agent entitlement — the config
//! vocabulary of augminted-bots' gateway listener.
//!
//! Split out of that repo's `bot-api-types` so an admin surface can live in
//! another repo without either depending on the other. See
//! `augminted-bots/docs/ADMIN_SURFACE_CONSOLIDATION_DESIGN.md` §6: the wire
//! and the widget are shared, the service stays home. What stayed behind is
//! the executor edge (`ExecuteActionRequest` and friends) — gateway→augie
//! traffic, which is Discord-shaped and belongs to that repo.
//!
//! ## This crate carries meaning, not just shapes
//!
//! There are now **two** renderers over this vocabulary — the standalone
//! gateway admin app and the client portal's pane — and the entitlement
//! editor is exactly where two screens disagreeing gets expensive: it decides
//! who pays.
//!
//! So the rules live here, beside the types they judge, rather than in either
//! renderer: [`AgentMode`] (the three states an entitlement can be in),
//! [`entitlement_problems`] / [`tidy_entitlement`] (what the server will
//! accept), and [`describe_wiring_change`] (what the audit trail says). A
//! second screen can lay these out differently; it cannot mean something
//! different by them.
//!
//! This is the same rule augminted-bots already applies to game logic —
//! shared crate owns the rules, the widget is presentation.
//!
//! **Mechanism public, policy private.** This repo is public. The vocabulary
//! belongs here; real role ids, guild snowflakes, tier values and endpoint
//! hostnames do not — including in tests.

use serde::{Deserialize, Serialize};

/// Per-guild wiring — the "if" side of the IFTTT split, as an event graph:
/// event sources wired to dispatchable actions. Owned by the gateway DO's
/// storage and edited live via the admin surface; the command catalog (the
/// "that") stays in the TOML/plugin config pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuildWiring {
    #[serde(default)]
    pub bindings: Vec<EventBinding>,

    /// Recent changes, newest first, capped — see `MAX_WIRING_CHANGES`.
    ///
    /// Kept with the wiring it describes rather than in a separate store: it is
    /// small, it is only ever read beside the thing it explains, and a second
    /// store would be a second thing to keep in step on every save.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<WiringChange>,

    /// Who in this guild may use the @mention agent, and how much.
    ///
    /// **Absent means the server is not entitled at all** — the agent stays
    /// silent rather than explaining itself, because "this server has not
    /// bought the agent" is a sales conversation and not something a bot should
    /// open in someone's channel.
    ///
    /// Lives beside the bindings because it is per-guild state this DO already
    /// persists and broadcasts, and because it is edited by the same operator
    /// on the same screen. It is NOT guild-editable: `gateway.admin` is granted
    /// by a role in HODLCroft, so a paying server's own admins cannot reach it
    /// — which is what stops the gate being decorative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentEntitlement>,
}

/// What the @mention agent costs a guild's members, per day.
///
/// An allocation rather than a boolean, and deliberately: a yes/no can express
/// *whether* someone may ask but never *how much*, so plan tiers would each
/// become a code path instead of a number. See
/// `docs/AUGIE_AGENT_GATING_DESIGN.md` §2.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentEntitlement {
    /// Role id → tokens per UTC day. A member holding several gets the LARGEST,
    /// so adding a higher tier never takes anything away.
    #[serde(default)]
    pub tiers: Vec<AgentTier>,

    /// What a member of this guild gets holding none of the tier roles.
    ///
    /// Zero — the default — means they are prompted to get the role. Set it
    /// above zero for a server whose whole membership is entitled, where
    /// naming a role would be ceremony.
    #[serde(default)]
    pub default_daily_tokens: u32,
}

/// How many changes a guild keeps.
///
/// A ring, not a log: this rides in the wiring record, which is read whole on
/// every admin connect, so unbounded growth would eventually make connecting
/// expensive. Twenty is enough to answer "what happened recently" — anything
/// older is a question for a real audit store, which this is not pretending to
/// be.
pub const MAX_WIRING_CHANGES: usize = 20;

/// One change an admin made to a guild's wiring.
///
/// Exists because entitlement decides who pays: "who granted this guild its
/// allowance, and when" must be answerable, and before this the DO did not even
/// learn which admin was connected.
///
/// Deliberately a SUMMARY, not a diff. A full before/after would carry every
/// binding on every save and grow without bound; what an operator needs months
/// later is who, when, and what changed in one line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WiringChange {
    /// Unix millis.
    pub at: u64,
    /// Display name + Discord id, or `"operator token"` for a token-authed
    /// change — which names the platform rather than guessing at a person.
    pub actor: String,
    /// What changed, one line. e.g. `"agent: by role, 2 tiers (was: off)"`.
    pub summary: String,
}

/// A guild role, as the admin surface needs it.
///
/// A flat wire type rather than `twilight_model::guild::Role`: the admin
/// frontend is wasm and must not pull a gateway model in, and this crosses a
/// socket either way. Three fields is all a picker renders.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildRole {
    pub id: String,
    pub name: String,
    /// `0xRRGGBB`, with zero meaning Discord's uncoloured default.
    #[serde(default)]
    pub color: u32,
}

/// One role's daily allowance.
///
/// No label field. The first version had one, on the theory that a bare
/// snowflake needs a human note beside it — but the picker resolves the role to
/// its live `@name`, which is a better label than anything typed here and
/// cannot go stale when the role is renamed. The raw id is the fallback, and it
/// only appears when the role is genuinely unresolvable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentTier {
    /// Discord role id.
    pub role: String,
    pub daily_tokens: u32,
}

/// The three states an entitlement can be in, named.
///
/// The data has always had three; a checkbox could only show two, leaving the
/// third (entitled, but open to everyone) encoded as "ticked, with the number
/// above zero" — which nothing on screen said. Naming them makes the config
/// readable at a glance and makes "which of these am I buying" answerable.
///
/// Lives here rather than in a renderer because **two renderers now exist**
/// and this is the vocabulary they must agree on. A screen may lay the three
/// out however it likes; it may not disagree about which one a given
/// entitlement *is*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    /// No entitlement at all. The agent ignores the server, silently.
    Off,
    /// Entitled with no role tiers: every member gets the same allowance.
    Everyone,
    /// Entitled by role. Members holding none of the tiers are prompted.
    ByRole,
}

impl AgentMode {
    /// Which mode an entitlement is in.
    #[must_use]
    pub fn of(agent: Option<&AgentEntitlement>) -> Self {
        match agent {
            None => Self::Off,
            // Tiers are what distinguishes the two "on" modes. A config with
            // both tiers and a default reads as by-role with a floor, which is
            // legal — and by-role is the honest label for it.
            Some(agent) if agent.tiers.is_empty() => Self::Everyone,
            Some(_) => Self::ByRole,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Everyone => "all members",
            Self::ByRole => "by role",
        }
    }

    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            // No upgrade pitch — an unentitled server is a sales conversation,
            // not something the bot opens in someone's channel.
            Self::Off => "the agent ignores this server entirely — no reply, not even a refusal",
            Self::Everyone => "every member may @mention the agent, up to a shared daily budget",
            Self::ByRole => {
                "only members holding one of these roles; everyone else is told which role to get"
            }
        }
    }

    /// The entitlement this mode starts from when it is selected.
    ///
    /// Switching mode REBUILDS rather than preserving what the other mode
    /// held: tiers kept across a trip through "everyone" would silently come
    /// back, which is the ambiguity naming the modes exists to remove.
    #[must_use]
    pub fn entitlement(self) -> Option<AgentEntitlement> {
        match self {
            Self::Off => None,
            // A non-zero default, or the mode would mean "entitled, granting
            // nothing" — which the server rejects on save.
            Self::Everyone => Some(AgentEntitlement {
                tiers: Vec::new(),
                default_daily_tokens: DEFAULT_DAILY_TOKENS,
            }),
            // One empty row, so the next thing to do is visible — and so the
            // mode reads back as by-role rather than snapping to "all
            // members", since tiers are what distinguish the two.
            Self::ByRole => Some(AgentEntitlement {
                tiers: vec![AgentTier::default()],
                default_daily_tokens: 0,
            }),
        }
    }
}

/// Starting allowance for a newly-entitled guild.
///
/// A number to be replaced by observed usage rather than defended: roughly
/// twenty tool-using answers at current shapes — enough to be useful, small
/// enough that a mistake is cheap.
pub const DEFAULT_DAILY_TOKENS: u32 = 250_000;

/// Normalise an entitlement before it is stored or judged.
///
/// Drops half-typed tiers rather than storing rows that match nobody. A UI
/// warns about an empty role id live; by save time it is a row the operator
/// started and abandoned, and keeping it would show a tier that silently does
/// nothing.
pub fn tidy_entitlement(agent: &mut AgentEntitlement) {
    for tier in &mut agent.tiers {
        tier.role = tier.role.trim().to_string();
    }
    agent.tiers.retain(|tier| !tier.role.is_empty());
}

/// What is wrong with an entitlement, if anything.
///
/// Pure, so the rules that decide who pays are testable without a Durable
/// Object — and shared, so a second admin surface cannot accept a config the
/// server will reject, or warn about one it would have taken.
pub fn entitlement_problems(agent: &AgentEntitlement) -> Vec<String> {
    let mut problems = Vec::new();

    // Entitled but granting nobody anything is almost certainly a half-finished
    // edit — and in the channel it is indistinguishable from an unentitled
    // server while being a different state internally. Say so rather than
    // store a config whose behaviour nobody could explain later.
    if agent.default_daily_tokens == 0 && agent.tiers.iter().all(|tier| tier.daily_tokens == 0) {
        problems.push(
            "agent: entitled, but nobody has an allowance — add a role tier with a token \
             budget, raise the default, or set the agent off"
                .to_string(),
        );
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for tier in &agent.tiers {
        // A duplicate is not ambiguous (the largest wins) but it is never
        // intended, and the losing row stays on screen looking effective.
        if !seen.insert(tier.role.as_str()) {
            problems.push(format!("agent: role {} appears twice", tier.role));
        }
        if !tier.role.chars().all(|c| c.is_ascii_digit()) {
            problems.push(format!(
                "agent: {} is not a Discord role id (digits only)",
                tier.role
            ));
        }
    }

    problems
}

/// One line describing what a save changed, for [`WiringChange::summary`].
///
/// Summary, not diff. What an operator needs months later is who, when, and
/// what — and the entitlement is the part that decides who pays, so it leads.
/// Binding counts follow, because "the wiring changed" with no shape at all is
/// barely worth recording.
pub fn describe_wiring_change(previous: Option<&GuildWiring>, next: &GuildWiring) -> String {
    let mut parts = Vec::new();

    let before = previous.and_then(|p| p.agent.as_ref());
    let after = next.agent.as_ref();
    if before != after {
        parts.push(format!(
            "agent: {} (was: {})",
            describe_entitlement(after),
            describe_entitlement(before)
        ));
    }

    let before_bindings = previous.map(|p| p.bindings.len()).unwrap_or(0);
    if before_bindings != next.bindings.len() {
        parts.push(format!(
            "bindings: {} (was: {before_bindings})",
            next.bindings.len()
        ));
    }

    match parts.is_empty() {
        // A save that changed neither is still a save — an edit to a binding's
        // patterns or actions, which the counts cannot see. Recording it as
        // "edited" beats recording nothing and leaving a gap in the timeline.
        true => "bindings edited".to_string(),
        false => parts.join("; "),
    }
}

/// An entitlement in one phrase, for the change log.
pub fn describe_entitlement(agent: Option<&AgentEntitlement>) -> String {
    match agent {
        None => "off".to_string(),
        Some(agent) if agent.tiers.is_empty() => {
            format!("all members, {}/day", agent.default_daily_tokens)
        }
        Some(agent) => format!("by role, {} tier(s)", agent.tiers.len()),
    }
}

impl AgentEntitlement {
    /// This member's daily allowance, or zero if they hold no entitled role.
    ///
    /// Pure — the caller does the Discord lookup, as everywhere else in this
    /// config.
    #[must_use]
    pub fn allowance(&self, member_roles: &[String]) -> u32 {
        self.tiers
            .iter()
            .filter(|tier| member_roles.iter().any(|held| held == &tier.role))
            .map(|tier| tier.daily_tokens)
            .max()
            .unwrap_or(self.default_daily_tokens)
            .max(self.default_daily_tokens)
    }
}

/// One event source and the actions wired to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBinding {
    /// Stable id (UI identity across edits/deltas).
    pub id: String,
    /// Per-binding kill-switch, so disabling isn't deleting.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Per-user firing cooldown in seconds (None = no cooldown). Lives on
    /// the binding — pacing is a property of the wiring, not of any catalog
    /// command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<u32>,
    pub event: EventSource,
    /// On fire: React actions run first, in list order (instant, and their
    /// order is visible on the message); the rest run CONCURRENTLY — a
    /// binding with several renders waits for its slowest, not the sum.
    #[serde(default)]
    pub actions: Vec<WiredAction>,
}

/// What fires a binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventSource {
    /// A guild message whose whole trimmed content is one of `patterns`
    /// (case-insensitive; trailing punctuation/emoji allowed).
    OnMessage { patterns: Vec<String> },

    /// A guild message that @mentions the bot.
    ///
    /// Carries no patterns, and that is the point: the whole message minus the
    /// mention is the question, and what it can be asked is decided by which
    /// tools the guild's plugins advertise — not by anything wired here.
    ///
    /// Mention-only, never ambient. A bot that answers unprompted in a busy
    /// channel is both expensive and unwelcome, and an @mention is the one
    /// unambiguous signal that someone is talking *to* it.
    OnMention {},
}

/// A dispatchable action — a raw building block with its own arguments, NOT
/// a reference into any guild's configured command catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WiredAction {
    /// Render one of the triggering user's owned assets (random pick within
    /// the policy, optionally trait-filtered) and post it as an inline reply.
    RandomOwnedAsset {
        /// 56-hex policy id; None → the guild's `default_policy_id`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        policy_id: Option<String>,
        #[serde(default)]
        style: RenderStyle,
        /// Trait filters: each entry is one search (categories ANDed, values
        /// within a category any-of), results unioned across entries — the
        /// same shape `asset_search` commands configure. Empty = any owned
        /// asset.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        variants: Vec<std::collections::HashMap<String, Vec<String>>>,
    },
    /// React to the triggering message: unicode emoji or custom `name:id`.
    React { emoji: String },

    /// Answer the triggering message with the @mention agent.
    ///
    /// Takes no arguments. The tools come from whatever the guild's plugins
    /// advertise and the caller's permission class, both resolved at dispatch
    /// time — a wiring that named tools would be a second place to keep in
    /// sync with the manifests, and would go stale the moment a plugin
    /// changed.
    ///
    /// Unlike the other actions this one costs a model call per fire, so the
    /// binding's `cooldown_seconds` is doing real work rather than politeness.
    Ask {},
}

impl std::fmt::Display for WiredAction {
    /// The action's wire tag.
    ///
    /// Deliberately the same string serde writes for `kind`, so a log line, a
    /// stored binding and an admin payload all say the same word — and adding
    /// a variant is a compile error here rather than a stale label at whatever
    /// call sites happened to match on it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RandomOwnedAsset { .. } => "random_owned_asset",
            Self::React { .. } => "react",
            Self::Ask {} => "ask",
        })
    }
}

/// How a rendered asset is presented. This is the gateway's WIRE vocabulary —
/// augie translates it to `bot_config::AssetResultStyle` at its render
/// boundary (bot-config depends on this crate, so the richer type can't live
/// here without a cycle).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderStyle {
    /// The plain asset image.
    #[default]
    Image,
    /// Asset image captioned "{greeting} from {asset name}!".
    Greeting { greeting: String },
    /// Asset composited into an overlay template (e.g.
    /// "blackflag/greetings"), optionally animated, optionally captioned.
    OverlayGreeting {
        overlay: String,
        #[serde(default)]
        animated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        greeting: Option<String>,
    },
}

/// What an action did, in enough detail to explain itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionTrace {
    /// The working out, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<TraceStep>,

    /// What was said back, if anything. Truncated by the executor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,

    /// Tokens spent. Zero for actions that never call a model.
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,

    /// The two SUBSETS that decide what the totals above actually cost.
    ///
    /// Cached prompt tokens bill at a fraction of uncached, so two runs of the
    /// same size can differ several-fold — and a change that broke cache-prefix
    /// stability would show up nowhere without this. Reasoning tokens bill as
    /// output and routinely dwarf the visible reply: a run reporting 69 out
    /// had spent nearer 870.
    ///
    /// Subsets, never additions — adding either to its total double-counts.
    #[serde(default)]
    pub cached_prompt_tokens: u32,
    #[serde(default)]
    pub reasoning_tokens: u32,

    /// Set when the action ended without producing a reply, explaining why —
    /// the silent failures that cost the most time to diagnose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One step of an action's working out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    pub kind: TraceKind,
    /// What was done — a tool name with its arguments, or a render's subject.
    pub label: String,
    /// What came back. Truncated by the executor; a full tool result would
    /// swamp the feed it is meant to explain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// False when this step reported a problem. The step is still shown —
    /// a failed lookup is usually the most interesting line in the trace.
    #[serde(default = "default_true")]
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    /// A tool the agent called.
    Tool,
    /// Context the executor resolved before or around the model — plugins
    /// loaded, identity looked up, prior context restored.
    Context,
    /// Something the executor wants said that isn't either of the above.
    Note,
}

impl TraceStep {
    pub fn tool(label: impl Into<String>, detail: impl Into<String>, ok: bool) -> Self {
        Self {
            kind: TraceKind::Tool,
            label: label.into(),
            detail: Some(detail.into()),
            ok,
        }
    }

    pub fn context(label: impl Into<String>) -> Self {
        Self {
            kind: TraceKind::Context,
            label: label.into(),
            detail: None,
            ok: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Normalize a reaction emoji to the form Discord's reaction endpoint wants:
/// a unicode emoji, or a bare `name:id` for custom emotes. Accepts the ways
/// people naturally write custom emotes and strips the decoration:
///
/// - `<:gm:1291…>` / `<a:gm:1291…>` — the message-mention form (what
///   `\:emote:` prints in Discord)
/// - `:gm:1291…` — the colon-prefixed slip
/// - `gm:1291…` — already canonical
///
/// (The `a` animated marker is mention-form syntax only; reactions take
/// `name:id` regardless.)
pub fn normalize_reaction_emoji(input: &str) -> String {
    let mut s = input.trim();
    if let Some(inner) = s.strip_prefix('<').and_then(|x| x.strip_suffix('>')) {
        s = inner;
    }
    if let Some(rest) = s.strip_prefix(':') {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("a:") {
        // Only the animated marker — an emote actually named "a" would have
        // no further colon after stripping.
        if rest.contains(':') {
            s = rest;
        }
    }
    s.to_string()
}

/// Whether a (normalized) reaction emoji is well-formed: either no colon at
/// all (assumed unicode) or `name:id` with a non-empty name and numeric id.
pub fn reaction_emoji_valid(emoji: &str) -> bool {
    match emoji.split_once(':') {
        None => !emoji.trim().is_empty(),
        Some((name, id)) => {
            !name.is_empty() && !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(role: &str, daily_tokens: u32) -> AgentTier {
        AgentTier {
            role: role.to_string(),
            daily_tokens,
        }
    }

    /// The largest tier wins, so adding a higher one never takes anything from
    /// someone who already had a lower one.
    #[test]
    fn several_roles_resolve_to_the_largest_allowance() {
        let entitlement = AgentEntitlement {
            tiers: vec![tier("member", 50_000), tier("patron", 200_000)],
            default_daily_tokens: 0,
        };

        assert_eq!(entitlement.allowance(&["member".into()]), 50_000);
        assert_eq!(
            entitlement.allowance(&["member".into(), "patron".into()]),
            200_000,
            "the higher tier, not the first matched"
        );
    }

    /// Zero is the whole gate: a member of an entitled server holding no
    /// entitled role gets nothing, and is prompted rather than ignored.
    #[test]
    fn no_matching_role_is_zero() {
        let entitlement = AgentEntitlement {
            tiers: vec![tier("member", 50_000)],
            default_daily_tokens: 0,
        };

        assert_eq!(entitlement.allowance(&["someone-else".into()]), 0);
        assert_eq!(entitlement.allowance(&[]), 0);
    }

    /// A server whose whole membership is entitled names no role. The default
    /// is also a FLOOR — a tier must never leave someone worse off than having
    /// matched nothing.
    #[test]
    fn the_default_is_a_floor_not_a_fallback() {
        let entitlement = AgentEntitlement {
            tiers: vec![tier("member", 10_000)],
            default_daily_tokens: 25_000,
        };

        assert_eq!(entitlement.allowance(&[]), 25_000);
        assert_eq!(
            entitlement.allowance(&["member".into()]),
            25_000,
            "a tier below the default must not demote a member"
        );
    }

    /// Absent entitlement is not an empty one: the server is unentitled and the
    /// agent is silent. Asserted on the wire shape because the distinction is
    /// carried by `Option`, and a `#[serde(default)]` that produced
    /// `Some(default)` would silently entitle every guild.
    #[test]
    fn wiring_without_an_agent_block_stays_unentitled() {
        let wiring: GuildWiring = serde_json::from_str(r#"{"bindings":[]}"#).expect("parses");
        assert!(wiring.agent.is_none());
    }

    #[test]
    fn mentions_become_readable() {
        let bot = "1372830411196993578";
        assert_eq!(
            humanise_mentions(&format!("<@{bot}> can you show me bargains?"), bot, "City Watch"),
            "@City Watch can you show me bargains?"
        );
        // The legacy nickname form too — same mention, different spelling.
        assert_eq!(
            humanise_mentions(&format!("<@!{bot}> hi"), bot, "City Watch"),
            "@City Watch hi"
        );
        // Someone else's mention is collapsed, not resolved: the listener has
        // no member cache and a feed line isn't worth a lookup.
        assert_eq!(
            humanise_mentions("<@999> and <@888> chatting", bot, "City Watch"),
            "@someone and @someone chatting"
        );
        // Nothing to do, and no infinite loop on an unterminated mention.
        assert_eq!(humanise_mentions("plain text", bot, "b"), "plain text");
        assert_eq!(humanise_mentions("<@unterminated", bot, "b"), "<@unterminated");
    }

    /// `Display` claims to be the serde tag. This makes that true rather than
    /// aspirational — the two are written in different places and would
    /// otherwise drift the first time a variant was renamed.
    #[test]
    fn display_matches_the_serde_tag_for_every_action() {
        let actions = [
            WiredAction::Ask {},
            WiredAction::React {
                emoji: "⚓".to_string(),
            },
            WiredAction::RandomOwnedAsset {
                policy_id: None,
                style: RenderStyle::default(),
                variants: vec![],
            },
        ];

        for action in actions {
            let json: serde_json::Value = serde_json::to_value(&action).unwrap();
            let tag = json["kind"].as_str().expect("actions are tagged");
            assert_eq!(
                action.to_string(),
                tag,
                "Display and the serde tag disagree for {tag}"
            );
        }
    }

    /// The three modes are a partition of the data — every entitlement is in
    /// exactly one, and `entitlement()` round-trips back to the mode it came
    /// from. Two renderers reading `of()` differently is the drift this
    /// crate exists to prevent, so it is asserted rather than assumed.
    #[test]
    fn the_modes_partition_every_entitlement() {
        for mode in [AgentMode::Off, AgentMode::Everyone, AgentMode::ByRole] {
            let built = mode.entitlement();
            assert_eq!(
                AgentMode::of(built.as_ref()),
                mode,
                "{} did not round-trip",
                mode.label()
            );
        }

        // The awkward one: tiers AND a default is by-role with a floor, not
        // "everyone". By-role is the honest label.
        let both = AgentEntitlement {
            tiers: vec![tier("1", 10)],
            default_daily_tokens: 5,
        };
        assert_eq!(AgentMode::of(Some(&both)), AgentMode::ByRole);
    }

    /// "All members" must be saveable the instant it is selected — it is a
    /// complete config, and a screen that cannot save what it just built is a
    /// dead end.
    #[test]
    fn selecting_all_members_is_immediately_saveable() {
        let mut agent = AgentMode::Everyone.entitlement().expect("entitled");
        tidy_entitlement(&mut agent);
        assert!(entitlement_problems(&agent).is_empty());
    }

    /// "By role" deliberately is NOT, and that asymmetry is the design: its
    /// seeded row is an empty prompt, `tidy` strips it, and the validator
    /// then says what is missing. The operator has one more thing to do and
    /// is told what it is.
    #[test]
    fn selecting_by_role_asks_for_a_tier_before_it_will_save() {
        let mut agent = AgentMode::ByRole.entitlement().expect("entitled");
        assert_eq!(agent.tiers.len(), 1, "seeded with a visible empty row");

        tidy_entitlement(&mut agent);
        assert!(agent.tiers.is_empty(), "the empty prompt row is not stored");
        assert!(
            !entitlement_problems(&agent).is_empty(),
            "entitled-but-nobody-paid must be refused, not silently stored"
        );

        agent.tiers.push(tier("123", 50_000));
        assert!(entitlement_problems(&agent).is_empty());
    }

    #[test]
    fn tidy_drops_abandoned_rows_rather_than_storing_dead_tiers() {
        let mut agent = AgentEntitlement {
            tiers: vec![tier(" 123 ", 10), tier("  ", 20)],
            default_daily_tokens: 0,
        };
        tidy_entitlement(&mut agent);
        assert_eq!(agent.tiers.len(), 1);
        assert_eq!(agent.tiers[0].role, "123", "role ids are trimmed");
    }

    #[test]
    fn the_validator_catches_what_a_reader_could_not() {
        // Entitled, nobody paid.
        assert!(!entitlement_problems(&AgentEntitlement::default()).is_empty());
        // A duplicate is not ambiguous, but is never intended.
        let dupes = AgentEntitlement {
            tiers: vec![tier("123", 10), tier("123", 20)],
            default_daily_tokens: 0,
        };
        assert!(entitlement_problems(&dupes)
            .iter()
            .any(|p| p.contains("twice")));
        // A role id that isn't one matches nobody, silently.
        let bad = AgentEntitlement {
            tiers: vec![tier("not-a-snowflake", 10)],
            default_daily_tokens: 0,
        };
        assert!(entitlement_problems(&bad)
            .iter()
            .any(|p| p.contains("digits only")));
    }

    /// The entitlement leads the summary, because it is the part that decides
    /// who pays.
    #[test]
    fn the_change_summary_leads_with_the_entitlement() {
        let before = GuildWiring::default();
        let after = GuildWiring {
            agent: AgentMode::Everyone.entitlement(),
            ..GuildWiring::default()
        };
        let summary = describe_wiring_change(Some(&before), &after);
        assert!(summary.starts_with("agent:"), "got {summary}");
        assert!(summary.contains("was: off"));

        // A save that moves neither count is still a save.
        assert_eq!(
            describe_wiring_change(Some(&before), &GuildWiring::default()),
            "bindings edited"
        );
    }

    #[test]
    fn reaction_emoji_normalization() {
        assert_eq!(normalize_reaction_emoji("gm:1291038099278790738"), "gm:1291038099278790738");
        assert_eq!(normalize_reaction_emoji(":gm:1291038099278790738"), "gm:1291038099278790738");
        assert_eq!(
            normalize_reaction_emoji("<:gm:1291038099278790738>"),
            "gm:1291038099278790738"
        );
        assert_eq!(
            normalize_reaction_emoji("<a:gm:1291038099278790738>"),
            "gm:1291038099278790738"
        );
        assert_eq!(normalize_reaction_emoji(" ⚓ "), "⚓");
        // An emote unluckily named "a" survives.
        assert_eq!(normalize_reaction_emoji("a:129"), "a:129");

        assert!(reaction_emoji_valid("gm:1291038099278790738"));
        assert!(reaction_emoji_valid("⚓"));
        assert!(!reaction_emoji_valid("gm:"));
        assert!(!reaction_emoji_valid(":123"));
        assert!(!reaction_emoji_valid("gm:notanid"));
        assert!(!reaction_emoji_valid(""));
    }
}

/// A guild the gateway connection has seen, for the admin UI's roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Epoch ms this guild was last seen in a GUILD_CREATE (i.e. on the most
    /// recent connect that included it). Storage-backed, so the roster works
    /// while the gateway sleeps — this says how fresh the entry is.
    pub last_seen_ms: f64,
    /// Number of event bindings configured (filled at snapshot build).
    #[serde(default)]
    pub binding_count: usize,
}


/// One message the listener looked at, and what it decided.
///
/// Records misses as well as hits — a hit is easy to see in the channel, and
/// it is the misses that need explaining.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentActivity {
    /// Unix milliseconds.
    pub at_ms: f64,

    /// The Discord message. Also how a later trace finds this entry, and the
    /// same id both workers log as `msg=`.
    #[serde(default)]
    pub message_id: String,

    pub guild_id: String,

    /// Username where Discord sent one, else the id.
    pub author: String,

    /// Opening of the message — enough to recognise which one it was, not a
    /// transcript. The listener truncates before this leaves the DO.
    pub preview: String,

    /// The binding that fired, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_binding: Option<String>,

    /// Why nothing fired, when nothing did.
    ///
    /// The load-bearing field: "no binding matched" and "we don't know our own
    /// user id yet" look identical from the channel and mean entirely
    /// different things.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// What the action did, once the executor reported back.
    ///
    /// Absent while in flight, and for actions that report nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<ActionTrace>,
}

/// Replace `<@id>` mentions with readable names.
///
/// The raw form is what Discord puts on the wire, and it is unreadable in a
/// feed — a 19-digit id crowds out the question someone actually asked. The
/// bot's own id becomes `@{bot_name}`; anyone else's is shortened rather than
/// resolved, since the listener has no member cache and a feed is not worth a
/// lookup per line.
pub fn humanise_mentions(content: &str, bot_user_id: &str, bot_name: &str) -> String {
    let mut out = content.to_string();
    if !bot_user_id.is_empty() {
        out = out
            .replace(&format!("<@!{bot_user_id}>"), &format!("@{bot_name}"))
            .replace(&format!("<@{bot_user_id}>"), &format!("@{bot_name}"));
    }

    // Anything left is someone else. Collapse it rather than leaving 19 digits.
    while let Some(start) = out.find("<@") {
        let Some(end) = out[start..].find('>').map(|i| start + i) else {
            break;
        };
        out.replace_range(start..=end, "@someone");
    }
    out
}

/// Status of one bot's gateway connection, returned by every control route
/// (`/bot/{name}/start|stop|status`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayStatus {
    /// Master switch — persisted, so "stopped" survives evictions.
    pub enabled: bool,
    /// A socket is currently open (does not guarantee READY yet).
    pub connected: bool,
    /// Resume state held: reconnects will resume rather than re-identify.
    pub has_session: bool,
    /// Recent messages the listener evaluated, oldest first.
    ///
    /// The answer to "I typed something and nothing happened" — which is
    /// otherwise unanswerable without a live tail, and by then the moment has
    /// passed. In-memory and small; a debugging aid, not an audit log.
    #[serde(default)]
    pub recent: Vec<RecentActivity>,

    /// Our own user id is known, so `on_mention` bindings can match.
    ///
    /// Only READY carries it and the usual reconnect is a RESUME, so this
    /// being false with `connected: true` is the fingerprint of a session that
    /// never identified — mentions match nothing and the bot looks mute.
    #[serde(default)]
    pub has_bot_id: bool,
    /// Last gateway sequence number seen.
    pub seq: Option<u64>,
    /// Heartbeat interval from Hello (0 until first Hello).
    pub heartbeat_interval_ms: u64,
    /// Seconds since the last inbound gateway event.
    pub last_event_age_secs: Option<u64>,
    /// Seconds this socket has been open.
    pub uptime_secs: Option<u64>,
    /// Seconds until TTL auto-stop, if a TTL is set.
    pub ttl_remaining_secs: Option<u64>,
    /// User messages seen this session (in-memory, resets on eviction).
    #[serde(default)]
    pub messages_seen: u64,
    /// Messages that arrived with BLANK content — a non-zero count with
    /// `messages_seen` climbing means the MESSAGE_CONTENT privileged intent
    /// is not enabled on the application in the Discord dev portal.
    #[serde(default)]
    pub messages_without_content: u64,
    /// Chat triggers matched and dispatched this session.
    #[serde(default)]
    pub triggers_dispatched: u64,
}
