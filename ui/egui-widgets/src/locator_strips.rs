//! `LocatorStrips` — a run as heat on the two address spaces it touched, and as sparks of
//! the reads themselves going out.
//!
//! Three horizontal bands. The **hash** on top (the bucket a lookup lands in), the
//! **hive** below it (the chunk a body comes out of), and a hairline **when** strip under
//! both: one tick per read while it is in the air, at the moment it went out. Every read
//! lights its cell where it landed and decays, so a run draws itself as two histograms
//! and a firing line.
//!
//! ## The contrast is the finding
//!
//! The bands are drawn with the SAME mapping, so they can be compared, and what they
//! say is not symmetric:
//!
//! - **The hash side is flat.** A transaction hash is uniformly distributed by
//!   construction —
//!   that is what makes the head's histogram worth having — so a wallet's accesses
//!   spread evenly across the bucket space and the band fills in level.
//! - **The position side clusters.** A wallet's transactions happen in bursts of time,
//!   and time is chunk order. Measured on 2026-09-27's wallet arm: 729 transactions
//!   landed in **129 distinct chunks**, where a uniform spread would have touched ~729.
//!
//! So one band says "no information here" and the other says "this wallet lived between
//! there and there". That is the whole story of the artifact, and it is exactly what the
//! node-link graph this replaces could not draw: a relation with no shape can only be
//! drawn as density, and density is what the hash side already is.
//!
//! ## The strip is the requests that are out
//!
//! Its axis is *time* rather than address, and it holds one tick per read **while that
//! read is in the air**, at the instant it went out. The moment an answer lands its tick is
//! gone — no trail, no fade — so what is on the strip is exactly the set of requests
//! outstanding now. A block against the right edge is a burst being answered; a spark
//! alone and **pinned to the left edge** is a request that has been out for longer than
//! the whole window, which is what makes a slow or hung read visible without a word; and a
//! quiet reader has no strip at all rather than an empty one.
//!
//! ## Cost and discipline
//!
//! Everything painted is a pure function of `now` — a read's heat is `f(now - at)`, with
//! no accumulation state between frames — so a still is reproducible and a scrub is
//! honest. A repaint is asked for only while something on screen is still **moving**: a
//! request that has been out longer than [`WINDOW`] keeps its mark — it is still
//! outstanding, which is worth knowing — but the mark does not change, so the paint can
//! stop. The widget is drawn at full strength: fade it at the placement with
//! `Ui::set_opacity`.

use egui::{Color32, CornerRadius, Painter, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::locator_graph::{Extent, Read, ReadTo};
use crate::theme::{Space, Theme, ThemeExt, with_alpha};

/// How long a read's heat lasts, in seconds.
///
/// Long enough that a run leaves a visible pile and short enough that the bands clear
/// before the next one — the run itself is ~1.2 s.
const HOLD: f32 = 3.0;

/// How much of the recent past the strip shows, in seconds.
///
/// It is a rolling window rather than the run's own timeline: a live run has no end to
/// lay out against yet, and a band that had to be told the run's length would have to be
/// redrawn as that length changed. It is also the reach of the **whole** widget — nothing
/// is drawn for a read once it is older than this.
const WINDOW: f32 = 4.0;

/// How tall the strip is by default, in points: a hairline, not a band. A pixel
/// dimension, not type.
// theme-exempt: a band's height, not a text size
const WHEN_HEIGHT: f32 = 8.0;

/// How wide one spark is, in points. A mark's width, not type.
// theme-exempt: a mark's width, not a text size
const SPARK: f32 = 1.0;

/// How many reads one cell needs before it is at full strength.
///
/// The bands are compared with each other, so this is the *same* for both: a cell with
/// one read is dim, a cell with four is bright, and a cell with a dozen is solid. That
/// is what turns "the hash side saw 719 reads spread out" and "the position side saw 129
/// reads piled up" into two shapes that look different.
const SATURATE: f32 = 1.5;

/// A cell is about this many points wide, and there are never more than the cap — a
/// metre-wide window must not cost a cell per pixel.
const CELL: f32 = 2.0;
const MAX_CELLS: usize = 1024;

pub struct LocatorStrips<'a> {
    reads: &'a [Read],
    extent: Extent,
    now: f32,
    size: Option<Vec2>,
    /// How tall the strip is, in points.
    when_height: f32,
}

impl<'a> LocatorStrips<'a> {
    /// The reads to draw, and the caller's clock in seconds — the same clock their `at`
    /// and `flight` are on.
    pub fn new(reads: &'a [Read], now: f32) -> Self {
        Self {
            reads,
            extent: Extent::default(),
            now,
            size: None,
            when_height: WHEN_HEIGHT,
        }
    }

    /// The size of both address spaces, where the caller knows it better than the
    /// published build.
    pub fn extent(mut self, extent: Extent) -> Self {
        self.extent = extent;
        self
    }

    /// How tall the strip is. Below a few points it crowds the two address bands, so it
    /// is capped at a third of what there is.
    pub fn when_height(mut self, height: f32) -> Self {
        self.when_height = height;
        self
    }

    /// An exact allocation instead of the whole available rect.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// Whether anything on screen is still **moving**, so the host knows to keep painting.
    ///
    /// ⚠️ **Not the same question as "is there ink"**, and the one state where they differ
    /// is why: a request that has been out longer than [`WINDOW`] keeps its mark (see
    /// [`weight_of`]) but the mark does not move, so a frame showing it is finished and a
    /// host that kept painting would paint the same pixels forever. Everything else is
    /// bounded by the window, which is also this widget's whole reach.
    pub fn busy(&self) -> bool {
        self.reads
            .iter()
            .any(|read| self.now - read.at < WINDOW && weight_of(read, self.now) > 0.0)
    }

    /// Draw it. Nothing is interactive, so the response is for layout only.
    pub fn show(self, ui: &mut Ui) -> Response {
        let size = self.size.unwrap_or_else(|| ui.available_size());
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if !ui.is_rect_visible(rect) || rect.width() < 8.0 || rect.height() < 4.0 {
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
        let gap = theme.space(Space::Sm);
        // The strip is capped at a third of what there is, so it can never crowd the two
        // address bands out of the rect it was given.
        let thin = self
            .when_height
            .min(((rect.height() - gap * 2.0) / 3.0).max(0.0))
            .max(0.0);
        let band = ((rect.height() - gap * 2.0 - thin) / 2.0).max(1.0);
        let hash = Rect::from_min_size(rect.min, vec2(rect.width(), band));
        let hive = Rect::from_min_size(
            pos2(rect.left(), hash.bottom() + gap),
            vec2(rect.width(), band),
        );
        let when = Rect::from_min_size(
            pos2(rect.left(), hive.bottom() + gap),
            vec2(rect.width(), thin),
        );

        // The sparks, before the ink, because whether there are any decides whether the
        // strip exists at all.
        let sparks = sparks(self.reads, self.now, WINDOW);

        // The tracks, so an empty band still reads as an axis rather than as nothing —
        // all but the strip, which is ink or nothing.
        for track in [hash, hive] {
            if track.height() <= 0.0 {
                continue;
            }
            painter.rect_filled(track, CornerRadius::ZERO, with_alpha(c.border, 34));
        }

        let cells = ((rect.width() / CELL).round() as usize).clamp(1, MAX_CELLS);
        self.band(
            painter,
            hash,
            cells,
            c.accent_cyan,
            |read| Some(read.from_bucket),
            self.extent.buckets,
        );
        self.band(
            painter,
            hive,
            cells,
            c.accent_blue,
            chunk_of,
            self.extent.chunks,
        );
        // ⚠️ Only while there is something on it. A strip that is always drawn is a third
        // lane a reader has to work out is empty; one that arrives with the first request
        // of a run is activity, which is the whole of what it is for.
        if !sparks.is_empty() && when.height() > 0.0 {
            painter.rect_filled(when, CornerRadius::ZERO, with_alpha(c.border, 34));
            draw_sparks(painter, when, &sparks, c.accent_green);
        }
    }

    /// One band: every read that belongs to it, binned by address and drawn as the heat
    /// it still carries.
    fn band(
        &self,
        painter: &Painter,
        rect: Rect,
        cells: usize,
        colour: Color32,
        place: impl Fn(&Read) -> Option<u32>,
        span: u32,
    ) {
        let mut heat = vec![0.0f32; cells];
        for read in self.reads {
            let Some(address) = place(read) else {
                continue;
            };
            let weight = weight_of(read, self.now);
            if weight <= 0.0 {
                continue;
            }
            heat[cell_of(address, span, cells)] += weight;
        }

        let width = rect.width() / cells as f32;
        for (i, weight) in heat.iter().enumerate() {
            if *weight <= 0.0 {
                continue;
            }
            let level = 1.0 - (-*weight / SATURATE).exp();
            let left = rect.left() + width * i as f32;
            painter.rect_filled(
                Rect::from_min_size(pos2(left, rect.top()), vec2(width, rect.height())),
                CornerRadius::ZERO,
                with_alpha(colour, (level * 255.0) as u8),
            );
        }
    }
}

/// One tick of the strip per read that is **still in the air**, at the instant it went
/// out, as a fraction of the window: `0.0` at the left edge, `1.0` at the right.
///
/// A read is an event on this axis and an answered one is not: the tick goes the moment
/// the answer lands, which is what makes the strip the set of requests outstanding now
/// rather than a history of the last few seconds. **A request out for longer than the
/// window pins to the left edge** rather than leaving it: the axis cannot say how much
/// longer, and one request taking twelve seconds — the cold fence read of a build, measured
/// 2026-09-27 — is the most interesting thing the widget has to show, not something to
/// drop.
fn sparks(reads: &[Read], now: f32, window: f32) -> Vec<f32> {
    let start = now - window;
    reads
        .iter()
        .filter(|read| read.flight.is_none() && read.at <= now)
        .map(|read| ((read.at - start) / window).clamp(0.0, 1.0))
        .collect()
}

/// The strip: one hairline per outstanding request, where it was dispatched.
fn draw_sparks(painter: &Painter, rect: Rect, sparks: &[f32], colour: Color32) {
    for spark in sparks {
        let x = rect.left() + rect.width() * spark;
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(SPARK, colour),
        );
    }
}

/// Which cell of `cells` an address falls in, clamped to the band.
fn cell_of(address: u32, span: u32, cells: usize) -> usize {
    if span == 0 {
        return 0;
    }
    let f = (address as f64 / span as f64).clamp(0.0, 1.0);
    ((f * cells as f64) as usize).min(cells - 1)
}

/// The chunk a read took its body out of — the position side's address.
fn chunk_of(read: &Read) -> Option<u32> {
    match read.to {
        ReadTo::Body { span } => Some(u32::from(span.chunk)),
        // A read of the index or of a shard has no position: it is addressed by hash, and
        // drawing it on the position band would claim an address it does not have.
        ReadTo::Index | ReadTo::Run { .. } => None,
    }
}

/// How much of a read is still showing, `0.0`..=`1.0`.
///
/// A read still in the air is at full heat — it is happening *now* — and stays there for as
/// long as it is out, however long that is: an unanswered request is not something to age
/// off the screen, and a run waiting on one read is a run worth seeing. A landed one fades
/// over [`HOLD`] from the moment it went out, and a read the clock has not reached is not
/// showing at all.
fn weight_of(read: &Read, now: f32) -> f32 {
    let age = now - read.at;
    if age < 0.0 {
        return 0.0;
    }
    match read.flight {
        None => 1.0,
        Some(_) => (1.0 - age / HOLD).clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locator_graph::Span;

    fn run(bucket: u32, at: f32) -> Read {
        Read {
            to: ReadTo::Run {
                shard: bucket / 512,
            },
            from_bucket: bucket,
            at,
            flight: Some(0.1),
        }
    }

    fn body(chunk: u16, at: f32) -> Read {
        Read {
            to: ReadTo::Body {
                span: Span {
                    chunk,
                    offset: 0,
                    len: 156,
                },
            },
            from_bucket: 0,
            at,
            flight: Some(0.1),
        }
    }

    /// How much of a band a set of reads covers, `0.0`..=`1.0`.
    fn coverage(
        reads: &[Read],
        cells: usize,
        span: u32,
        place: impl Fn(&Read) -> Option<u32>,
    ) -> f32 {
        let mut heat = vec![0.0f32; cells];
        for read in reads {
            if let Some(address) = place(read) {
                heat[cell_of(address, span, cells)] += 1.0;
            }
        }
        heat.iter().filter(|w| **w > 0.0).count() as f32 / cells as f32
    }

    #[test]
    fn the_hash_side_is_flat_where_the_position_side_clusters() {
        // The whole reason there are two bands, and the claim they make: a hash spreads a
        // wallet evenly across the bucket space, and chain position does not. 729
        // transactions touched 129 chunks where uniform would have been ~729.
        let uniform: Vec<Read> = (0..729u32)
            .map(|i| run(i.wrapping_mul(2_654_435_761) % (1 << 24), 0.0))
            .collect();
        let clustered: Vec<Read> = (0..129u32)
            .map(|i| body(6_000 + (i % 12) as u16 * 20, 0.0))
            .collect();

        let hash = coverage(&uniform, 64, 1 << 24, |read| Some(read.from_bucket));
        let hive = coverage(&clustered, 64, 9_203, chunk_of);
        assert!(hash > 0.9, "the hash side is not flat: {hash}");
        assert!(hive < 0.3, "the position side is not clustered: {hive}");
        assert!(
            hash > hive * 3.0,
            "the two bands have to look different: {hash} against {hive}"
        );
    }

    #[test]
    fn a_read_lands_in_the_cell_its_address_says() {
        assert_eq!(
            cell_of(0, 9_203, 64),
            0,
            "the first chunk is the first cell"
        );
        assert_eq!(
            cell_of(9_202, 9_203, 64),
            63,
            "the last chunk is the last cell"
        );
        // Monotone: a larger address never lands left of a smaller one. That is the
        // property the band is read by, and it is worth more than a magic middle cell —
        // an earlier version of this test asserted cell 32 for chunk 4,601 and was simply
        // wrong, because the midpoint of 9,203 is 4,601.5 and the floor puts it at 31.
        let mut previous = 0;
        for chunk in (0..9_203u32).step_by(53) {
            let cell = cell_of(chunk, 9_203, 64);
            assert!(
                cell >= previous,
                "chunk {chunk} landed left of its predecessor"
            );
            previous = cell;
        }
        // Past the end is the end: a pointer naming a chunk the corpus does not have must
        // not wrap to the far left, which is where a modulo would put it.
        assert_eq!(cell_of(999_999, 9_203, 64), 63);
        // A degenerate axis is a cell, not a panic.
        assert_eq!(cell_of(5, 0, 64), 0);
        assert_eq!(cell_of(5, 10, 1), 0);
    }

    #[test]
    fn a_read_of_the_index_is_not_drawn_on_the_position_band() {
        // It has no position to be placed at, and inventing one would be the graph's
        // mistake all over again.
        let index = Read {
            to: ReadTo::Index,
            from_bucket: 64,
            at: 0.0,
            flight: Some(0.1),
        };
        assert_eq!(chunk_of(&index), None);
        assert_eq!(coverage(&[index], 64, 9_203, chunk_of), 0.0);
    }

    #[test]
    fn the_heat_fades_over_the_hold_and_then_goes() {
        let read = body(10, 0.0);
        assert_eq!(weight_of(&read, 0.0), 1.0, "it arrives at full heat");
        let mid = weight_of(&read, HOLD * 0.5);
        assert!(mid > 0.0 && mid < 1.0, "half way is half: {mid}");
        assert_eq!(weight_of(&read, HOLD + 0.01), 0.0);
        // And a fade is monotone, so a cell never brightens as it ages.
        let mut previous = 1.0;
        for step in 1..30 {
            let now = HOLD * step as f32 / 30.0;
            let weight = weight_of(&read, now);
            assert!(weight <= previous, "heat rose at {now}");
            previous = weight;
        }
    }

    #[test]
    fn a_read_still_in_the_air_is_heat_now() {
        // It has not landed, so there is no flight to fade against — and it is happening,
        // which is the strongest thing a read can be doing. That holds for as long as it
        // is out, however long that is: a request that has taken twelve seconds is a run
        // waiting, and the one thing on screen worth looking at.
        let flying = Read {
            flight: None,
            ..body(10, 0.0)
        };
        assert_eq!(weight_of(&flying, 0.0), 1.0);
        assert_eq!(weight_of(&flying, HOLD * 0.9), 1.0);
        assert_eq!(weight_of(&flying, WINDOW - 0.01), 1.0);
        assert_eq!(weight_of(&flying, WINDOW), 1.0, "still out is still hot");
        assert_eq!(
            weight_of(&flying, 600.0),
            1.0,
            "ten minutes later, still out"
        );
        // What lapses at the window is the *repaint*, not the mark: see `busy`.
        assert!(LocatorStrips::new(&[flying], WINDOW - 0.01).busy());
        assert!(!LocatorStrips::new(&[flying], WINDOW).busy());
    }

    #[test]
    fn a_read_the_clock_has_not_reached_is_not_heat() {
        let later = body(10, 5.0);
        assert_eq!(weight_of(&later, 4.9), 0.0);
        assert!(!LocatorStrips::new(&[later], 4.9).busy());
    }

    #[test]
    fn a_run_that_is_over_stops_asking_to_be_painted() {
        let over = vec![body(10, 0.0), run(64, 0.0)];
        assert!(LocatorStrips::new(&over, HOLD * 0.5).busy());
        assert!(LocatorStrips::new(&over, HOLD - 0.01).busy());
        assert!(
            !LocatorStrips::new(&over, HOLD).busy(),
            "dark is dark: the heat is gone and so is the paint"
        );
        assert!(!LocatorStrips::new(&over, WINDOW + 0.01).busy());
        assert!(!LocatorStrips::new(&[], 0.0).busy());
    }

    #[test]
    fn the_paint_stops_when_the_ink_stops_moving() {
        // ⚠️ The direction that matters: a widget that reported itself idle while
        // something on screen was still changing would freeze mid-fade. The converse is
        // false on purpose — a request that has been out longer than the window keeps its
        // mark and stops needing repaints, because its mark is not moving — so this pins
        // the implication rather than an equality.
        let runs = [
            vec![body(10, 0.0)],
            vec![run(64, 0.0)],
            vec![Read {
                flight: None,
                ..body(10, 0.0)
            }],
        ];
        for reads in &runs {
            let count = reads.len();
            for step in 0..90 {
                let now = step as f32 * 0.1;
                let ink = reads.iter().any(|read| weight_of(read, now) > 0.0);
                if LocatorStrips::new(reads, now).busy() {
                    assert!(ink, "busy with nothing drawn at {now} with {count} reads");
                }
            }
        }
        // And a landed read stops at the hold, not a frame later or a frame early.
        let landed = vec![body(10, 0.0)];
        assert!(LocatorStrips::new(&landed, HOLD - 0.01).busy());
        assert!(!LocatorStrips::new(&landed, HOLD).busy());
    }

    #[test]
    fn a_spark_sits_where_its_read_went_out() {
        // Time is the axis, and a read is an event on it: half way through the window is
        // half way along the strip, whatever the read is carrying.
        let reads = [
            Read {
                flight: None,
                ..body(1, 2.0)
            },
            Read {
                flight: None,
                ..body(2, 3.0)
            },
        ];
        let fired = sparks(&reads, 4.0, 4.0);
        assert_eq!(fired.len(), 2, "one tick per read, not one per slice");
        assert_eq!(fired[0], 0.5);
        assert_eq!(fired[1], 0.75);
    }

    #[test]
    fn a_fulfilled_request_is_not_on_the_strip() {
        // The strip is the requests that are OUT, so a read that has been answered is off
        // it — no trail and no fade: having a flight is all it takes. Its heat in the
        // bands goes on fading, which is the whole difference between the two.
        let landed = [body(1, 3.0)];
        assert!(
            sparks(&landed, 4.0, WINDOW).is_empty(),
            "answered 0.1 after it went"
        );
        assert!(
            weight_of(&landed[0], 4.0) > 0.0,
            "while its heat is still up"
        );
        // The same read, still out, is one tick where it went out.
        let out = [Read {
            flight: None,
            ..body(1, 3.0)
        }];
        assert_eq!(sparks(&out, 4.0, WINDOW), vec![0.75]);
    }

    #[test]
    fn a_read_still_out_keeps_its_spark_however_long_it_has_been_out() {
        // Longer out than a landed read's heat lasts, and still a spark: it is still
        // being waited on.
        let hung = [Read {
            flight: None,
            ..body(1, 0.0)
        }];
        assert_eq!(sparks(&hung, HOLD + 0.9, WINDOW).len(), 1);
        // Out past the window it does not leave the strip, it **pins to the left edge** —
        // which is what the axis can say past that point: out for longer than the window
        // shows. The repaint stops (that mark does not move); the mark does not.
        assert_eq!(sparks(&hung, WINDOW + 2.0, WINDOW), vec![0.0]);
        assert!(!LocatorStrips::new(&hung, WINDOW + 2.0).busy());
        // A landed read, by contrast, is gone the moment its heat is.
        let landed = [body(1, 0.0)];
        assert!(sparks(&landed, WINDOW + 2.0, WINDOW).is_empty());
    }

    #[test]
    fn nothing_is_drawn_for_a_read_the_clock_has_not_reached() {
        let later = [Read {
            flight: None,
            ..body(1, 5.0)
        }];
        assert!(sparks(&later, 4.0, 4.0).is_empty());
    }
}
