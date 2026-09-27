//! Reading the protocol's string-valued enums.

use std::fmt;

use serde::de::{self, Visitor};

/// Deserialises a JSON string through a total function.
///
/// The protocol's enums — the event position, a reply's status, an event type —
/// all arrive as bare strings whose known values are few and whose unknown values
/// must survive. A function that never fails is what that shape wants, so each
/// one keeps an `Unknown` arm and this visitor applies it.
pub(crate) struct MapStrVisitor<F>(pub(crate) F);

impl<'de, F, T> Visitor<'de> for MapStrVisitor<F>
where
    F: Fn(&str) -> T,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
        Ok((self.0)(value))
    }
}
