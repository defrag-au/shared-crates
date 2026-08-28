//! Backend for a Discord Activity.
//!
//! Runs the launch chain and then carries API requests as the widget the
//! Activity has become:
//!
//! ```text
//! handshake → AUTHORIZE → POST api/token (your worker) → AUTHENTICATE → Ready
//! ```
//!
//! Every URL is relative. Discord's proxy maps `<root>/api/…` onto the host
//! the Activity's URL mapping names; an absolute host is blocked by the
//! Activity CSP, and the symptom is a request that never resolves rather than
//! one that fails.
//!
//! # Why the chain lives here
//!
//! It is identical for every Activity the platform ships — only the client id
//! and the exchange path differ, and both are configuration. Written into each
//! game's `main`, it is five parse-and-branch hops that each have to get their
//! error handling right, and the first thing a copy loses is the distinction
//! between "Discord refused" and "our worker refused".

use super::{
    ArcadeBackend, BackendState, Poll, ReqId, TokenExchangeRequest, TokenExchangeResponse,
};
use discord_miniquad::{Activity, Phase, PollResult};
use serde::{Deserialize, Serialize};

/// Where the code-for-token exchange lives, under the mount prefix.
const EXCHANGE_PATH: &str = "/api/token";

/// OAuth scopes the Activity asks for.
///
/// `identify` only. Roles are resolved server-side from the bot's view of the
/// guild, so asking the player for `guilds.members.read` would buy a consent
/// prompt and nothing else — and a scope a player can decline is a scope that
/// can break eligibility for reasons they will never connect to the outcome.
const SCOPES: &[&str] = &["identify"];

#[derive(Serialize)]
struct AuthorizeArgs<'a> {
    client_id: &'a str,
    response_type: &'a str,
    state: &'a str,
    prompt: &'a str,
    scope: &'a [&'a str],
}

/// Kept as a struct rather than a `Value` so a rename upstream is a parse
/// failure that says so, not a silent `None`.
#[derive(Deserialize)]
struct AuthorizeResponse {
    code: String,
}

#[derive(Serialize)]
struct AuthenticateArgs<'a> {
    access_token: &'a str,
}

#[derive(Deserialize)]
struct AuthenticateResponse {
    user: AuthenticatedUser,
}

#[derive(Deserialize)]
struct AuthenticatedUser {
    username: String,
    #[serde(default)]
    global_name: Option<String>,
}

/// `discord-miniquad` hands out its own `ReqId` newtype over `i32`; the
/// harness trait deals in a plain `u32`. Converted at the boundary rather than
/// leaking either into the other — these are the only two places the two
/// numbering schemes meet.
fn to_local(id: discord_miniquad::ReqId) -> ReqId {
    id.0 as u32
}

fn to_discord(id: ReqId) -> discord_miniquad::ReqId {
    discord_miniquad::ReqId(id as i32)
}

/// How long to wait for Discord to answer the handshake before giving up.
///
/// The handshake is a `postMessage` with no reply guarantee: if the client
/// never answers — wrong `targetOrigin`, a frame it no longer tracks — nothing
/// rejects and nothing logs. Without a deadline the Activity sits on
/// "connecting to Discord" forever, which is the least informative failure
/// available and takes a debugging session to tell apart from a slow network.
const HANDSHAKE_TIMEOUT_SECS: f64 = 8.0;

/// One hop of the launch chain.
enum Step {
    /// Waiting on the postMessage handshake; nothing issued yet.
    Handshake,
    Authorizing(ReqId),
    Exchanging(ReqId),
    Authenticating {
        req: ReqId,
        widget_token: String,
    },
    Ready {
        widget_token: String,
    },
    /// Terminal. Playable, but nothing will be submitted.
    Offline(String),
}

pub struct DiscordBackend {
    activity: Activity,
    client_id: String,
    /// Path prefix the bundle is mounted under, e.g. `/arcade`. Empty for a
    /// bundle at the origin root.
    mount: String,
    step: Step,
    player_name: Option<String>,
    /// When the handshake started, for [`HANDSHAKE_TIMEOUT_SECS`].
    started_at: f64,
}

impl DiscordBackend {
    /// `client_id` must be the Discord application that owns the URL mapping
    /// this bundle is served under — the handshake identifies the iframe as
    /// that app, and a mismatch is refused by the client with no useful
    /// diagnostic.
    pub fn new(client_id: impl Into<String>) -> Self {
        let client_id = client_id.into();
        Self {
            activity: Activity::new(&client_id),
            client_id,
            mount: String::new(),
            step: Step::Handshake,
            player_name: None,
            started_at: macroquad::time::get_time(),
        }
    }

    /// Set the path prefix the bundle is served under, e.g. `/arcade`.
    ///
    /// Every API URL is then **root-relative** (`/arcade/api/seed`) rather than
    /// document-relative. That distinction is load-bearing and easy to get
    /// wrong: a document-relative `api/seed` resolves against whatever
    /// directory the current page sits in, so the lobby at `/arcade/` would
    /// reach `/arcade/api/seed` while a game at `/arcade/xeno-invaders/`
    /// would reach `/arcade/xeno-invaders/api/seed` — a 404 that appears only
    /// on the nested page, and only in the surface that is hardest to debug.
    ///
    /// Root-relative still goes through Discord's proxy: the iframe's origin is
    /// `<app_id>.discordsays.com`, and prefix mappings are matched there.
    pub fn with_mount(mut self, mount: impl Into<String>) -> Self {
        self.mount = mount.into().trim_end_matches('/').to_string();
        self
    }

    /// Absolute-from-root URL for an API path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.mount, path)
    }

    /// The widget token, once the chain has resolved.
    fn token(&self) -> Option<&str> {
        match &self.step {
            Step::Ready { widget_token } | Step::Authenticating { widget_token, .. } => {
                Some(widget_token)
            }
            _ => None,
        }
    }

    fn advance(&mut self) {
        // `std::mem::replace` rather than matching on `&mut self.step`: each
        // hop consumes the previous state's owned token, and borrowing would
        // force a clone of it on every frame.
        let step = std::mem::replace(&mut self.step, Step::Handshake);

        self.step = match step {
            Step::Handshake => match self.activity.update() {
                Phase::Connecting => {
                    if macroquad::time::get_time() - self.started_at > HANDSHAKE_TIMEOUT_SECS {
                        Step::Offline(
                            "Discord did not answer the handshake — playing without a leaderboard"
                                .to_string(),
                        )
                    } else {
                        Step::Handshake
                    }
                }
                Phase::Failed(err) => Step::Offline(format!("not running as an Activity ({err})")),
                Phase::Ready => {
                    let args = serde_json::to_string(&AuthorizeArgs {
                        client_id: &self.client_id,
                        response_type: "code",
                        state: "",
                        prompt: "none",
                        scope: SCOPES,
                    })
                    .expect("static args serialise");
                    // Command names are UPPERCASE on the wire.
                    Step::Authorizing(to_local(self.activity.command("AUTHORIZE", &args)))
                }
            },

            Step::Authorizing(req) => match self.activity.poll(to_discord(req)) {
                PollResult::Pending => Step::Authorizing(req),
                PollResult::Err { data } => {
                    Step::Offline(format!("Discord refused sign-in: {data}"))
                }
                PollResult::Ok { data } => match serde_json::from_str::<AuthorizeResponse>(&data) {
                    Err(e) => Step::Offline(format!("unexpected AUTHORIZE reply: {e}")),
                    Ok(authorized) => {
                        let body = serde_json::to_string(&TokenExchangeRequest {
                            code: &authorized.code,
                            client_id: &self.client_id,
                            guild_id: self.activity.context().guild_id.as_deref(),
                        })
                        .expect("static body serialises");
                        Step::Exchanging(to_local(
                            self.activity.http_post(&self.url(EXCHANGE_PATH), &body),
                        ))
                    }
                },
            },

            Step::Exchanging(req) => match self.activity.poll(to_discord(req)) {
                PollResult::Pending => Step::Exchanging(req),
                PollResult::Err { data } => {
                    Step::Offline(format!("sign-in exchange failed: {data}"))
                }
                PollResult::Ok { data } => {
                    match serde_json::from_str::<TokenExchangeResponse>(&data) {
                        Err(e) => Step::Offline(format!("unexpected exchange reply: {e}")),
                        Ok(tokens) => {
                            if !tokens.display_name.is_empty() {
                                self.player_name = Some(tokens.display_name.clone());
                            }
                            let args = serde_json::to_string(&AuthenticateArgs {
                                access_token: &tokens.access_token,
                            })
                            .expect("static args serialise");
                            Step::Authenticating {
                                req: to_local(self.activity.command("AUTHENTICATE", &args)),
                                widget_token: tokens.widget_token,
                            }
                        }
                    }
                }
            },

            Step::Authenticating { req, widget_token } => match self.activity.poll(to_discord(req))
            {
                PollResult::Pending => Step::Authenticating { req, widget_token },
                // The token is already in hand and the server will verify it
                // independently, so a failure here costs the greeting, not the
                // session. Staying playable beats being correct about a step
                // whose only local product is a display name.
                PollResult::Err { .. } => Step::Ready { widget_token },
                PollResult::Ok { data } => {
                    if let Ok(auth) = serde_json::from_str::<AuthenticateResponse>(&data) {
                        self.player_name =
                            Some(auth.user.global_name.unwrap_or(auth.user.username));
                    }
                    Step::Ready { widget_token }
                }
            },

            terminal => terminal,
        };
    }
}

impl ArcadeBackend for DiscordBackend {
    fn update(&mut self) -> BackendState {
        self.advance();

        match &self.step {
            Step::Handshake => BackendState::Connecting("connecting to Discord"),
            Step::Authorizing(_) => BackendState::Connecting("signing in"),
            Step::Exchanging(_) => BackendState::Connecting("verifying"),
            Step::Authenticating { .. } => BackendState::Connecting("almost there"),
            Step::Ready { .. } => BackendState::Ready,
            Step::Offline(reason) => BackendState::Offline(reason.clone()),
        }
    }

    fn post(&mut self, path: &str, body_json: &str) -> ReqId {
        let url = self.url(path);
        to_local(match self.token() {
            Some(token) => self.activity.http_post_auth(&url, body_json, token),
            // Unauthenticated rather than a panic: the request will be refused
            // by the server, which is the right outcome and a legible one.
            None => self.activity.http_post(&url, body_json),
        })
    }

    fn poll(&mut self, id: ReqId) -> Poll {
        match self.activity.poll(to_discord(id)) {
            PollResult::Pending => Poll::Pending,
            PollResult::Ok { data } => Poll::Ok(data),
            PollResult::Err { data } => Poll::Err(data),
        }
    }

    fn player_name(&self) -> Option<&str> {
        self.player_name.as_deref()
    }
}
