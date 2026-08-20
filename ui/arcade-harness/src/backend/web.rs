//! Backend for a plain browser tab: absolute API origin, handoff token from
//! the page URL.
//!
//! This is the surface a `widget_handoff` command opens — the platform mints an
//! identity token, puts it in the link, and the page arrives already knowing
//! who the player is. No OAuth round trip, because one already happened in
//! Discord before the link was clicked.

use super::{ArcadeBackend, BackendState, Poll, ReqId};
use quad_net::http_request::{Method, Request, RequestBuilder};
use std::collections::HashMap;

/// Query parameter carrying the platform identity token.
///
/// `token` is what every other widget handoff in the platform uses; matching it
/// means the arcade needs no special case in the command config.
const TOKEN_PARAM: &str = "token";

pub struct WebBackend {
    /// Absolute origin of the score API, e.g. `https://arcade-api.augmint.bot`.
    api_base: String,
    token: Option<String>,
    player_name: Option<String>,
    pending: HashMap<ReqId, Request>,
    next_id: ReqId,
}

impl WebBackend {
    /// Build from the page's own query string.
    ///
    /// A missing token is [`BackendState::Offline`], not an error: someone
    /// opening the arcade URL directly should still get to play, they just
    /// have nowhere to submit.
    pub fn from_page(api_base: impl Into<String>) -> Self {
        Self {
            api_base: into_base(api_base),
            token: query_param(TOKEN_PARAM),
            player_name: query_param("name"),
            pending: HashMap::new(),
            next_id: 1,
        }
    }

    /// Build with a token already in hand.
    pub fn with_token(api_base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            api_base: into_base(api_base),
            token: Some(token.into()),
            player_name: None,
            pending: HashMap::new(),
            next_id: 1,
        }
    }
}

fn into_base(base: impl Into<String>) -> String {
    // Trailing slashes would double up against the leading slash on every API
    // path constant.
    base.into().trim_end_matches('/').to_string()
}

/// Read a parameter from the page URL.
///
/// Goes through the miniquad plugin bridge rather than web-sys — under
/// miniquad's `gl.js` there is no wasm-bindgen glue at all, so `web_sys` is
/// not merely heavy here, it cannot run.
#[cfg(target_arch = "wasm32")]
fn query_param(key: &str) -> Option<String> {
    miniquad_platform::launch_query(key)
}

#[cfg(not(target_arch = "wasm32"))]
fn query_param(_key: &str) -> Option<String> {
    None
}

impl ArcadeBackend for WebBackend {
    fn update(&mut self) -> BackendState {
        match self.token {
            Some(_) => BackendState::Ready,
            None => BackendState::Offline(
                "no session token in the page URL — scores will not be submitted".to_string(),
            ),
        }
    }

    fn post(&mut self, path: &str, body_json: &str) -> ReqId {
        let id = self.next_id;
        self.next_id += 1;

        let mut builder = RequestBuilder::new(&format!("{}{path}", self.api_base))
            .method(Method::Post)
            .header("Content-Type", "application/json")
            .body(body_json);

        if let Some(token) = &self.token {
            builder = builder.header("Authorization", &format!("Bearer {token}"));
        }

        self.pending.insert(id, builder.send());
        id
    }

    fn poll(&mut self, id: ReqId) -> Poll {
        let Some(request) = self.pending.get_mut(&id) else {
            return Poll::Err("no such request".to_string());
        };

        match request.try_recv() {
            None => Poll::Pending,
            Some(result) => {
                // Drop the handle on completion: quad-net delivers once, and
                // holding it would leak one entry per request for the life of
                // the session.
                self.pending.remove(&id);
                match result {
                    Ok(body) => Poll::Ok(body),
                    Err(e) => Poll::Err(format!("{e:?}")),
                }
            }
        }
    }

    fn player_name(&self) -> Option<&str> {
        self.player_name.as_deref()
    }
}
