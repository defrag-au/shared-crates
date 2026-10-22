//! Worker-side OAuth routes.
//!
//! A consuming worker mounts two routes: [`begin_login`] where the app's
//! "Log in with Discord" links, and [`finish_login`] at the redirect URI.
//!
//! ```ignore
//! .get("/auth/discord", |req, ctx| {
//!     discord_auth::worker::begin_login(&client_id, &redirect_uri, &[scopes::IDENTIFY])
//! })
//! .get_async("/auth/discord/callback", |req, ctx| async move {
//!     discord_auth::worker::finish_login(&req, &CONFIG, &settings).await
//! })
//! ```
//!
//! [`handle_login`] and [`handle_callback`] are the same without the `state`
//! check, for workers mounted before it existed.
//!
//! The handler:
//! 1. reads `?code=` (+ `?state=`) from the callback URL,
//! 2. exchanges the code for a user access token (Discord `/oauth2/token`),
//! 3. reads the user id (`/users/@me`), the user's roles in each configured
//!    guild (`/users/@me/guilds/{guild}/member`), and the user's computed
//!    permissions per guild (`/users/@me/guilds`),
//! 4. resolves entitlements via the [`GuildRoleConfig`],
//! 5. mints an `authorizations` session JWT (HS256, shared secret), and
//! 6. 302-redirects to the app with the token in the URL fragment
//!    (`#session=<jwt>`) — fragments aren't sent to servers, so the token
//!    doesn't leak into logs.

use authorizations::{SessionClaims, mint_token};
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

    // 2. Identity (id + display name + avatar for the UI).
    let user =
        match discord_get::<DiscordUser>(&access_token, "https://discord.com/api/v10/users/@me")
            .await
        {
            Ok(u) => u,
            Err(e) => return Response::error(format!("identify failed: {e}"), 502),
        };
    let user_id = user.id.clone();
    // Prefer the global (display) name, fall back to username.
    let display_name = user.global_name.clone().or_else(|| user.username.clone());

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

    // 4. Permissions per guild — one call, and the only source for "is this
    //    person an administrator/manage-server here". A guild the user is not
    //    in simply isn't in the list. Failure leaves the role grants working.
    let mut permissions_by_guild: HashMap<String, u64> = HashMap::new();
    if let Ok(guilds) = discord_get::<Vec<DiscordGuild>>(
        &access_token,
        "https://discord.com/api/v10/users/@me/guilds",
    )
    .await
    {
        for guild in guilds {
            let mut permissions = guild
                .permissions
                .as_deref()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            if guild.owner {
                permissions |= crate::GuildPermission::Administrator.bit();
            }
            permissions_by_guild.insert(guild.id, permissions);
        }
    }

    // 5. Resolve entitlements (role grants, permission grants + direct
    //    per-user admin grants).
    let entitlements =
        config.resolve_with_permissions(&user_id, &roles_by_guild, &permissions_by_guild);
    // The guild the session was granted through (first that contributed),
    // for provenance only.
    let granted_via = config
        .servers
        .iter()
        .find(|s| {
            roles_by_guild.contains_key(&s.guild_id)
                || permissions_by_guild.contains_key(&s.guild_id)
        })
        .map(|s| s.guild_id.clone());

    // 6. Mint session.
    let claims = SessionClaims {
        guild: granted_via,
        name: display_name,
        avatar: user.avatar.clone(),
        ..SessionClaims::for_discord(user_id, entitlements.join(" "))
    };
    let token = match mint_token(claims, settings.session_secret, settings.ttl) {
        Ok(t) => t,
        Err(e) => return Response::error(format!("session mint failed: {e}"), 500),
    };

    // 7. Redirect to the app with the token in the fragment.
    let location = format!("{}#session={token}", settings.app_url.trim_end_matches('#'));
    let headers = Headers::new();
    headers.set("Location", &location)?;
    Ok(Response::empty()?.with_status(302).with_headers(headers))
}

/// The cookie carrying a login's `state` nonce from [`begin_login`] to
/// [`finish_login`].
const STATE_COOKIE: &str = "discord_auth_state";
/// How long a login may take, in seconds, before its `state` is forgotten.
const STATE_SECS: u32 = 600;

/// Begin login, checked: a fresh random `state` nonce goes to Discord in the
/// authorize URL and to the browser in a short-lived `HttpOnly` cookie, and
/// [`finish_login`] accepts the callback only when the two agree. That stops a
/// forged callback logging a victim into someone else's account (login CSRF).
///
/// Mount it on the route the app's "Log in with Discord" links to, and
/// [`finish_login`] on `redirect_uri`'s path.
pub fn begin_login(client_id: &str, redirect_uri: &str, scopes: &[&str]) -> Result<Response> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|e| Error::RustError(format!("state nonce: {e}")))?;
    let state: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let mut response = handle_login(client_id, redirect_uri, scopes, &state)?;
    response.headers_mut().set(
        "Set-Cookie",
        &format!(
            "{STATE_COOKIE}={state}; Max-Age={STATE_SECS}; Path=/; HttpOnly; Secure; SameSite=Lax"
        ),
    )?;
    Ok(response)
}

/// Finish a login [`begin_login`] began: check the callback's `state` against
/// the cookie, then [`handle_callback`], and forget the cookie either way.
pub async fn finish_login(
    req: &Request,
    config: &GuildRoleConfig,
    settings: &CallbackSettings<'_>,
) -> Result<Response> {
    let state = req
        .url()?
        .query_pairs()
        .find_map(|(k, v)| (k == "state").then(|| v.into_owned()));
    let cookie = req.headers().get("Cookie")?.unwrap_or_default();
    let expected = cookie_value(&cookie, STATE_COOKIE);
    let mut response = match (state, expected) {
        (Some(state), Some(expected)) if !state.is_empty() && state == expected => {
            handle_callback(req, config, settings).await?
        }
        _ => Response::error(
            "this login did not start here, or took too long: please log in again",
            400,
        )?,
    };
    response.headers_mut().set(
        "Set-Cookie",
        &format!("{STATE_COOKIE}=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Lax"),
    )?;
    Ok(response)
}

/// A cookie's value from a `Cookie` header.
fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|pair| {
        let (k, v) = pair.trim().split_once('=')?;
        (k == name).then_some(v)
    })
}

/// Begin login: 302-redirect the browser to Discord's authorize URL. The
/// worker owns all OAuth config (client id, redirect, scopes), so the
/// frontend's "Login with Discord" affordance is just a link to the route
/// that calls this — no client id or redirect needs to live in the app.
///
/// `state` is passed through unchecked: prefer [`begin_login`] and
/// [`finish_login`], which set and check a nonce.
pub fn handle_login(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    state: &str,
) -> Result<Response> {
    let url = crate::authorize_url(client_id, redirect_uri, scopes, state);
    let headers = Headers::new();
    headers.set("Location", &url)?;
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
    resp.json().await
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct DiscordUser {
    id: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    global_name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
}

#[derive(serde::Deserialize)]
struct GuildMember {
    #[serde(default)]
    roles: Vec<String>,
}

/// One entry from `GET /users/@me/guilds`.
#[derive(serde::Deserialize)]
struct DiscordGuild {
    id: String,
    /// The user's computed permissions in the guild, as a decimal string.
    /// Absent when the `guilds` scope was not granted; read as zero.
    #[serde(default)]
    permissions: Option<String>,
    /// Owners may report a permissions value that does not include the
    /// administrator bit, so ownership is treated as that bit itself.
    #[serde(default)]
    owner: bool,
}
