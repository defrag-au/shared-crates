//! Agent-callable tools: [`ToolInvocation`] and [`ToolResponse`].
//!
//! ## Why this isn't [`crate::CommandInvocation`]
//!
//! A tool call looks like a command call and is not one, in two ways that
//! matter:
//!
//! **There is no interaction.** A command invocation carries an
//! `interaction_token` and `application_id` because a slash command *is* a
//! Discord interaction. An agent answering an `@mention` over the gateway has
//! neither — the trigger was an ordinary message. Reusing the command type
//! would mean two required fields filled with empty strings, and a plugin
//! reasonably trusting them.
//!
//! **Arguments are richer than options.** [`crate::OptionValue`] mirrors
//! Discord's option types, which are flat scalars. A tool's arguments are
//! whatever its JSON Schema declares — `{"trait_bits": [47]}` has no
//! `OptionValue` representation at all.
//!
//! ## Not everything on an invocation comes from the model
//!
//! [`ToolInvocation::arguments`] is the only model-authored field. `config` and
//! [`ToolInvocation::attachments`] are host-populated, and that split is the
//! protocol's central safety property rather than a convenience: **the
//! untrusted party names the intent, the trusted path supplies the data.**
//!
//! A model cannot see a Discord attachment, so an image passed as an argument
//! would be a URL it invented. A guild's `policy_id` passed as an argument
//! would be a collection it guessed. Both failures look like answers.
//!
//! ## Why tools are a separate list from commands
//!
//! They overlap but neither contains the other. `bargains` is worth both
//! advertising as `/bargains` and offering to an agent. `resolve_traits` —
//! "given the word 'ghoul', which trait value is that?" — is only ever a step
//! an agent takes on the way to something else; nobody types it. And a
//! destructive command may be worth registering for a human to run
//! deliberately while being exactly what you don't want a model reaching for.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CommandResponse, InvokingUser, PermissionClass, PluginBlock};

/// An agent asking a plugin to run one of its advertised tools.
///
/// Sent to `POST /_augie/tool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// The tool's advertised name.
    pub tool: String,

    /// Arguments, already parsed and matching the tool's declared schema.
    ///
    /// Augie parses the model's raw output before dispatching, so a plugin
    /// receives JSON rather than a string that might not be JSON at all. A
    /// parse failure is reported to the model as a [`ToolResponse::error`]
    /// without the plugin ever being called.
    #[serde(default)]
    pub arguments: serde_json::Value,

    pub user: InvokingUser,

    pub guild_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,

    /// The message that prompted this call, when there was one.
    ///
    /// An **anchor**, not a capability — unlike an interaction token it grants
    /// nothing, it only says what this answer is about. A tool that hands work
    /// to a queue needs it: without one, a result produced a minute later can
    /// only be posted loose in the channel, reading as unrelated to the
    /// question that caused it.
    ///
    /// `None` where there is no such message — a page turn, or any invocation
    /// that did not begin with someone saying something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// The caller's resolved class. Augie has already filtered the tool list
    /// to what this class may use; passed for the same defence-in-depth reason
    /// as on a command invocation.
    pub permission_class: PermissionClass,

    /// Per-guild configuration, verbatim from the guild's opt-in block —
    /// identical to [`crate::CommandInvocation::config`].
    ///
    /// This is load-bearing for agents specifically. It is how a guild's
    /// `policy_id` reaches the tool **without passing through the model**: the
    /// user says "show me the ghouls", the model never names a collection, and
    /// the tool resolves one from config. A model-supplied policy id would be
    /// both a hallucination risk and an injection lever, since the question
    /// came from a public channel.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, String>,

    /// Files the user attached to the message that triggered this call.
    ///
    /// Host-populated, and deliberately **not** part of the tool's declared
    /// schema: the model cannot see an attachment, so it must not be able to
    /// name one. It decides *whether* to call a tool that consumes images and
    /// how to scope the search; the host decides which bytes that tool
    /// receives.
    ///
    /// Same trust direction as [`Self::config`], for the same reason — the
    /// untrusted party names the intent, the trusted path supplies the data.
    /// Were the URL model-supplied, a plugin fetching it would be an SSRF
    /// lever; it is safe here precisely because the model never touched it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<InvocationAttachment>,
}

/// One file the user attached, as the plugin needs to see it.
///
/// A URL rather than inline bytes: a 4MB image is ~5.3MB of base64 across a
/// service binding for something the plugin can fetch itself — and a plugin
/// doing visual work already fetches renditions on every call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationAttachment {
    /// Where to fetch it. A Discord CDN URL today: **signed and short-lived**.
    ///
    /// Fine for a call made in the same turn, which is the only time this is
    /// sent. A fetch minutes later may 403, and a plugin should say so plainly
    /// rather than report an empty result — "the image link expired" and
    /// "nothing matched" are opposite answers.
    pub url: String,

    /// MIME type as Discord reported it, when it reported one.
    ///
    /// The host has already filtered to images before sending — a plugin
    /// reading this is choosing a decoder, not deciding whether to trust it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Size in bytes, as Discord reported it. Lets a plugin refuse something
    /// too large without fetching it first.
    pub size_bytes: u64,
}

impl InvocationAttachment {
    /// Is this an image, by its declared type?
    ///
    /// A missing `content_type` is **not** an image: guessing from a file
    /// extension is how a `.png` that is really a zip reaches an image decoder.
    pub fn is_image(&self) -> bool {
        self.content_type
            .as_deref()
            .is_some_and(|mime| mime.starts_with("image/"))
    }
}

/// How a tool call ended.
///
/// An enum rather than a pair of booleans: "failed" and "needs the user to
/// choose" are different states with different consequences — one invites the
/// model to work around it, the other must stop it dead — and a second flag
/// would let both be true at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// The tool answered.
    #[default]
    Ok,
    /// The tool failed in a way the model can work around — a bad argument, an
    /// unknown name. The content explains it.
    Failed,
    /// The tool has asked the user a question and cannot proceed until it is
    /// answered.
    ///
    /// The question lives in `presentation`, as components the user interacts
    /// with; the answer arrives later as a component callback, not as another
    /// tool call. **The model must not retry or guess** — there is nothing to
    /// retry, and guessing is the behaviour this exists to replace.
    AwaitingInput,
}

/// The rest of a result the user can page through.
///
/// A tool that answers with more than fits one message renders **every** page
/// up front and hands them over. The host remembers them against the request
/// and swaps between them on a click.
///
/// ## A page turn is not a request
///
/// The tempting design is to re-invoke the tool with a page number. It reads
/// as cheaper — one page fetched at a time — and it is wrong, because a page
/// turn then has to reproduce the original request exactly: re-resolve the
/// collection, re-resolve the trait, re-read the guild config the model never
/// saw, re-authorise the caller. Each of those is a chance to land somewhere
/// slightly different, and each failure surfaces as a page that errors under a
/// card that rendered perfectly.
///
/// Paging is a **view over a result already computed**. Rendering the pages at
/// query time makes that structural: there is nothing left to resolve, so
/// nothing left to resolve differently. It also means the list cannot shift
/// under the reader between page one and page five.
///
/// ## Consequences worth knowing
///
/// - The plugin is not called again, so a page turn costs no query, no
///   round trip and no permission check — the data was fetched and authorised
///   once, when it was asked for.
/// - The plugin holds no state. It renders and returns; the host remembers.
/// - Cap what you render. A result of thousands should return the first
///   sensible span and set [`Self::truncated`], not a thousand pages.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    /// Pages **after** the one in [`ToolResponse::presentation`], in order.
    ///
    /// Empty means the result fitted one page and needs no controls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rest: Vec<Vec<PluginBlock>>,

    /// Total matching items, when the plugin can say so **from data**.
    ///
    /// `None` when it genuinely doesn't know. The host then shows a page
    /// number with no total rather than dividing by a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,

    /// How many items were rendered, across every page.
    ///
    /// With [`Self::total`] this says whether the reader is looking at all of
    /// the matches or the first slice of them — so truncation is *derived*
    /// from two counts rather than asserted by a flag that could disagree
    /// with them.
    #[serde(default)]
    pub shown: u32,

    /// What this set is, for the pager line — e.g. `"Perps · Post It"`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl PageInfo {
    /// How many pages in total, counting the one in `presentation`.
    pub fn page_count(&self) -> usize {
        self.rest.len() + 1
    }

    /// True when there is nothing to page.
    pub fn is_single_page(&self) -> bool {
        self.rest.is_empty()
    }

    /// Did more match than was rendered?
    ///
    /// Derived, never asserted — a flag saying "truncated" could contradict
    /// the two counts beside it, and then the card claims one thing while its
    /// own numbers say another.
    pub fn is_truncated(&self) -> bool {
        self.total.is_some_and(|total| total > self.shown)
    }
}

/// What a tool produced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolResponse {
    /// What the model reads.
    ///
    /// Write it for a model, not a log or a user: dense, factual, and specific
    /// enough to act on. `"no trait value matching 'ghoul' — closest are
    /// 'Ghoul Skin', 'Ghoulish Grin'"` gives it somewhere to go;
    /// `"ERR_NOT_FOUND"` does not.
    pub content: String,

    #[serde(default)]
    pub status: ToolStatus,

    /// Messages this tool posted to Discord itself.
    ///
    /// Most tools post nothing — they answer, and the host posts. A tool that
    /// *does* post (a picture, say) leaves a message the host never saw, and a
    /// reply to it is a reply to something the host cannot look up. Reporting
    /// the ids lets the host record the same conversational context against
    /// them, so replying to the artefact works as well as replying to the prose
    /// about it.
    ///
    /// Without this the failure is quiet and confusing: the user replies to the
    /// picture — the obvious thing to reply to — and the follow-up is treated
    /// as a fresh question with none of the context that produced it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub posted_message_ids: Vec<String>,

    /// How to *show* this result, if plain prose won't do.
    ///
    /// The model writes the words; a plugin that can do better than words —
    /// a gallery of thumbnails, a rendered chart, paging buttons — supplies
    /// that here. Rendering stays with the service that owns the data, which
    /// is the same reason [`CommandResponse::render`] exists.
    ///
    /// **The last presentation in a run wins.** A run that calls
    /// `resolve_traits` then `find_assets` should show the assets, and later
    /// calls are nearer the answer than earlier ones. A tool whose result is
    /// only an intermediate step should leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<CommandResponse>,

    /// How `presentation` sits in a larger result, when it is one page of one.
    ///
    /// Set this and the host adds the controls, stores whatever a page turn
    /// needs, and calls back with the page it wants. Leave it `None` and the
    /// result is whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<PageInfo>,
}

impl ToolResponse {
    #[must_use]
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::Ok,
            posted_message_ids: Vec::new(),
            presentation: None,
            page: None,
        }
    }

    /// The tool needs the user to choose before it can answer.
    ///
    /// `content` is what the model reads — it should say a question was asked,
    /// so the model relays that rather than inventing a result. `showing` then
    /// carries the question itself.
    #[must_use]
    pub fn asking(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::AwaitingInput,
            posted_message_ids: Vec::new(),
            presentation: None,
            page: None,
        }
    }

    pub fn is_error(&self) -> bool {
        self.status == ToolStatus::Failed
    }

    pub fn awaiting_input(&self) -> bool {
        self.status == ToolStatus::AwaitingInput
    }

    /// A failure the model should see and work around.
    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: ToolStatus::Failed,
            posted_message_ids: Vec::new(),
            presentation: None,
            page: None,
        }
    }

    /// Attach the reply Discord should show for this result.
    #[must_use]
    pub fn showing(mut self, presentation: CommandResponse) -> Self {
        self.presentation = Some(presentation);
        self
    }

    /// Record a message this tool posted itself.
    ///
    /// Call it for every message the tool put in the channel, so a reply to any
    /// of them carries the same context as a reply to the host's prose.
    #[must_use]
    pub fn posted(mut self, message_id: impl Into<String>) -> Self {
        self.posted_message_ids.push(message_id.into());
        self
    }

    /// Declare this result to be one page of a larger set.
    ///
    /// The host adds the controls and calls back for other pages — see
    /// [`PageInfo`] for why the plugin does not draw them itself.
    #[must_use]
    pub fn paged(mut self, page: PageInfo) -> Self {
        self.page = Some(page);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_carry_shapes_an_option_value_cannot() {
        // The reason this type exists rather than reusing CommandInvocation.
        let json = r#"{
            "tool": "find_assets",
            "arguments": {"trait_bits": [47, 12], "page": 0},
            "user": {"id": "1", "username": "u", "display_name": "U"},
            "guild_id": "2",
            "permission_class": "everyone"
        }"#;

        let parsed: ToolInvocation = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool, "find_assets");
        assert_eq!(parsed.arguments["trait_bits"].as_array().unwrap().len(), 2);
    }

    /// Attachments ride on the invocation, never in `arguments`. A model that
    /// could name one would name one it never saw — the hallucination class
    /// this whole trust direction exists to remove.
    #[test]
    fn attachments_arrive_outside_the_arguments() {
        let json = r#"{
            "tool": "visual_search",
            "arguments": {},
            "user": {"id": "1", "username": "u", "display_name": "U"},
            "guild_id": "2",
            "permission_class": "everyone",
            "attachments": [
                {"url": "https://cdn.discordapp.com/x.png?ex=1&hm=2",
                 "content_type": "image/png", "size_bytes": 51200}
            ]
        }"#;

        let parsed: ToolInvocation = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.attachments.len(), 1);
        assert!(parsed.attachments[0].is_image());
        // And the model's own argument object stayed empty — the image is not
        // in there and never was.
        assert_eq!(parsed.arguments, serde_json::json!({}));
    }

    /// Every existing plugin deserialises an invocation that predates this
    /// field, and every tool that consumes no images sends nothing on the wire.
    #[test]
    fn an_invocation_without_attachments_is_unchanged() {
        let json = r#"{
            "tool": "assets",
            "arguments": {},
            "user": {"id": "1", "username": "u", "display_name": "U"},
            "guild_id": "2",
            "permission_class": "everyone"
        }"#;
        let parsed: ToolInvocation = serde_json::from_str(json).unwrap();
        assert!(parsed.attachments.is_empty());

        let wire = serde_json::to_string(&parsed).unwrap();
        assert!(!wire.contains("attachments"), "{wire}");
    }

    /// A type Discord didn't report is not an image. Sniffing the extension
    /// instead is how a `.png` that is really a zip reaches a decoder.
    #[test]
    fn an_undeclared_content_type_is_not_an_image() {
        let untyped = InvocationAttachment {
            url: "https://cdn.discordapp.com/thing.png".to_string(),
            content_type: None,
            size_bytes: 10,
        };
        assert!(!untyped.is_image());

        let text = InvocationAttachment {
            content_type: Some("text/plain".to_string()),
            ..untyped.clone()
        };
        assert!(!text.is_image());
    }

    #[test]
    fn a_result_with_no_presentation_omits_it_from_the_wire() {
        let json = serde_json::to_string(&ToolResponse::ok("112 matches")).unwrap();
        assert!(!json.contains("presentation"), "{json}");
        // `status` rides along so the outcome is explicit on every result
        // rather than inferred from absence.
        assert!(json.contains("status"), "{json}");
    }

    /// Awaiting input is not a failure. A model told "this failed" retries or
    /// works around it; a model told "I asked the user" must do neither.
    #[test]
    fn awaiting_input_is_distinct_from_failure() {
        let asking = ToolResponse::asking("asked which trait they meant");
        assert!(asking.awaiting_input());
        assert!(!asking.is_error(), "a question is not a failure");

        let failed = ToolResponse::error("nope");
        assert!(failed.is_error());
        assert!(!failed.awaiting_input());
    }

    fn text_page(content: &str) -> Vec<PluginBlock> {
        vec![PluginBlock::Text {
            content: content.to_string(),
        }]
    }

    /// The count includes the page in `presentation`, which is not in `rest` —
    /// off by one here shows up as a card claiming one page fewer than it has.
    #[test]
    fn the_page_count_includes_the_one_already_shown() {
        let paged = PageInfo {
            rest: vec![text_page("two"), text_page("three")],
            ..Default::default()
        };
        assert_eq!(paged.page_count(), 3);
        assert!(!paged.is_single_page());
    }

    /// A whole result must not sprout controls.
    #[test]
    fn a_result_that_fits_one_page_needs_no_controls() {
        let whole = PageInfo {
            total: Some(3),
            ..Default::default()
        };
        assert!(whole.is_single_page());
        assert_eq!(whole.page_count(), 1);
    }

    /// Rendered pages survive the wire — they ARE the stored result, so a
    /// lossy round trip would page to blank cards.
    #[test]
    fn rendered_pages_round_trip() {
        let paged = PageInfo {
            rest: vec![text_page("second page")],
            total: Some(80),
            shown: 30,
            label: "Black Flag · Skin: Ghoul".to_string(),
        };

        let back: PageInfo = serde_json::from_str(&serde_json::to_string(&paged).unwrap()).unwrap();
        assert_eq!(back, paged);
    }

    /// Truncation is the relationship between two counts, not a third field
    /// that could disagree with them.
    #[test]
    fn truncation_is_derived_from_the_counts() {
        let partial = PageInfo {
            total: Some(80),
            shown: 30,
            ..Default::default()
        };
        assert!(partial.is_truncated());

        let complete = PageInfo {
            total: Some(30),
            shown: 30,
            ..Default::default()
        };
        assert!(!complete.is_truncated());

        // Unknown total: nothing is claimed either way.
        let unknown = PageInfo {
            total: None,
            shown: 30,
            ..Default::default()
        };
        assert!(!unknown.is_truncated());
    }

    /// Paging is opt-in: a plugin that says nothing about it gets nothing on
    /// the wire, so every existing tool result is unaffected.
    #[test]
    fn an_unpaged_result_omits_paging_from_the_wire() {
        let json = serde_json::to_string(&ToolResponse::ok("112 matches")).unwrap();
        assert!(!json.contains("page"), "{json}");
    }

    /// `total` is absent rather than zero when unknown — the host shows a page
    /// number with no denominator instead of dividing by a guess.
    #[test]
    fn an_unknown_total_is_absent_not_zero() {
        let page = PageInfo {
            rest: vec![text_page("two")],
            total: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&page).unwrap();
        assert!(!json.contains("total"), "{json}");

        let back: PageInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, page);
    }

    #[test]
    fn an_error_is_still_content_the_model_reads() {
        let response = ToolResponse::error("no trait value matching 'ghoul'");
        assert!(response.is_error());
        assert!(response.content.contains("ghoul"));
    }
}
