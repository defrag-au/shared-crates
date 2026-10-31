//! What the reader is told when something goes wrong, written once so every
//! client says the same thing. The statuses are auth-service's own (see
//! `LinkError::status` in its `social_link.rs`), and the sentences follow the
//! HTML link page's, which people already meet.

use crate::Reply;

/// A reply that arrived but could not be read.
pub const UNREADABLE: &str = "The link service sent something this page couldn't read. Try again.";
/// A Cardano wallet whose reward address is not bech32.
pub const UNREADABLE_ADDRESS: &str =
    "This wallet's stake address couldn't be read, so it can't be linked here.";
/// A signature for another chain than the one being linked.
pub const WRONG_SIGNATURE: &str = "The wallet answered with the wrong kind of signature.";
/// No reply at all.
pub const OFFLINE: &str = "Couldn't reach the link service. Check your connection and try again.";
/// A token auth-service refused, as the `/link-wallet` funnel words it.
pub const EXPIRED: &str = "Your session expired. Run /link-wallet in Discord again.";

/// A failed reply to a link step, in the reader's words.
pub fn reply_failed(reply: &Reply) -> String {
    match reply {
        Reply::Ok(_) => String::new(),
        Reply::Offline(_) => OFFLINE.into(),
        Reply::Status { status, body } => match status {
            401 => EXPIRED.into(),
            409 => "That wallet is already linked to another Discord account.".into(),
            410 => "The request to sign expired. Try again.".into(),
            400 => {
                let why = body.trim();
                if why.is_empty() {
                    "The signature was rejected.".into()
                } else {
                    format!("The signature was rejected: {why}")
                }
            }
            s if *s >= 500 => "The link service had a problem. Try again in a moment.".into(),
            s => format!("The link service answered {s}. Try again in a moment."),
        },
    }
}

/// An unlink that did not happen.
pub fn unlink_failed(reply: &Reply) -> String {
    match reply {
        Reply::Status { status: 400, body } if !body.trim().is_empty() => {
            format!("Couldn't unlink: {}", body.trim())
        }
        reply => format!("Couldn't unlink. {}", reply_failed(reply)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16, body: &str) -> Reply {
        Reply::Status {
            status,
            body: body.into(),
        }
    }

    #[test]
    fn each_status_says_what_will_help() {
        assert!(reply_failed(&status(409, "")).contains("another Discord account"));
        assert!(reply_failed(&status(410, "")).contains("Try again"));
        assert_eq!(
            reply_failed(&status(400, "key does not own address")),
            "The signature was rejected: key does not own address"
        );
        assert_eq!(reply_failed(&status(401, "")), EXPIRED);
        assert_eq!(reply_failed(&Reply::Offline("dns".into())), OFFLINE);
        assert!(reply_failed(&status(503, "")).contains("had a problem"));
    }
}
