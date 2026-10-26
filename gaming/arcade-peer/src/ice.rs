//! The ICE server list a connection is built with.
//!
//! Two lists meet: the short-lived credentials the room mints (`GET /ice`) and
//! the defaults a build carries so a link works with no mint at all. The rule is
//! small and load-bearing — the room's list comes first and every entry of it is
//! kept; a default earns its place only if it names an address the room's list
//! does not already have. Cloudflare's mint carries its own STUN server, so
//! listing the default again buys nothing; Google's is a second operator and is
//! the entry that survives.
//!
//! It lives here rather than in the shim so the rule can be tested without a
//! browser, and so the list a report shows is the list the connection was built
//! from rather than one rebuilt by eye.

/// One `RTCIceServer`: one or more URLs sharing a credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl IceServer {
    /// A server with no credential, which is what a STUN entry is.
    pub fn stun(urls: &[&str]) -> Self {
        Self {
            urls: urls.iter().map(|url| (*url).to_string()).collect(),
            username: None,
            credential: None,
        }
    }

    pub fn with_credential(mut self, username: &str, credential: &str) -> Self {
        self.username = Some(username.to_string());
        self.credential = Some(credential.to_string());
        self
    }

    /// Whether every URL this server names is already covered by `seen`.
    fn covered(&self, seen: &[&str]) -> bool {
        self.urls.iter().all(|url| seen.contains(&url.as_str()))
    }
}

/// The list a connection should be built with: the room's entries in order, then
/// each default that adds an address the room did not name.
pub fn merge(room: &[IceServer], defaults: &[IceServer]) -> Vec<IceServer> {
    let mut seen: Vec<&str> = Vec::new();
    let mut merged = Vec::new();

    for server in room {
        for url in &server.urls {
            if !seen.contains(&url.as_str()) {
                seen.push(url);
            }
        }
        merged.push(server.clone());
    }
    for server in defaults {
        if server.covered(&seen) {
            continue;
        }
        for url in &server.urls {
            if !seen.contains(&url.as_str()) {
                seen.push(url);
            }
        }
        merged.push(server.clone());
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOUDFLARE_STUN: &str = "stun:stun.cloudflare.com:3478";
    const GOOGLE_STUN: &str = "stun:stun.l.google.com:19302";
    const CLOUDFLARE_TURN: &str = "turn:turn.cloudflare.com:3478?transport=udp";

    /// The measured shape of a mint response: one STUN entry and one TURN entry
    /// carrying six URLs. The default Cloudflare STUN is covered by the first;
    /// Google's is a second operator and survives.
    #[test]
    fn the_rooms_list_comes_first_and_an_uncovered_default_is_kept() {
        let room = vec![
            IceServer::stun(&[CLOUDFLARE_STUN]),
            IceServer::stun(&[CLOUDFLARE_TURN]).with_credential("key-id", "token"),
        ];
        let defaults = vec![
            IceServer::stun(&[CLOUDFLARE_STUN]),
            IceServer::stun(&[GOOGLE_STUN]),
        ];

        let merged = merge(&room, &defaults);
        assert_eq!(merged.len(), 3, "{merged:?}");
        assert_eq!(merged[0].urls, vec![CLOUDFLARE_STUN.to_string()]);
        assert_eq!(merged[1].urls, vec![CLOUDFLARE_TURN.to_string()]);
        assert_eq!(merged[2].urls, vec![GOOGLE_STUN.to_string()]);
        assert_eq!(merged[1].credential.as_deref(), Some("token"));
    }

    /// A default that names nothing new is dropped entirely rather than merged
    /// field by field: its URLs are already reachable through the room's entry.
    #[test]
    fn a_default_that_adds_nothing_is_dropped() {
        let room = vec![IceServer::stun(&[CLOUDFLARE_STUN])];
        let defaults = vec![IceServer::stun(&[CLOUDFLARE_STUN])];
        assert_eq!(merge(&room, &defaults), room);
    }

    /// No mint is not no ICE: the defaults are the list, which is what a link
    /// with no TURN key connects on.
    #[test]
    fn an_empty_room_list_leaves_the_defaults() {
        let defaults = vec![IceServer::stun(&[CLOUDFLARE_STUN, GOOGLE_STUN])];
        assert_eq!(merge(&[], &defaults), defaults);
    }

    /// One server may name several URLs, and a default is covered only if all of
    /// them are. A default that adds one address is kept whole, because
    /// `RTCIceServer` shares a credential across its URLs.
    #[test]
    fn a_server_is_covered_only_when_every_url_it_names_is() {
        let room = vec![IceServer::stun(&[CLOUDFLARE_STUN])];
        let defaults = vec![IceServer::stun(&[CLOUDFLARE_STUN, GOOGLE_STUN])];
        let merged = merge(&room, &defaults);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].urls.len(), 2);
    }
}
