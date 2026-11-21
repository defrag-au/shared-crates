//! `Gate` — the whole wallet-stake admin auth surface for a worker, so a new
//! worker is: bind the shared `WALLET_AUTH` KV, set two allowlist vars, wire
//! three routes, wrap handlers.
//!
//! Three paths converge on one [`AuthPrincipal`]:
//!
//! - `X-Internal-Key` / `X-Debug-Token` bearer → [`AuthPrincipal::Bearer`],
//!   the operator break-glass (via `api-auth`).
//! - `X-Session-Token` → one KV lookup of a session this gate's realm issued
//!   (`POST /auth/verify`, after a CIP-30 `signData` over our challenge).
//! - `X-Stake-*` CIP-8 per-request signature → full verify, for CLI callers.
//!
//! The allowlist is per worker: `SUPER_ADMIN_STAKES` / `WHITELISTED_STAKES`
//! in that worker's env, checked when a session is issued (the tier is
//! frozen into the record) and on every per-request-signed call. Every
//! failure is a bare 401 — no distinguishers leak to the client.
//!
//! Lifted from the client portal's `auth.rs` + `auth_routes.rs`; the model
//! is documented in `docs/design/PLATFORM_ADMIN_AUTH.md`.
//!
//! ```rust,ignore
//! static GATE: Gate = Gate::new(GateConfig {
//!     kv_binding: "WALLET_AUTH",
//!     realm: Realm::new("abandonware-marketplace"),
//!     ..GateConfig::DEFAULT
//! });
//!
//! Router::new()
//!     .post_async("/auth/challenge", |req, ctx| async move { GATE.challenge(req, &ctx.env).await })
//!     .post_async("/auth/verify",    |req, ctx| async move { GATE.verify(req, &ctx.env).await })
//!     .post_async("/auth/logout",    |req, ctx| async move { GATE.logout(req, &ctx.env).await })
//!     .post_async("/api/thing", |mut req, ctx| async move {
//!         let (principal, body) = match GATE.with_body(&mut req, &ctx.env).await {
//!             Ok(x) => x,
//!             Err(resp) => return Ok(resp),
//!         };
//!         …
//!     })
//! ```

use api_auth::{AuthKeyType, allowlist};
use pallas_addresses::Network;
use serde::Serialize;
use wallet_auth_wire::{StakeChallenge, StakeSession, StakeVerifyRequest};
use worker_stack::worker::*;

use crate::per_request::{PerRequestError, verify_per_request};
use crate::realm::Realm;
use crate::session::{DEFAULT_SESSION_TTL_SECS, WalletAuthStore};

/// Everything a gate needs to know, all `'static` so a worker can hold one
/// in a `static`.
#[derive(Debug, Clone, Copy)]
pub struct GateConfig {
    /// The KV namespace binding holding challenges and sessions. Bind the
    /// SHARED per-environment namespace; the realm keeps records apart.
    pub kv_binding: &'static str,
    /// This worker's identity in that namespace. Use the wrangler `name`.
    pub realm: Realm,
    /// Env var carrying `cardano:mainnet` / `cardano:preprod`.
    pub network_var: &'static str,
    /// Env vars carrying the comma-delimited bech32 stake allowlists.
    pub super_admin_var: &'static str,
    pub whitelisted_var: &'static str,
    /// Session lifetime.
    pub session_ttl_secs: u64,
}

impl GateConfig {
    /// The conventional names; override `kv_binding` and `realm` per worker.
    pub const DEFAULT: GateConfig = GateConfig {
        kv_binding: "WALLET_AUTH",
        realm: Realm::LEGACY,
        network_var: "NETWORK",
        super_admin_var: "SUPER_ADMIN_STAKES",
        whitelisted_var: "WHITELISTED_STAKES",
        session_ttl_secs: DEFAULT_SESSION_TTL_SECS,
    };
}

/// A configured gate. Cheap to hold in a `static`.
pub struct Gate {
    config: GateConfig,
}

/// Who is authenticated on this request.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthPrincipal {
    /// Bearer-authenticated. `internal` is true for `X-Internal-Key`.
    Bearer { internal: bool },
    /// A CIP-8-verified wallet with its allowlist tier.
    Stake { address: String, tier: StakeTier },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StakeTier {
    SuperAdmin,
    Whitelisted,
}

impl StakeTier {
    pub const ALL: [StakeTier; 2] = [StakeTier::SuperAdmin, StakeTier::Whitelisted];

    /// The string a session record carries.
    pub fn as_str(self) -> &'static str {
        match self {
            StakeTier::SuperAdmin => "super_admin",
            StakeTier::Whitelisted => "whitelisted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    fn from_allowlist(tier: allowlist::Tier) -> Option<Self> {
        match tier {
            allowlist::Tier::SuperAdmin => Some(StakeTier::SuperAdmin),
            allowlist::Tier::Whitelisted => Some(StakeTier::Whitelisted),
            allowlist::Tier::Rejected => None,
        }
    }
}

impl AuthPrincipal {
    /// The verified stake address acting on this request, if any.
    pub fn stake_address(&self) -> Option<&str> {
        match self {
            Self::Bearer { .. } => None,
            Self::Stake { address, .. } => Some(address),
        }
    }

    /// Super-admin for read / observability. Either bearer, or a super-admin
    /// stake.
    pub fn is_super_admin(&self) -> bool {
        matches!(
            self,
            Self::Bearer { .. }
                | Self::Stake {
                    tier: StakeTier::SuperAdmin,
                    ..
                }
        )
    }

    /// Privileged for custodial / destructive routes: the internal key or a
    /// super-admin stake. The operator debug token is deliberately excluded.
    pub fn is_privileged(&self) -> bool {
        matches!(
            self,
            Self::Bearer { internal: true }
                | Self::Stake {
                    tier: StakeTier::SuperAdmin,
                    ..
                }
        )
    }

    /// The tier as a session-record string; bearer callers read as super-admin.
    pub fn tier_label(&self) -> &'static str {
        match self {
            Self::Bearer { .. } => StakeTier::SuperAdmin.as_str(),
            Self::Stake { tier, .. } => tier.as_str(),
        }
    }
}

/// Why authentication failed — server-side logging only.
#[derive(Debug)]
pub enum AuthFailure {
    Missing,
    BadTimestamp,
    BadNetwork,
    Verify(PerRequestError),
    NotAllowlisted(String),
}

impl AuthFailure {
    pub fn detail(&self) -> String {
        match self {
            Self::Missing => "missing auth headers".into(),
            Self::BadTimestamp => "X-Stake-Timestamp not a u64".into(),
            Self::BadNetwork => "NETWORK env var missing or unsupported".into(),
            Self::Verify(e) => format!("CIP-8 verify: {e}"),
            Self::NotAllowlisted(stake) => format!("stake not on allowlist: {stake}"),
        }
    }
}

impl Gate {
    pub const fn new(config: GateConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &GateConfig {
        &self.config
    }

    /// The worker's configured Cardano network.
    pub fn network(&self, env: &Env) -> Option<Network> {
        let configured: chains::ChainRef = env
            .var(self.config.network_var)
            .ok()?
            .to_string()
            .parse()
            .ok()?;
        // Mainnet vs testnet is the only distinction this path needs; preprod and
        // preview share pallas' one testnet network. `cardano:testnet` still reads, as
        // preprod. An EVM chain parses but is refused here — there is no pallas network
        // for it.
        if configured.is_mainnet() {
            Some(Network::Mainnet)
        } else if configured.is_cardano() {
            Some(Network::Testnet)
        } else {
            None
        }
    }

    /// The `chain:network` string, for Koios host selection and for telling
    /// the frontend which network to expect.
    pub fn network_string(&self, env: &Env) -> String {
        env.var(self.config.network_var)
            .map(|v| v.to_string())
            .unwrap_or_default()
    }

    /// Resolve a verified stake against this worker's allowlists.
    pub fn allowlist_tier(&self, env: &Env, stake_address: &str) -> Option<StakeTier> {
        let super_admin = env
            .var(self.config.super_admin_var)
            .map(|v| v.to_string())
            .unwrap_or_default();
        let whitelisted = env
            .var(self.config.whitelisted_var)
            .map(|v| v.to_string())
            .unwrap_or_default();
        let super_admin_set = allowlist::parse_stakes(&super_admin);
        let whitelisted_set = allowlist::parse_stakes(&whitelisted);
        StakeTier::from_allowlist(allowlist::tier_for(
            stake_address,
            &super_admin_set,
            &whitelisted_set,
        ))
    }

    /// The shared namespace, or the 500 to return when it is not bound.
    fn kv(&self, env: &Env) -> std::result::Result<kv::KvStore, Response> {
        env.kv(self.config.kv_binding).map_err(|e| {
            tracing::error!("{} not bound: {e}", self.config.kv_binding);
            Response::error("server misconfigured (KV)", 500).expect("error response")
        })
    }

    // ── The gate ─────────────────────────────────────────────────────

    /// Authenticate a request: bearer, then session token, then CIP-8
    /// per-request signature.
    pub async fn authenticate(
        &self,
        req: &Request,
        env: &Env,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> std::result::Result<AuthPrincipal, AuthFailure> {
        if let Ok(caller) = api_auth::authenticate(req, env, AuthKeyType::Any).await {
            return Ok(AuthPrincipal::Bearer {
                internal: caller.is_full_trust(),
            });
        }

        if let Some(token) = req.headers().get("X-Session-Token").ok().flatten()
            && let Ok(kv) = env.kv(self.config.kv_binding)
        {
            let store = WalletAuthStore::new(&kv, self.config.realm);
            match store.lookup_session(&token).await {
                Ok(Some(record)) => {
                    let tier = StakeTier::parse(&record.tier).ok_or_else(|| {
                        AuthFailure::NotAllowlisted(format!(
                            "session record has invalid tier '{}'",
                            record.tier
                        ))
                    })?;
                    return Ok(AuthPrincipal::Stake {
                        address: record.stake_address,
                        tier,
                    });
                }
                Ok(None) => {} // absent / expired / another realm — fall through
                Err(e) => tracing::warn!("session lookup failed: {e}"),
            }
        }

        let headers = req.headers();
        let (stake, ts_str, cose_sig, cose_key) = match (
            headers.get("X-Stake-Address").ok().flatten(),
            headers.get("X-Stake-Timestamp").ok().flatten(),
            headers.get("X-Stake-CoseSig").ok().flatten(),
            headers.get("X-Stake-CoseKey").ok().flatten(),
        ) {
            (Some(s), Some(t), Some(sg), Some(k)) => (s, t, sg, k),
            _ => return Err(AuthFailure::Missing),
        };
        let timestamp: u64 = ts_str.parse().map_err(|_| AuthFailure::BadTimestamp)?;
        let now_unix = (js_sys::Date::now() / 1000.0) as u64;
        let network = self.network(env).ok_or(AuthFailure::BadNetwork)?;
        let verified = verify_per_request(
            method, path, body, timestamp, now_unix, &cose_sig, &cose_key, &stake, network,
        )
        .map_err(AuthFailure::Verify)?;
        let tier = self
            .allowlist_tier(env, &verified.stake_address)
            .ok_or_else(|| AuthFailure::NotAllowlisted(verified.stake_address.clone()))?;
        Ok(AuthPrincipal::Stake {
            address: verified.stake_address,
            tier,
        })
    }

    /// Read body bytes + authenticate (POST routes). On failure the `Err` is
    /// the 401 to return.
    pub async fn with_body(
        &self,
        req: &mut Request,
        env: &Env,
    ) -> std::result::Result<(AuthPrincipal, Vec<u8>), Response> {
        let method = req.method().to_string();
        let path = req.path();
        let body_bytes = req.bytes().await.map_err(|e| {
            Response::error(format!("invalid body: {e}"), 400).expect("error response")
        })?;
        match self
            .authenticate(req, env, &method, &path, &body_bytes)
            .await
        {
            Ok(p) => Ok((p, body_bytes)),
            Err(e) => {
                tracing::warn!("auth failed for {method} {path}: {}", e.detail());
                Err(Response::error("Unauthorized", 401).expect("error response"))
            }
        }
    }

    /// Authenticate without consuming the body (GET routes).
    pub async fn only(
        &self,
        req: &Request,
        env: &Env,
    ) -> std::result::Result<AuthPrincipal, Response> {
        let method = req.method().to_string();
        let path = req.path();
        match self.authenticate(req, env, &method, &path, b"").await {
            Ok(p) => Ok(p),
            Err(e) => {
                tracing::warn!("auth failed for {method} {path}: {}", e.detail());
                Err(Response::error("Unauthorized", 401).expect("error response"))
            }
        }
    }

    // ── The routes ───────────────────────────────────────────────────

    /// `POST /auth/challenge` — a fresh challenge to sign. The payload pins
    /// the worker's host, so a dev signature never verifies on production.
    pub async fn challenge(&self, req: Request, env: &Env) -> Result<Response> {
        let Some(domain) = req
            .url()
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
        else {
            return Response::error("could not determine host", 500);
        };
        let kv = match self.kv(env) {
            Ok(kv) => kv,
            Err(resp) => return Ok(resp),
        };
        let store = WalletAuthStore::new(&kv, self.config.realm);
        match store.create_challenge(&domain).await {
            Ok(issuance) => Response::from_json(&StakeChallenge {
                challenge_id: issuance.challenge_id,
                payload_to_sign_hex: issuance.payload_to_sign_hex,
                expires_at_ms: issuance.expires_at_ms,
                network: self.network_string(env),
            }),
            Err(e) => {
                tracing::error!("create_challenge: {e}");
                Response::error("internal error", 500)
            }
        }
    }

    /// `POST /auth/verify` — exchange the signed challenge for a session,
    /// gated on this worker's allowlist.
    pub async fn verify(&self, mut req: Request, env: &Env) -> Result<Response> {
        let body: StakeVerifyRequest = match req.json().await {
            Ok(b) => b,
            Err(e) => return Response::error(format!("invalid request body: {e}"), 400),
        };
        let kv = match self.kv(env) {
            Ok(kv) => kv,
            Err(resp) => return Ok(resp),
        };
        let Some(network) = self.network(env) else {
            return Response::error("server misconfigured (NETWORK)", 500);
        };
        let store = WalletAuthStore::new(&kv, self.config.realm);
        let verified = match store
            .consume_challenge(
                &body.challenge_id,
                &body.cose_sig_hex,
                &body.cose_key_hex,
                network,
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("consume_challenge failed: {e}");
                return Response::error("Unauthorized", 401);
            }
        };
        let Some(tier) = self.allowlist_tier(env, &verified.stake_address) else {
            tracing::warn!(
                "verify: stake {} not on the {} allowlist",
                verified.stake_address,
                self.config.realm
            );
            return Response::error("Unauthorized", 401);
        };
        match store
            .issue_session(
                &verified.stake_address,
                tier.as_str(),
                self.config.session_ttl_secs,
            )
            .await
        {
            Ok(session) => Response::from_json(&StakeSession {
                token: session.token,
                stake_address: session.stake_address,
                tier: session.tier,
                expires_at_ms: session.expires_at_ms,
            }),
            Err(e) => {
                tracing::error!("issue_session: {e}");
                Response::error("internal error", 500)
            }
        }
    }

    /// `POST /auth/logout` — revoke the session. Idempotent without a token.
    pub async fn logout(&self, req: Request, env: &Env) -> Result<Response> {
        #[derive(Serialize)]
        struct LogoutResponse {
            revoked: bool,
        }
        let Some(token) = req.headers().get("X-Session-Token").ok().flatten() else {
            return Response::from_json(&LogoutResponse { revoked: false });
        };
        let kv = match self.kv(env) {
            Ok(kv) => kv,
            Err(resp) => return Ok(resp),
        };
        match WalletAuthStore::new(&kv, self.config.realm)
            .revoke_session(&token)
            .await
        {
            Ok(()) => Response::from_json(&LogoutResponse { revoked: true }),
            Err(e) => {
                tracing::error!("revoke_session: {e}");
                Response::error("internal error", 500)
            }
        }
    }
}
