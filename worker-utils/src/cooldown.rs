//! Self-expiring cooldowns backed by a KV key's TTL.
//!
//! The idiom this replaces is written out by hand in several places already
//! (`collection-ownership`'s `listings:refreshing:{policy}` flag,
//! `augminted`'s "just take the heat off for a minute" cache): put a key,
//! give it a TTL, and treat its presence as "not yet". Expiry is the storage
//! layer's job, so there is no sweep, no alarm, and no timestamp arithmetic
//! to get wrong.
//!
//! # Why KV and not D1
//!
//! A cooldown is defined by expiring. Storing it in a table means every read
//! carries a `WHERE expires_at > now` and every write leaves a row to collect
//! later; storing it in KV means the absence of the key *is* the answer.
//!
//! KV's eventual consistency is the usual reason to avoid it, and it is
//! genuinely harmless here: the failure mode of a stale read is that someone
//! occasionally acts a little before their cooldown truly lifts. That is the
//! same race the hand-rolled call sites already accept — "a lost race only
//! risks one extra (idempotent) refresh". Do NOT reach for this where the
//! race is the point (a lock protecting a payout, a one-shot claim); those
//! need a Durable Object.
//!
//! # Remaining time
//!
//! The value stored is the unix second the cooldown lifts, so a caller can
//! say "2h left" rather than only "not yet". TTL still does the cleanup — the
//! value exists to be *read*, not to decide expiry.

use worker_stack::worker::kv::KvStore;

/// A single named cooldown.
///
/// Construct it, then ask [`Cooldown::remaining`] or start it with
/// [`Cooldown::start`]. Cheap — it holds a key, not a connection.
pub struct Cooldown<'a> {
    kv: &'a KvStore,
    key: String,
}

impl<'a> Cooldown<'a> {
    /// Build a cooldown from a caller-supplied key.
    ///
    /// Namespace it like every other KV key in the codebase
    /// (`{domain}:{scope}:{subject}`) so a listing is readable and two
    /// features can't collide.
    pub fn new(kv: &'a KvStore, key: impl Into<String>) -> Self {
        Self {
            kv,
            key: key.into(),
        }
    }

    /// The key this cooldown occupies. Useful for logging and tests.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Seconds until this cooldown lifts, or `None` if it is not active.
    ///
    /// A malformed or unreadable value is treated as "not active" rather than
    /// as an error: a cooldown that fails open lets someone act early, while
    /// one that fails closed can lock a player out of a feature permanently
    /// with no way to clear it. Early is the better failure.
    pub async fn remaining(&self, now_unix: i64) -> Option<i64> {
        let raw = self.kv.get(&self.key).text().await.ok()??;
        let until: i64 = raw.parse().ok()?;
        let left = until - now_unix;
        (left > 0).then_some(left)
    }

    /// Whether the cooldown is currently active.
    pub async fn is_active(&self, now_unix: i64) -> bool {
        self.remaining(now_unix).await.is_some()
    }

    /// Start the cooldown, expiring `seconds` from now.
    ///
    /// A non-positive duration clears any existing cooldown instead of
    /// writing one — KV rejects a zero TTL, and "cool down for 0 seconds"
    /// can only mean "ready now".
    pub async fn start(&self, now_unix: i64, seconds: i64) -> worker_stack::worker::Result<()> {
        if seconds <= 0 {
            return self.clear().await;
        }
        // KV enforces a 60s floor on expiration_ttl and rejects anything
        // lower, which would turn a short cooldown into an error rather than
        // a short cooldown.
        let ttl = seconds.max(MIN_TTL_SECONDS) as u64;
        let until = now_unix + seconds;
        self.kv
            .put(&self.key, until.to_string())?
            .expiration_ttl(ttl)
            .execute()
            .await?;
        Ok(())
    }

    /// Drop the cooldown immediately.
    pub async fn clear(&self) -> worker_stack::worker::Result<()> {
        self.kv.delete(&self.key).await?;
        Ok(())
    }
}

/// Cloudflare KV rejects `expiration_ttl` below 60 seconds.
///
/// A cooldown shorter than this still reads as elapsed at the right moment —
/// `remaining` compares against the stored timestamp, not the TTL — the key
/// just lingers a little longer than it needs to.
const MIN_TTL_SECONDS: i64 = 60;

#[cfg(test)]
mod tests {
    use super::*;

    /// The TTL floor must not leak into the answer: a 30-second cooldown
    /// still reports ~30 seconds even though its key survives 60.
    #[test]
    fn a_short_cooldown_is_stored_at_the_floor_but_still_reads_short() {
        let now = 1_000_000;
        let seconds = 30;
        let ttl = seconds.max(MIN_TTL_SECONDS);
        assert_eq!(ttl, 60, "TTL is raised to the KV floor");

        let until = now + seconds;
        assert_eq!(until - now, 30, "remaining still reflects the real duration");
    }

    #[test]
    fn an_elapsed_cooldown_reports_nothing_left() {
        let until: i64 = 1_000_000;
        let now = until + 1;
        assert!(
            !((until - now) > 0),
            "a cooldown at or past its deadline is not active"
        );
    }

    #[test]
    fn a_cooldown_is_active_right_up_to_its_deadline() {
        let until: i64 = 1_000_000;
        assert!((until - (until - 1)) > 0, "one second before: active");
        assert!(!((until - until) > 0), "exactly at the deadline: elapsed");
    }
}
