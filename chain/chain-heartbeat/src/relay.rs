//! Subscribing to a heartbeat host over HTTP.
//!
//! The host keeps a [`SubscriberRegistry`] and POSTs every
//! [`crate::HeartbeatFrame`] as JSON to each subscriber's `deliver_to` URL,
//! with `Authorization: Bearer <token>` using the token the subscriber chose
//! when it registered. Subscriptions are leases. A subscriber renews while it
//! has listeners and deregisters when the last one leaves, so the host only
//! wakes objects someone is looking at. [`SubscriptionLease`] is that
//! subscriber side as a pure state machine.
//!
//! Browsers attached to a subscriber receive the frames as ui-flow
//! notifications on [`CHAIN_DOMAIN`].

use serde::{Deserialize, Serialize};

/// The ui-flow notify domain chain frames travel on.
pub const CHAIN_DOMAIN: &str = "chain";

/// Subscribers one host will deliver to.
pub const MAX_SUBSCRIBERS: usize = 64;

/// Lease bounds. Short enough that an abandoned subscriber stops being woken
/// within minutes; long enough that renewals are rare.
pub const MIN_LEASE_SECS: u64 = 60;
pub const MAX_LEASE_SECS: u64 = 900;

/// Consecutive failed deliveries before a subscriber is dropped.
pub const DROP_AFTER_FAILURES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubscriberId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub subscriber: SubscriberId,
    /// Where frames are POSTed. `https://`, or `http://localhost` for dev.
    pub deliver_to: String,
    /// Echoed back as the bearer on every delivery.
    pub token: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub lease_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribeRequest {
    pub subscriber: SubscriberId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SubscribeResponse {
    Added {
        #[serde(with = "wasm_safe_serde::u64_required")]
        expires_at_ms: u64,
    },
    Renewed {
        #[serde(with = "wasm_safe_serde::u64_required")]
        expires_at_ms: u64,
    },
    Rejected {
        reason: Rejection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    Full,
    InsecureUrl,
    EmptyToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Removal {
    Removed,
    Unknown,
}

/// One registered subscriber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscriber {
    pub id: SubscriberId,
    pub deliver_to: String,
    pub token: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub expires_at_ms: u64,
    #[serde(default)]
    pub consecutive_failures: u32,
}

/// How one delivery went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Any 2xx.
    Delivered,
    /// A network error or any other status. Counted towards
    /// [`DROP_AFTER_FAILURES`].
    Failed,
    /// 404 or 410: the subscriber no longer exists. Dropped at once.
    Gone,
}

/// What the registry did with a subscriber after a delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterDelivery {
    Kept,
    Dropped,
}

/// Host side: who to deliver to. Persist it whole (it is small and bounded).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriberRegistry {
    subscribers: Vec<Subscriber>,
}

impl SubscriberRegistry {
    pub fn register(&mut self, request: SubscribeRequest, now_ms: u64) -> SubscribeResponse {
        if !deliverable_url(&request.deliver_to) {
            return SubscribeResponse::Rejected {
                reason: Rejection::InsecureUrl,
            };
        }
        if request.token.is_empty() {
            return SubscribeResponse::Rejected {
                reason: Rejection::EmptyToken,
            };
        }
        self.expire(now_ms);
        let expires_at_ms =
            now_ms + request.lease_secs.clamp(MIN_LEASE_SECS, MAX_LEASE_SECS) * 1000;

        if let Some(existing) = self
            .subscribers
            .iter_mut()
            .find(|s| s.id == request.subscriber)
        {
            existing.deliver_to = request.deliver_to;
            existing.token = request.token;
            existing.expires_at_ms = expires_at_ms;
            existing.consecutive_failures = 0;
            return SubscribeResponse::Renewed { expires_at_ms };
        }
        if self.subscribers.len() >= MAX_SUBSCRIBERS {
            return SubscribeResponse::Rejected {
                reason: Rejection::Full,
            };
        }
        self.subscribers.push(Subscriber {
            id: request.subscriber,
            deliver_to: request.deliver_to,
            token: request.token,
            expires_at_ms,
            consecutive_failures: 0,
        });
        SubscribeResponse::Added { expires_at_ms }
    }

    pub fn remove(&mut self, id: &SubscriberId) -> Removal {
        let before = self.subscribers.len();
        self.subscribers.retain(|s| &s.id != id);
        if self.subscribers.len() < before {
            Removal::Removed
        } else {
            Removal::Unknown
        }
    }

    /// Drop expired leases. Returns how many went.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.subscribers.len();
        self.subscribers.retain(|s| s.expires_at_ms > now_ms);
        before - self.subscribers.len()
    }

    /// Subscribers with a live lease.
    pub fn targets(&self, now_ms: u64) -> impl Iterator<Item = &Subscriber> {
        self.subscribers
            .iter()
            .filter(move |s| s.expires_at_ms > now_ms)
    }

    pub fn record(&mut self, id: &SubscriberId, outcome: DeliveryOutcome) -> AfterDelivery {
        let Some(index) = self.subscribers.iter().position(|s| &s.id == id) else {
            return AfterDelivery::Dropped;
        };
        let drop = match outcome {
            DeliveryOutcome::Delivered => {
                self.subscribers[index].consecutive_failures = 0;
                false
            }
            DeliveryOutcome::Failed => {
                let subscriber = &mut self.subscribers[index];
                subscriber.consecutive_failures += 1;
                subscriber.consecutive_failures >= DROP_AFTER_FAILURES
            }
            DeliveryOutcome::Gone => true,
        };
        if drop {
            self.subscribers.remove(index);
            AfterDelivery::Dropped
        } else {
            AfterDelivery::Kept
        }
    }

    pub fn len(&self) -> usize {
        self.subscribers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }
}

fn deliverable_url(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1")
}

/// What a subscriber should do about its lease right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseAction {
    /// Listeners and no lease: subscribe.
    Register,
    /// The lease is inside its last third: subscribe again.
    Renew,
    /// A lease and no listeners: unsubscribe, so the host stops waking us.
    Deregister,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaseState {
    Inactive,
    Active { expires_at_ms: u64 },
}

/// Subscriber side: keeps a lease only while something is listening.
///
/// A failed renewal changes nothing: the lease stays active until it expires,
/// the host keeps delivering until then, and the next check tries again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionLease {
    lease_secs: u64,
    state: LeaseState,
}

impl SubscriptionLease {
    pub fn new(lease_secs: u64) -> Self {
        Self {
            lease_secs: lease_secs.clamp(MIN_LEASE_SECS, MAX_LEASE_SECS),
            state: LeaseState::Inactive,
        }
    }

    pub fn next(&self, listeners: usize, now_ms: u64) -> LeaseAction {
        match (self.state, listeners) {
            (LeaseState::Inactive, 0) => LeaseAction::Hold,
            (LeaseState::Inactive, _) => LeaseAction::Register,
            (LeaseState::Active { .. }, 0) => LeaseAction::Deregister,
            (LeaseState::Active { expires_at_ms }, _)
                if now_ms + self.renew_margin_ms() >= expires_at_ms =>
            {
                LeaseAction::Renew
            }
            (LeaseState::Active { .. }, _) => LeaseAction::Hold,
        }
    }

    pub fn request(
        &self,
        subscriber: SubscriberId,
        deliver_to: String,
        token: String,
    ) -> SubscribeRequest {
        SubscribeRequest {
            subscriber,
            deliver_to,
            token,
            lease_secs: self.lease_secs,
        }
    }

    /// Record the host's answer to a register or renew.
    pub fn answered(&mut self, response: SubscribeResponse) {
        self.state = match response {
            SubscribeResponse::Added { expires_at_ms }
            | SubscribeResponse::Renewed { expires_at_ms } => LeaseState::Active { expires_at_ms },
            SubscribeResponse::Rejected { .. } => LeaseState::Inactive,
        };
    }

    /// Record a deregistration (or give up on the lease).
    pub fn released(&mut self) {
        self.state = LeaseState::Inactive;
    }

    /// How long until [`Self::next`] can change without a listener change:
    /// the renewal point of an active lease. `None` when inactive.
    pub fn check_again_in_ms(&self, now_ms: u64) -> Option<u64> {
        match self.state {
            LeaseState::Inactive => None,
            LeaseState::Active { expires_at_ms } => Some(
                expires_at_ms
                    .saturating_sub(self.renew_margin_ms())
                    .saturating_sub(now_ms),
            ),
        }
    }

    fn renew_margin_ms(&self) -> u64 {
        self.lease_secs * 1000 / 3
    }
}

/// Whether a delivery's bearer matches the token a subscriber registered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCheck {
    Valid,
    Invalid,
}

/// Check an `Authorization` header value against the expected token, in time
/// independent of where they differ.
pub fn check_bearer(authorization: Option<&str>, token: &str) -> TokenCheck {
    let Some(presented) = authorization.and_then(|h| h.strip_prefix("Bearer ")) else {
        return TokenCheck::Invalid;
    };
    let (a, b) = (presented.as_bytes(), token.as_bytes());
    if token.is_empty() || a.len() != b.len() {
        return TokenCheck::Invalid;
    }
    let difference = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    if difference == 0 {
        TokenCheck::Valid
    } else {
        TokenCheck::Invalid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str) -> SubscribeRequest {
        SubscribeRequest {
            subscriber: SubscriberId(id.to_string()),
            deliver_to: format!("https://{id}.example/_internal/heartbeat"),
            token: "secret".to_string(),
            lease_secs: 300,
        }
    }

    #[test]
    fn registering_adds_then_renews() {
        let mut registry = SubscriberRegistry::default();
        assert_eq!(
            registry.register(request("book"), 1_000),
            SubscribeResponse::Added {
                expires_at_ms: 301_000
            }
        );
        assert_eq!(
            registry.register(request("book"), 200_000),
            SubscribeResponse::Renewed {
                expires_at_ms: 500_000
            }
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn leases_are_clamped_and_expire() {
        let mut registry = SubscriberRegistry::default();
        let mut short = request("short");
        short.lease_secs = 1;
        let mut long = request("long");
        long.lease_secs = 86_400;
        registry.register(short, 0);
        registry.register(long, 0);
        assert_eq!(registry.targets(59_999).count(), 2);
        assert_eq!(registry.targets(60_000).count(), 1);
        assert_eq!(registry.expire(900_000), 2);
        assert!(registry.is_empty());
    }

    #[test]
    fn unsafe_or_unauthenticated_registrations_are_refused() {
        let mut registry = SubscriberRegistry::default();
        let mut plain = request("plain");
        plain.deliver_to = "http://example.com/hook".to_string();
        assert_eq!(
            registry.register(plain, 0),
            SubscribeResponse::Rejected {
                reason: Rejection::InsecureUrl
            }
        );
        let mut local = request("local");
        local.deliver_to = "http://localhost:8787/_internal/heartbeat".to_string();
        assert!(matches!(
            registry.register(local, 0),
            SubscribeResponse::Added { .. }
        ));
        let mut tokenless = request("tokenless");
        tokenless.token.clear();
        assert_eq!(
            registry.register(tokenless, 0),
            SubscribeResponse::Rejected {
                reason: Rejection::EmptyToken
            }
        );
    }

    #[test]
    fn a_full_registry_refuses_newcomers_but_renews_members() {
        let mut registry = SubscriberRegistry::default();
        for i in 0..MAX_SUBSCRIBERS {
            registry.register(request(&format!("s{i}")), 0);
        }
        assert_eq!(
            registry.register(request("late"), 0),
            SubscribeResponse::Rejected {
                reason: Rejection::Full
            }
        );
        assert!(matches!(
            registry.register(request("s0"), 0),
            SubscribeResponse::Renewed { .. }
        ));
    }

    #[test]
    fn failing_subscribers_are_dropped() {
        let mut registry = SubscriberRegistry::default();
        registry.register(request("flaky"), 0);
        registry.register(request("gone"), 0);
        let flaky = SubscriberId("flaky".to_string());
        let gone = SubscriberId("gone".to_string());

        assert_eq!(
            registry.record(&flaky, DeliveryOutcome::Failed),
            AfterDelivery::Kept
        );
        assert_eq!(
            registry.record(&flaky, DeliveryOutcome::Delivered),
            AfterDelivery::Kept
        );
        for _ in 0..DROP_AFTER_FAILURES - 1 {
            registry.record(&flaky, DeliveryOutcome::Failed);
        }
        assert_eq!(registry.len(), 2, "a success resets the count");
        assert_eq!(
            registry.record(&flaky, DeliveryOutcome::Failed),
            AfterDelivery::Dropped
        );
        assert_eq!(
            registry.record(&gone, DeliveryOutcome::Gone),
            AfterDelivery::Dropped
        );
        assert!(registry.is_empty());
        assert_eq!(registry.remove(&gone), Removal::Unknown);
    }

    #[test]
    fn a_lease_follows_its_listeners() {
        let mut lease = SubscriptionLease::new(300);
        assert_eq!(lease.next(0, 0), LeaseAction::Hold);
        assert_eq!(lease.next(2, 0), LeaseAction::Register);

        lease.answered(SubscribeResponse::Added {
            expires_at_ms: 300_000,
        });
        assert_eq!(lease.next(2, 100_000), LeaseAction::Hold);
        // Renew inside the last third.
        assert_eq!(lease.check_again_in_ms(100_000), Some(100_000));
        assert_eq!(lease.next(2, 200_000), LeaseAction::Renew);
        assert_eq!(lease.next(0, 200_000), LeaseAction::Deregister);

        lease.released();
        assert_eq!(lease.next(0, 200_000), LeaseAction::Hold);
        assert_eq!(lease.check_again_in_ms(200_000), None);

        lease.answered(SubscribeResponse::Rejected {
            reason: Rejection::Full,
        });
        assert_eq!(lease.next(1, 0), LeaseAction::Register);
    }

    #[test]
    fn bearer_tokens_must_match_exactly() {
        assert_eq!(
            check_bearer(Some("Bearer secret"), "secret"),
            TokenCheck::Valid
        );
        assert_eq!(
            check_bearer(Some("Bearer secreT"), "secret"),
            TokenCheck::Invalid
        );
        assert_eq!(
            check_bearer(Some("Bearer secrets"), "secret"),
            TokenCheck::Invalid
        );
        assert_eq!(check_bearer(Some("secret"), "secret"), TokenCheck::Invalid);
        assert_eq!(check_bearer(None, "secret"), TokenCheck::Invalid);
        assert_eq!(check_bearer(Some("Bearer "), ""), TokenCheck::Invalid);
    }

    #[test]
    fn requests_and_responses_round_trip() {
        let json = serde_json::to_string(&request("book")).unwrap();
        assert_eq!(
            serde_json::from_str::<SubscribeRequest>(&json).unwrap(),
            request("book")
        );
        let response = SubscribeResponse::Rejected {
            reason: Rejection::Full,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"outcome":"rejected","reason":"full"}"#);
    }
}
