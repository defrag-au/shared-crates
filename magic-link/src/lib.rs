//! `magic-link` — sign a waiting device in by approving it from somewhere else.
//!
//! Two flows, one shape. A device that wants a session (a desktop app, a TV)
//! starts a [`Grant`] and waits on it; the person approves it out of band;
//! the device then claims what was approved, once.
//!
//! - **Email link** ([`start_email_link`]): the device asks for a link to be
//!   emailed; opening it — on *any* device — approves the waiting one. No
//!   loopback server, no redirect back to the app.
//! - **Pairing code** ([`start_pairing`]): the device shows a short code; a
//!   person who is already signed in types it elsewhere to approve it. The
//!   shape a TV needs, where there is no keyboard to type an email.
//!
//! Pure: no I/O, no clock, no randomness of its own. The caller supplies the
//! time and [`Entropy`] from a CSPRNG, stores [`Grant`]s (it is `Serialize`),
//! and turns an approved subject into a session however it mints sessions.
//! Secrets are held only as SHA-256 hashes, so a stored grant hands none out.
//!
//! Formalised from cnft.dev-workers' `auth-provider` magic links, with two of
//! its faults designed out:
//! - **Single use is a state transition, not a check followed by a write.**
//!   Every operation takes `&mut Grant` and moves it along
//!   `Pending → Approved → Claimed`; run where one grant is handled at a time
//!   (a Durable Object), a second use sees the moved state. The old
//!   check-then-`UPDATE` let two concurrent verifies both succeed.
//! - **No attempt counter that cannot count.** A wrong secret finds no grant
//!   (the caller looks grants up by id or by [`code_key`]), so a per-grant
//!   counter never sees the guesses. Pairing codes are short enough to guess:
//!   rate-limit *the approver* where codes are looked up.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How long an emailed link stays good.
pub const LINK_TTL_SECS: u64 = 15 * 60;
/// How long a pairing code stays good.
pub const CODE_TTL_SECS: u64 = 10 * 60;
/// Characters in a pairing code.
pub const CODE_LEN: usize = 6;
/// No 0/O or 1/I: a code is read off a TV and typed on a phone. Exactly 32
/// symbols, so a random byte masked to 5 bits picks one uniformly.
const CODE_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Random bytes from the caller's CSPRNG, one draw per grant: 16 for the
/// grant id, 32 for the claim secret, 32 for the approval secret.
pub type Entropy = [u8; 80];

/// How a grant gets approved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Approval {
    /// By opening the link emailed to this address, which becomes the
    /// approved subject.
    EmailLink { email: String },
    /// By a signed-in person entering the code; they name the subject.
    PairingCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GrantState {
    Pending,
    Approved {
        subject: String,
    },
    /// The device has collected the approval; nothing more can happen.
    Claimed,
}

/// One sign-in in progress. Store it keyed by [`Grant::id`] (email links) or
/// by [`code_key`] (pairing codes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// Public: it travels in the emailed link and in the device's polls.
    pub id: String,
    pub approval: Approval,
    pub created_at: u64,
    pub expires_at: u64,
    pub state: GrantState,
    /// SHA-256 of the secret only the waiting device holds.
    claim_hash: String,
    /// SHA-256 of the link token or the pairing code.
    approve_hash: String,
}

/// A freshly started grant, and the secrets that exist only here: hand
/// `claim_secret` to the waiting device, and `approve_secret` out of band (in
/// the email, or on the device's screen). Store only `grant`.
#[derive(Debug)]
pub struct Issued {
    pub grant: Grant,
    pub claim_secret: String,
    /// The link token, or the pairing code as the device should show it
    /// ([`format_code`]).
    pub approve_secret: String,
}

/// What a waiting device learns when it polls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Not approved yet; poll again.
    Waiting,
    /// Approved for this subject. Returned once — the grant is now spent.
    Approved { subject: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("this sign-in has expired; start again")]
    Expired,
    #[error("this sign-in has already been used")]
    AlreadyUsed,
    #[error("that link or code is not valid")]
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not an email address: {0:?}")]
pub struct InvalidEmail(pub String);

/// Start an email-link sign-in for `email`.
pub fn start_email_link(email: &str, now: u64, entropy: &Entropy) -> Result<Issued, InvalidEmail> {
    let email = normalize_email(email)?;
    let (id, claim_secret, token) = split(entropy);
    Ok(Issued {
        grant: Grant {
            id,
            approval: Approval::EmailLink { email },
            created_at: now,
            expires_at: now + LINK_TTL_SECS,
            state: GrantState::Pending,
            claim_hash: sha256_hex(claim_secret.as_bytes()),
            approve_hash: sha256_hex(token.as_bytes()),
        },
        claim_secret,
        approve_secret: token,
    })
}

/// Start a pairing-code sign-in. The code is `approve_secret`, formatted for
/// display (`K7F-29Q`).
pub fn start_pairing(now: u64, entropy: &Entropy) -> Issued {
    let (id, claim_secret, _) = split(entropy);
    let code: String = entropy[48..48 + CODE_LEN]
        .iter()
        .map(|b| CODE_ALPHABET[(b & 0x1f) as usize] as char)
        .collect();
    Issued {
        grant: Grant {
            id,
            approval: Approval::PairingCode,
            created_at: now,
            expires_at: now + CODE_TTL_SECS,
            state: GrantState::Pending,
            claim_hash: sha256_hex(claim_secret.as_bytes()),
            approve_hash: sha256_hex(code.as_bytes()),
        },
        claim_secret,
        approve_secret: format_code(&code),
    }
}

impl Grant {
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// Approve an email-link grant with the token from its link. The
    /// approved subject is the email the link was sent to.
    pub fn approve_link(&mut self, token: &str, now: u64) -> Result<&str, Refused> {
        let Approval::EmailLink { email } = &self.approval else {
            return Err(Refused::Invalid);
        };
        self.check_pending(now)?;
        if !secret_matches(token, &self.approve_hash) {
            return Err(Refused::Invalid);
        }
        self.state = GrantState::Approved {
            subject: email.clone(),
        };
        Ok(email)
    }

    /// Approve a pairing-code grant for `subject` — the signed-in approver's
    /// identity, which the device will be signed in as. `code` as typed.
    pub fn approve_code(&mut self, code: &str, subject: &str, now: u64) -> Result<(), Refused> {
        if self.approval != Approval::PairingCode {
            return Err(Refused::Invalid);
        }
        self.check_pending(now)?;
        let Some(code) = normalize_code(code) else {
            return Err(Refused::Invalid);
        };
        if !secret_matches(&code, &self.approve_hash) {
            return Err(Refused::Invalid);
        }
        self.state = GrantState::Approved {
            subject: subject.to_owned(),
        };
        Ok(())
    }

    /// The waiting device polls with its claim secret. An approval is handed
    /// over exactly once. An approval is collectable after the grant's expiry
    /// — the person did approve in time — so a device that polls slowly is
    /// not locked out; the caller prunes old grants.
    pub fn claim(&mut self, claim_secret: &str, now: u64) -> Result<Claim, Refused> {
        if !secret_matches(claim_secret, &self.claim_hash) {
            return Err(Refused::Invalid);
        }
        match &self.state {
            GrantState::Pending if self.is_expired(now) => Err(Refused::Expired),
            GrantState::Pending => Ok(Claim::Waiting),
            GrantState::Approved { subject } => {
                let subject = subject.clone();
                self.state = GrantState::Claimed;
                Ok(Claim::Approved { subject })
            }
            GrantState::Claimed => Err(Refused::AlreadyUsed),
        }
    }

    fn check_pending(&self, now: u64) -> Result<(), Refused> {
        match self.state {
            GrantState::Pending if self.is_expired(now) => Err(Refused::Expired),
            GrantState::Pending => Ok(()),
            _ => Err(Refused::AlreadyUsed),
        }
    }
}

/// Where to store (and find) a pairing grant: derived from the code, so the
/// approver's typed code finds it. `None` if what was typed cannot be a code.
/// Accepts the code as shown or as typed (`k7f 29q`, `K7F-29Q`).
pub fn code_key(code: &str) -> Option<String> {
    normalize_code(code).map(|c| format!("code:{}", sha256_hex(c.as_bytes())))
}

/// A code as a device should show it: `K7F29Q` → `K7F-29Q`.
pub fn format_code(code: &str) -> String {
    let (a, b) = code.split_at(code.len() / 2);
    format!("{a}-{b}")
}

/// Emails compare case-insensitively and without surrounding space, so a
/// person gets the same account however they type their address.
pub fn normalize_email(email: &str) -> Result<String, InvalidEmail> {
    let e = email.trim().to_lowercase();
    let valid = match e.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
                && !e.chars().any(char::is_whitespace)
        }
        None => false,
    };
    if valid {
        Ok(e)
    } else {
        Err(InvalidEmail(email.to_owned()))
    }
}

fn normalize_code(code: &str) -> Option<String> {
    let c: String = code
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let valid = c.len() == CODE_LEN && c.bytes().all(|b| CODE_ALPHABET.contains(&b));
    valid.then_some(c)
}

fn split(entropy: &Entropy) -> (String, String, String) {
    (
        hex(&entropy[..16]),
        hex(&entropy[16..48]),
        hex(&entropy[48..80]),
    )
}

fn secret_matches(presented: &str, expected_hash: &str) -> bool {
    constant_time_eq(
        sha256_hex(presented.as_bytes()).as_bytes(),
        expected_hash.as_bytes(),
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compare without stopping at the first difference. Lengths are compared
/// whole — folding them into a byte lets lengths 256 apart compare equal.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = u8::from(a.len() != b.len());
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn entropy(seed: u8) -> Entropy {
        std::array::from_fn(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
    }

    #[test]
    fn an_emailed_link_approves_the_waiting_device_once() {
        let issued = start_email_link("  Jen@Example.COM ", NOW, &entropy(1)).unwrap();
        let mut grant = issued.grant;
        assert_eq!(
            grant.claim(&issued.claim_secret, NOW + 1),
            Ok(Claim::Waiting)
        );
        assert_eq!(
            grant.approve_link(&issued.approve_secret, NOW + 60),
            Ok("jen@example.com")
        );
        assert_eq!(
            grant.claim(&issued.claim_secret, NOW + 61),
            Ok(Claim::Approved {
                subject: "jen@example.com".into()
            })
        );
        // Spent: neither the link nor the claim works again.
        assert_eq!(
            grant.claim(&issued.claim_secret, NOW + 62),
            Err(Refused::AlreadyUsed)
        );
        assert_eq!(
            grant.approve_link(&issued.approve_secret, NOW + 63),
            Err(Refused::AlreadyUsed)
        );
    }

    #[test]
    fn a_link_opened_twice_approves_only_once() {
        let issued = start_email_link("jen@example.com", NOW, &entropy(2)).unwrap();
        let mut grant = issued.grant;
        assert!(grant.approve_link(&issued.approve_secret, NOW).is_ok());
        assert_eq!(
            grant.approve_link(&issued.approve_secret, NOW),
            Err(Refused::AlreadyUsed)
        );
    }

    #[test]
    fn wrong_secrets_are_refused_and_change_nothing() {
        let issued = start_email_link("jen@example.com", NOW, &entropy(3)).unwrap();
        let mut grant = issued.grant.clone();
        assert_eq!(
            grant.approve_link("not-the-token", NOW),
            Err(Refused::Invalid)
        );
        assert_eq!(
            grant.claim("not-the-claim-secret", NOW),
            Err(Refused::Invalid)
        );
        assert_eq!(grant, issued.grant);
    }

    #[test]
    fn an_expired_link_cannot_approve_but_an_approval_can_still_be_collected() {
        let issued = start_email_link("jen@example.com", NOW, &entropy(4)).unwrap();
        let late = NOW + LINK_TTL_SECS;

        let mut unapproved = issued.grant.clone();
        assert_eq!(
            unapproved.approve_link(&issued.approve_secret, late),
            Err(Refused::Expired)
        );
        assert_eq!(
            unapproved.claim(&issued.claim_secret, late),
            Err(Refused::Expired)
        );

        let mut approved = issued.grant;
        approved
            .approve_link(&issued.approve_secret, NOW + 10)
            .unwrap();
        assert!(matches!(
            approved.claim(&issued.claim_secret, late),
            Ok(Claim::Approved { .. })
        ));
    }

    #[test]
    fn a_pairing_code_signs_the_device_in_as_the_approver() {
        let issued = start_pairing(NOW, &entropy(5));
        let shown = issued.approve_secret.clone();
        assert_eq!(shown.len(), CODE_LEN + 1);
        assert_eq!(&shown[3..4], "-");

        // Typed on a phone: lower case, a space instead of the dash.
        let typed = shown.replace('-', " ").to_lowercase();
        assert_eq!(code_key(&typed), code_key(&shown));

        let mut grant = issued.grant;
        grant
            .approve_code(&typed, "jen@example.com", NOW + 30)
            .unwrap();
        assert_eq!(
            grant.claim(&issued.claim_secret, NOW + 31),
            Ok(Claim::Approved {
                subject: "jen@example.com".into()
            })
        );
    }

    #[test]
    fn codes_avoid_ambiguous_characters_and_bad_input_finds_nothing() {
        for seed in 0..=255 {
            let code = start_pairing(NOW, &entropy(seed)).approve_secret;
            assert!(
                !code.contains(['0', 'O', '1', 'I']),
                "ambiguous character in {code}"
            );
        }
        for bad in ["", "ABC", "ABCDEFG", "ABC-D0F", "ABC-DOF", "ABC-D1F"] {
            assert_eq!(code_key(bad), None, "{bad:?} should not be a code");
        }
    }

    #[test]
    fn a_grant_cannot_be_approved_the_wrong_way() {
        let link = start_email_link("jen@example.com", NOW, &entropy(6));
        let mut link = link.unwrap().grant;
        assert_eq!(
            link.approve_code("ABC-DEF", "someone", NOW),
            Err(Refused::Invalid)
        );

        let pairing = start_pairing(NOW, &entropy(7));
        let mut grant = pairing.grant;
        assert_eq!(
            grant.approve_link(&pairing.approve_secret, NOW),
            Err(Refused::Invalid)
        );
    }

    #[test]
    fn a_stored_grant_holds_no_secret() {
        let issued = start_email_link("jen@example.com", NOW, &entropy(8)).unwrap();
        let stored = serde_json::to_string(&issued.grant).unwrap();
        assert!(!stored.contains(&issued.claim_secret));
        assert!(!stored.contains(&issued.approve_secret));
        let back: Grant = serde_json::from_str(&stored).unwrap();
        assert_eq!(back, issued.grant);
    }

    #[test]
    fn emails_are_normalised_and_checked() {
        assert_eq!(normalize_email(" A@B.co ").unwrap(), "a@b.co");
        for bad in [
            "", "no-at", "@b.co", "a@", "a@b", "a@.co", "a@b.", "a b@c.co", "a@b@c.co",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be refused");
        }
    }
}
