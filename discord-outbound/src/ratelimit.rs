//! Proactive, route-keyed rate-limit tracking.
//!
//! Moved from augminted-bots' `discord-api`, which had the only one. The two
//! repos each had half of what is wanted and neither had both:
//!
//! - **Proactive** (here): remember a 429 and *don't send* into a bucket known
//!   to be closed. Saves the round trip and, more importantly, stops a burst
//!   from deepening its own hole.
//! - **Caller-visible** (`DiscordError::RateLimited`): when a 429 does happen,
//!   the caller sees it rather than having it swallowed. That is what lets
//!   cnft.dev-workers' `notification-dispatcher` map one onto a queue retry
//!   delay — swallowing it would convert visible backpressure into silent
//!   latency.
//!
//! They are complementary, and this crate keeps both: the tracker gates
//! *sending*, and a 429 that happens anyway still propagates.
//!
//! # The clock is an argument
//!
//! The original called `worker::Date::now()` inline, which pinned it to
//! Cloudflare Workers and made it untestable. Time comes in as a parameter
//! instead: platforms differ on how to read a clock (`worker::Date` in a
//! Worker, `js_sys::Date` in a browser, `SystemTime` natively, and
//! `SystemTime::now()` *panics* on `wasm32-unknown-unknown`), and the client
//! already has a platform seam to read it at.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tracks which routes are known to be rate limited, and until when.
#[derive(Debug, Clone, Default)]
pub struct RateLimitTracker {
    limits: Arc<Mutex<HashMap<String, u64>>>,
}

impl RateLimitTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a 429 for a route.
    pub fn record(&self, route: &str, now_ms: u64, retry_after_ms: u64) {
        // A poisoned lock means another thread panicked mid-update. The
        // tracker is an optimisation, so recovering and carrying on is better
        // than propagating a panic into a send path.
        let mut limits = self.limits.lock().unwrap_or_else(|e| e.into_inner());
        limits.insert(route.to_string(), now_ms + retry_after_ms);
    }

    /// How long this route is still closed for, if it is.
    ///
    /// Expired entries are dropped as they are found, so the map stays the size
    /// of the *currently* limited routes rather than every route ever used.
    pub fn wait_ms(&self, route: &str, now_ms: u64) -> Option<u64> {
        let mut limits = self.limits.lock().unwrap_or_else(|e| e.into_inner());
        match limits.get(route) {
            Some(&reset_at) if now_ms < reset_at => Some(reset_at - now_ms),
            Some(_) => {
                limits.remove(route);
                None
            }
            None => None,
        }
    }

    /// Forget everything. For tests, and for a caller that knows a limit is
    /// stale (a new token, say).
    pub fn clear(&self) {
        self.limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

/// Which bucket a URL belongs to.
///
/// Coarse on purpose. Discord's real buckets are keyed on the route template
/// plus its *major* parameters, and it returns the bucket id on every response
/// — mirroring that properly means reading `X-RateLimit-Bucket` and keying on
/// it. Until then, grouping by route family is a conservative approximation:
/// it can make one channel's limit pause another, but it cannot let a genuinely
/// limited route through, and only the second failure mode costs a message.
pub fn route_key(url: &str) -> &'static str {
    if url.contains("/webhooks/") {
        "webhooks"
    } else if url.contains("/guilds/") {
        "guilds"
    } else if url.contains("/channels/") {
        "channels"
    } else {
        "other"
    }
}

/// Does this error body carry Cloudflare error code 1015?
///
/// The "you are being rate limited" challenge served by *Discord's own*
/// Cloudflare to a shared Workers egress IP — an edge block, not a Discord
/// per-route 429, and it needs a different answer than backoff.
///
/// Detection only. Discord unblocked Cloudflare egress on 2025-12-05
/// (discord-api-docs#6145) and the fallback relay built for this had processed
/// nothing in the 30 days before it was removed. Kept so that if it ever
/// returns it reads as a named thing rather than a mystery.
pub fn is_cloudflare_block(body: &str) -> bool {
    body.contains("error code: 1015")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_route_is_never_limited() {
        let tracker = RateLimitTracker::new();
        assert_eq!(tracker.wait_ms("channels", 1_000), None);
    }

    #[test]
    fn a_recorded_limit_counts_down_and_then_expires() {
        let tracker = RateLimitTracker::new();
        tracker.record("channels", 1_000, 500);

        assert_eq!(tracker.wait_ms("channels", 1_000), Some(500));
        assert_eq!(tracker.wait_ms("channels", 1_400), Some(100));
        // At the reset instant it is open, not "0ms left" — a caller that saw
        // Some(0) would sleep for nothing.
        assert_eq!(tracker.wait_ms("channels", 1_500), None);
    }

    /// Routes must not share state. A webhook limit pausing channel posts would
    /// stop augie answering interactions because a notifier got throttled.
    #[test]
    fn routes_are_tracked_separately() {
        let tracker = RateLimitTracker::new();
        tracker.record("webhooks", 0, 1_000);

        assert_eq!(tracker.wait_ms("webhooks", 0), Some(1_000));
        assert_eq!(tracker.wait_ms("channels", 0), None);
    }

    /// Reading an expired limit drops it, so a long-lived client does not
    /// accumulate an entry per route it ever touched.
    #[test]
    fn expired_entries_are_dropped_on_read() {
        let tracker = RateLimitTracker::new();
        tracker.record("channels", 0, 100);
        assert_eq!(tracker.wait_ms("channels", 500), None);

        assert_eq!(
            tracker.limits.lock().unwrap().len(),
            0,
            "an expired entry must not be kept"
        );
    }

    #[test]
    fn route_keys_group_by_family() {
        assert_eq!(
            route_key("https://discord.com/api/v10/channels/1/messages"),
            "channels"
        );
        assert_eq!(
            route_key("https://discord.com/api/v10/webhooks/1/tok"),
            "webhooks"
        );
        assert_eq!(
            route_key("https://discord.com/api/v10/guilds/1/members"),
            "guilds"
        );
        assert_eq!(route_key("https://discord.com/api/v10/users/@me"), "other");
    }

    /// An edge block is not a bucket. Reading a 1015 as an ordinary 429 would
    /// record a rate limit against a route that was never limited.
    #[test]
    fn a_cloudflare_block_is_not_an_ordinary_rate_limit() {
        assert!(is_cloudflare_block("<html>error code: 1015</html>"));
        assert!(!is_cloudflare_block(
            r#"{"message":"You are being rate limited.","retry_after":1.5}"#
        ));
    }
}
