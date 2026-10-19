//! Where a player's holdings come from, as requests the edge performs.

use cardano_assets::PolicyId;

/// The public collection-ownership service.
///
/// Its bundle route answers any origin (`Access-Control-Allow-Origin: *`), so
/// a browser game can call it directly. `ownership.hodlcroft.com` is the same
/// worker under another name.
pub const OWNERSHIP_BASE: &str = "https://ownership.cnft.dev";

/// A GET for the edge crate to perform. A description, not a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Get {
    pub url: String,
    /// Sent as `Authorization: Bearer <token>` when present.
    pub bearer: Option<String>,
}

/// Where a player's holdings are read from.
///
/// See the crate docs for why the two are not equally trustworthy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The ownership service, asked about stake addresses the client
    /// supplied. Build with [`Source::stakes`], which validates them.
    Stakes { base: String, stakes: Vec<String> },
    /// The game's own worker, which knows who the bearer token belongs to and
    /// which wallets they verified.
    ///
    /// # The route a worker serves
    ///
    /// `GET {prefix}/{policy_id}` with optional `?cursor=…`, authorised by the
    /// bearer token. It answers with the ownership service's bundle response
    /// for the player's linked stake addresses, traits decoded — in practice
    /// it forwards [`bundle_url`] and returns the body unchanged. A player
    /// with no linked wallet gets an empty `entries`, not an error: "link a
    /// wallet" is a next step.
    ///
    /// `prefix` is relative (`api/holdings`) inside a Discord Activity, whose
    /// content security policy blocks every other origin.
    Worker { prefix: String, bearer: String },
}

/// Why a [`Source`] was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// No stake addresses at all: there is nobody to look up.
    NoStakes,
    /// Not a bech32 stake address. Refused rather than sent, because it is
    /// interpolated into a URL.
    NotAStakeAddress(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStakes => write!(f, "no stake addresses to look up"),
            Self::NotAStakeAddress(s) => write!(f, "not a stake address: {s:?}"),
        }
    }
}

impl std::error::Error for SourceError {}

impl Source {
    /// Holdings of these stake addresses, from the ownership service at
    /// `base` (normally [`OWNERSHIP_BASE`]).
    ///
    /// Duplicates are dropped and order is kept. Each must look like a bech32
    /// stake address: `stake1…` on mainnet or `stake_test1…` on a testnet,
    /// then lowercase letters and digits only.
    pub fn stakes<I, S>(base: &str, stakes: I) -> Result<Self, SourceError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out: Vec<String> = Vec::new();
        for s in stakes {
            let s = s.as_ref().trim();
            if !is_stake_address(s) {
                return Err(SourceError::NotAStakeAddress(s.to_string()));
            }
            if !out.iter().any(|seen| seen == s) {
                out.push(s.to_string());
            }
        }
        if out.is_empty() {
            return Err(SourceError::NoStakes);
        }
        Ok(Self::Stakes {
            base: base.trim_end_matches('/').to_string(),
            stakes: out,
        })
    }

    /// Holdings from the game's worker. See [`Source::Worker`] for the route.
    pub fn worker(prefix: &str, bearer: &str) -> Self {
        Self::Worker {
            prefix: prefix.trim_end_matches('/').to_string(),
            bearer: bearer.to_string(),
        }
    }

    /// The request for one page of a policy's holdings.
    pub fn page(&self, policy_id: &PolicyId, cursor: Option<&str>) -> Get {
        match self {
            Self::Stakes { base, stakes } => Get {
                url: bundle_url(base, policy_id, stakes, cursor),
                bearer: None,
            },
            Self::Worker { prefix, bearer } => {
                let mut url = format!("{prefix}/{policy_id}");
                if let Some(c) = cursor {
                    url.push_str("?cursor=");
                    url.push_str(&encode_component(c));
                }
                Get {
                    url,
                    bearer: Some(bearer.clone()),
                }
            }
        }
    }
}

/// The ownership service's bundle URL for these stakes, traits decoded.
///
/// What [`Source::Stakes`] requests, and what a worker serving
/// [`Source::Worker`] forwards to (with a service-binding host as `base`), so
/// both ask the same question in the same words.
///
/// No `limit`: with a stake filter the service returns every matching row,
/// so one request is the common case. A `next_cursor` in the response still
/// pages, and [`crate::HoldingsLoad`] follows it.
pub fn bundle_url(
    base: &str,
    policy_id: &PolicyId,
    stakes: &[String],
    cursor: Option<&str>,
) -> String {
    let stakes: Vec<String> = stakes.iter().map(|s| encode_component(s)).collect();
    let mut url = format!(
        "{}/api/bundle/{policy_id}?stake={}&traits=decoded",
        base.trim_end_matches('/'),
        stakes.join(",")
    );
    if let Some(c) = cursor {
        url.push_str("&cursor=");
        url.push_str(&encode_component(c));
    }
    url
}

fn is_stake_address(s: &str) -> bool {
    let body = s
        .strip_prefix("stake_test1")
        .or_else(|| s.strip_prefix("stake1"));
    body.is_some_and(|b| {
        !b.is_empty()
            && b.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

/// Percent-encode everything outside RFC 3986's unreserved set.
///
/// A cursor is hex today, but it is the server's opaque string, and one that
/// grew a `&` would otherwise rewrite the query.
pub(crate) fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOLHEADS: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";
    // Real stake addresses of Toolheads holders, from the live bundle in
    // `tests/fixtures/toolheads-rarest.json`.
    const HOLDER_A: &str = "stake1u90usm20k3svksccyrrry4gxxq2arsnnkguaee4z5tn3s9cgmhth6";
    const HOLDER_B: &str = "stake1u9a975x4edaya3k850q8v0u56dksgtkl9tqrfauxh4t7t8qyfcfzd";

    fn policy() -> PolicyId {
        PolicyId::new(TOOLHEADS).unwrap()
    }

    #[test]
    fn a_stakes_source_asks_the_ownership_service_for_decoded_traits() {
        let source = Source::stakes(OWNERSHIP_BASE, [HOLDER_A, HOLDER_B]).unwrap();
        let get = source.page(&policy(), None);
        assert_eq!(
            get.url,
            format!(
                "https://ownership.cnft.dev/api/bundle/{TOOLHEADS}?stake={HOLDER_A},{HOLDER_B}&traits=decoded"
            )
        );
        assert_eq!(get.bearer, None);

        let next = source.page(&policy(), Some("546f6f6c"));
        assert!(next.url.ends_with("&traits=decoded&cursor=546f6f6c"));
    }

    #[test]
    fn stakes_are_validated_deduplicated_and_trimmed() {
        let Source::Stakes { stakes, .. } = Source::stakes(
            "https://x.test/",
            [
                HOLDER_A,
                " stake1u90usm20k3svksccyrrry4gxxq2arsnnkguaee4z5tn3s9cgmhth6 ",
            ],
        )
        .unwrap() else {
            panic!("expected a stakes source");
        };
        assert_eq!(stakes, vec![HOLDER_A.to_string()]);

        assert_eq!(
            Source::stakes(OWNERSHIP_BASE, Vec::<String>::new()),
            Err(SourceError::NoStakes)
        );
        // A payment address is not a stake address, and an injection attempt
        // never reaches the URL.
        for bad in [
            "addr1qx",
            "stake1",
            "stake1abc&limit=1",
            "STAKE1ABC",
            "stake1abc,stake1def",
        ] {
            assert!(
                matches!(
                    Source::stakes(OWNERSHIP_BASE, [bad]),
                    Err(SourceError::NotAStakeAddress(_))
                ),
                "{bad}"
            );
        }
        // Testnet addresses are accepted.
        assert!(Source::stakes(OWNERSHIP_BASE, ["stake_test1uq0abc"]).is_ok());
    }

    #[test]
    fn a_worker_source_is_relative_and_carries_the_bearer() {
        let source = Source::worker("api/holdings/", "tok");
        let get = source.page(&policy(), None);
        assert_eq!(get.url, format!("api/holdings/{TOOLHEADS}"));
        assert_eq!(get.bearer.as_deref(), Some("tok"));

        let next = source.page(&policy(), Some("a&b=c"));
        assert_eq!(
            next.url,
            format!("api/holdings/{TOOLHEADS}?cursor=a%26b%3Dc")
        );
    }
}
