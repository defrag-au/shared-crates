//! The links a canvas can open, typed.
//!
//! A canvas has no `<a href>`, so a tap on a drawn link is opened by
//! `platform.js` from inside the tap's own handler (see [`crate::link_zone`]).
//! That makes the address a value the page follows on the game's say-so, and
//! an address can come from anywhere: an API reply, a collection's metadata, a
//! query parameter. So the page follows only a [`Link`], and a `Link` is only
//! built through a matcher that accepts one exact shape per kind:
//!
//! | Kind | Shape | The page |
//! |---|---|---|
//! | [`Link::Page`] | `https://host[:port][/path][?query][#fragment]` | opens it in a new tab |
//! | [`Link::CardanoWallet`] | `web+cardano://browse/v1?uri=<percent-encoded page>` (CIP-158) | follows it in place: the phone hands it to a Cardano wallet app's browser |
//!
//! Everything else is refused, loudly: `javascript:`, `data:`, `http:`,
//! credentials before the host (`https://good.example@evil.example`), a
//! backslash (which a browser reads as a slash in a URL's authority), any
//! whitespace or control character, anything outside ASCII (a look-alike host
//! must arrive as punycode, where it is visibly foreign).
//!
//! `platform.js` checks each kind again against the same shape before
//! following it. The two must agree; `the_page_script_checks_the_same_shapes`
//! pins the patterns it uses.

use std::fmt;

/// The longest address followed. Browsers take more; nothing this crate opens
/// needs it, and a cap keeps a pathological value out of the page.
pub const MAX_LEN: usize = 2048;

/// The fixed head of a CIP-158 link: the `browse` authority, version 1.
const CIP158_HEAD: &str = "web+cardano://browse/v1?uri=";

/// A link the page may open from a tap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// A web page, opened in a new tab.
    Page(HttpsUrl),
    /// CIP-158: the page opened in a Cardano wallet app's own browser, where
    /// that wallet is injected. Nothing happens on a device with no wallet app
    /// registered for `web+cardano:`, and the page cannot tell, so offer
    /// another way beside it.
    CardanoWallet(HttpsUrl),
}

impl Link {
    /// A web page, from its `https:` address.
    pub fn page(url: &str) -> Result<Self, LinkError> {
        HttpsUrl::parse(url).map(Link::Page)
    }

    /// CIP-158: `url`, an `https:` page, opened in a Cardano wallet app.
    pub fn cardano_wallet(url: &str) -> Result<Self, LinkError> {
        HttpsUrl::parse(url).map(Link::CardanoWallet)
    }

    /// Recognise an address the page would follow: the inverse of
    /// [`Link::href`]. Each kind's matcher is tried in turn and the first that
    /// accepts the whole address wins; the kinds' shapes do not overlap.
    pub fn matching(href: &str) -> Result<Self, LinkError> {
        if starts_with_ignoring_case(href, "web+cardano:") {
            return Self::match_cardano_wallet(href);
        }
        Self::match_page(href)
    }

    /// The [`Link::Page`] matcher: an `https:` address.
    pub fn match_page(href: &str) -> Result<Self, LinkError> {
        HttpsUrl::parse(href).map(Link::Page)
    }

    /// The [`Link::CardanoWallet`] matcher: CIP-158's `browse` authority,
    /// version 1, whose `uri` is a percent-encoded `https:` page and nothing
    /// else. CIP-158 also allows `http:` and a bare host; neither is opened
    /// from here.
    pub fn match_cardano_wallet(href: &str) -> Result<Self, LinkError> {
        check_len(href)?;
        let encoded = href
            .strip_prefix(CIP158_HEAD)
            .ok_or(LinkError::NotCardanoBrowse)?;
        if encoded.is_empty() {
            return Err(LinkError::Empty);
        }
        let target = percent_decode(encoded)?;
        HttpsUrl::parse(&target).map(Link::CardanoWallet)
    }

    /// The address the page follows.
    pub fn href(&self) -> String {
        match self {
            Link::Page(url) => url.as_str().to_string(),
            Link::CardanoWallet(url) => format!("{CIP158_HEAD}{}", percent_encode(url.as_str())),
        }
    }

    /// The page the link ends at, whichever way it gets there.
    pub fn target(&self) -> &HttpsUrl {
        match self {
            Link::Page(url) | Link::CardanoWallet(url) => url,
        }
    }

    /// The kind as `platform.js` numbers it; 0 is no link.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) fn kind_code(&self) -> u32 {
        match self {
            Link::Page(_) => 1,
            Link::CardanoWallet(_) => 2,
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.href())
    }
}

/// An `https:` address that passed [`HttpsUrl::parse`]: ASCII, no whitespace,
/// no credentials, a host of letters, digits, hyphens and dots, and the scheme
/// and host in lower case.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HttpsUrl(String);

impl HttpsUrl {
    pub fn parse(url: &str) -> Result<Self, LinkError> {
        check_len(url)?;
        if url.is_empty() {
            return Err(LinkError::Empty);
        }
        if let Some(c) = url.chars().find(|c| !c.is_ascii()) {
            return Err(LinkError::NotAscii(c));
        }
        if let Some(c) = url
            .chars()
            .find(|c| c.is_ascii_control() || c.is_ascii_whitespace())
        {
            return Err(LinkError::Whitespace(c));
        }
        if let Some(c) = url.chars().find(|&c| c == '\\' || c == '"') {
            return Err(LinkError::ForbiddenCharacter(c));
        }
        if !starts_with_ignoring_case(url, "https://") {
            return Err(LinkError::NotHttps);
        }
        let rest = &url["https://".len()..];
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if authority.contains('@') {
            return Err(LinkError::Credentials);
        }
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let host = host.to_ascii_lowercase();
        check_host(&host)?;
        if let Some(port) = port {
            check_port(port)?;
        }
        let authority = match port {
            Some(port) => format!("{host}:{port}"),
            None => host,
        };
        Ok(HttpsUrl(format!("https://{authority}{tail}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HttpsUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why an address is not a [`Link`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    Empty,
    TooLong(usize),
    NotAscii(char),
    Whitespace(char),
    ForbiddenCharacter(char),
    NotHttps,
    Credentials,
    BadHost(String),
    BadPort(String),
    NotCardanoBrowse,
    BadEncoding,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Empty => f.write_str("the address is empty"),
            LinkError::TooLong(len) => write!(f, "the address is {len} long, over {MAX_LEN}"),
            LinkError::NotAscii(c) => write!(f, "{c:?} is not ASCII; a host must be punycode"),
            LinkError::Whitespace(c) => write!(f, "{c:?} is whitespace or a control character"),
            LinkError::ForbiddenCharacter(c) => write!(f, "{c:?} is not allowed in a link"),
            LinkError::NotHttps => f.write_str("only https: pages are opened"),
            LinkError::Credentials => f.write_str("credentials before the host are refused"),
            LinkError::BadHost(host) => write!(f, "{host:?} is not a host name"),
            LinkError::BadPort(port) => write!(f, "{port:?} is not a port"),
            LinkError::NotCardanoBrowse => {
                write!(f, "a Cardano wallet link starts {CIP158_HEAD:?}")
            }
            LinkError::BadEncoding => f.write_str("the wallet link's uri is not percent-encoded"),
        }
    }
}

impl std::error::Error for LinkError {}

fn check_len(s: &str) -> Result<(), LinkError> {
    match s.len() > MAX_LEN {
        true => Err(LinkError::TooLong(s.len())),
        false => Ok(()),
    }
}

fn starts_with_ignoring_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Dot-separated labels of lower-case letters, digits and inner hyphens.
fn check_host(host: &str) -> Result<(), LinkError> {
    let bad = || LinkError::BadHost(host.to_string());
    if host.is_empty() || host.len() > 253 {
        return Err(bad());
    }
    for label in host.split('.') {
        let ok = !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !ok {
            return Err(bad());
        }
    }
    Ok(())
}

fn check_port(port: &str) -> Result<(), LinkError> {
    let ok = (1..=5).contains(&port.len())
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u32>().is_ok_and(|p| (1..=65_535).contains(&p));
    match ok {
        true => Ok(()),
        false => Err(LinkError::BadPort(port.to_string())),
    }
}

/// RFC 3986's unreserved characters pass; every other byte is `%XX`.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The inverse of [`percent_encode`], accepting only its alphabet: an
/// unreserved character or a `%` and two hex digits.
fn percent_decode(s: &str) -> Result<String, LinkError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3).ok_or(LinkError::BadEncoding)?;
                let hex = std::str::from_utf8(hex).map_err(|_| LinkError::BadEncoding)?;
                if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(LinkError::BadEncoding);
                }
                let b = u8::from_str_radix(hex, 16).map_err(|_| LinkError::BadEncoding)?;
                out.push(b);
                i += 3;
            }
            b @ (b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') => {
                out.push(b);
                i += 1;
            }
            _ => return Err(LinkError::BadEncoding),
        }
    }
    String::from_utf8(out).map_err(|_| LinkError::BadEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDOFF: &str = "https://robinhood-dev.perp.quest/?handoff=K7QX4MN2";

    fn page_err(url: &str) -> LinkError {
        Link::page(url).expect_err(url)
    }

    // ── Pages that open ─────────────────────────────────────────────────

    #[test]
    fn ordinary_pages_open_as_given() {
        for url in [
            "https://robinhood.perp.quest",
            "https://robinhood.perp.quest/",
            HANDOFF,
            "https://www.wayup.io/collection/abc?tab=listings#top",
            "https://localhost:8443/dev",
            "https://a.b.c.d.example/x/y/z?a=1&b=2;c=3",
            "https://example.com/path%20with%20escapes",
            "https://example.com/~user/_x-y.z",
            "https://example.com/@user",
            "https://example.com/?next=https://other.example/",
            "https://123.45.67.89/",
            "https://xn--nxasmq6b.example/",
        ] {
            let link = Link::page(url).unwrap_or_else(|e| panic!("{url}: {e}"));
            assert_eq!(link.href(), url);
        }
    }

    #[test]
    fn the_scheme_and_host_are_lower_cased_and_nothing_else() {
        let link = Link::page("HTTPS://Robinhood.PERP.quest/Path?Q=A").unwrap();
        assert_eq!(link.href(), "https://robinhood.perp.quest/Path?Q=A");
    }

    // ── Schemes that never open ─────────────────────────────────────────

    #[test]
    fn only_https_opens_as_a_page() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "javascript://example.com/%0Aalert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "http://example.com/",
            "ftp://example.com/",
            "blob:https://example.com/uuid",
            "about:blank",
            "//example.com/",
            "example.com",
            "/relative/path",
            "https:example.com",
            "https:/example.com",
            "web+cardano://browse/v1?uri=https%3A%2F%2Fexample.com",
            "intent://browse#Intent;scheme=web+cardano;end",
        ] {
            assert_eq!(page_err(url), LinkError::NotHttps, "{url}");
        }
    }

    #[test]
    fn a_scheme_hidden_behind_whitespace_or_a_control_character_is_refused() {
        for url in [
            " https://example.com/",
            "https://example.com/ ",
            "\thttps://example.com/",
            "\nhttps://example.com/",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "\u{0}https://example.com/",
            "https://exa\u{7f}mple.com/",
            "https://example.com/\r\nSet-Cookie:x",
            "https://example.com/a b",
        ] {
            assert!(matches!(page_err(url), LinkError::Whitespace(_)), "{url:?}");
        }
    }

    // ── Authorities that lie ────────────────────────────────────────────

    #[test]
    fn credentials_before_the_host_are_refused() {
        for url in [
            "https://robinhood.perp.quest@evil.example/",
            "https://user:pass@example.com/",
            "https://@example.com/",
            "https://robinhood.perp.quest:443@evil.example/",
        ] {
            assert_eq!(page_err(url), LinkError::Credentials, "{url}");
        }
    }

    #[test]
    fn a_backslash_or_quote_anywhere_is_refused() {
        for url in [
            "https://evil.example\\@good.example/",
            "https://evil.example\\good.example/",
            "https://example.com/\\\\evil.example",
            "https://example.com/\"onmouseover=\"x",
        ] {
            assert!(
                matches!(page_err(url), LinkError::ForbiddenCharacter(_)),
                "{url}"
            );
        }
    }

    #[test]
    fn a_look_alike_host_must_be_punycode() {
        for url in [
            "https://rоbinhood.perp.quest/", // a Cyrillic о
            "https://robinhood.perp.quest/ü",
            "https://example.com\u{2044}evil.example/", // a fraction slash
            "https://example.com\u{ff0f}evil.example/", // a full-width solidus
            "https://example.com/\u{202e}txt.exe",      // a right-to-left override
        ] {
            assert!(matches!(page_err(url), LinkError::NotAscii(_)), "{url}");
        }
    }

    #[test]
    fn a_host_is_labels_of_letters_digits_and_inner_hyphens() {
        for url in [
            "https://",
            "https:///path",
            "https://?q",
            "https://#f",
            "https://.example.com/",
            "https://example.com./",
            "https://exa..mple.com/",
            "https://-example.com/",
            "https://example-.com/",
            "https://exa_mple.com/",
            "https://exa%2Emple.com/",
            "https://[::1]/",
            "https://example.com%2F@evil/",
            "https://exa*mple.com/",
            "https://exa'mple.com/",
            "https://exa<mple.com/",
        ] {
            let err = page_err(url);
            assert!(
                matches!(err, LinkError::BadHost(_) | LinkError::Credentials),
                "{url}: {err:?}"
            );
        }
        let long_label = format!("https://{}.example/", "a".repeat(64));
        assert!(matches!(page_err(&long_label), LinkError::BadHost(_)));
        let long_host = format!("https://{}example/", "abcdefghi.".repeat(26));
        assert!(matches!(page_err(&long_host), LinkError::BadHost(_)));
    }

    #[test]
    fn a_port_is_one_to_65535() {
        for port in ["443", "1", "8080", "65535"] {
            assert!(Link::page(&format!("https://example.com:{port}/")).is_ok());
        }
        for port in [
            "", "0", "65536", "99999", "123456", "80a", "-1", "+80", " 80", "8:0",
        ] {
            let url = format!("https://example.com:{port}/");
            let err = page_err(&url);
            assert!(
                matches!(err, LinkError::BadPort(_) | LinkError::Whitespace(_)),
                "{url}: {err:?}"
            );
        }
    }

    #[test]
    fn empty_and_oversized_addresses_are_refused() {
        assert_eq!(page_err(""), LinkError::Empty);
        let long = format!("https://example.com/{}", "a".repeat(MAX_LEN));
        assert!(matches!(page_err(&long), LinkError::TooLong(_)));
        let just = format!(
            "https://example.com/{}",
            "a".repeat(MAX_LEN - "https://example.com/".len())
        );
        assert!(Link::page(&just).is_ok());
    }

    // ── CIP-158 wallet links ────────────────────────────────────────────

    #[test]
    fn a_wallet_link_is_cip158_with_the_page_percent_encoded() {
        let link = Link::cardano_wallet(HANDOFF).unwrap();
        assert_eq!(
            link.href(),
            "web+cardano://browse/v1?uri=https%3A%2F%2Frobinhood-dev.perp.quest%2F%3Fhandoff%3DK7QX4MN2"
        );
        assert_eq!(link.target().as_str(), HANDOFF);
    }

    #[test]
    fn cip158s_own_examples_round_trip() {
        // The SundaeSwap example from CIP-158, re-encoded the way this crate does.
        let page = "https://app.sundae.fi/exchange?given=ada.lovelace&taken=c48cbb3d5e57ed56e276bc45f99ab39abe94e6cd7ac39fb402da47ad.0014df105553444d&routeIdent=64f35d26b237ad58e099041bc14c687ea7fdc58969d7d5b66e2540ef";
        let link = Link::cardano_wallet(page).unwrap();
        assert_eq!(Link::matching(&link.href()).unwrap(), link);
    }

    #[test]
    fn the_encoded_page_carries_nothing_a_url_parser_would_split_on() {
        let page = "https://example.com/a/b?x=1&y=2#frag;p=@!$'()*+,";
        let href = Link::cardano_wallet(page).unwrap().href();
        let encoded = href.strip_prefix(CIP158_HEAD).unwrap();
        for c in [
            ':', '/', '?', '#', '&', '=', '@', '+', ';', ',', '!', '$', '\'', '(', ')', '*',
        ] {
            assert!(!encoded.contains(c), "{c} left in {encoded}");
        }
    }

    #[test]
    fn a_wallet_link_only_carries_an_https_page() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,x",
            "http://example.com/",
            "example.com",
            "web+cardano://browse/v1?uri=https%3A%2F%2Fexample.com",
        ] {
            assert_eq!(
                Link::cardano_wallet(url).expect_err(url),
                LinkError::NotHttps,
                "{url}"
            );
        }
        assert_eq!(
            Link::cardano_wallet("https://good.example@evil.example/").unwrap_err(),
            LinkError::Credentials
        );
    }

    #[test]
    fn the_wallet_matcher_takes_only_the_browse_v1_head() {
        for href in [
            "web+cardano://browse/v2?uri=https%3A%2F%2Fexample.com",
            "web+cardano://browse?uri=https%3A%2F%2Fexample.com",
            "web+cardano://connect/v1?identifier=abc",
            "web+cardano://transfer/v1/addr1",
            "web+cardano:addr1qx",
            "web+cardano://browse/v1?url=https%3A%2F%2Fexample.com",
            "web+cardano://browse/v1?other=1&uri=https%3A%2F%2Fexample.com",
            "WEB+CARDANO://BROWSE/V1?uri=https%3A%2F%2Fexample.com",
            " web+cardano://browse/v1?uri=https%3A%2F%2Fexample.com",
        ] {
            assert_eq!(
                Link::match_cardano_wallet(href).expect_err(href),
                LinkError::NotCardanoBrowse,
                "{href}"
            );
        }
    }

    #[test]
    fn the_wallet_matcher_refuses_a_uri_that_is_not_strictly_encoded() {
        for encoded in [
            "https://example.com", // raw, not encoded
            "https%3A%2F%2Fexample.com&x=1",
            "https%3A%2F%2Fexample.com#frag",
            "https%3A%2F%2Fexample.com%",
            "https%3A%2F%2Fexample.com%2",
            "https%3A%2F%2Fexample.com%ZZ",
            "https%3A%2F%2Fexample.com%+1",
            "https%3A%2F%2Fexa mple.com",
            "https%3A%2F%2Fexample.com%FF", // not UTF-8
        ] {
            let href = format!("{CIP158_HEAD}{encoded}");
            assert_eq!(
                Link::match_cardano_wallet(&href).expect_err(&href),
                LinkError::BadEncoding,
                "{href}"
            );
        }
        assert_eq!(
            Link::match_cardano_wallet(CIP158_HEAD).unwrap_err(),
            LinkError::Empty
        );
    }

    #[test]
    fn the_wallet_matcher_checks_the_decoded_page_like_any_other() {
        for (page, want) in [
            ("javascript:alert(1)", LinkError::NotHttps),
            ("http://example.com/", LinkError::NotHttps),
            ("https://good.example@evil.example/", LinkError::Credentials),
            ("https://example.com/\n", LinkError::Whitespace('\n')),
            (
                "https://evil.example\\good/",
                LinkError::ForbiddenCharacter('\\'),
            ),
            ("https://rоbinhood.perp.quest/", LinkError::NotAscii('о')),
        ] {
            let href = format!("{CIP158_HEAD}{}", percent_encode(page));
            assert_eq!(
                Link::match_cardano_wallet(&href).unwrap_err(),
                want,
                "{page:?}"
            );
        }
    }

    #[test]
    fn hex_case_in_the_encoding_does_not_matter() {
        let lower = format!("{CIP158_HEAD}https%3a%2f%2fexample.com%2f");
        let upper = format!("{CIP158_HEAD}https%3A%2F%2Fexample.com%2F");
        assert_eq!(
            Link::matching(&lower).unwrap(),
            Link::matching(&upper).unwrap()
        );
    }

    // ── The matchers together ───────────────────────────────────────────

    #[test]
    fn matching_names_the_kind_and_round_trips_every_built_link() {
        for page in [
            "https://robinhood.perp.quest/",
            HANDOFF,
            "https://example.com:8443/a?b=c#d",
        ] {
            for link in [
                Link::page(page).unwrap(),
                Link::cardano_wallet(page).unwrap(),
            ] {
                assert_eq!(Link::matching(&link.href()).unwrap(), link, "{link}");
            }
        }
        assert!(matches!(
            Link::matching("https://example.com/").unwrap(),
            Link::Page(_)
        ));
        assert!(matches!(
            Link::matching("web+cardano://browse/v1?uri=https%3A%2F%2Fexample.com").unwrap(),
            Link::CardanoWallet(_)
        ));
    }

    #[test]
    fn matching_refuses_what_neither_matcher_takes() {
        for href in [
            "",
            "javascript:alert(1)",
            "web+cardano://connect/v1?identifier=x",
            "web+cardano:addr1qx",
            "intent://browse#Intent;end",
            "metamask://dapp/example.com",
        ] {
            assert!(Link::matching(href).is_err(), "{href}");
        }
    }

    #[test]
    fn kinds_are_numbered_as_the_page_script_expects() {
        assert_eq!(Link::page(HANDOFF).unwrap().kind_code(), 1);
        assert_eq!(Link::cardano_wallet(HANDOFF).unwrap().kind_code(), 2);
    }

    #[test]
    fn the_page_script_checks_the_same_shapes() {
        let js = crate::PLUGIN_JS;
        assert!(js.contains("const LINK_PAGE = 1;"));
        assert!(js.contains("const LINK_CARDANO_WALLET = 2;"));
        assert!(js.contains(
            r"const PAGE_SHAPE = /^https:\/\/[a-z0-9.-]+(:[0-9]{1,5})?([/?#][!#-[\]-~]*)?$/;"
        ));
        assert!(js.contains(
            r"const CARDANO_WALLET_SHAPE = /^web\+cardano:\/\/browse\/v1\?uri=[A-Za-z0-9%._~-]+$/;"
        ));
    }
}
