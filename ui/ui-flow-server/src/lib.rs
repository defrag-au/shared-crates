//! Server-side ui-flow helpers for sockets held by a Durable Object.
//!
//! ui-flow's wire protocol already lets a client subscribe to notification
//! domains, but every server so far accepted `Subscribe` and dropped it, so a
//! `Notify` went to every socket. This crate is the missing server half.
//!
//! - [`Subscriptions`] is one socket's domain set. It lives inside the host's
//!   own socket attachment, reached through [`SocketAttachment`], because a
//!   socket has exactly one attachment and hosts already use it for connection
//!   info. Living there is also what carries it through hibernation.
//! - [`encode_notify`] encodes a `Notify` once for any number of sockets.
//! - With the `worker` feature, [`durable::apply_subscription`] updates a
//!   socket's attachment from a client message, and [`durable::notify`] sends a
//!   `Notify` only to the sockets subscribed to its domain.
//!
//! **Why attachments and not tags.** Cloudflare fixes a socket's tags when it
//! is accepted, so a client could never subscribe after connecting. Filtering
//! by attachment decodes each socket's attachment per notification, which is
//! cheap at the socket counts one object holds.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ui_flow_protocol::{ClientMessage, ProtocolError, ServerMessage};

/// Domains one socket may hold. Attachments are size-limited, and no client
/// needs more.
pub const MAX_DOMAINS: usize = 16;

/// Longest accepted domain name, for the same reason.
pub const MAX_DOMAIN_LEN: usize = 128;

/// One socket's notification domains.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Subscriptions {
    domains: BTreeSet<String>,
}

/// What a subscription message did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionChange {
    /// The set changed: persist the attachment.
    Changed,
    Unchanged,
    /// Nothing was applied.
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    TooManyDomains,
    DomainTooLong,
    EmptyDomain,
}

impl Subscriptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `domains`, all or nothing.
    pub fn subscribe(&mut self, domains: &[String]) -> SubscriptionChange {
        if let Some(rejection) = domains.iter().find_map(|d| validate(d)) {
            return SubscriptionChange::Rejected(rejection);
        }
        let added: Vec<&String> = domains
            .iter()
            .filter(|d| !self.domains.contains(*d))
            .collect();
        let distinct_added: BTreeSet<&String> = added.into_iter().collect();
        if self.domains.len() + distinct_added.len() > MAX_DOMAINS {
            return SubscriptionChange::Rejected(Rejection::TooManyDomains);
        }
        if distinct_added.is_empty() {
            return SubscriptionChange::Unchanged;
        }
        self.domains.extend(distinct_added.into_iter().cloned());
        SubscriptionChange::Changed
    }

    pub fn unsubscribe(&mut self, domains: &[String]) -> SubscriptionChange {
        let before = self.domains.len();
        for domain in domains {
            self.domains.remove(domain);
        }
        if self.domains.len() == before {
            SubscriptionChange::Unchanged
        } else {
            SubscriptionChange::Changed
        }
    }

    /// Apply a client message if it is a subscription message; `None` for any
    /// other message.
    pub fn apply<A>(&mut self, message: &ClientMessage<A>) -> Option<SubscriptionChange> {
        match message {
            ClientMessage::Subscribe { domains } => Some(self.subscribe(domains)),
            ClientMessage::Unsubscribe { domains } => Some(self.unsubscribe(domains)),
            ClientMessage::Ping { .. }
            | ClientMessage::Resync { .. }
            | ClientMessage::Action { .. }
            | ClientMessage::Signal { .. } => None,
        }
    }

    pub fn contains(&self, domain: &str) -> bool {
        self.domains.contains(domain)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.domains.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.domains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

fn validate(domain: &str) -> Option<Rejection> {
    if domain.is_empty() {
        Some(Rejection::EmptyDomain)
    } else if domain.len() > MAX_DOMAIN_LEN {
        Some(Rejection::DomainTooLong)
    } else {
        None
    }
}

/// A host's socket attachment that carries [`Subscriptions`].
pub trait SocketAttachment: Serialize + DeserializeOwned {
    fn subscriptions(&self) -> &Subscriptions;
    fn subscriptions_mut(&mut self) -> &mut Subscriptions;
}

/// Encode a `Notify` once. A `Notify` carries no state or delta, so those type
/// parameters are unit here, and any client decodes the bytes with its own.
pub fn encode_notify<E: Serialize>(domain: &str, event: &E) -> Result<Vec<u8>, ProtocolError> {
    let message: ServerMessage<(), (), &E> = ServerMessage::notify(domain, event, None);
    ui_flow_protocol::encode(&message)
}

#[cfg(feature = "worker")]
pub mod durable {
    //! Durable Object glue.

    use serde::Serialize;
    use ui_flow_protocol::ClientMessage;
    use worker_stack::worker::{Error, Result, State, WebSocket};

    use super::{SocketAttachment, SubscriptionChange, encode_notify};

    /// Apply a client subscription message to `ws`'s attachment, persisting it
    /// when it changed. `None` when the message is not a subscription message
    /// or the socket carries no attachment of type `T`.
    pub fn apply_subscription<A, T: SocketAttachment>(
        ws: &WebSocket,
        message: &ClientMessage<A>,
    ) -> Result<Option<SubscriptionChange>> {
        let Some(mut attachment) = ws.deserialize_attachment::<T>()? else {
            return Ok(None);
        };
        let change = attachment.subscriptions_mut().apply(message);
        if change == Some(SubscriptionChange::Changed) {
            ws.serialize_attachment(&attachment)?;
        }
        Ok(change)
    }

    /// What a notification reached.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct NotifyReport {
        pub sent: usize,
        /// Sockets not subscribed to the domain, or not carrying a `T` at all
        /// (a host's other kinds of socket).
        pub skipped: usize,
        pub failed: usize,
    }

    /// Send a `Notify` to every socket subscribed to `domain`, encoding once.
    pub fn notify<T: SocketAttachment, E: Serialize>(
        state: &State,
        domain: &str,
        event: &E,
    ) -> Result<NotifyReport> {
        let bytes = encode_notify(domain, event)
            .map_err(|e| Error::RustError(format!("encode notify: {e}")))?;
        let mut report = NotifyReport::default();
        for ws in state.get_websockets() {
            match ws.deserialize_attachment::<T>() {
                Ok(Some(attachment)) if attachment.subscriptions().contains(domain) => {
                    match ws.send_with_bytes(&bytes) {
                        Ok(()) => report.sent += 1,
                        Err(_) => report.failed += 1,
                    }
                }
                _ => report.skipped += 1,
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domains(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn subscribe_and_unsubscribe_report_whether_anything_changed() {
        let mut subs = Subscriptions::new();
        assert_eq!(
            subs.subscribe(&domains(&["chain", "book"])),
            SubscriptionChange::Changed
        );
        assert_eq!(
            subs.subscribe(&domains(&["chain"])),
            SubscriptionChange::Unchanged
        );
        assert!(subs.contains("chain"));
        assert_eq!(
            subs.unsubscribe(&domains(&["chain", "nope"])),
            SubscriptionChange::Changed
        );
        assert_eq!(
            subs.unsubscribe(&domains(&["chain"])),
            SubscriptionChange::Unchanged
        );
        assert_eq!(subs.iter().collect::<Vec<_>>(), vec!["book"]);
    }

    #[test]
    fn invalid_subscriptions_apply_nothing() {
        let mut subs = Subscriptions::new();
        assert_eq!(
            subs.subscribe(&domains(&["ok", ""])),
            SubscriptionChange::Rejected(Rejection::EmptyDomain)
        );
        assert!(subs.is_empty());
        assert_eq!(
            subs.subscribe(&["x".repeat(MAX_DOMAIN_LEN + 1)]),
            SubscriptionChange::Rejected(Rejection::DomainTooLong)
        );

        let many: Vec<String> = (0..MAX_DOMAINS).map(|i| format!("d{i}")).collect();
        assert_eq!(subs.subscribe(&many), SubscriptionChange::Changed);
        assert_eq!(
            subs.subscribe(&domains(&["one-more"])),
            SubscriptionChange::Rejected(Rejection::TooManyDomains)
        );
        // Re-subscribing to held domains never counts against the cap.
        assert_eq!(
            subs.subscribe(&domains(&["d0", "d0"])),
            SubscriptionChange::Unchanged
        );
    }

    #[test]
    fn only_subscription_messages_are_applied() {
        let mut subs = Subscriptions::new();
        let subscribe: ClientMessage<()> = ClientMessage::subscribe(domains(&["chain"]));
        assert_eq!(subs.apply(&subscribe), Some(SubscriptionChange::Changed));
        let ping: ClientMessage<()> = ClientMessage::ping(1);
        assert_eq!(subs.apply(&ping), None);
    }

    #[test]
    fn an_encoded_notify_decodes_with_any_clients_types() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        enum AppEvent {
            Sold { price: u64 },
        }
        #[derive(Debug, Serialize, Deserialize)]
        struct AppState {
            count: u32,
        }

        let bytes = encode_notify("book", &AppEvent::Sold { price: 7 }).unwrap();
        let decoded: ServerMessage<AppState, Vec<u8>, AppEvent> =
            ui_flow_protocol::decode(&bytes).unwrap();
        match decoded {
            ServerMessage::Notify { domain, event, .. } => {
                assert_eq!(domain, "book");
                assert_eq!(event, AppEvent::Sold { price: 7 });
            }
            other => panic!("expected a notify, got {other:?}"),
        }
    }

    /// Chain frames travel as ui-flow notifications. Their `u64`s go through
    /// `wasm_safe_serde` (which deserialises via a JSON value) and their enums
    /// are internally tagged (which serde buffers), and neither is guaranteed to
    /// work over MessagePack just because it works over JSON.
    #[test]
    fn a_heartbeat_frame_survives_messagepack() {
        use chain_heartbeat::{
            BlockBeat, CHAIN_DOMAIN, ChainEvent, ChainPoint, Heartbeat, HeartbeatFrame, Network,
            SyncState,
        };

        let beat = BlockBeat {
            height: 13_358_656,
            slot: 186_000_000,
            hash: "ab".repeat(32),
            issuer_pool: "cd".repeat(28),
            body_size: 41_234,
            tx_count: Some(17),
            block_time_unix: Some(1_777_566_291),
            vrf_output: None,
        };
        let mut heartbeat = Heartbeat::new(Network::Mainnet);
        heartbeat.connected(1_777_566_291_000);
        heartbeat.roll_forward(beat.clone(), SyncState::AtTip, 1_777_566_291_000);

        let frames = [
            heartbeat.resync_frame(1_777_566_300_000),
            HeartbeatFrame::Events {
                events: vec![
                    ChainEvent::Connected { version: 14 },
                    ChainEvent::RollForward {
                        beat,
                        sync: SyncState::CatchingUp,
                    },
                    ChainEvent::RollBackward {
                        to: Some(ChainPoint {
                            slot: 186_000_000,
                            hash: [7; 32],
                        }),
                    },
                    ChainEvent::RollBackward { to: None },
                    ChainEvent::KeepAliveAcknowledged,
                    ChainEvent::BlockTransactions {
                        txs: chain_heartbeat::BlockTxs::new(
                            13_358_657,
                            186_000_020,
                            &[[0xab; 32], [0xcd; 32]],
                            vec![1],
                        ),
                    },
                ],
            },
            HeartbeatFrame::UpstreamLost,
        ];

        for frame in frames {
            let bytes = encode_notify(CHAIN_DOMAIN, &frame).unwrap();
            let decoded: ServerMessage<(), (), HeartbeatFrame> =
                ui_flow_protocol::decode(&bytes).unwrap();
            match decoded {
                ServerMessage::Notify { domain, event, .. } => {
                    assert_eq!(domain, CHAIN_DOMAIN);
                    assert_eq!(event, frame);
                }
                other => panic!("expected a notify, got {other:?}"),
            }
        }
    }

    #[test]
    fn subscriptions_survive_an_attachment_round_trip() {
        #[derive(Serialize, Deserialize)]
        struct Conn {
            actor: String,
            #[serde(default)]
            subscriptions: Subscriptions,
        }
        let mut conn = Conn {
            actor: "a".into(),
            subscriptions: Subscriptions::new(),
        };
        conn.subscriptions.subscribe(&domains(&["chain"]));
        let bytes = ui_flow_protocol::encode(&conn).unwrap();
        let back: Conn = ui_flow_protocol::decode(&bytes).unwrap();
        assert_eq!(back.actor, "a");
        assert!(back.subscriptions.contains("chain"));
    }
}
