//! `LocatorGraph` — a lookup drawn as wires: a hash narrowing to a byte span, and the corpus settling it.
//!
//! The locator is two structures that never touch. The **index** is keyed by
//! hash — a 24-bit bucket, the shard holding that bucket's entries, the bucket's
//! run inside it, and an entry of which eight bytes are the hash. The **corpus**
//! is ordered by position — chunk after chunk of raw bytes, with no keys in it at
//! all. They meet at exactly one thing: the entry's [`Span`], a chunk, an offset
//! and a length.
//!
//! ## What it refuses to draw
//!
//! - **Blocks are not a level.** The persisted shape is chunks of bytes; blocks
//!   and their transactions are carved out during decode and have no address of
//!   their own. There is no block mark here, because there is no block pointer to
//!   draw.
//! - **The index locates transactions, not outrefs.** An output reference falls
//!   out of decoding the body at the span; nothing indexes it.
//! - **Eight bytes is not an identity.** An entry keeps eight of the hash's
//!   thirty-two, so one bucket can hold several transactions. That is why every
//!   mark ends in a re-hash: the corpus settles what the index only hints at, and
//!   the re-hash is one of the two places the answer can die.
//!
//! ## The axes, each of which means one thing
//!
//! Vertical position is **address**: the bucket's place in the 24-bit space, the
//! shard's among 32,768, the chunk's ordinal. Horizontal position is **stage**:
//! hash, bucket, shard, run, entry, the crossing, chunk. The band at the far
//! right is one log byte axis **inside a chunk** — where the span starts in it,
//! and how long it is. Nothing is decoration: move a mark and it has said
//! something false about a real address.
//!
//! ## One wire is one OBJECT, not one lookup
//!
//! [`reads`] collapses lookups to the objects they imply, because a shard holds
//! 512 buckets and a chunk holds many transactions. A run that locates 729
//! transactions reads ~719 entry shards and 129 chunks, and that ratio is the
//! thing worth seeing; one wire per lookup would hide it.
//!
//! **But a host that watched the requests go out should hand those over instead**
//! — see [`LocatorGraph::fired`]. A derived set is what the plan implied; the
//! observed set is what the browser did, it includes the reads the index makes
//! for itself ([`ReadTo::Index`]), and it is the only thing that can show a run
//! that has not finished yet. A read that has not landed carries `flight: None`
//!
//! ## Cost
//!
//! `reads()` is the only part that is not one draw per mark, and it is a sort and
//! a dedup: **1.8 ms for a 729-lookup run in a debug build** — 20 ms while it was
//! still scanning its own output once per lookup. The marks are one primitive per
//! stage per lookup, and the wires are one per OBJECT, so a run that locates 729
//! transactions draws ~719 entry wires and ~129 body wires rather than 729 of
//! each.
//!
//! ## Ambient by construction
//!
//! No label, no hover, no tooltip, and drawn at full strength so a story is a
//! fair test — fade it at the placement with `Ui::set_opacity`. Everything
//! painted is a pure function of `now`, so a still is reproducible and there is
//! no animation state to keep; a repaint is asked for only while a read or a
//! lookup is in the air, so an idle graph costs nothing.
//!
//! **Nothing travels, so nothing here is gated on reduced motion.** A wire's
//! animation is its opacity: out dim with its far end outlined, back at full
//! strength with the far end filled. A packet drawn *along* a wire would be a
//! position nobody measured — a read's flight is not known until the read is over,
//! which is exactly the case a live feed exists to draw — so the widget does not
//! draw one. Fades are permitted under reduced motion, and there is nothing else
//! here to suppress.

use egui::{Painter, Pos2, Rect, Response, Sense, Shape, Ui, Vec2, pos2, vec2};

use crate::theme::{Space, Theme, ThemeExt, hairline, stroke, with_alpha};

// ============================================================================
// What the index and the corpus are
// ============================================================================

/// A byte span inside one chunk — the addressing the index stores and the
/// corpus answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    /// Which chunk file, in corpus order.
    pub chunk: u16,
    /// Bytes from the start of that chunk.
    pub offset: u32,
    /// How many bytes the transaction occupies.
    pub len: u16,
}

/// How big both structures are, so every mark has an axis to sit on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    /// Buckets in the hash head — the first 24 bits of a transaction hash.
    pub buckets: u32,
    /// Entry shards, 512 buckets to each.
    pub shards: u32,
    /// Chunk files in the corpus.
    pub chunks: u32,
}

impl Extent {
    /// The buckets one entry shard holds — the index's own `SHARD_BUCKETS`.
    pub const BUCKETS_PER_SHARD: u32 = 512;

    /// The published build: a 24-bit bucket, 32,768 entry shards and chunks
    /// `0..=9202`. Read off the index's own pointer
    /// (`index/tx/v1/09202-416476537b08`), not chosen.
    pub const PUBLISHED: Self = Self {
        buckets: 1 << 24,
        shards: 32_768,
        chunks: 9_203,
    };
}

impl Default for Extent {
    fn default() -> Self {
        Self::PUBLISHED
    }
}

// ============================================================================
// What a lookup is
// ============================================================================

/// One transaction located: where the hash sent the search, what it found, and
/// how long the search took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lookup {
    /// The first 24 bits of the hash — the head's histogram address.
    pub bucket: u32,
    /// The entry shard holding that bucket's entries.
    pub shard: u32,
    /// How many entries the bucket's run holds. A bucket usually holds a
    /// handful; two transactions sharing a bucket is a collision, and a wider
    /// run is what one looks like.
    pub entries: u32,
    /// Where the body is.
    pub span: Span,
    /// Whether re-hashing the body at that span reproduced the hash.
    pub settled: bool,
    /// Seconds on the caller's clock when the hash was offered.
    pub at: f32,
    /// Seconds the lookup was in the air — a measured read, not a drawing.
    pub flight: f32,
}

/// What a read was for — the one thing its wire has to be placed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadTo {
    /// The index itself: the fence pair bounding a bucket, and the entry index its
    /// shard starts at. Both come off the directory, and neither crosses to the
    /// corpus.
    Index,
    /// An entry shard — read once however many buckets land in it.
    Run { shard: u32 },
    /// A chunk — read once however many transactions it holds.
    Body { span: Span },
}

/// One read of one object: the unit a request is counted in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Read {
    /// Which object.
    pub to: ReadTo,
    /// The bucket of the first lookup that needed it — where the wire starts.
    pub from_bucket: u32,
    /// Seconds when the object was asked for.
    pub at: f32,
    /// Seconds it took — `None` while it is still in the air, which is **not** the
    /// same as instantaneous, and is the difference between a wire that is still
    /// carrying a packet and one that has landed.
    pub flight: Option<f32>,
}

/// The reads a set of lookups implies, **one per object**, at the time the first
/// lookup that needed it went out.
///
/// This is the honest count and it is always smaller than the lookups: a shard
/// holds 512 buckets, and a chunk holds every transaction inside it.
///
/// Runs once per frame per instance, so it is a sort and a dedup rather than a
/// scan of the output per lookup — the scan was quadratic and cost 20 ms a call
/// at the size of a single wallet run.
pub fn reads(lookups: &[Lookup]) -> Vec<Read> {
    let mut runs: Vec<Read> = Vec::with_capacity(lookups.len());
    let mut bodies: Vec<Read> = Vec::with_capacity(lookups.len());
    for lookup in lookups {
        runs.push(Read {
            to: ReadTo::Run {
                shard: lookup.shard,
            },
            from_bucket: lookup.bucket,
            at: lookup.at,
            flight: Some(lookup.flight),
        });
        bodies.push(Read {
            to: ReadTo::Body { span: lookup.span },
            from_bucket: lookup.bucket,
            at: lookup.at,
            flight: Some(lookup.flight),
        });
    }

    // Sort by the object and then by time, so the first of each run of equal
    // objects is the earliest lookup that needed it — which is the one whose
    // bucket the wire starts from.
    runs.sort_by(|a, b| shard_of(a).cmp(&shard_of(b)).then(a.at.total_cmp(&b.at)));
    runs.dedup_by_key(|read| shard_of(read));
    bodies.sort_by(|a, b| chunk_of(a).cmp(&chunk_of(b)).then(a.at.total_cmp(&b.at)));
    bodies.dedup_by_key(|read| chunk_of(read));

    let mut out = runs;
    out.append(&mut bodies);
    out.sort_by(|a, b| a.at.total_cmp(&b.at).then_with(|| a.to.cmp(&b.to)));
    out
}

/// The object a run read is keyed by.
fn shard_of(read: &Read) -> u32 {
    match read.to {
        ReadTo::Run { shard } => shard,
        ReadTo::Index | ReadTo::Body { .. } => 0,
    }
}

/// The object a body read is keyed by — one read serves every transaction the
/// chunk holds.
fn chunk_of(read: &Read) -> u16 {
    match read.to {
        ReadTo::Body { span } => span.chunk,
        ReadTo::Index | ReadTo::Run { .. } => 0,
    }
}

// ============================================================================
// Time — every mark is a pure function of it
// ============================================================================

/// The beats of a lookup, as fractions of its flight: when each of its marks
/// arrives.
///
/// These are **drawing constants, not measurements**. What a read costs is
/// `flight`, which the caller measured; how the flight is split across the
/// drawing is not a claim about the world.
const BEAT_BUCKET: f32 = 0.08;
const BEAT_SHARD: f32 = 0.20;
const BEAT_RUN: f32 = 0.32;
const BEAT_BODY_READ: f32 = 0.66;
const BEAT_SETTLE: f32 = 0.84;

/// How long a WIRE outlives its request, in seconds.
///
/// ⚠️ Short on purpose. A wire is a request in progress and should go when the request
/// does; the pile of wires that outlived their requests is what turned a run into a
/// hatch. What stays is the mark below.
const LINGER: f32 = 0.55;

/// How long the MARKS a run leaves stay: the pip where a read landed, and a lookup's own
/// marks once the run has reported.
///
/// Long on purpose — a mark is *evidence*, and the pile a run builds is the picture. A
/// run whose evidence faded as fast as its wires left nothing to look at, and nothing
/// that grew.
const EVIDENCE: f32 = 2.6;

/// How long an ant takes to cross to the far end, in seconds.
///
/// ⚠️ **A drawing constant, and the one thing in this file that is not a measurement.**
/// The browser does not know how long a read will take until it is back, so an ant's
/// position cannot be a position: it is *how long that ant has been out*, at a fixed
/// speed. What is measured is the **flash** — the instant the read lands — and the ant
/// is gone by then. An ant that has reached the far end and is waiting there is a read
/// that is taking a while, which is true.
const OUT: f32 = 0.15;

/// The resolve flash: how long it lasts, and how far it spreads, in seconds and points.
const FLASH: f32 = 0.22;
const FLASH_TO: f32 = 9.0;

/// How much of a beat it takes a mark to arrive.
const RISE: f32 = 0.06;

/// How long a read that has not landed takes to reach full strength, in seconds.
///
/// A read in the air has no flight to scale its arrival by — that is the point of it —
/// so it rises over a fixed, short beat instead of popping in.
const RISE_SECS: f32 = 0.09;

/// Seconds since `at`, while the mark is still on screen — `None` before it starts and
/// once `hold` seconds have passed since it landed.
///
/// `hold` is the whole difference between a wire and the evidence it leaves: both are
/// pure functions of age and disagree only about how long the mark is worth showing.
fn age_of(at: f32, flight: Option<f32>, now: f32, hold: f32) -> Option<f32> {
    let age = now - at;
    if age < 0.0 {
        return None;
    }
    match flight {
        Some(flight) if age > flight + hold => None,
        _ => Some(age),
    }
}

/// How far through its flight something is, `0.0`..=`1.0`.
///
/// A read that has not landed is drawn at the START of its wire: where the *data* is is
/// not known yet. The **ant** is a different question — see [`OUT`].
fn progress_of(at: f32, flight: Option<f32>, now: f32, hold: f32) -> Option<f32> {
    let age = age_of(at, flight, now, hold)?;
    Some(match flight {
        None => 0.0,
        Some(flight) if flight <= 0.0 => 1.0,
        Some(flight) => (age / flight).clamp(0.0, 1.0),
    })
}

/// How strongly something is inked: it arrives over the first [`RISE`] of its flight,
/// holds, and fades over `hold` seconds after landing. A read still in the air does not
/// fade — it is not finished, it is happening.
fn ink_of(at: f32, flight: Option<f32>, now: f32, hold: f32) -> f32 {
    let Some(age) = age_of(at, flight, now, hold) else {
        return 0.0;
    };
    let rise = match flight {
        // Instantaneous: it is already over, so it is already at full strength.
        Some(flight) if flight <= 0.0 => 1.0,
        Some(flight) => (age / (flight * RISE)).clamp(0.0, 1.0),
        // In the air: no flight to scale by, so a fixed short beat instead of a pop.
        None => (age / RISE_SECS).clamp(0.0, 1.0),
    };
    let fall = match flight {
        None => 1.0,
        Some(flight) => ((flight + hold - age) / hold).clamp(0.0, 1.0),
    };
    rise.min(fall)
}

/// How much of a beat has passed: `0.0` before it, `1.0` once it has arrived.
fn arrived(progress: f32, beat: f32) -> f32 {
    ((progress - beat) / RISE).clamp(0.0, 1.0)
}

// ============================================================================
// Placement
// ============================================================================

/// Where `index` of `count` sits on a vertical band, top to bottom.
fn y_of(index: u32, count: u32, band: (f32, f32)) -> f32 {
    let (top, bottom) = band;
    if count <= 1 {
        return top;
    }
    let f = (index.min(count - 1) as f32 / (count - 1) as f32).clamp(0.0, 1.0);
    top + (bottom - top) * f
}

/// The byte axis inside a chunk runs from one byte to this many, log —
/// `u32::MAX` is 4 GiB, and a Cardano chunk is cut well inside that.
const SPAN_BYTES: f32 = 64.0 * 1024.0 * 1024.0;

/// Where a byte offset sits inside the chunk band, log. Byte 0 is the left end.
fn offset_at(offset: u32, band: (f32, f32)) -> f32 {
    let (left, right) = band;
    let f = ((offset.max(1) as f32).log2() / SPAN_BYTES.log2()).clamp(0.0, 1.0);
    left + (right - left) * f
}

/// How wide a span of `len` bytes is drawn on that axis — never narrower than a
/// mark, because a 156-byte body at the far end of a log axis is still read off
/// the network.
fn len_at(len: u16, band: (f32, f32)) -> f32 {
    let (left, right) = band;
    let f = ((len.max(1) as f32).log2() / SPAN_BYTES.log2()).clamp(0.0, 1.0);
    ((right - left) * f).max(MIN_MARK)
}

/// The narrowest a mark is drawn, in points. A pixel dimension, not type.
// theme-exempt: a mark's floor, not a text size
const MIN_MARK: f32 = 2.0;

/// Rails and bands, as fractions of the usable width.
///
/// The first three are ONE address space drawn three ways: a bucket, its shard, and the
/// bucket's slot in the index. `512 × 32768` is `1 << 24`, so a bucket and its shard sit
/// at the **same height** — which is why every wire among them is short and horizontal,
/// and why the address space reads as one column rather than three.
const RAIL_BUCKET: f32 = 0.06;
const RAIL_SHARD: f32 = 0.20;
const RAIL_ENTRY: f32 = 0.36;
/// Position, not hash: the only axis a read can cross to.
const RAIL_CORPUS: f32 = 0.66;
const BAND_SPAN: (f32, f32) = (0.80, 0.99);

/// Where the rails are this frame, resolved once.
struct Rails {
    bucket: f32,
    shard: f32,
    entry: f32,
    corpus: f32,
}

/// Where a read's wire starts and ends.
///
/// ⚠️ **Each kind advances the pipeline by ONE gap.** The index is read to learn which
/// shard holds a bucket, the shard is read to get the bucket's entries, and only the
/// **body** read crosses to the corpus. The first shape drew all three as full-width
/// chords, and a random permutation drawn that way is a hatched rectangle: 850 wires
/// each spanning the canvas at an unrelated angle, saying nothing the two ends were not
/// already saying. A wire one gap wide is legible at four times the count.
fn wire_ends(
    to: ReadTo,
    from_bucket: u32,
    extent: Extent,
    rails: &Rails,
    band: (f32, f32),
) -> (Pos2, Pos2) {
    let bucket = y_of(from_bucket, extent.buckets, band);
    match to {
        // Find the shard. It is the same address at the same height, so this wire is a
        // horizontal one: a lookup that narrowed the hash has not moved.
        ReadTo::Index => (pos2(rails.bucket, bucket), pos2(rails.shard, bucket)),
        // Fetch the bucket's entries out of that shard, and deliver them to the index's
        // own edge.
        ReadTo::Run { shard } => (
            pos2(rails.shard, y_of(shard, extent.shards, band)),
            pos2(rails.entry, bucket),
        ),
        // The crossing: a chunk, a byte offset and a length, on an axis that is not the
        // hash's and never will be.
        ReadTo::Body { span } => (
            pos2(rails.entry, bucket),
            pos2(rails.corpus, y_of(span.chunk as u32, extent.chunks, band)),
        ),
    }
}

/// Cells a twenty-four-byte entry is drawn as, and how many of them are hash.
const ENTRY_CELLS: u32 = 24;
const ENTRY_HASH_CELLS: u32 = 8;

/// A quadratic curve from `from` to `to`, bowed perpendicular by `bow` points.
fn curve(from: Pos2, to: Pos2, bow: f32, steps: usize) -> Vec<Pos2> {
    let d = to - from;
    let mid = from + d * 0.5;
    let ctrl = if d.length() < 0.5 {
        mid
    } else {
        mid + vec2(-d.y, d.x).normalized() * bow
    };
    let (a, b, c) = (from.to_vec2(), ctrl.to_vec2(), to.to_vec2());
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            (a * (u * u) + b * (2.0 * u * t) + c * (t * t)).to_pos2()
        })
        .collect()
}

// ============================================================================
// The widget
// ============================================================================

/// The locator, drawn as the wires a run of lookups actually fires.
///
/// Build one per frame from the lookups still in the air and the caller's own
/// clock; there is no state to keep between frames.
pub struct LocatorGraph<'a> {
    lookups: &'a [Lookup],
    /// The reads the host observed, when it has them — see [`LocatorGraph::fired`].
    fired: Option<&'a [Read]>,
    extent: Extent,
    now: f32,
    size: Option<Vec2>,
}

impl<'a> LocatorGraph<'a> {
    /// The lookups to draw, and the caller's clock in seconds — the same clock
    /// their `at` and `flight` are on.
    pub fn new(lookups: &'a [Lookup], now: f32) -> Self {
        Self {
            lookups,
            fired: None,
            extent: Extent::default(),
            now,
            size: None,
        }
    }

    /// The size of both structures, where the caller knows it better than the
    /// published build.
    pub fn extent(mut self, extent: Extent) -> Self {
        self.extent = extent;
        self
    }

    /// An exact allocation instead of the whole available rect — for a story, or
    /// for a band of a surface.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// Whether anything is still in the air, so the host knows to keep painting.
    ///
    /// ⚠️ Both sources count. A run in progress has **fired reads and no lookups
    /// yet** — the lookups are only knowable once the entry runs have come back —
    /// so a `busy()` that only asked the lookups would let the host stop painting
    /// exactly while the run was happening.
    pub fn busy(&self) -> bool {
        let lookups = self
            .lookups
            .iter()
            .any(|lookup| age_of(lookup.at, Some(lookup.flight), self.now, EVIDENCE).is_some());
        let fired = self
            .fired
            .unwrap_or_default()
            .iter()
            .any(|read| age_of(read.at, read.flight, self.now, EVIDENCE).is_some());
        lookups || fired
    }

    /// The wires that actually went out, in the order they did — the reads the
    /// host observed, rather than the ones its lookups imply.
    ///
    /// ⚠️ **Supply this when the host can see the run happening.** `reads()`
    /// derives a wire set from finished lookups, which is all a caller has once a
    /// run is over; a caller that watched the requests go out should hand those
    /// over instead, because a derived superset is a picture of what the plan
    /// implied rather than of what the browser did — and it cannot show a run that
    /// has not finished yet. A read still in the air is one with `flight: None`.
    pub fn fired(mut self, reads: &'a [Read]) -> Self {
        self.fired = Some(reads);
        self
    }

    /// Draw it. Nothing is interactive, so the response is for layout only.
    pub fn show(self, ui: &mut Ui) -> Response {
        let size = self.size.unwrap_or_else(|| ui.available_size());
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if !ui.is_rect_visible(rect) || rect.width() < 1.0 || rect.height() < 1.0 {
            return response;
        }
        if self.busy() {
            ui.ctx().request_repaint();
        }
        let theme = ui.tokens();
        self.paint(&ui.painter_at(rect), &theme, rect);
        response
    }

    fn paint(&self, painter: &Painter, theme: &Theme, rect: Rect) {
        let c = &theme.color;
        let pad = theme.space(Space::Sm);
        let x0 = rect.left() + pad;
        let width = (rect.width() - pad * 2.0).max(1.0);
        let band = (rect.top() + pad, rect.bottom() - pad);
        let at = |f: f32| x0 + width * f;
        let span_band = (at(BAND_SPAN.0), at(BAND_SPAN.1));

        self.rails(painter, theme, at, band);

        // The wires. Given the observed reads, those are the wires — a derived
        // superset would be a picture of what the plan implied rather than of what
        // the browser did.
        let derived;
        let wires: &[Read] = match self.fired {
            Some(fired) => fired,
            None => {
                derived = reads(self.lookups);
                &derived
            }
        };
        let rails = Rails {
            bucket: at(RAIL_BUCKET),
            shard: at(RAIL_SHARD),
            entry: at(RAIL_ENTRY),
            corpus: at(RAIL_CORPUS),
        };
        for read in wires {
            let Some(age) = age_of(read.at, read.flight, self.now, EVIDENCE) else {
                continue;
            };
            let (from, to) = wire_ends(read.to, read.from_bucket, self.extent, &rails, band);
            let colour = match read.to {
                ReadTo::Index => c.accent_yellow,
                ReadTo::Run { .. } => c.accent_cyan,
                ReadTo::Body { .. } => c.accent_blue,
            };
            // A wire whose two ends are at the SAME height is bowed instead of drawn flat:
            // the index's three rails are one address space, so most wires are like that,
            // and a band of flat rules reads as a hatch where a band of shallow arcs reads
            // as a band.
            let drop = to.y - from.y;
            let bow = if drop.abs() < 1.0 {
                (to.x - from.x) * 0.18
            } else {
                drop * 0.18
            };
            let points = curve(from, to, bow, 8);

            match read.flight {
                // Out. The wire goes dim with its far end outlined, and an ant is on it.
                //
                // ⚠️ **The ant's position is the one thing here that is a drawing rather
                // than a measurement** — see [`OUT`]. Its speed is fixed, so an ant that
                // has reached the hive and is waiting there is a read that is taking a
                // while. The instant it lands is the flash below, and that is measured.
                None => {
                    let ink = (age / RISE_SECS).clamp(0.0, 1.0);
                    painter.add(Shape::line(
                        points.clone(),
                        stroke(1.0, with_alpha(colour, (55.0 * ink) as u8)),
                    ));
                    painter.circle_stroke(
                        to,
                        2.0,
                        stroke(1.0, with_alpha(colour, (170.0 * ink) as u8)),
                    );
                    if let Some(ant) = sample(&points, (age / OUT).clamp(0.0, 1.0)) {
                        painter.circle_filled(ant, 1.5, with_alpha(colour, (235.0 * ink) as u8));
                    }
                }
                // Back. The wire holds its strength for [`LINGER`] and then goes — a
                // wire that outlived its request is a wire after the fact, and 850 of
                // those is the hatch this replaced. What stays is the pip, for
                // [`EVIDENCE`], which is the pile the run is building.
                Some(flight) => {
                    let wire = ink_of(read.at, read.flight, self.now, LINGER);
                    if wire > 0.0 {
                        painter.add(Shape::line(
                            points.clone(),
                            stroke(1.0, with_alpha(colour, (150.0 * wire) as u8)),
                        ));
                    }
                    let since = age - flight;
                    if since < FLASH {
                        let t = (since / FLASH).clamp(0.0, 1.0);
                        painter.circle_stroke(
                            to,
                            2.0 + FLASH_TO * t,
                            stroke(1.0, with_alpha(colour, (240.0 * (1.0 - t)) as u8)),
                        );
                    }
                    let pip = ink_of(read.at, read.flight, self.now, EVIDENCE);
                    if pip > 0.0 {
                        painter.circle_filled(to, 1.6, with_alpha(colour, (210.0 * pip) as u8));
                    }
                }
            }
        }

        for lookup in self.lookups {
            let Some(progress) = progress_of(lookup.at, Some(lookup.flight), self.now, EVIDENCE)
            else {
                continue;
            };
            let ink = ink_of(lookup.at, Some(lookup.flight), self.now, EVIDENCE);
            if ink <= 0.0 {
                continue;
            }
            self.lookup(painter, theme, lookup, progress, ink, at, band, span_band);
        }
    }

    /// The rails the marks sit between: the hash space, the shard space, the
    /// index's own edge, the corpus, and the byte axis inside a chunk.
    fn rails(&self, painter: &Painter, theme: &Theme, at: impl Fn(f32) -> f32, band: (f32, f32)) {
        let c = &theme.color;
        let (top, bottom) = band;
        for (x, alpha) in [
            (at(RAIL_BUCKET), 70u8),
            (at(RAIL_SHARD), 70),
            (at(RAIL_ENTRY), 110),
            (at(RAIL_CORPUS), 110),
        ] {
            painter.line_segment(
                [pos2(x, top), pos2(x, bottom)],
                hairline(with_alpha(c.border, alpha)),
            );
        }
        let (left, right) = (at(BAND_SPAN.0), at(BAND_SPAN.1));
        painter.line_segment(
            [pos2(left, bottom), pos2(right, bottom)],
            hairline(with_alpha(c.border, 60)),
        );
    }

    /// One lookup's own marks, in the order the lookup passes them.
    #[allow(clippy::too_many_arguments)]
    fn lookup(
        &self,
        painter: &Painter,
        theme: &Theme,
        lookup: &Lookup,
        progress: f32,
        ink: f32,
        at: impl Fn(f32) -> f32,
        band: (f32, f32),
        span_band: (f32, f32),
    ) {
        let c = &theme.color;
        let fade = |base: f32| (base * ink) as u8;
        let bucket_y = y_of(lookup.bucket, self.extent.buckets, band);
        let shard_y = y_of(lookup.shard, self.extent.shards, band);
        let chunk_y = y_of(lookup.span.chunk as u32, self.extent.chunks, band);
        let rail = at(RAIL_ENTRY);

        // The hash narrows: the bucket it lands in, then the shard holding it.
        let bucket = arrived(progress, BEAT_BUCKET);
        if bucket > 0.0 {
            let tick = at(RAIL_BUCKET);
            painter.line_segment(
                [pos2(tick - 3.0, bucket_y), pos2(tick + 3.0, bucket_y)],
                stroke(1.0, with_alpha(c.text_primary, fade(200.0 * bucket))),
            );
        }
        let shard = arrived(progress, BEAT_SHARD);
        if shard > 0.0 {
            let tick = at(RAIL_SHARD);
            painter.line_segment(
                [pos2(tick - 3.0, shard_y), pos2(tick + 3.0, shard_y)],
                stroke(1.0, with_alpha(c.accent_cyan, fade(190.0 * shard))),
            );
            painter.line_segment(
                [
                    pos2(at(RAIL_BUCKET) + 3.0, bucket_y),
                    pos2(tick - 3.0, shard_y),
                ],
                stroke(1.0, with_alpha(c.accent_cyan, fade(70.0))),
            );
        }

        // The bucket's run: how many entries it has to look through, and the
        // entry itself — a third of it is hash.
        let run = arrived(progress, BEAT_RUN);
        if run > 0.0 {
            let from = at(RAIL_SHARD) + 5.0;
            let run_len = (lookup.entries.max(1) as f32).log2() * 1.6 + 2.0;
            painter.line_segment(
                [pos2(from, shard_y), pos2(from + run_len, shard_y)],
                stroke(1.0, with_alpha(c.accent_cyan, fade(150.0 * run))),
            );
            painter.line_segment(
                [pos2(from, shard_y), pos2(rail, bucket_y)],
                stroke(1.0, with_alpha(c.text_muted, fade(60.0))),
            );
        }

        let entry = arrived(progress, BEAT_SETTLE);
        let entry_w = 14.0;
        let cells = ENTRY_CELLS as f32;
        painter.line_segment(
            [
                pos2(rail, bucket_y),
                pos2(rail + entry_w * (ENTRY_HASH_CELLS as f32 / cells), bucket_y),
            ],
            stroke(2.0, with_alpha(c.accent_cyan, fade(200.0 * entry.max(0.2)))),
        );
        painter.line_segment(
            [
                pos2(rail + entry_w * (ENTRY_HASH_CELLS as f32 / cells), bucket_y),
                pos2(rail + entry_w, bucket_y),
            ],
            stroke(2.0, with_alpha(c.border, fade(170.0))),
        );

        // The body, on the byte axis inside its chunk — and how much of it came
        // off the wire.
        let body = arrived(progress, BEAT_BODY_READ);
        if body > 0.0 {
            painter.line_segment(
                [
                    pos2(at(RAIL_CORPUS), chunk_y),
                    pos2(at(RAIL_CORPUS) + 6.0, chunk_y),
                ],
                stroke(1.0, with_alpha(c.accent_blue, fade(180.0 * body))),
            );
            let left = offset_at(lookup.span.offset, span_band);
            let width = len_at(lookup.span.len, span_band);
            painter.line_segment(
                [pos2(left, chunk_y), pos2(left + width, chunk_y)],
                stroke(2.0, with_alpha(c.accent_blue, fade(210.0 * body))),
            );
        }

        // The re-hash: back across the gap to the entry, green when the body
        // reproduced the hash and red when it did not.
        let settle = arrived(progress, BEAT_SETTLE);
        if settle > 0.0 {
            let colour = if lookup.settled { c.success } else { c.error };
            let from = pos2(offset_at(lookup.span.offset, span_band), chunk_y);
            let to = pos2(rail + entry_w * 0.5, bucket_y);
            painter.add(Shape::line(
                curve(from, to, -(from.y - to.y) * 0.22, 8),
                stroke(1.0, with_alpha(colour, fade(160.0 * settle))),
            ));
        }
    }
}

/// The point `t` of the way along a polyline.
fn sample(points: &[Pos2], t: f32) -> Option<Pos2> {
    if points.len() < 2 {
        return None;
    }
    let f = (t.clamp(0.0, 1.0) * (points.len() - 1) as f32).clamp(0.0, f32::MAX);
    let i = (f.floor() as usize).min(points.len() - 2);
    let local = f - i as f32;
    Some(points[i] + (points[i + 1] - points[i]) * local)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(bucket: u32, shard: u32, chunk: u16, at: f32) -> Lookup {
        Lookup {
            bucket,
            shard,
            entries: 6,
            span: Span {
                chunk,
                offset: 3_426_006,
                len: 156,
            },
            settled: true,
            at,
            flight: 1.0,
        }
    }

    #[test]
    #[ignore = "timing, not correctness — run with --ignored --nocapture"]
    fn reads_stays_cheap_at_the_scale_of_a_run() {
        // 729 transactions, spread over the real shard and chunk spaces, so the
        // dedup has real work to do rather than collapsing to one object.
        let roster: Vec<Lookup> = (0..729u32)
            .map(|i| {
                let bucket = (i.wrapping_mul(2_654_435_761)) % (1 << 24);
                Lookup {
                    bucket,
                    shard: bucket / 512,
                    entries: 6,
                    span: Span {
                        chunk: (i % 9_203) as u16,
                        offset: 1 << 20,
                        len: 200,
                    },
                    settled: true,
                    at: i as f32 * 0.0017,
                    flight: 0.9,
                }
            })
            .collect();

        let rounds = 20;
        let started = std::time::Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(reads(&roster));
        }
        let per_call = started.elapsed() / rounds;
        println!(
            "reads() over {} lookups   {:>10.2?}/call   {} reads",
            roster.len(),
            per_call,
            reads(&roster).len()
        );
    }

    #[test]
    fn a_bucket_sits_where_its_address_says_and_nowhere_else() {
        let band = (0.0, 100.0);
        assert_eq!(y_of(0, 1 << 24, band), 0.0);
        assert_eq!(y_of((1 << 24) - 1, 1 << 24, band), 100.0);
        // Monotone, so two buckets never swap places between frames.
        let mut previous = -1.0;
        for bucket in (0..1 << 24).step_by(97_531) {
            let y = y_of(bucket, 1 << 24, band);
            assert!(y > previous, "bucket {bucket} moved backwards");
            previous = y;
        }
        // A count with one place is a place, not a division by zero.
        assert_eq!(y_of(0, 1, band), 0.0);
        // Past the end is the end, never off the band.
        assert_eq!(y_of(1 << 25, 1 << 24, band), 100.0);
    }

    #[test]
    fn one_object_is_one_wire_however_many_lookups_land_in_it() {
        // Three transactions, two of them in one chunk, none sharing a shard.
        let lookups = [
            lookup(10, 0, 900, 0.0),
            lookup(20, 1, 900, 0.1),
            lookup(30, 2, 910, 0.2),
        ];
        let reads = reads(&lookups);
        let runs = reads
            .iter()
            .filter(|r| matches!(r.to, ReadTo::Run { .. }))
            .count();
        let bodies = reads
            .iter()
            .filter(|r| matches!(r.to, ReadTo::Body { .. }))
            .count();
        assert_eq!(runs, 3, "three shards, three reads");
        assert_eq!(bodies, 2, "two chunks hold the three transactions");
    }

    #[test]
    fn a_read_is_stamped_with_the_first_lookup_that_needed_it() {
        let lookups = [lookup(10, 4, 900, 2.0), lookup(11, 4, 900, 0.5)];
        let reads = reads(&lookups);
        assert_eq!(reads.len(), 2);
        assert!(
            reads.iter().all(|r| (r.at - 0.5).abs() < f32::EPSILON),
            "a shared object is read once, when it was first asked for"
        );
        assert_eq!(reads[0].from_bucket, 11);
    }

    #[test]
    fn a_bucket_and_its_shard_are_drawn_at_the_same_height() {
        // `512 × 32768 == 1 << 24`, so the bucket axis and the shard axis are the same
        // space at two resolutions. That is what makes the index's three rails one
        // column, and every wire among them a short horizontal one.
        let extent = Extent::default();
        assert_eq!(
            u64::from(Extent::BUCKETS_PER_SHARD) * u64::from(extent.shards),
            u64::from(extent.buckets),
            "the axes have drifted apart"
        );
        let band = (0.0, 1000.0);
        for bucket in [0u32, 7, 999, 1 << 20, (1 << 24) - 1] {
            let at_bucket = y_of(bucket, extent.buckets, band);
            let at_shard = y_of(bucket / Extent::BUCKETS_PER_SHARD, extent.shards, band);
            assert!(
                (at_bucket - at_shard).abs() < 0.2,
                "bucket {bucket} is at {at_bucket} but its shard is at {at_shard}"
            );
        }
    }

    #[test]
    fn every_wire_advances_the_pipeline_by_one_gap() {
        let extent = Extent::default();
        let rails = Rails {
            bucket: 10.0,
            shard: 40.0,
            entry: 70.0,
            corpus: 130.0,
        };
        let band = (0.0, 100.0);
        let bucket = 64;
        let ends = |to| wire_ends(to, bucket, extent, &rails, band);

        // Find the shard, fetch the entries, then cross — one gap each, in order.
        let (from, to) = ends(ReadTo::Index);
        assert_eq!((from.x, to.x), (10.0, 40.0));
        assert_eq!(from.y, to.y, "narrowing the hash does not move it");

        let (from, to) = ends(ReadTo::Run { shard: 0 });
        assert_eq!((from.x, to.x), (40.0, 70.0));

        let (from, to) = ends(ReadTo::Body {
            span: Span {
                chunk: 9_150,
                offset: 0,
                len: 156,
            },
        });
        assert_eq!((from.x, to.x), (70.0, 130.0));
        // The body is the read that leaves the index: its two ends are the only pair
        // that are not the same address space.
        assert_ne!(from.y, to.y, "the corpus is not ordered by hash");
    }

    #[test]
    fn a_lookup_that_has_not_started_draws_nothing() {
        let l = lookup(1, 0, 1, 5.0);
        assert_eq!(age_of(l.at, Some(l.flight), 4.9, LINGER), None);
        assert_eq!(progress_of(l.at, Some(l.flight), 4.9, LINGER), None);
        assert_eq!(ink_of(l.at, Some(l.flight), 4.9, LINGER), 0.0);
    }

    #[test]
    fn a_flight_that_is_over_lingers_then_goes() {
        let l = lookup(1, 0, 1, 0.0);
        let during = ink_of(l.at, Some(l.flight), 0.5, LINGER);
        let after = ink_of(l.at, Some(l.flight), l.flight + LINGER * 0.5, LINGER);
        let gone = ink_of(l.at, Some(l.flight), l.flight + LINGER + 0.01, LINGER);
        assert!(during > after, "it must be fading, not holding");
        assert!(
            after > 0.0,
            "a mark that vanishes mid-flight reads as a gap"
        );
        assert_eq!(gone, 0.0);
        assert_eq!(
            progress_of(l.at, Some(l.flight), l.flight + LINGER, LINGER),
            Some(1.0)
        );
    }

    #[test]
    fn a_zero_flight_read_is_arrived_rather_than_undefined() {
        let l = lookup(1, 0, 1, 0.0);
        assert_eq!(progress_of(l.at, Some(0.0), 0.0, LINGER), Some(1.0));
        assert_eq!(ink_of(l.at, Some(0.0), 0.0, LINGER), 1.0);
        assert_eq!(progress_of(l.at, Some(0.0), LINGER + 0.01, LINGER), None);
    }

    #[test]
    fn the_pile_outlives_the_wire_it_came_from() {
        // A wire is a request in progress and goes when the request does. What stays is
        // the mark where it landed — and it stays long enough that a run's evidence
        // accumulates instead of fading as fast as the traffic did.
        let read = Read {
            to: ReadTo::Index,
            from_bucket: 64,
            at: 0.0,
            flight: Some(0.4),
        };
        let after_the_wire = read.at + 0.4 + LINGER + 0.05;
        assert_eq!(ink_of(read.at, read.flight, after_the_wire, LINGER), 0.0);
        assert!(ink_of(read.at, read.flight, after_the_wire, EVIDENCE) > 0.0);
        assert_eq!(
            ink_of(
                read.at,
                read.flight,
                read.at + 0.4 + EVIDENCE + 0.01,
                EVIDENCE
            ),
            0.0
        );
    }

    #[test]
    fn a_read_still_in_the_air_is_not_an_instant_read() {
        // `None` is "not landed yet", which is not the same as "took no time": a
        // packet parked at the END of a wire would say the read is already over.
        assert_eq!(progress_of(0.0, None, 0.4, LINGER), Some(0.0));
        assert_eq!(ink_of(0.0, None, 0.4, LINGER), 1.0);
        // And a read in the air does not fade — it is happening, not finishing.
        assert_eq!(ink_of(0.0, None, LINGER * 4.0, LINGER), 1.0);
        // Once it lands, the flight it reports is the flight it gets.
        assert_eq!(progress_of(0.0, Some(1.0), 0.5, LINGER), Some(0.5));
    }

    #[test]
    fn an_ant_crosses_at_a_drawing_speed_and_never_past_the_hive() {
        // The ant's position is the ONE thing here that is animation rather than
        // measurement, so it is worth pinning what it can and cannot say: it is out
        // after one `OUT`, it is still exactly at the hive after ten, and it never
        // overshoots into an address it has no business claiming.
        let at = |age: f32| (age / OUT).clamp(0.0, 1.0);
        assert_eq!(
            at(0.0),
            0.0,
            "an ant is at the dispatch the instant it goes"
        );
        assert_eq!(at(OUT), 1.0);
        assert_eq!(at(OUT * 10.0), 1.0, "a slow read is an ant waiting");
    }

    #[test]
    fn a_run_in_progress_is_busy_before_any_lookup_exists() {
        // The reason the fired feed exists: mid-run there are wires and NO
        // lookups — the lookups are only knowable once the entry runs come back —
        // so a `busy()` that asked only the lookups would let the host stop
        // painting exactly while the run was happening.
        let fired = [Read {
            to: ReadTo::Index,
            from_bucket: 64,
            at: 0.0,
            flight: None,
        }];
        assert!(LocatorGraph::new(&[], 0.2).fired(&fired).busy());

        // ⚠️ A landed read keeps the host painting long after its wire has gone. The
        // wire holds for `LINGER` and the pip it left holds for `EVIDENCE`, and it is the
        // PIP that is on screen — stopping the paint here would freeze a mark mid-fade.
        let landed = [Read {
            flight: Some(0.3),
            ..fired[0]
        }];
        assert!(
            LocatorGraph::new(&[], 0.3 + LINGER + 0.01)
                .fired(&landed)
                .busy(),
            "the pile is still up"
        );

        // Landed and faded out is not busy.
        assert!(
            !LocatorGraph::new(&[], 0.3 + EVIDENCE + 0.01)
                .fired(&landed)
                .busy()
        );

        // With no fired feed it falls back to the lookups, as before.
        assert!(LocatorGraph::new(&[lookup(1, 0, 1, 0.0)], 0.5).busy());
    }

    #[test]
    fn the_byte_axis_is_the_offset_inside_its_chunk_not_the_file() {
        let band = (0.0, 100.0);
        assert_eq!(offset_at(0, band), 0.0, "byte zero is the left end");
        assert_eq!(offset_at(1, band), 0.0);
        // A transaction three mebibytes into its chunk sits past the middle of
        // the axis, because the axis is logarithmic and chunks are large.
        let at_3mib = offset_at(3 * 1024 * 1024, band);
        assert!(
            at_3mib > 55.0 && at_3mib < 100.0,
            "3 MiB landed at {at_3mib}"
        );
        // Past the axis is the end of the axis.
        assert_eq!(offset_at(1024 * 1024 * 1024, band), 100.0);
    }

    #[test]
    fn a_body_that_came_off_the_wire_is_never_drawn_as_nothing() {
        let band = (0.0, 100.0);
        assert!(len_at(156, band) >= MIN_MARK);
        assert!(len_at(1, band) >= MIN_MARK);
        // Longer bodies are drawn longer, monotonically, and never past the band.
        let mut previous = 0.0;
        for len in [64u16, 256, 4_096, 65_535] {
            let w = len_at(len, band);
            assert!(w > previous, "{len} bytes was not wider than the last");
            assert!(w <= 100.0);
            previous = w;
        }
    }

    #[test]
    fn an_entry_shows_a_third_of_its_bytes_and_that_is_the_hint() {
        let inked = ENTRY_HASH_CELLS as f32 / ENTRY_CELLS as f32;
        assert!(
            (inked - 1.0 / 3.0).abs() < 0.001,
            "an entry carries eight of twenty-four bytes, not {inked}"
        );
    }

    #[test]
    fn a_collision_is_two_lookups_leaving_one_bucket_for_two_chunks() {
        // The same bucket, two different bodies: what eight bytes of hash
        // cannot rule out on its own.
        let band = (0.0, 100.0);
        let a = lookup(64, 0, 9_150, 0.0);
        let b = lookup(64, 0, 4_400, 0.0);
        assert_eq!(y_of(a.bucket, 1 << 24, band), y_of(b.bucket, 1 << 24, band));
        assert_ne!(a.span.chunk, b.span.chunk);
    }

    #[test]
    fn a_dot_travels_the_wire_and_lands_on_both_ends() {
        let points = curve(pos2(0.0, 0.0), pos2(100.0, 40.0), 12.0, 12);
        assert_eq!(sample(&points, 0.0), Some(pos2(0.0, 0.0)));
        assert_eq!(sample(&points, 1.0), Some(pos2(100.0, 40.0)));
        let mid = sample(&points, 0.5).expect("a middle");
        assert!(mid.x > 0.0 && mid.x < 100.0);
        assert!(mid.y > 20.0, "the bow is not on the straight line");
        // A wire between two coincident points is a point, not a NaN.
        let dots = curve(pos2(5.0, 5.0), pos2(5.0, 5.0), 20.0, 4);
        assert!(dots.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }
}
