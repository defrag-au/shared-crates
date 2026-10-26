//! A candidate line, parsed.
//!
//! The browser writes these and only the browser can apply them; what travels
//! between peers is text. Parsing it here is what lets a *decision* — is this
//! end of the path a relay, did the far side gather anything, does this
//! candidate belong to the attempt it arrived in — be made once, by a function
//! with tests, instead of by a regex in the shim.
//!
//! The shim did that with `/ typ ([a-z]+)/`, and the failure mode was real: a
//! second function declaration silently replaced the counter's and every
//! gathered candidate was counted as `null`. A Rust function cannot be defined
//! twice, and a test can be written for every shape a candidate takes.
//!
//! The format is RFC 8839 §5.1: `candidate:<foundation> <component>
//! <transport> <priority> <address> <port> typ <type> [<extension> <value>]…`.
//! Unknown `typ` values parse as [`CandidateKind::Unknown`] rather than failing,
//! because a browser may add one and a link is not worth refusing over a label.
//! Unknown *extensions* are skipped: they are key/value pairs and none of them
//! change whether the path works.

use std::fmt;

/// The `typ` of a candidate: what the path to this address is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateKind {
    /// An address on this machine (often an mDNS name, which is why it cannot
    /// cross a NAT).
    Host,
    /// A mapped address a STUN server reported.
    Srflx,
    /// A reflexive address learned from a peer's check.
    Prflx,
    /// A TURN allocation: traffic is relayed, which is the label that explains a
    /// round trip nowhere near the two players' distance.
    Relay,
    /// A `typ` this build does not know. Kept rather than refused.
    Unknown,
}

impl CandidateKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "host" => Self::Host,
            "srflx" => Self::Srflx,
            "prflx" => Self::Prflx,
            "relay" => Self::Relay,
            _ => Self::Unknown,
        }
    }

    /// The word the wire format uses, for a report written back out.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Srflx => "srflx",
            Self::Prflx => "prflx",
            Self::Relay => "relay",
            Self::Unknown => "unknown",
        }
    }
}

/// One candidate line, split into the fields worth deciding on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub foundation: String,
    pub component: u16,
    pub transport: String,
    pub priority: u32,
    /// An IP address, or an mDNS name (`.local`) for a host candidate.
    pub address: String,
    pub port: u16,
    pub kind: CandidateKind,
    /// The ICE username fragment this candidate belongs to. This is the field
    /// that says whether a candidate describes the attempt it arrived in, so an
    /// attempt that restarted cannot add an old transport's addresses.
    pub ufrag: Option<String>,
}

impl Candidate {
    /// Parse a line, with or without the SDP `a=` attribute prefix.
    pub fn parse(line: &str) -> Result<Self, CandidateError> {
        let line = line.trim();
        let line = line.strip_prefix("a=").unwrap_or(line);
        let mut fields = line.split(' ');

        let first = fields.next().ok_or(CandidateError::NotACandidate)?;
        let foundation = first
            .strip_prefix("candidate:")
            .ok_or(CandidateError::NotACandidate)?;
        if foundation.is_empty() {
            return Err(CandidateError::NotACandidate);
        }

        let mut next = || fields.next().ok_or(CandidateError::Truncated);

        let component =
            next().and_then(|value| value.parse().map_err(|_| CandidateError::BadComponent))?;
        let transport = next()?.to_ascii_lowercase();
        let priority =
            next().and_then(|value| value.parse().map_err(|_| CandidateError::BadPriority))?;
        let address = next()?.to_string();
        let port = next().and_then(|value| value.parse().map_err(|_| CandidateError::BadPort))?;

        if next()? != "typ" {
            return Err(CandidateError::NoType);
        }
        let kind = CandidateKind::parse(next()?);

        let mut ufrag = None;
        while let Some(key) = fields.next() {
            // Every extension is a key and a value. A trailing key with no value
            // is malformed, but it cannot change the answer, so it is skipped.
            let Some(value) = fields.next() else {
                break;
            };
            if key == "ufrag" && !value.is_empty() {
                ufrag = Some(value.to_string());
            }
        }

        Ok(Self {
            foundation: foundation.to_string(),
            component,
            transport,
            priority,
            address,
            port,
            kind,
            ufrag,
        })
    }

    pub fn is_relay(&self) -> bool {
        self.kind == CandidateKind::Relay
    }

    /// Whether this candidate belongs to the attempt with `ufrag`. A candidate
    /// that names no ufrag is never refused on this ground: older lines omit it.
    pub fn belongs_to(&self, ufrag: &str) -> bool {
        self.ufrag.as_deref().is_none_or(|theirs| theirs == ufrag)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateError {
    /// Not a candidate line at all: no `candidate:` prefix.
    NotACandidate,
    /// A line whose fields stop early.
    Truncated,
    BadComponent,
    BadPriority,
    BadPort,
    /// The `typ` keyword is missing, so the line is not one this format defines.
    NoType,
}

impl fmt::Display for CandidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotACandidate => f.write_str("not a candidate line"),
            Self::Truncated => f.write_str("candidate line ends early"),
            Self::BadComponent => f.write_str("candidate component is not a number"),
            Self::BadPriority => f.write_str("candidate priority is not a number"),
            Self::BadPort => f.write_str("candidate port is not a number"),
            Self::NoType => f.write_str("candidate line has no `typ`"),
        }
    }
}

impl std::error::Error for CandidateError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real line, gathered by Brave in LHR during the four-region mesh run.
    const HOST: &str = "candidate:42160643 1 udp 2113937151 \
13fa1d0e-a8d1-4edf-92a3-946661dc37a4.local 62798 typ host generation 0 ufrag RE6O network-cost 999";

    /// A real server-reflexive line from the same run.
    const SRFLX: &str = "candidate:286029249 1 udp 1677729535 103.100.225.117 61623 \
typ srflx raddr 0.0.0.0 rport 0 generation 0 ufrag 1m1v network-cost 999";

    /// Relay candidates from the TURN run, whose `typ` is what a report reads to
    /// explain a round trip that is nowhere near the players' distance.
    const RELAY: &str = "candidate:842163049 1 udp 1677729535 104.28.164.135 50032 \
typ relay raddr 10.0.0.1 rport 60753 generation 0 ufrag K3pQ network-cost 10";

    #[test]
    fn a_host_candidate_is_parsed_including_its_ufrag() {
        let candidate = Candidate::parse(HOST).expect("a real host line");
        assert_eq!(candidate.foundation, "42160643");
        assert_eq!(candidate.component, 1);
        assert_eq!(candidate.transport, "udp");
        assert_eq!(candidate.priority, 2113937151);
        assert_eq!(
            candidate.address,
            "13fa1d0e-a8d1-4edf-92a3-946661dc37a4.local"
        );
        assert_eq!(candidate.port, 62798);
        assert_eq!(candidate.kind, CandidateKind::Host);
        assert_eq!(candidate.ufrag.as_deref(), Some("RE6O"));
    }

    #[test]
    fn each_type_is_named_exactly() {
        assert_eq!(Candidate::parse(SRFLX).unwrap().kind, CandidateKind::Srflx);
        let relay = Candidate::parse(RELAY).expect("a real relay line");
        assert_eq!(relay.kind, CandidateKind::Relay);
        assert!(relay.is_relay());
        assert!(!Candidate::parse(SRFLX).unwrap().is_relay());
    }

    /// A `typ` a browser adds later is a label this build does not know, not a
    /// line worth refusing: the path still works.
    #[test]
    fn an_unknown_type_is_kept_rather_than_refused() {
        let line = "candidate:1 1 udp 10 192.0.2.1 5000 typ quic generation 0";
        assert_eq!(
            Candidate::parse(line).expect("an unknown type").kind,
            CandidateKind::Unknown
        );
    }

    #[test]
    fn the_sdp_attribute_prefix_is_accepted() {
        let attribute = format!("a={RELAY}");
        assert_eq!(
            Candidate::parse(&attribute).expect("an a= line"),
            Candidate::parse(RELAY).expect("a bare line")
        );
    }

    #[test]
    fn an_ipv6_address_is_one_field() {
        let line = "candidate:7 1 udp 2122260223 2001:db8::1 50000 typ host generation 0";
        let candidate = Candidate::parse(line).expect("an ipv6 line");
        assert_eq!(candidate.address, "2001:db8::1");
        assert_eq!(candidate.port, 50000);
    }

    /// A candidate belongs to the attempt whose ufrag it names. A candidate that
    /// names none is never refused, because older lines do not carry one.
    #[test]
    fn a_candidate_can_be_matched_to_the_attempt_it_belongs_to() {
        let candidate = Candidate::parse(RELAY).expect("a real relay line");
        assert!(candidate.belongs_to("K3pQ"));
        assert!(!candidate.belongs_to("other"));
    }

    #[test]
    fn a_line_that_is_not_a_candidate_is_refused() {
        assert_eq!(Candidate::parse(""), Err(CandidateError::NotACandidate));
        assert_eq!(
            Candidate::parse("candidate: 1 udp 10 1.2.3.4 5 typ host"),
            Err(CandidateError::NotACandidate)
        );
        assert_eq!(
            Candidate::parse("m=application 9 UDP/DTLS/SCTP"),
            Err(CandidateError::NotACandidate)
        );
    }

    #[test]
    fn a_truncated_line_says_which_field_ran_out() {
        assert_eq!(
            Candidate::parse("candidate:1 1 udp"),
            Err(CandidateError::Truncated)
        );
        assert_eq!(
            Candidate::parse("candidate:1 1 udp 10 1.2.3.4 5"),
            Err(CandidateError::Truncated)
        );
        assert_eq!(
            Candidate::parse("candidate:1 1 udp 10 1.2.3.4 5 host"),
            Err(CandidateError::NoType)
        );
    }

    #[test]
    fn a_number_that_is_not_a_number_is_named() {
        assert_eq!(
            Candidate::parse("candidate:1 one udp 10 1.2.3.4 5 typ host"),
            Err(CandidateError::BadComponent)
        );
        assert_eq!(
            Candidate::parse("candidate:1 1 udp ten 1.2.3.4 5 typ host"),
            Err(CandidateError::BadPriority)
        );
        assert_eq!(
            Candidate::parse("candidate:1 1 udp 10 1.2.3.4 five typ host"),
            Err(CandidateError::BadPort)
        );
    }
}
