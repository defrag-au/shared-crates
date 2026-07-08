//! Worker-side OAuth callback mixin.
//!
//! A consuming worker mounts one route and calls [`handle_callback`]:
//!
//! ```ignore
//! .get_async("/auth/discord/callback", |req, ctx| async move {
//!     discord_auth::worker::handle_callback(&req, &ctx.env, &CONFIG, &settings).await
//! })
//! ```
//!
//! The handler:
//! 1. reads `?code=` (+ `?state=`) from the callback URL,
//! 2. exchanges the code for a user access token (Discord `/oauth2/token`),
//! 3. reads the user id (`/users/@me`) and, for each configured guild, the
//!    user's roles (`/users/@me/guilds/{guild}/member`),
//! 4. resolves entitlements via the [`GuildRoleConfig`],
//! 5. mints an `authorizations` session JWT (HS256, shared secret), and
//! 6. 302-redirects to the app with the token in the URL fragment
//!    (`#session=<jwt>`) — fragments aren't sent to servers, so the token
//!    doesn't leak into logs.

use authorizations::{mint_token, SessionClaims};
use std::collections::HashMap;
use worker_stack::worker::*;

use crate::GuildRoleConfig;

/// Deployment settings the consuming worker supplies (from wrangler vars +
/// secrets). Kept separate from the checked-in [`GuildRoleConfig`] because
/// these are per-environment.
pub struct CallbackSettings<'a> {
    pub client_id: &'a str,
    /// Discord app client secret (a worker secret).
    pub client_secret: &'a str,
    /// Must exactly match the redirect registered on the Discord app AND
    /// the one used to build the authorize URL.
    pub redirect_uri: &'a str,
    /// Where to send the user after minting the session — the app URL. The
    /// token is appended as `#session=<jwt>`.
    pub app_url: &'a str,
    /// HS256 secret shared with every verifier (same key `authorizations`
    /// verifies with).
    pub session_secret: &'a [u8],
    /// Session lifetime.
    pub ttl: chrono::Duration,
}

/// Handle the OAuth redirect. Returns a 302 to the app on success, or a
/// 4xx with a plain-text reason on failure.
pub async fn handle_callback(
    req: &Request,
    config: &GuildRoleConfig,
    settings: &CallbackSettings<'_>,
) -> Result<Response> {
    let url = req.url()?;
    let code = url
        .query_pairs()
        .find_map(|(k, v)| (k == "code").then(|| v.into_owned()));
    let Some(code) = code else {
        // Discord sends `?error=access_denied` when the user declines.
        let err = url
            .query_pairs()
            .find_map(|(k, v)| (k == "error").then(|| v.into_owned()))
            .unwrap_or_else(|| "missing code".to_string());
        return Response::error(format!("discord auth cancelled: {err}"), 400);
    };

    // 1. Exchange code → user access token.
    let access_token = match exchange_code(&code, settings).await {
        Ok(t) => t,
        Err(e) => return Response::error(format!("token exchange failed: {e}"), 502),
    };

    // 2. User id.
    let user_id =
        match discord_get::<DiscordUser>(&access_token, "https://discord.com/api/v10/users/@me")
            .await
        {
            Ok(u) => u.id,
            Err(e) => return Response::error(format!("identify failed: {e}"), 502),
        };

    // 3. Roles per configured guild (config = allowlist). A guild the user
    //    isn't in / that the endpoint refuses just contributes no roles.
    let mut roles_by_guild: HashMap<String, Vec<String>> = HashMap::new();
    for guild_id in config.guild_ids() {
        let member_url = format!("https://discord.com/api/v10/users/@me/guilds/{guild_id}/member");
        match discord_get::<GuildMember>(&access_token, &member_url).await {
            Ok(m) => {
                roles_by_guild.insert(guild_id.to_string(), m.roles);
            }
            Err(_) => {
                // Not a member, or the endpoint declined (e.g. bot absent) —
                // no grants from this guild.
            }
        }
    }

    // 4. Resolve entitlements.
    let entitlements = config.resolve(&roles_by_guild);
    // The guild the session was granted through (first that contributed),
    // for provenance only.
    let granted_via = config
        .servers
        .iter()
        .find(|s| roles_by_guild.contains_key(&s.guild_id))
        .map(|s| s.guild_id.clone());

    // 5. Mint session.
    let claims = SessionClaims {
        sub: user_id,
        guild: granted_via,
        tier: None,
        ent: entitlements.join(" "),
    };
    let token = match mint_token(claims, settings.session_secret, settings.ttl) {
        Ok(t) => t,
        Err(e) => return Response::error(format!("session mint failed: {e}"), 500),
    };

    // 6. Redirect to the app with the token in the fragment.
    let location = format!("{}#session={token}", settings.app_url.trim_end_matches('#'));
    let headers = Headers::new();
    headers.set("Location", &location)?;
    Ok(Response::empty()?.with_status(302).with_headers(headers))
}

/// POST the authorization-code grant to Discord's token endpoint
/// (form-urlencoded, as Discord requires) and return the access token.
async fn exchange_code(code: &str, settings: &CallbackSettings<'_>) -> Result<String> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}",
        crate::percent_encode(code),
        crate::percent_encode(settings.redirect_uri),
        crate::percent_encode(settings.client_id),
        crate::percent_encode(settings.client_secret),
    );
    let headers = Headers::new();
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.into()));
    let request = Request::new_with_init("https://discord.com/api/v10/oauth2/token", &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    if resp.status_code() != 200 {
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::RustError(format!(
            "token endpoint {}: {text}",
            resp.status_code()
        )));
    }
    let parsed: TokenResponse = resp.json().await?;
    Ok(parsed.access_token)
}

/// Authenticated GET against the Discord API with a user Bearer token.
async fn discord_get<T: serde::de::DeserializeOwned>(access_token: &str, url: &str) -> Result<T> {
    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {access_token}"))?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    if resp.status_code() != 200 {
        return Err(Error::RustError(format!(
            "GET {url} -> {}",
            resp.status_code()
        )));
    }
    Ok(resp.json().await?)
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct DiscordUser {
    id: String,
}

#[derive(serde::Deserialize)]
struct GuildMember {
    #[serde(default)]
    roles: Vec<String>,
}
