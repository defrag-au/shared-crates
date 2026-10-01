//! Where the follower has read up to.
//!
//! There is no replay on the stream — OpenSea does not re-send what a connection
//! error lost, and a deploy tears every socket — so this is **not** a position to
//! resume the socket from. There is no such position. It is the record of how far
//! the *consumer* has got, and its job is to tell a reconcile where to start: a
//! socket that was down for an hour left an hour-wide hole, and the only thing that
//! fills it is reading the chain from the last point the ledger was known good.
//!
//! So there are two waters on a row and they answer different questions:
//!
//! - `last_sent_at` / `last_event_timestamp` — how fresh the stream reading is.
//! - `reconciled_block` — how far the chain has been checked against.
//!
//! A gap between them is normal and is exactly the state a reconcile exists to
//! close. A row where the block is ahead of the stream is what a follower that
//! reconciled on startup and has not yet received an event looks like, which is why
//! every field is optional and a row can be created by a reconcile alone.
//!
//! # Writing is explicit, and rare
//!
//! [`Checkpoints::observe`] folds an event into memory and writes nothing; the file
//! is rewritten by [`Checkpoints::flush`]. A row per event, which is what writing
//! on every event amounts to, would be hundreds of thousands of rows a day for a
//! handful of collections — and the file only ever needs the current row per topic,
//! so append-per-event is all cost and no benefit.
//!
//! `flush` rewrites the whole set into a temporary file and renames it over the
//! target, so the file on disk is always one of the two complete versions and never
//! a half-written one. The cost is that a crash loses the events since the last
//! flush, which is why the caller decides the cadence — see `examples/follow.rs`,
//! which flushes every hundred events. That window is bounded and stated rather than
//! silent, and it is no worse than the stream itself, which can lose an arbitrary
//! window to a socket drop.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use opensea_stream::EventStamp;
use serde::{Deserialize, Serialize};

/// What has been seen for one topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The topic — `collection:<slug>`.
    pub topic: String,
    /// The latest movement timestamp seen.
    ///
    /// Movements carry `event_timestamp` and metadata updates do not, so this stands
    /// at the last movement rather than at the last event.
    pub last_event_timestamp: Option<String>,
    /// The latest `sent_at` seen — the source's clock, and the only timestamp a
    /// metadata update has.
    pub last_sent_at: Option<String>,
    /// The last block a reconcile has covered for this collection.
    pub reconciled_block: Option<u64>,
    /// When the row was last written, in Unix seconds. Our clock, not the stream's,
    /// so it is for spotting a stalled follower rather than for ordering anything.
    pub updated_at: u64,
}

impl Checkpoint {
    /// A row for a topic with nothing yet known about it.
    fn empty(topic: &str) -> Self {
        Self {
            topic: topic.to_owned(),
            last_event_timestamp: None,
            last_sent_at: None,
            reconciled_block: None,
            updated_at: now(),
        }
    }
}

impl fmt::Display for Checkpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sent = self.last_sent_at.as_deref().unwrap_or("never");
        write!(formatter, "{}: stream {sent}", self.topic)?;
        if let Some(block) = self.reconciled_block {
            write!(formatter, ", reconciled to block {block}")?;
        }
        Ok(())
    }
}

/// The checkpoints, one row per topic, last write wins.
#[derive(Debug, Clone)]
pub struct Checkpoints {
    path: Option<PathBuf>,
    by_topic: BTreeMap<String, Checkpoint>,
    dirty: BTreeSet<String>,
}

impl Checkpoints {
    /// Checkpoints that nothing persists — for a test, or a follower whose only job
    /// is to stream.
    pub fn in_memory() -> Self {
        Self {
            path: None,
            by_topic: BTreeMap::new(),
            dirty: BTreeSet::new(),
        }
    }

    /// Opens a JSONL checkpoint file, if there is one.
    ///
    /// A missing file is an empty set rather than an error: the first run of a
    /// follower has no checkpoints, and that is not a failure.
    ///
    /// A line that will not parse is skipped with a warning — the realistic cause is
    /// a truncation from an interrupted write, and dropping the row it touched is a
    /// smaller failure than refusing to start.
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let mut by_topic = BTreeMap::new();

        if path.exists() {
            for line in BufReader::new(File::open(&path)?).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Checkpoint>(&line) {
                    Ok(checkpoint) => {
                        by_topic.insert(checkpoint.topic.clone(), checkpoint);
                    }
                    Err(error) => {
                        tracing::warn!("checkpoint: skipping an unreadable row: {error}");
                    }
                }
            }
        }

        Ok(Self {
            path: Some(path),
            by_topic,
            dirty: BTreeSet::new(),
        })
    }

    /// The file being written, when there is one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// One topic's row.
    pub fn get(&self, topic: &str) -> Option<&Checkpoint> {
        self.by_topic.get(topic)
    }

    /// Every row, ordered by topic.
    pub fn iter(&self) -> impl Iterator<Item = &Checkpoint> {
        self.by_topic.values()
    }

    /// Whether there are no rows.
    pub fn is_empty(&self) -> bool {
        self.by_topic.is_empty()
    }

    /// Whether anything has changed since the last [`Checkpoints::flush`].
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Folds an observed event into its topic's row. Writes nothing.
    ///
    /// `event_timestamp` is the movement's own field where the event has one; a
    /// metadata update passes `None`, which leaves the last movement's timestamp in
    /// place rather than clearing it — the field means "the last movement seen", not
    /// "the last event".
    pub fn observe(&mut self, topic: &str, stamp: &EventStamp, event_timestamp: Option<&str>) {
        let row = self
            .by_topic
            .entry(topic.to_owned())
            .or_insert_with(|| Checkpoint::empty(topic));

        row.last_sent_at = Some(stamp.sent_at.clone());
        if let Some(timestamp) = event_timestamp {
            row.last_event_timestamp = Some(timestamp.to_owned());
        }
        row.updated_at = now();

        self.dirty.insert(topic.to_owned());
    }

    /// Records that a reconcile has covered up to a block. Writes nothing.
    ///
    /// Creates the row when there is none, because reconciling before receiving an
    /// event is the normal startup order: the chain is read first, then the socket
    /// is joined.
    pub fn reconcile(&mut self, topic: &str, block: u64) {
        let row = self
            .by_topic
            .entry(topic.to_owned())
            .or_insert_with(|| Checkpoint::empty(topic));

        row.reconciled_block = Some(block);
        row.updated_at = now();

        self.dirty.insert(topic.to_owned());
    }

    /// Writes the current set out, if anything changed.
    ///
    /// Rewritten rather than appended, into a temporary file that is renamed over
    /// the target: the file is therefore always a complete version of the set, and a
    /// crash mid-write leaves the previous one rather than a truncated one.
    pub fn flush(&mut self) -> io::Result<()> {
        if self.dirty.is_empty() {
            return Ok(());
        }
        let Some(path) = self.path.clone() else {
            self.dirty.clear();
            return Ok(());
        };

        let temporary = temporary_path(&path);
        {
            let mut file = BufWriter::new(File::create(&temporary)?);
            for row in self.by_topic.values() {
                serde_json::to_writer(&mut file, row).map_err(io::Error::other)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
        }
        std::fs::rename(&temporary, &path)?;

        self.dirty.clear();
        Ok(())
    }
}

/// The sidecar `flush` writes and then renames over the target.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Unix seconds. A clock set before 1970 is not a case to handle, only to name, and
/// it reads as zero.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// A path under the temp directory that no other test run shares.
#[cfg(test)]
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "opensea-checkpoints-{name}-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(temporary_path(&path));
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensea_stream::{EventStamp, EventType, Topic};

    fn stamp(sent_at: &str) -> EventStamp {
        EventStamp {
            topic: Topic::collection("x"),
            event_type: EventType::ItemMetadataUpdated,
            version: 2,
            sent_at: sent_at.to_owned(),
        }
    }

    #[test]
    fn a_movement_sets_both_timestamps_and_a_metadata_update_leaves_one_alone() {
        let mut checkpoints = Checkpoints::in_memory();
        checkpoints.observe(
            "collection:x",
            &stamp("2026-09-27T00:00:00Z"),
            Some("2026-09-26T23:59:00Z"),
        );
        checkpoints.observe("collection:x", &stamp("2026-09-27T00:01:00Z"), None);

        let row = checkpoints.get("collection:x").unwrap();
        assert_eq!(row.last_sent_at.as_deref(), Some("2026-09-27T00:01:00Z"));
        assert_eq!(
            row.last_event_timestamp.as_deref(),
            Some("2026-09-26T23:59:00Z")
        );
    }

    #[test]
    fn reconciling_before_any_event_is_a_row_rather_than_a_failure() {
        let mut checkpoints = Checkpoints::in_memory();
        checkpoints.reconcile("collection:x", 17_000_000);

        let row = checkpoints.get("collection:x").unwrap();
        assert_eq!(row.reconciled_block, Some(17_000_000));
        assert_eq!(row.last_sent_at, None);
    }

    #[test]
    fn a_reconcile_keeps_the_stream_watermark_and_the_other_way_round() {
        let mut checkpoints = Checkpoints::in_memory();
        checkpoints.observe("collection:x", &stamp("2026-09-27T00:00:00Z"), None);
        checkpoints.reconcile("collection:x", 100);

        let row = checkpoints.get("collection:x").unwrap();
        assert_eq!(row.last_sent_at.as_deref(), Some("2026-09-27T00:00:00Z"));
        assert_eq!(row.reconciled_block, Some(100));
    }

    #[test]
    fn observing_without_flushing_writes_nothing() {
        let path = scratch("unflushed");
        let mut checkpoints = Checkpoints::open(&path).unwrap();
        checkpoints.observe("collection:x", &stamp("2026-09-27T00:00:00Z"), None);

        assert!(checkpoints.is_dirty());
        assert!(!path.exists(), "nothing should have been written yet");

        checkpoints.flush().unwrap();
        assert!(path.exists());
        assert!(!checkpoints.is_dirty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_flushed_file_round_trips_and_holds_one_row_per_topic() {
        let path = scratch("round-trip");
        let mut checkpoints = Checkpoints::open(&path).unwrap();
        checkpoints.reconcile("collection:x", 100);
        checkpoints.flush().unwrap();
        checkpoints.observe("collection:x", &stamp("2026-09-27T00:00:00Z"), None);
        checkpoints.flush().unwrap();
        checkpoints.reconcile("collection:x", 200);
        checkpoints.flush().unwrap();
        checkpoints.reconcile("collection:y", 5);
        checkpoints.flush().unwrap();

        let reopened = Checkpoints::open(&path).unwrap();
        assert_eq!(
            reopened.get("collection:x").unwrap().reconciled_block,
            Some(200)
        );
        assert_eq!(
            reopened
                .get("collection:x")
                .unwrap()
                .last_sent_at
                .as_deref(),
            Some("2026-09-27T00:00:00Z")
        );
        assert_eq!(
            reopened.get("collection:y").unwrap().reconciled_block,
            Some(5)
        );
        // One row per topic, however many times it was written — the file is the
        // current set, not a journal of it.
        assert_eq!(reopened.iter().count(), 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_is_an_empty_set_not_an_error() {
        let path = std::env::temp_dir().join("opensea-checkpoints-does-not-exist.jsonl");
        let _ = std::fs::remove_file(&path);
        assert!(Checkpoints::open(&path).unwrap().is_empty());
    }

    #[test]
    fn a_truncated_last_row_is_skipped_rather_than_refused() {
        let path = scratch("truncated");
        std::fs::write(
            &path,
            "{\"topic\":\"collection:x\",\"last_sent_at\":null,\"last_event_timestamp\":null,\"reconciled_block\":7,\"updated_at\":0}\n{\"topic\":\"collect",
        )
        .unwrap();

        let checkpoints = Checkpoints::open(&path).unwrap();
        assert_eq!(
            checkpoints.get("collection:x").unwrap().reconciled_block,
            Some(7)
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flushing_leaves_no_sidecar_behind() {
        let path = scratch("sidecar");
        let mut checkpoints = Checkpoints::open(&path).unwrap();
        checkpoints.reconcile("collection:x", 1);
        checkpoints.flush().unwrap();

        assert!(path.exists());
        assert!(!temporary_path(&path).exists());

        let _ = std::fs::remove_file(&path);
    }
}
