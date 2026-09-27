//! The five-element frame, and the payloads a consumer sends or recognises.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::ser::{SerializeTuple, Serializer};
use serde::{Deserialize, Serialize};

use crate::event_type::EventType;
use crate::filter::EventFilter;
use crate::strings::MapStrVisitor;
use crate::topic::Topic;

/// How many positions a Phoenix frame has, and therefore what [`serde_json`] is
/// told to expect.
pub const FRAME_LEN: usize = 5;

/// A Phoenix frame: `[join_ref, ref, topic, event, payload]`.
///
/// Generic over its payload because the shape depends on the frame: a filtered
/// join carries [`EventFilter`], a leave or heartbeat carries [`EmptyPayload`], a
/// reply carries [`Reply`], and an event carries whatever that event carries.
/// [`FrameHeader`] is the way to read the routing fields without committing to a
/// body type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame<P> {
    /// The join this frame belongs to. `None` on a heartbeat.
    pub join_ref: Option<String>,
    /// Correlates a reply with the request that caused it.
    pub reference: Option<String>,
    /// Which subscription, or the system topic.
    pub topic: Topic,
    /// A protocol message, or the event type.
    pub event: FrameEvent,
    /// The body.
    pub payload: P,
}

impl<P: Serialize> Frame<P> {
    /// Encodes the frame to its wire form.
    pub fn to_wire(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl<P: for<'de> Deserialize<'de>> Frame<P> {
    /// Reads a frame from its wire form.
    pub fn from_wire(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }
}

impl Frame<EventFilter> {
    /// Subscribes to a topic — or reseats an existing subscription with it.
    ///
    /// `[ref, ref, topic, "phx_join", {"event_types": […]}]`, with `{}` for
    /// [`EventFilter::All`]. A join names itself as its own `join_ref`, which is
    /// what every later leave and event frame for that subscription refers back
    /// to.
    ///
    /// # Reseating does not need a leave
    ///
    /// Sending this for a topic already joined is accepted, and it replaces what
    /// the subscription carries — measured taking a subscription that had been
    /// delivering ~55 frames/s to silence by re-joining it with a filter that
    /// matches nothing. So changing a filter is this frame, not a leave followed
    /// by a join.
    pub fn join(topic: Topic, filter: EventFilter, reference: impl Into<String>) -> Self {
        let reference = reference.into();
        Self {
            join_ref: Some(reference.clone()),
            reference: Some(reference),
            topic,
            event: FrameEvent::Join,
            payload: filter,
        }
    }
}

impl Frame<EmptyPayload> {
    /// Leaves a topic: `[join_ref, ref, topic, "phx_leave", {}]`.
    ///
    /// The `join_ref` must be the one the current join was made with, or the
    /// socket has no way to know which subscription is meant.
    pub fn leave(topic: Topic, join_ref: impl Into<String>, reference: impl Into<String>) -> Self {
        Self {
            join_ref: Some(join_ref.into()),
            reference: Some(reference.into()),
            topic,
            event: FrameEvent::Leave,
            payload: EmptyPayload {},
        }
    }

    /// The keepalive: `[null, ref, "phoenix", "heartbeat", {}]`.
    ///
    /// Phoenix presumes the connection dead without one every 30 s, and the
    /// socket's own documentation adds: reconnect if a reply has not arrived by
    /// the time the next one is due.
    pub fn heartbeat(reference: impl Into<String>) -> Self {
        Self {
            join_ref: None,
            reference: Some(reference.into()),
            topic: Topic::System,
            event: FrameEvent::Heartbeat,
            payload: EmptyPayload {},
        }
    }
}

/// The `{}` a leave or heartbeat carries — an empty object, not `null`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyPayload {}

/// What a frame is about: one of the socket's four protocol messages, or an
/// event type.
///
/// The event position and an event's own `event_type` field carry the same value,
/// so both are [`EventType`] — and its `Unknown` arm is what keeps a type OpenSea
/// adds later from being fatal to a reader.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FrameEvent {
    /// `phx_join`
    Join,
    /// `phx_leave`
    Leave,
    /// `phx_reply`
    Reply,
    /// `heartbeat`
    Heartbeat,
    /// A streamed event.
    Event(EventType),
}

impl FrameEvent {
    /// The `event` position's wire value.
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Join => "phx_join",
            Self::Leave => "phx_leave",
            Self::Reply => "phx_reply",
            Self::Heartbeat => "heartbeat",
            Self::Event(event_type) => event_type.as_wire(),
        }
    }

    /// The inverse of [`FrameEvent::as_wire`], total by construction.
    fn from_wire(value: &str) -> Self {
        match value {
            "phx_join" => Self::Join,
            "phx_leave" => Self::Leave,
            "phx_reply" => Self::Reply,
            "heartbeat" => Self::Heartbeat,
            other => Self::Event(EventType::from_wire(other)),
        }
    }
}

impl Serialize for FrameEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for FrameEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(MapStrVisitor(FrameEvent::from_wire))
    }
}

/// The body of a `phx_reply`.
///
/// Nothing in a reply changes what a consumer does except on refusal, where the
/// `reason` is the only place a bad request explains itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// Whether the socket accepted the request.
    pub status: ReplyStatus,
    /// The request's own result.
    #[serde(default)]
    pub response: ReplyResponse,
}

/// What a reply's `response` object holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyResponse {
    /// Why a request was refused, when it was.
    #[serde(default)]
    pub reason: Option<String>,
}

/// A reply's `status`, kept as a string when it is not one of the two the
/// protocol documents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReplyStatus {
    /// `ok`
    Ok,
    /// `error`
    Error,
    /// Anything else, carried verbatim.
    Other(String),
}

impl ReplyStatus {
    /// The `status` position's wire value.
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Other(value) => value,
        }
    }

    /// The inverse of [`ReplyStatus::as_wire`], total by construction.
    fn from_wire(value: &str) -> Self {
        match value {
            "ok" => Self::Ok,
            "error" => Self::Error,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl Serialize for ReplyStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for ReplyStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(MapStrVisitor(ReplyStatus::from_wire))
    }
}

/// A frame's routing fields, with the body discarded.
///
/// This is how a consumer decides what a frame is — which subscription, which
/// event — without paying to parse a body it may not act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    /// The join this frame belongs to.
    pub join_ref: Option<String>,
    /// The request a reply answers.
    pub reference: Option<String>,
    /// Which subscription, or the system topic.
    pub topic: Topic,
    /// A protocol message, or the event type.
    pub event: FrameEvent,
}

impl FrameHeader {
    /// Reads a frame's routing fields, skipping its body.
    pub fn from_wire(raw: &str) -> Result<Self, serde_json::Error> {
        let frame: Frame<IgnoredAny> = serde_json::from_str(raw)?;
        Ok(Self {
            join_ref: frame.join_ref,
            reference: frame.reference,
            topic: frame.topic,
            event: frame.event,
        })
    }
}

impl<P: Serialize> Serialize for Frame<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(FRAME_LEN)?;
        tuple.serialize_element(&self.join_ref)?;
        tuple.serialize_element(&self.reference)?;
        tuple.serialize_element(&self.topic)?;
        tuple.serialize_element(&self.event)?;
        tuple.serialize_element(&self.payload)?;
        tuple.end()
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for Frame<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FrameVisitor<P>(PhantomData<P>);

        impl<'de, P: Deserialize<'de>> Visitor<'de> for FrameVisitor<P> {
            type Value = Frame<P>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Phoenix frame: [join_ref, ref, topic, event, payload]")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Frame<P>, A::Error> {
                let join_ref = seq
                    .next_element::<Option<String>>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let reference = seq
                    .next_element::<Option<String>>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let topic = seq
                    .next_element::<Topic>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let event = seq
                    .next_element::<FrameEvent>()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let payload = seq
                    .next_element::<P>()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;

                Ok(Frame {
                    join_ref,
                    reference,
                    topic,
                    event,
                    payload,
                })
            }
        }

        deserializer.deserialize_tuple(FRAME_LEN, FrameVisitor(PhantomData))
    }
}
