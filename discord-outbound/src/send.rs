//! The two verbs: [`DiscordOutbound::send`] and [`DiscordOutbound::edit`].
//!
//! # Two, not fifteen
//!
//! augminted-bots' `discord-api` grew fifteen send/edit entry points, which
//! were not features but cells in a grid: `{reply?} × {plain | embeds |
//! components | v2} × {image?}`. Components V2 is what made that unaffordable —
//! a proper rollout adds `v2_with_image`, `v2_reply_with_image`, `edit_v2` and
//! so on, indefinitely.
//!
//! Every one of those cells is a property of the *message*, so they belong in
//! [`MessageBody`], not in a method name:
//!
//! | Was a method | Is now |
//! |---|---|
//! | `…_with_image` | `body.attachments` non-empty → multipart instead of JSON |
//! | `…_with_embeds` | `body.embeds` non-empty |
//! | `…_with_components` | `body.rows` non-empty |
//! | `…_v2` | `body.layout` non-empty → sets `IS_COMPONENTS_V2`, drops content |
//! | `…_reply` | `Target::reply` |
//! | `send_follow_up` | `Target::followup` |
//!
//! Each of those is decided once, in [`wire::body`], rather than being encoded
//! in the name of the function a caller happened to pick.
//!
//! # The platform seam is one method wide
//!
//! Everything Discord-specific — multipart vs JSON, the flags, 429 parsing,
//! rate-limit bookkeeping — lives here, once. A platform implementation
//! supplies [`HttpTransport::execute`] and a clock, and nothing else. That is
//! what stops the native and wasm paths drifting, which they had.

use discord_message::{wire, MessageBody, MessageTarget};

use crate::ratelimit::{self, RateLimitTracker};
use crate::{DiscordError, SentMessage, Target};

/// Identifies this client to Discord. Required on every request.
pub const USER_AGENT: &str = "defrag-discord-outbound/1.0";

/// Extra delivery semantics that are about *this transmission* rather than
/// about what gets rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SendOptions {
    /// Visible only to the invoking user. Only meaningful on
    /// [`Target::Followup`] — a channel post has no one to be private to, and
    /// Discord ignores the flag there.
    pub ephemeral: bool,

    /// What to do when the route is already known to be rate limited.
    pub on_rate_limit: Backoff,
}

impl SendOptions {
    pub fn ephemeral() -> Self {
        Self {
            ephemeral: true,
            ..Default::default()
        }
    }

    /// Give up rather than wait if the route is closed. Right for anything a
    /// user is not waiting on — a progress edit, a best-effort notice.
    pub fn best_effort() -> Self {
        Self {
            on_rate_limit: Backoff::Skip,
            ..Default::default()
        }
    }
}

/// What to do when the tracker says a route is closed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Backoff {
    /// Wait out the remaining window, then send. The default: a caller that
    /// asked to send a message usually means it.
    #[default]
    Wait,
    /// Return [`DiscordError::RateLimited`] immediately without sending.
    ///
    /// For messages whose value expires — a progress edit that will be
    /// superseded is worse than useless if it lands after the final one.
    Skip,
}

/// Posting and editing Discord messages.
///
/// Implemented once per HTTP stack. Callers use [`send`](Self::send) and
/// [`edit`](Self::edit); everything else on this trait is the seam those two
/// are built on.
#[allow(async_fn_in_trait)]
pub trait DiscordOutbound {
    /// Run one HTTP request. The only thing a platform has to supply.
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, DiscordError>;

    /// Milliseconds since the epoch.
    ///
    /// A parameter rather than a call into `std`, because `SystemTime::now()`
    /// panics on `wasm32-unknown-unknown` and every platform here reads a clock
    /// differently.
    fn now_ms(&self) -> u64;

    /// The bot token, for routes that need one. `None` is legitimate: a client
    /// that only ever posts followups never needs it.
    fn bot_token(&self) -> Option<&str>;

    /// Shared rate-limit state.
    fn rate_limits(&self) -> &RateLimitTracker;

    /// Wait out a known-closed route. Platforms sleep differently, and a
    /// client that cannot sleep can decline to.
    async fn sleep_ms(&self, millis: u64);

    /// Post a new message.
    async fn send(
        &self,
        target: &Target,
        body: &MessageBody,
        options: SendOptions,
    ) -> Result<SentMessage, DiscordError> {
        let url = target.send_url();

        let mut payload = wire::body(body);
        if let Some(message_id) = target.reply_to() {
            payload = payload.replying_to(message_id);
        }
        if options.ephemeral {
            payload = payload.with_flags(wire::EPHEMERAL);
        }

        let json = serde_json::to_string(&payload)?;
        self.dispatch(
            Method::Post,
            &url,
            json,
            body,
            target.needs_bot_token(),
            options.on_rate_limit,
        )
        .await
    }

    /// Replace a message that already exists.
    ///
    /// # The V2 flag is one-way
    ///
    /// `IS_COMPONENTS_V2` cannot be removed from a message once Discord has set
    /// it, so a message posted as V2 must be edited as V2 forever. That is not
    /// enforced here — nothing in an edit knows how the message was created —
    /// but it is why [`wire::body`] derives the flag from the body every time
    /// rather than taking it as an argument: pass the same *shape* of body and
    /// the flag follows automatically.
    ///
    /// # Attachments
    ///
    /// An edit whose body has no attachments sends no `attachments` key, which
    /// tells Discord to leave the message's existing files alone. Clearing them
    /// deliberately is not expressible through this verb.
    async fn edit(
        &self,
        target: &MessageTarget,
        body: &MessageBody,
        options: SendOptions,
    ) -> Result<SentMessage, DiscordError> {
        if !target.is_complete() {
            return Err(DiscordError::Config(
                "message target has an empty segment; the URL would 404 as if the message were gone"
                    .to_string(),
            ));
        }

        // Ephemerality is fixed when a message is created and cannot be
        // edited, so the flag is dropped rather than sent and ignored.
        let payload = wire::body(body);
        let json = serde_json::to_string(&payload)?;

        self.dispatch(
            Method::Patch,
            &target.edit_url(),
            json,
            body,
            target.needs_bot_token(),
            options.on_rate_limit,
        )
        .await
    }

    /// The shared half: gate on the tracker, pick a transport, read the answer.
    #[doc(hidden)]
    async fn dispatch(
        &self,
        method: Method,
        url: &str,
        payload_json: String,
        body: &MessageBody,
        needs_bot_token: bool,
        backoff: Backoff,
    ) -> Result<SentMessage, DiscordError> {
        let route = ratelimit::route_key(url);

        if let Some(wait) = self.rate_limits().wait_ms(route, self.now_ms()) {
            match backoff {
                Backoff::Wait => {
                    tracing::info!("route '{route}' is rate limited; waiting {wait}ms");
                    self.sleep_ms(wait).await;
                }
                Backoff::Skip => {
                    return Err(DiscordError::RateLimited {
                        retry_after: wait as f64 / 1000.0,
                        global: false,
                    })
                }
            }
        }

        // Multipart or JSON is decided here and nowhere else: it is whether
        // there are bytes, not which method the caller reached for.
        let (content_type, request_body) = if body.attachments.is_empty() {
            ("application/json".to_string(), payload_json.into_bytes())
        } else {
            let files: Vec<crate::multipart::File<'_>> = body
                .attachments
                .iter()
                .map(|attachment| crate::multipart::File {
                    filename: &attachment.filename,
                    content_type: attachment.content_type(),
                    data: &attachment.data,
                })
                .collect();
            let (bytes, boundary) = crate::multipart::body(&payload_json, &files);
            (crate::multipart::content_type(&boundary), bytes)
        };

        let mut headers = vec![
            ("Content-Type".to_string(), content_type),
            ("User-Agent".to_string(), USER_AGENT.to_string()),
        ];
        if needs_bot_token {
            let token = self.bot_token().ok_or_else(|| {
                DiscordError::Config(
                    "this route needs a bot token and the client has none".to_string(),
                )
            })?;
            headers.push(("Authorization".to_string(), format!("Bot {token}")));
        }

        let response = self
            .execute(HttpRequest {
                method,
                url: url.to_string(),
                headers,
                body: request_body,
            })
            .await?;

        self.interpret(route, response)
    }

    /// Turn a raw response into a result, recording anything worth remembering.
    #[doc(hidden)]
    fn interpret(&self, route: &str, response: HttpResponse) -> Result<SentMessage, DiscordError> {
        if (200..300).contains(&response.status) {
            return Ok(serde_json::from_str(&response.body)?);
        }

        if response.status == 429 {
            // A 1015 is Discord's *edge* refusing a shared egress IP, not a
            // per-route bucket. Recording it against the route would close a
            // bucket that was never limited.
            if ratelimit::is_cloudflare_block(&response.body) {
                tracing::error!(
                    "Cloudflare 1015 block on '{route}' — an edge block, not a Discord bucket. \
                     This was supposed to be gone; see `ratelimit::is_cloudflare_block`."
                );
                return Err(DiscordError::CloudflareBlocked);
            }

            let parsed: Option<RateLimitBody> = serde_json::from_str(&response.body).ok();
            // Discord's `retry_after` is seconds, and it has been fractional
            // since v8. Truncating it to whole seconds means retrying early.
            let retry_after = parsed.as_ref().map(|b| b.retry_after).unwrap_or(1.0);
            let global = parsed.as_ref().is_some_and(|b| b.global);

            self.rate_limits()
                .record(route, self.now_ms(), (retry_after * 1000.0).ceil() as u64);

            tracing::warn!(
                "rate limited on '{route}': retry after {retry_after:.2}s (global: {global})"
            );
            return Err(DiscordError::RateLimited {
                retry_after,
                global,
            });
        }

        Err(DiscordError::Request(format!(
            "Discord API error {}: {}",
            response.status, response.body
        )))
    }
}

#[derive(serde::Deserialize)]
struct RateLimitBody {
    retry_after: f64,
    #[serde(default)]
    global: bool,
}

/// One fully-formed HTTP request, ready for whatever stack is in play.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The bytes that came back, and the status.
///
/// The body is kept as a string rather than parsed, because the error paths
/// need it verbatim — a Cloudflare 1015 is HTML, not JSON.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Post,
    Patch,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Patch => "PATCH",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_message::{Attachment, PluginActionRow, PluginBlock, PluginComponent, PluginEmbed};
    use std::cell::RefCell;

    /// A client whose transport records instead of sending.
    ///
    /// The whole point of the seam: everything Discord-specific is in the
    /// default methods above, so a fake that only answers `execute` exercises
    /// all of it. Neither of the two real clients could be tested at all
    /// before — one needed a browser, the other a network.
    struct Fake {
        sent: RefCell<Vec<HttpRequest>>,
        reply: RefCell<HttpResponse>,
        slept: RefCell<Vec<u64>>,
        now: u64,
        rate_limits: RateLimitTracker,
        token: Option<String>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                sent: RefCell::new(Vec::new()),
                reply: RefCell::new(HttpResponse {
                    status: 200,
                    body: r#"{"id":"999","channel_id":"1"}"#.to_string(),
                }),
                slept: RefCell::new(Vec::new()),
                now: 1_000,
                rate_limits: RateLimitTracker::new(),
                token: Some("tok".to_string()),
            }
        }
    }

    impl Fake {
        fn answering(status: u16, body: &str) -> Self {
            Self {
                reply: RefCell::new(HttpResponse {
                    status,
                    body: body.to_string(),
                }),
                ..Default::default()
            }
        }

        fn last(&self) -> HttpRequest {
            self.sent
                .borrow()
                .last()
                .expect("a request was sent")
                .clone()
        }

        fn payload(&self) -> serde_json::Value {
            let request = self.last();
            serde_json::from_slice(&request.body).expect("a JSON body")
        }

        fn header(&self, name: &str) -> Option<String> {
            self.last()
                .headers
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    impl DiscordOutbound for Fake {
        async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, DiscordError> {
            self.sent.borrow_mut().push(request);
            Ok(self.reply.borrow().clone())
        }
        fn now_ms(&self) -> u64 {
            self.now
        }
        fn bot_token(&self) -> Option<&str> {
            self.token.as_deref()
        }
        fn rate_limits(&self) -> &RateLimitTracker {
            &self.rate_limits
        }
        async fn sleep_ms(&self, millis: u64) {
            self.slept.borrow_mut().push(millis);
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        // These futures never actually yield — the fake transport is
        // synchronous — so a trivial executor is enough and pulls in no runtime.
        futures_lite_block_on(future)
    }

    fn futures_lite_block_on<F: std::future::Future>(mut future: F) -> F::Output {
        use std::pin::Pin;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

        let waker = unsafe { Waker::from_raw(clone(std::ptr::null())) };
        let mut context = Context::from_waker(&waker);
        let mut future = unsafe { Pin::new_unchecked(&mut future) };

        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }

    fn button() -> PluginComponent {
        PluginComponent::Button {
            custom_id: "a".into(),
            label: "Go".into(),
            style: Default::default(),
            emoji: None,
            disabled: false,
        }
    }

    // ── The grid, collapsed ─────────────────────────────────────────────
    //
    // Each of these was its own method on `discord-api`. They are now all the
    // same call, differing only in the body handed to it.

    #[test]
    fn a_plain_channel_post_is_json_with_a_bot_token() {
        let fake = Fake::default();
        let sent = block_on(fake.send(
            &Target::channel("123"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ))
        .unwrap();

        assert_eq!(sent.id, "999");
        assert_eq!(fake.last().method, Method::Post);
        assert_eq!(
            fake.last().url,
            "https://discord.com/api/v10/channels/123/messages"
        );
        assert_eq!(fake.header("Authorization").as_deref(), Some("Bot tok"));
        assert_eq!(
            fake.header("Content-Type").as_deref(),
            Some("application/json")
        );
        assert_eq!(fake.payload()["content"], "hi");
    }

    /// `send_channel_message_reply`, as a target rather than a method.
    #[test]
    fn a_reply_carries_a_message_reference() {
        let fake = Fake::default();
        block_on(fake.send(
            &Target::reply("123", "456"),
            &MessageBody::text("re"),
            SendOptions::default(),
        ))
        .unwrap();

        assert_eq!(fake.payload()["message_reference"]["message_id"], "456");
    }

    /// `send_follow_up`. The interaction token in the path is the credential,
    /// so a bot token must NOT be sent alongside it.
    #[test]
    fn a_followup_uses_the_webhook_route_and_no_bot_token() {
        let fake = Fake::default();
        block_on(fake.send(
            &Target::followup("app", "int-tok"),
            &MessageBody::text("hi"),
            SendOptions::ephemeral(),
        ))
        .unwrap();

        assert_eq!(
            fake.last().url,
            "https://discord.com/api/v10/webhooks/app/int-tok"
        );
        assert_eq!(fake.header("Authorization"), None);
        assert_eq!(fake.payload()["flags"], wire::EPHEMERAL);
    }

    /// `send_channel_message_with_image`. The transport switch is the presence
    /// of bytes, not the name of the method the caller reached for.
    #[test]
    fn attachments_switch_the_transport_to_multipart() {
        let fake = Fake::default();
        let body = MessageBody::text("look")
            .with_attachment(Attachment::new("render.png", vec![0x89, 0x50, 0x4E, 0x47]));

        block_on(fake.send(&Target::channel("1"), &body, SendOptions::default())).unwrap();

        let content_type = fake.header("Content-Type").unwrap();
        assert!(
            content_type.starts_with("multipart/form-data; boundary="),
            "{content_type}"
        );

        let raw = String::from_utf8_lossy(&fake.last().body).to_string();
        assert!(
            raw.contains(r#"name="files[0]"; filename="render.png""#),
            "{raw}"
        );
        // And the payload still declares it, by the index the part uses.
        assert!(
            raw.contains(r#""attachments":[{"id":0,"filename":"render.png"}]"#),
            "{raw}"
        );
    }

    /// A body with no bytes must not pay for multipart.
    #[test]
    fn no_attachments_means_no_multipart() {
        let fake = Fake::default();
        block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ))
        .unwrap();

        assert_eq!(
            fake.header("Content-Type").as_deref(),
            Some("application/json")
        );
    }

    /// `send_channel_message_v2_reply`, and the rule that makes V2 work at all.
    #[test]
    fn a_v2_body_sets_the_flag_and_drops_the_classic_fields() {
        let fake = Fake::default();
        let body = MessageBody {
            content: Some("dropped".into()),
            embeds: vec![PluginEmbed::default()],
            layout: vec![PluginBlock::Text {
                content: "kept".into(),
            }],
            ..Default::default()
        };

        block_on(fake.send(&Target::channel("1"), &body, SendOptions::default())).unwrap();

        let payload = fake.payload();
        assert_eq!(payload["flags"], wire::IS_COMPONENTS_V2);
        assert!(payload.get("content").is_none(), "{payload}");
        assert!(payload.get("embeds").is_none(), "{payload}");
    }

    /// `send_channel_message_with_components`.
    #[test]
    fn classic_rows_render_as_components() {
        let fake = Fake::default();
        let body = MessageBody {
            content: Some("pick".into()),
            rows: vec![PluginActionRow::new(vec![button()])],
            ..Default::default()
        };

        block_on(fake.send(&Target::channel("1"), &body, SendOptions::default())).unwrap();

        let payload = fake.payload();
        assert_eq!(payload["components"][0]["type"], 1);
        assert_eq!(payload["content"], "pick");
        assert!(payload.get("flags").is_none(), "classic sets no flags");
    }

    // ── Edits ───────────────────────────────────────────────────────────

    #[test]
    fn an_edit_patches_the_targets_own_url() {
        let fake = Fake::default();
        let target = MessageTarget::Channel {
            channel_id: "1".into(),
            message_id: "2".into(),
        };

        block_on(fake.edit(&target, &MessageBody::text("new"), SendOptions::default())).unwrap();

        assert_eq!(fake.last().method, Method::Patch);
        assert_eq!(
            fake.last().url,
            "https://discord.com/api/v10/channels/1/messages/2"
        );
        assert_eq!(fake.header("Authorization").as_deref(), Some("Bot tok"));
    }

    /// The interaction route reaches ephemeral messages, and its token in the
    /// path is the credential — a bot token here is at best redundant.
    #[test]
    fn an_interaction_edit_sends_no_bot_token() {
        let fake = Fake::default();
        let target = MessageTarget::Interaction {
            application_id: "app".into(),
            interaction_token: "tok".into(),
            message_id: "2".into(),
        };

        block_on(fake.edit(&target, &MessageBody::text("new"), SendOptions::default())).unwrap();

        assert_eq!(
            fake.last().url,
            "https://discord.com/api/v10/webhooks/app/tok/messages/2"
        );
        assert_eq!(fake.header("Authorization"), None);
    }

    /// `IS_COMPONENTS_V2` cannot be removed once set, so an edit of a V2
    /// message must never emit `content` — that would be a message Discord
    /// rejects, losing the update entirely.
    #[test]
    fn an_edit_of_a_v2_message_never_emits_content() {
        let fake = Fake::default();
        let target = MessageTarget::Channel {
            channel_id: "1".into(),
            message_id: "2".into(),
        };
        let body = MessageBody {
            content: Some("would break it".into()),
            layout: vec![PluginBlock::Text {
                content: "kept".into(),
            }],
            ..Default::default()
        };

        block_on(fake.edit(&target, &body, SendOptions::default())).unwrap();

        let payload = fake.payload();
        assert!(payload.get("content").is_none(), "{payload}");
        assert_eq!(payload["flags"], wire::IS_COMPONENTS_V2);
    }

    /// An empty segment builds a URL like `.../messages/` that 404s
    /// indistinguishably from a message that is genuinely gone. Caught before
    /// the request rather than diagnosed from a log afterwards.
    #[test]
    fn an_incomplete_target_never_reaches_the_network() {
        let fake = Fake::default();
        let target = MessageTarget::Channel {
            channel_id: "1".into(),
            message_id: String::new(),
        };

        let result = block_on(fake.edit(&target, &MessageBody::text("x"), SendOptions::default()));

        assert!(matches!(result, Err(DiscordError::Config(_))));
        assert!(
            fake.sent.borrow().is_empty(),
            "nothing should have been sent"
        );
    }

    // ── Rate limiting ───────────────────────────────────────────────────

    /// The caller-visible half. `notification-dispatcher` maps this onto a
    /// queue retry delay, so swallowing it would turn visible backpressure
    /// into silent latency.
    #[test]
    fn a_429_reaches_the_caller_with_its_retry_after() {
        let fake = Fake::answering(
            429,
            r#"{"message":"You are being rate limited.","retry_after":1.75,"global":false}"#,
        );

        let result = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ));

        // Fractional, not truncated — retrying at 1s would be early.
        assert!(matches!(
            result,
            Err(DiscordError::RateLimited { retry_after, global: false })
                if (retry_after - 1.75).abs() < f64::EPSILON
        ));
    }

    /// The proactive half: having seen a 429, the next send waits instead of
    /// spending a round trip to be told again.
    #[test]
    fn a_recorded_limit_makes_the_next_send_wait_first() {
        let fake = Fake::answering(429, r#"{"retry_after":2.0,"global":false}"#);
        let _ = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ));

        // Same route, still inside the window.
        *fake.reply.borrow_mut() = HttpResponse {
            status: 200,
            body: r#"{"id":"1"}"#.to_string(),
        };
        block_on(fake.send(
            &Target::channel("2"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ))
        .unwrap();

        assert_eq!(fake.slept.borrow().as_slice(), &[2_000]);
    }

    /// Best-effort messages skip rather than wait — a progress edit that lands
    /// after the final one is worse than one that never lands.
    #[test]
    fn best_effort_skips_a_closed_route_without_sending() {
        let fake = Fake::default();
        fake.rate_limits.record("channels", fake.now, 5_000);

        let result = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("progress"),
            SendOptions::best_effort(),
        ));

        assert!(matches!(result, Err(DiscordError::RateLimited { .. })));
        assert!(
            fake.sent.borrow().is_empty(),
            "nothing should have been sent"
        );
        assert!(fake.slept.borrow().is_empty(), "best effort must not wait");
    }

    /// A 1015 is Discord's edge refusing an IP, not a bucket. Recording it
    /// against the route would close a bucket that was never limited, and it
    /// needs a different answer than backoff.
    #[test]
    fn a_cloudflare_block_is_its_own_error_and_records_no_limit() {
        let fake = Fake::answering(429, "<html>error code: 1015</html>");

        let result = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ));

        assert!(matches!(result, Err(DiscordError::CloudflareBlocked)));
        assert_eq!(fake.rate_limits.wait_ms("channels", fake.now), None);
    }

    /// Routes are tracked separately, so a throttled webhook must not stop a
    /// channel post.
    #[test]
    fn one_routes_limit_does_not_close_another() {
        let fake = Fake::default();
        fake.rate_limits.record("webhooks", fake.now, 5_000);

        block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ))
        .unwrap();

        assert!(fake.slept.borrow().is_empty());
    }

    #[test]
    fn a_server_error_is_reported_with_its_body() {
        let fake = Fake::answering(400, r#"{"code":50035,"message":"Invalid Form Body"}"#);

        let result = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ));

        let Err(DiscordError::Request(message)) = result else {
            panic!("expected a request error");
        };
        assert!(message.contains("400"), "{message}");
        assert!(message.contains("Invalid Form Body"), "{message}");
    }

    /// A client with no bot token can still post followups, but must fail
    /// loudly rather than send an unauthenticated channel post.
    #[test]
    fn a_channel_post_without_a_token_fails_before_sending() {
        let fake = Fake {
            token: None,
            ..Default::default()
        };

        let result = block_on(fake.send(
            &Target::channel("1"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ));

        assert!(matches!(result, Err(DiscordError::Config(_))));
        assert!(fake.sent.borrow().is_empty());

        // …and the followup route is unaffected.
        block_on(fake.send(
            &Target::followup("app", "tok"),
            &MessageBody::text("hi"),
            SendOptions::default(),
        ))
        .unwrap();
    }
}
