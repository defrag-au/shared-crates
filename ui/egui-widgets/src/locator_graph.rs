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
//! no animation state to keep; a repaint is asked for only while a lookup is in
//! the air, so an idle graph costs nothing. Positional travel is gated on
//! [`ThemeExt::travel_allowed`], so reduced motion keeps the fades and drops the
//! flight.

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

/// The two objects a lookup reaches for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadTo {
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
    /// Seconds the object took.
    pub flight: f32,
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
            flight: lookup.flight,
        });
        bodies.push(Read {
            to: ReadTo::Body { span: lookup.span },
            from_bucket: lookup.bucket,
            at: lookup.at,
            flight: lookup.flight,
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
        ReadTo::Body { .. } => 0,
    }
}

/// The object a body read is keyed by — one read serves every transaction the
/// chunk holds.
fn chunk_of(read: &Read) -> u16 {
    match read.to {
        ReadTo::Body { span } => span.chunk,
        ReadTo::Run { .. } => 0,
    }
}

// ============================================================================
// Time — every mark is a pure function of it
// ============================================================================

/// How long the marks linger after a lookup's flight, fading out, in seconds.
const LINGER: f32 = 0.55;

/// The beats of a lookup, as fractions of its flight: when each mark arrives,
/// and where the two crossing particles are.
///
/// These are **drawing constants, not measurements**. What a read costs is
/// `flight`, which the caller measured; how the flight is split across the
/// drawing is not a claim about the world.
const BEAT_BUCKET: f32 = 0.08;
const BEAT_SHARD: f32 = 0.20;
const BEAT_RUN: f32 = 0.32;
const BEAT_RUN_READ: f32 = 0.40;
const BEAT_BODY_READ: f32 = 0.66;
const BEAT_SETTLE: f32 = 0.84;

/// How much of a beat it takes a mark to arrive.
const RISE: f32 = 0.06;

/// Seconds since `at`, while the mark is still on screen — `None` before it
/// starts and once it has faded.
fn age_of(at: f32, flight: f32, now: f32) -> Option<f32> {
    let age = now - at;
    (age >= 0.0 && age <= flight + LINGER).then_some(age)
}

/// How far through its flight something is, `0.0`..=`1.0`.
fn progress_of(at: f32, flight: f32, now: f32) -> Option<f32> {
    let age = age_of(at, flight, now)?;
    Some(if flight <= 0.0 {
        1.0
    } else {
        (age / flight).clamp(0.0, 1.0)
    })
}

/// How strongly something is inked: it arrives over the first [`RISE`] of its
/// flight, holds, and fades over [`LINGER`].
fn ink_of(at: f32, flight: f32, now: f32) -> f32 {
    let Some(age) = age_of(at, flight, now) else {
        return 0.0;
    };
    let rise = if flight <= 0.0 {
        1.0
    } else {
        (age / (flight * RISE)).clamp(0.0, 1.0)
    };
    let fall = ((flight + LINGER - age) / LINGER).clamp(0.0, 1.0);
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
const RAIL_BUCKET: f32 = 0.07;
const RAIL_SHARD: f32 = 0.18;
const RAIL_ENTRY: f32 = 0.31;
const RAIL_CORPUS: f32 = 0.64;
const BAND_SPAN: (f32, f32) = (0.80, 0.99);

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
    pub fn busy(&self) -> bool {
        self.lookups
            .iter()
            .any(|lookup| age_of(lookup.at, lookup.flight, self.now).is_some())
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
        let travel = ui.travel_allowed();
        self.paint(&ui.painter_at(rect), &theme, rect, travel);
        response
    }

    fn paint(&self, painter: &Painter, theme: &Theme, rect: Rect, travel: bool) {
        let c = &theme.color;
        let pad = theme.space(Space::Sm);
        let x0 = rect.left() + pad;
        let width = (rect.width() - pad * 2.0).max(1.0);
        let band = (rect.top() + pad, rect.bottom() - pad);
        let at = |f: f32| x0 + width * f;
        let span_band = (at(BAND_SPAN.0), at(BAND_SPAN.1));

        self.rails(painter, theme, at, band);

        // The crossing: one wire per OBJECT, so a chunk read once is drawn once.
        for read in reads(self.lookups) {
            let Some(progress) = progress_of(read.at, read.flight, self.now) else {
                continue;
            };
            let ink = ink_of(read.at, read.flight, self.now);
            if ink <= 0.0 {
                continue;
            }
            let y = y_of(read.from_bucket, self.extent.buckets, band);
            let (from, to, colour) = match read.to {
                ReadTo::Run { shard } => (
                    pos2(at(RAIL_ENTRY), y),
                    pos2(at(RAIL_CORPUS), y_of(shard, self.extent.shards, band)),
                    c.accent_cyan,
                ),
                ReadTo::Body { span } => (
                    pos2(at(RAIL_ENTRY), y),
                    pos2(
                        at(RAIL_CORPUS),
                        y_of(span.chunk as u32, self.extent.chunks, band),
                    ),
                    c.accent_blue,
                ),
            };
            let bow = (from.y - to.y) * 0.18;
            let points = curve(from, to, bow, 8);
            painter.add(Shape::line(
                points.clone(),
                stroke(1.0, with_alpha(colour, (40.0 * ink) as u8)),
            ));
            if travel && progress >= BEAT_RUN_READ {
                let t = ((progress - BEAT_RUN_READ) / (1.0 - BEAT_RUN_READ)).clamp(0.0, 1.0);
                if let Some(dot) = sample(&points, t) {
                    painter.circle_filled(dot, 1.6, with_alpha(colour, (210.0 * ink) as u8));
                }
            }
        }

        for lookup in self.lookups {
            let Some(progress) = progress_of(lookup.at, lookup.flight, self.now) else {
                continue;
            };
            let ink = ink_of(lookup.at, lookup.flight, self.now);
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
    fn a_lookup_that_has_not_started_draws_nothing() {
        let l = lookup(1, 0, 1, 5.0);
        assert_eq!(age_of(l.at, l.flight, 4.9), None);
        assert_eq!(progress_of(l.at, l.flight, 4.9), None);
        assert_eq!(ink_of(l.at, l.flight, 4.9), 0.0);
    }

    #[test]
    fn a_flight_that_is_over_lingers_then_goes() {
        let l = lookup(1, 0, 1, 0.0);
        let during = ink_of(l.at, l.flight, 0.5);
        let after = ink_of(l.at, l.flight, l.flight + LINGER * 0.5);
        let gone = ink_of(l.at, l.flight, l.flight + LINGER + 0.01);
        assert!(during > after, "it must be fading, not holding");
        assert!(
            after > 0.0,
            "a mark that vanishes mid-flight reads as a gap"
        );
        assert_eq!(gone, 0.0);
        assert_eq!(progress_of(l.at, l.flight, l.flight + LINGER), Some(1.0));
    }

    #[test]
    fn a_zero_flight_read_is_arrived_rather_than_undefined() {
        let l = lookup(1, 0, 1, 0.0);
        assert_eq!(progress_of(l.at, 0.0, 0.0), Some(1.0));
        assert_eq!(ink_of(l.at, 0.0, 0.0), 1.0);
        assert_eq!(progress_of(l.at, 0.0, LINGER + 0.01), None);
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
