//! What a TURN mint answers, and how long it is worth keeping.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

/// One entry of `RTCPeerConnection`'s `iceServers` list.
///
/// One entry may carry several `urls`: a relay reachable over both UDP and TCP
/// is one server, not two, and it is configured with one credential.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

impl fmt::Debug for IceServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IceServer")
            .field("urls", &self.urls)
            .field("username", &self.username.is_some())
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// `urls` on the wire: a string for one URL, an array for several.
#[derive(Clone, PartialEq, Eq)]
struct Urls(Vec<String>);

impl<'de> Deserialize<'de> for Urls {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UrlsVisitor;

        impl<'de> Visitor<'de> for UrlsVisitor {
            type Value = Urls;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a URL or an array of URLs")
            }

            fn visit_str<E: de::Error>(self, url: &str) -> Result<Urls, E> {
                Ok(Urls(vec![url.to_string()]))
            }

            fn visit_string<E: de::Error>(self, url: String) -> Result<Urls, E> {
                Ok(Urls(vec![url]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Urls, A::Error> {
                let mut urls = Vec::new();
                while let Some(url) = seq.next_element::<String>()? {
                    urls.push(url);
                }
                Ok(Urls(urls))
            }
        }

        deserializer.deserialize_any(UrlsVisitor)
    }
}

#[derive(Deserialize)]
struct RawServer {
    urls: Urls,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    credential: Option<String>,
}

impl<'de> Deserialize<'de> for IceServer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawServer::deserialize(deserializer)?;
        if raw.urls.0.is_empty() || raw.urls.0.iter().any(|url| url.is_empty()) {
            return Err(de::Error::custom("a TURN server with no URLs"));
        }
        Ok(Self {
            urls: raw.urls.0,
            username: raw.username,
            credential: raw.credential,
        })
    }
}

/// `iceServers` on the wire: an object when there is one server, an array when
/// there are several. Both shapes are answered by Cloudflare Realtime, so both
/// are accepted and neither is normalised away by a caller.
struct Servers(Vec<IceServer>);

impl<'de> Deserialize<'de> for Servers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ServersVisitor;

        impl<'de> Visitor<'de> for ServersVisitor {
            type Value = Servers;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a TURN server or an array of them")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Servers, A::Error> {
                let server = IceServer::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Servers(vec![server]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Servers, A::Error> {
                let mut servers = Vec::new();
                while let Some(server) = seq.next_element::<IceServer>()? {
                    servers.push(server);
                }
                Ok(Servers(servers))
            }
        }

        deserializer.deserialize_any(ServersVisitor)
    }
}

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "iceServers")]
    ice_servers: Servers,
}

/// Why a mint's body is unusable.
#[derive(Debug)]
pub enum MintError {
    /// The mint answered a status that is not a success. Carries the provider's
    /// own words, bounded, because `401` alone does not say whether the key was
    /// revoked or the token was mistyped.
    Status { status: u16, said: String },
    /// The body is not the response shape.
    Json(serde_json::Error),
    /// The response parsed, and names no servers.
    NoServers,
}

impl fmt::Display for MintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MintError::Status { status, said } => {
                write!(f, "the mint answered {status}: {said}")
            }
            MintError::Json(error) => write!(f, "the mint response did not parse: {error}"),
            MintError::NoServers => f.write_str("the mint response names no ICE servers"),
        }
    }
}

impl std::error::Error for MintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MintError::Status { .. } => None,
            MintError::Json(error) => Some(error),
            MintError::NoServers => None,
        }
    }
}

/// Credentials a key has minted, and the age that governs their reuse.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Minted {
    pub servers: Vec<IceServer>,
    /// When the mint answered, in Unix ms. Supplied by the caller because the
    /// clock is a platform concern (a Worker's `Date`, a literal in a test).
    pub minted_at_ms: u64,
    /// The TTL the mint asked for. The response does not say: Cloudflare's
    /// credential is a JWT and nothing here decodes its expiry, so the request's
    /// value is the honest one to record.
    pub ttl_s: u64,
}

impl Minted {
    /// Parse a mint's answer, status included.
    ///
    /// Cloudflare answers **`201 Created`**, not `200`, so any `2xx` is a good
    /// mint and anything else is the provider's error — a check for `200` alone
    /// rejects every real mint. This was found the first time the route ran
    /// against the live API, which is why it is here rather than in the caller.
    pub fn parse_response(
        status: u16,
        body: &str,
        minted_at_ms: u64,
        ttl_s: u64,
    ) -> Result<Self, MintError> {
        if !(200..300).contains(&status) {
            return Err(MintError::Status {
                status,
                said: body.chars().take(200).collect(),
            });
        }
        Self::parse(body, minted_at_ms, ttl_s)
    }

    /// Parse a mint's body into the servers plus the age they were given.
    pub fn parse(body: &str, minted_at_ms: u64, ttl_s: u64) -> Result<Self, MintError> {
        let response: Response = serde_json::from_str(body).map_err(MintError::Json)?;
        if response.ice_servers.0.is_empty() {
            return Err(MintError::NoServers);
        }
        Ok(Self {
            servers: response.ice_servers.0,
            minted_at_ms,
            ttl_s,
        })
    }

    /// When these credentials stop working.
    pub fn expires_at_ms(&self) -> u64 {
        self.minted_at_ms
            .saturating_add(self.ttl_s.saturating_mul(1_000))
    }

    /// Whether they are worth reusing at `now_ms`, leaving `margin_s` unspent.
    ///
    /// The margin is not politeness: a credential that expires between this
    /// answer and the browser's `RTCPeerConnection` gathering candidates fails
    /// the connection, and a fresh mint costs one request.
    pub fn is_fresh(&self, now_ms: u64, margin_s: u64) -> bool {
        now_ms.saturating_add(margin_s.saturating_mul(1_000)) < self.expires_at_ms()
    }

    /// The body a browser can take as-is.
    pub fn body(&self) -> IceServersBody<'_> {
        IceServersBody::new(&self.servers)
    }
}

/// `{"iceServers":[…]}` — what `RTCPeerConnection` is configured from.
#[derive(Serialize)]
pub struct IceServersBody<'a> {
    #[serde(rename = "iceServers")]
    ice_servers: &'a [IceServer],
}

impl<'a> IceServersBody<'a> {
    pub fn new(servers: &'a [IceServer]) -> Self {
        Self {
            ice_servers: servers,
        }
    }
}

impl IceServersBody<'static> {
    /// The answer when nothing could be minted. Empty rather than a STUN list:
    /// the client already has its own default, and repeating it here would be
    /// two lists to keep in step.
    pub fn empty() -> Self {
        Self { ice_servers: &[] }
    }
}

/// The body a mint request carries.
///
/// `ttl` is the life asked for. Cloudflare answers with the key's own TTL if
/// that is shorter, and the response does not say which was used — so the
/// caller records the value it asked for and keeps a margin ([`Minted`]).
#[derive(Serialize)]
pub struct MintRequest {
    pub ttl: u64,
}

impl MintRequest {
    pub fn json(ttl_s: u64) -> Result<String, serde_json::Error> {
        serde_json::to_string(&MintRequest { ttl: ttl_s })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The object shape: one entry, three URLs, which is what Cloudflare
    /// Realtime answers for a key. The credential is a fixture and not a
    /// secret — no test here touches a real one.
    const ONE: &str = r#"{
        "iceServers": {
            "urls": [
                "stun:stun.cloudflare.com:3478",
                "turn:turn.cloudflare.com:3478?transport=udp",
                "turn:turn.cloudflare.com:3478?transport=tcp"
            ],
            "username": "1780000000",
            "credential": "fixture-credential"
        }
    }"#;

    /// The array shape, and a bare string for a single URL.
    const SEVERAL: &str = r#"{
        "iceServers": [
            { "urls": "stun:stun.cloudflare.com:3478" },
            { "urls": ["turn:turn.cloudflare.com:3478?transport=udp"], "username": "u", "credential": "c" }
        ]
    }"#;

    #[test]
    fn the_object_shape_is_one_server() {
        let minted = Minted::parse(ONE, 1_000, 3_600).expect("the object shape");
        assert_eq!(minted.servers.len(), 1);
        assert_eq!(minted.servers[0].urls.len(), 3);
        assert_eq!(minted.servers[0].username.as_deref(), Some("1780000000"));
        assert_eq!(
            minted.servers[0].credential.as_deref(),
            Some("fixture-credential")
        );
    }

    #[test]
    fn the_array_shape_is_several_servers() {
        let minted = Minted::parse(SEVERAL, 1_000, 3_600).expect("the array shape");
        assert_eq!(minted.servers.len(), 2);
        assert_eq!(minted.servers[0].urls, ["stun:stun.cloudflare.com:3478"]);
        assert!(minted.servers[0].username.is_none());
        assert_eq!(
            minted.servers[1].urls,
            ["turn:turn.cloudflare.com:3478?transport=udp"]
        );
    }

    #[test]
    fn parse_keeps_the_age_it_was_given() {
        let minted = Minted::parse(ONE, 1_234, 900).expect("a mint");
        assert_eq!(minted.minted_at_ms, 1_234);
        assert_eq!(minted.ttl_s, 900);
    }

    #[test]
    fn a_response_with_no_servers_is_refused() {
        let error = Minted::parse(r#"{"iceServers":[]}"#, 0, 3_600).expect_err("no servers");
        assert!(matches!(error, MintError::NoServers));
    }

    #[test]
    fn a_server_with_no_urls_is_refused() {
        let error = Minted::parse(r#"{"iceServers":{"urls":[]}}"#, 0, 3_600).expect_err("no urls");
        assert!(matches!(error, MintError::Json(_)));
        assert!(error.to_string().contains("no URLs"));
    }

    #[test]
    fn something_that_is_not_the_response_is_refused() {
        let error = Minted::parse("not json at all", 0, 3_600).expect_err("not a response");
        assert!(matches!(error, MintError::Json(_)));
    }

    #[test]
    fn a_mint_is_fresh_until_its_margin_runs_out() {
        // 1000 s of life, and 200 s of it kept back.
        let minted = Minted::parse(ONE, 0, 1_000).expect("a mint");
        assert!(minted.is_fresh(0, 200));
        assert!(minted.is_fresh(799_000, 200));
        assert!(!minted.is_fresh(800_000, 200));
        assert!(!minted.is_fresh(1_000_000, 200));
    }

    #[test]
    fn a_zero_ttl_is_never_fresh() {
        let minted = Minted::parse(ONE, 0, 0).expect("a mint");
        assert!(!minted.is_fresh(0, 0));
    }

    #[test]
    fn debug_does_not_print_the_credential() {
        let minted = Minted::parse(ONE, 0, 3_600).expect("a mint");
        let printed = format!("{minted:?}");
        assert!(printed.contains("turn:turn.cloudflare.com:3478?transport=udp"));
        assert!(!printed.contains("fixture-credential"));
    }

    #[test]
    fn the_body_is_what_a_browser_takes() {
        let minted = Minted::parse(ONE, 0, 3_600).expect("a mint");
        let body = serde_json::to_value(minted.body()).expect("a body");
        let servers = body["iceServers"].as_array().expect("an array");
        assert_eq!(servers.len(), 1);
        assert_eq!(
            servers[0]["credential"].as_str(),
            Some("fixture-credential")
        );
    }

    #[test]
    fn an_empty_body_is_an_empty_list() {
        let body = serde_json::to_value(IceServersBody::empty()).expect("a body");
        assert_eq!(body["iceServers"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_created_mint_is_accepted() {
        // What Cloudflare Realtime actually answers: 201, and the array shape.
        let minted = Minted::parse_response(201, ONE, 0, 3_600).expect("a 201 mint");
        assert_eq!(minted.servers.len(), 1);
    }

    #[test]
    fn an_error_status_is_refused_with_the_providers_words() {
        let error = Minted::parse_response(401, r#"{"error":"unauthorized"}"#, 0, 3_600)
            .expect_err("a 401");
        assert!(matches!(error, MintError::Status { status: 401, .. }));
        assert!(error.to_string().contains("unauthorized"));
        assert!(!error.to_string().contains("fixture-credential"));
    }

    #[test]
    fn the_mint_request_asks_for_a_ttl() {
        assert_eq!(MintRequest::json(3_600).expect("json"), r#"{"ttl":3600}"#);
    }
}
