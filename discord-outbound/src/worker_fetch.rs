//! Transport over `worker::Fetch` — the Cloudflare Workers runtime's own.
//!
//! The third of the three HTTP stacks this crate had to reconcile. augminted-bots'
//! workers reach Discord three ways today: `reqwest` (`discord-api`), and raw
//! `worker::Fetch` in at least two places that each hand-rolled their own
//! multipart body. This is the one that lets those become `send`/`edit` calls
//! without adding an HTTP client to a worker that already has a perfectly good
//! one built in.
//!
//! Worth it over just using the `native` (reqwest) feature there: `worker::Fetch`
//! is already linked in every Worker, so this transport costs nothing in bundle
//! size, where reqwest-on-wasm32 is a whole HTTP stack plus its wasm-bindgen
//! glue.

use worker_stack::js_sys;
use worker_stack::worker::{Fetch, Method as WorkerMethod, Request, RequestInit};

use crate::ratelimit::RateLimitTracker;
use crate::{DiscordError, DiscordOutbound, HttpRequest, HttpResponse, Method};

/// Discord client for a Cloudflare Worker, over the runtime's own `fetch`.
///
/// The bot token is optional: a worker that only posts interaction followups
/// never needs one, and several do exactly that — the interaction token in the
/// path is the credential. Constructing without a token and then attempting a
/// channel post is a [`DiscordError::Config`], not a silent unauthenticated
/// request.
pub struct WorkerDiscordClient {
    bot_token: Option<String>,
    rate_limits: RateLimitTracker,
}

impl WorkerDiscordClient {
    /// A client that can post anywhere, including channels.
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: Some(bot_token.into()),
            rate_limits: RateLimitTracker::new(),
        }
    }

    /// A client for interaction followups and edits only.
    ///
    /// The honest constructor for a plugin: it holds no Discord credentials by
    /// design, and the interaction token it was handed is all it needs.
    pub fn tokenless() -> Self {
        Self {
            bot_token: None,
            rate_limits: RateLimitTracker::new(),
        }
    }
}

impl DiscordOutbound for WorkerDiscordClient {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, DiscordError> {
        let mut init = RequestInit::new();
        init.with_method(match request.method {
            Method::Post => WorkerMethod::Post,
            Method::Patch => WorkerMethod::Patch,
        });
        // A `Uint8Array`, not a string: a multipart body is binary, and pushing
        // image bytes through `String` would corrupt every one that is not
        // valid UTF-8.
        init.with_body(Some(
            js_sys::Uint8Array::from(request.body.as_slice()).into(),
        ));

        let built = Request::new_with_init(&request.url, &init)?;
        for (name, value) in &request.headers {
            built.headers().set(name, value)?;
        }

        let mut response = Fetch::Request(built).send().await?;
        let status = response.status_code();
        // Verbatim, not parsed: a Cloudflare 1015 body is HTML, and an error
        // that fails to parse must still be reportable.
        let body = response.text().await.unwrap_or_default();

        Ok(HttpResponse { status, body })
    }

    fn now_ms(&self) -> u64 {
        worker_stack::worker::Date::now().as_millis()
    }

    fn bot_token(&self) -> Option<&str> {
        self.bot_token.as_deref()
    }

    fn rate_limits(&self) -> &RateLimitTracker {
        &self.rate_limits
    }

    async fn sleep_ms(&self, millis: u64) {
        worker_stack::worker::Delay::from(std::time::Duration::from_millis(millis)).await;
    }
}
