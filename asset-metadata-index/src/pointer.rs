//! The pointer: the one mutable object, flipped last.
//!
//! A **text manifest**, following the artifact family's convention
//! (`OFFER_INDEX_FORMAT.md` §3, `DATUM_RESOLVER_FORMAT.md` §6.2): one
//! `key value` per line, the version line **first**, and readers that refuse a
//! version they do not read. It is ops-readable, extensible without a binary
//! format bump, and off the hot path — readers cache it, and a worker usually
//! takes it from KV.
//!
//! A reader's first read. It names the built coverage — and therefore how far
//! the directory is complete and which frontier segments a reader must also
//! consult for a policy the directory has not caught up with.

use crate::error::DecodeError;
use crate::{NETWORK_MAINNET, NETWORK_PREPROD, POINTER_VERSION};

/// The manifest's version line, verbatim, is `ami_version <n>`.
pub const VERSION_KEY: &str = "ami_version";

/// The current build, as read from the pointer manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    /// Network byte ([`crate::NETWORK_MAINNET`] / [`crate::NETWORK_PREPROD`]).
    pub network: u8,
    /// Segments exist through this chunk.
    pub built_through_chunk: u16,
    /// The directory is complete through this chunk; segments newer than it
    /// are the frontier a reader scans.
    pub directory_through_chunk: u16,
    /// Total records across every segment in the build (ops).
    pub record_count: u64,
    /// Unix seconds of the publish.
    pub published_at: u64,
}

impl Pointer {
    /// Parse the manifest from fetched bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, DecodeError> {
        let text = std::str::from_utf8(bytes).map_err(|_| DecodeError::BadField(VERSION_KEY))?;
        Self::parse_str(text)
    }

    /// Parse the manifest text. Unknown keys are ignored so an additive
    /// field can land without breaking older readers; a missing or malformed
    /// known key is refused — a pointer that half-parses would pin a build
    /// nobody can describe.
    pub fn parse_str(text: &str) -> Result<Self, DecodeError> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let first = lines.next().ok_or(DecodeError::MissingField(VERSION_KEY))?;
        let (key, value) = split(first).ok_or(DecodeError::BadField(VERSION_KEY))?;
        if key != VERSION_KEY {
            return Err(DecodeError::BadField(VERSION_KEY));
        }
        let version: u8 = value
            .parse()
            .map_err(|_| DecodeError::BadField(VERSION_KEY))?;
        if version != POINTER_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }

        let mut network = None;
        let mut built_through_chunk = None;
        let mut directory_through_chunk = None;
        let mut record_count = None;
        let mut published_at = None;
        for line in lines {
            let Some((key, value)) = split(line) else {
                return Err(DecodeError::BadField("pointer"));
            };
            match key {
                "network" => {
                    network = Some(match value {
                        "mainnet" => NETWORK_MAINNET,
                        "preprod" => NETWORK_PREPROD,
                        _ => return Err(DecodeError::BadField("network")),
                    });
                }
                "built_through" => {
                    built_through_chunk = Some(
                        value
                            .parse()
                            .map_err(|_| DecodeError::BadField("built_through"))?,
                    );
                }
                "directory_through" => {
                    directory_through_chunk = Some(
                        value
                            .parse()
                            .map_err(|_| DecodeError::BadField("directory_through"))?,
                    );
                }
                "records" => {
                    record_count = Some(
                        value
                            .parse()
                            .map_err(|_| DecodeError::BadField("records"))?,
                    );
                }
                "published_at" => {
                    published_at = Some(
                        value
                            .parse()
                            .map_err(|_| DecodeError::BadField("published_at"))?,
                    );
                }
                _ => {}
            }
        }
        Ok(Self {
            network: network.ok_or(DecodeError::MissingField("network"))?,
            built_through_chunk: built_through_chunk
                .ok_or(DecodeError::MissingField("built_through"))?,
            directory_through_chunk: directory_through_chunk
                .ok_or(DecodeError::MissingField("directory_through"))?,
            record_count: record_count.ok_or(DecodeError::MissingField("records"))?,
            published_at: published_at.ok_or(DecodeError::MissingField("published_at"))?,
        })
    }

    /// Encode the manifest. A publisher writes this last.
    #[must_use]
    pub fn encode(&self) -> String {
        let network = match self.network {
            NETWORK_MAINNET => "mainnet",
            NETWORK_PREPROD => "preprod",
            // The crate's only two network bytes; anything else is written
            // through as-is and refused by every reader as a bad field.
            _ => "unknown",
        };
        format!(
            "{VERSION_KEY} {POINTER_VERSION}\n\
             network {network}\n\
             built_through {}\n\
             directory_through {}\n\
             records {}\n\
             published_at {}\n",
            self.built_through_chunk,
            self.directory_through_chunk,
            self.record_count,
            self.published_at
        )
    }
}

fn split(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.trim().splitn(2, ' ');
    let key = parts.next()?;
    let value = parts.next()?.trim();
    (!key.is_empty() && !value.is_empty()).then_some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer() -> Pointer {
        Pointer {
            network: NETWORK_MAINNET,
            built_through_chunk: 9142,
            directory_through_chunk: 9139,
            record_count: 19_021_375,
            published_at: 1_792_000_000,
        }
    }

    #[test]
    fn a_pointer_round_trips() {
        let manifest = pointer().encode();
        assert_eq!(Pointer::parse_str(&manifest).expect("parses"), pointer());
        assert_eq!(
            Pointer::parse(manifest.as_bytes()).expect("parses"),
            pointer()
        );
    }

    /// The version line must be FIRST — a pointer whose version arrives later
    /// is a text from another format, not this one.
    #[test]
    fn the_version_line_must_come_first() {
        let text = "network mainnet\nami_version 1\n";
        assert!(matches!(
            Pointer::parse_str(text),
            Err(DecodeError::BadField(VERSION_KEY))
        ));
    }

    #[test]
    fn a_future_version_is_refused() {
        let text = "ami_version 3\nnetwork mainnet\n";
        assert!(matches!(
            Pointer::parse_str(text),
            Err(DecodeError::UnsupportedVersion(3))
        ));
    }

    #[test]
    fn missing_and_malformed_fields_are_refused() {
        let full = pointer().encode();
        let no_network: String = full
            .lines()
            .filter(|l| !l.starts_with("network"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(matches!(
            Pointer::parse_str(&no_network),
            Err(DecodeError::MissingField("network"))
        ));

        let bad_records = full.replace("records 19021375", "records lots");
        assert!(matches!(
            Pointer::parse_str(&bad_records),
            Err(DecodeError::BadField("records"))
        ));
    }

    /// Additive fields are ignored, not fatal — the family's manifests grow
    /// lines, and an older reader must keep reading them.
    #[test]
    fn unknown_keys_are_ignored() {
        let mut text = pointer().encode();
        text.push_str("future_thing yes\n");
        assert_eq!(Pointer::parse_str(&text).expect("parses"), pointer());
    }
}
