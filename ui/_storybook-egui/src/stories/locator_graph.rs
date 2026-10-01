//! `LocatorGraph` — a run as it happens, then as it is reported.
//!
//! The two halves are the two things the browser can know. **While the run is going**
//! it knows which objects it has asked for and which have come back, and nothing else —
//! where a transaction's body is sits in an entry it has not fetched yet — so the wires
//! are fed live. **When the run is over** it knows the lookups, and those are replayed
//! over the length the run took.
//!
//! A still is included too, because the widget claims one is reproducible: every mark is
//! a pure function of the clock it is handed.

use egui::{RichText, Sense, UiBuilder, vec2};
use egui_widgets::locator_graph::{LocatorGraph, Lookup, Read, ReadTo, Span, reads};

/// The wallet arm's length in the phase table, and its three waves.
///
/// Measured 2026-09-27 against the published mirror: the index 6 reads over 270 ms,
/// the entry runs 719 over 543 ms, the bodies 129 over 295 ms, 1,246 ms wall.
const RUN: f32 = 1.108;
const INDEX_UNTIL: f32 = 0.270;
const RUNS_UNTIL: f32 = 0.813;

/// A cold read, which is what most of these were.
const COLD: f32 = 0.4;

/// How many objects the reader has in the air at once — `chunks::DEFAULT_FANOUT`.
///
/// This is the number that decides how much ink is on screen, so the fixture has to
/// carry it rather than putting every read in the air at the same moment.
const FANOUT: usize = 256;

/// The loop the story's clock runs on. Long enough for the run, its replay, and the pile
/// of evidence it leaves (which holds for `EVIDENCE`), so there is a beat of quiet before
/// the next run starts.
const LOOP: f32 = 5.6;

/// Lookups in the run — 729 transactions located, as measured.
const FOUND: u32 = 729;

/// The one lookup recorded end to end — bucket 64, shard 0, a six-entry run, chunk 9150,
/// offset 3,426,006, len 156 — and the re-hash agreed.
fn recorded() -> Lookup {
    Lookup {
        bucket: 64,
        shard: 0,
        entries: 6,
        span: Span {
            chunk: 9_150,
            offset: 3_426_006,
            len: 156,
        },
        settled: true,
        at: 0.0,
        flight: COLD,
    }
}

/// SplitMix64, so the fixture is identical on every run and every machine.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
}

/// The transactions the run located.
///
/// ⚠️ One of them is real and the rest are a stand-in: the coordinates are spread over
/// the REAL axes (24-bit buckets, 32,768 shards, 9,203 chunks) and the timings over the
/// REAL phases, because a run's own trace is what the browser feeds it.
fn roster() -> Vec<Lookup> {
    let mut seed = Seeded(0x10CA_7021);
    let mut out = Vec::with_capacity(FOUND as usize);
    out.push(recorded());
    for i in 1..FOUND {
        let bucket = seed.next() % (1 << 24);
        out.push(Lookup {
            bucket,
            shard: bucket / 512,
            // A bucket holds a handful of entries; a wider one is a collision.
            entries: 1 + seed.next() % 7,
            span: Span {
                chunk: (seed.next() % 9_203) as u16,
                // Offsets spread evenly along the log axis they are drawn on.
                offset: 1 << (seed.next() % 26),
                len: 120 + (seed.next() % 280) as u16,
            },
            settled: seed.next() % 97 != 0,
            at: i as f32 * (RUN / FOUND as f32),
            flight: COLD,
        });
    }
    out
}

/// The reads the run made, in the three waves the phase table measured.
///
/// ⚠️ The COUNTS come out of the lookups and are real. The placement in time is the
/// phase table's, because what a fixture has to stand in for is a feed: a read going out
/// at a moment, with nothing yet known about when it comes back.
fn fired(found: &[Lookup]) -> Vec<Read> {
    let mut out = Vec::new();

    // The index: one read per directory batch, caused by whichever bucket reached it
    // first. Six of them, and they go out before anything else.
    let every = (found.len() / 6).max(1);
    for (i, lookup) in found.iter().step_by(every).take(6).enumerate() {
        out.push(Read {
            to: ReadTo::Index,
            from_bucket: lookup.bucket,
            at: i as f32 * (INDEX_UNTIL / 6.0),
            flight: Some(COLD),
        });
    }

    let mut runs = Vec::new();
    let mut bodies = Vec::new();
    for read in reads(found) {
        match read.to {
            ReadTo::Run { .. } => runs.push(read),
            ReadTo::Body { .. } => bodies.push(read),
            ReadTo::Index => {}
        }
    }
    wave(&mut runs, INDEX_UNTIL, RUNS_UNTIL);
    wave(&mut bodies, RUNS_UNTIL, RUN);
    out.append(&mut runs);
    out.append(&mut bodies);
    out
}

/// Place a phase's reads as the reader actually issues them: **one after another at the
/// rate the fan-out allows**, rather than all at once.
///
/// ⚠️ This is the difference between a run and a hatched rectangle. `locate_wallet` reads
/// with `buffered(DEFAULT_FANOUT)`, so at most that many objects are ever in the air at
/// once; the first version of this fixture gave every read the same flight and no bound,
/// which put twice the browser's concurrency on screen and is what made it a block.
///
/// Issuing evenly and giving each read the window divided by the number of waves gives
/// exactly the fan-out in the air — `step / issue == n / waves == FANOUT` — and it
/// streams instead of pulsing, because nothing completes in lockstep.
fn wave(reads: &mut [Read], from: f32, to: f32) {
    let n = reads.len().max(1) as f32;
    let waves = reads.len().div_ceil(FANOUT).max(1) as f32;
    let step = (to - from) / waves;
    for (i, read) in reads.iter_mut().enumerate() {
        read.at = from + (to - from) * i as f32 / n;
        read.flight = Some(step);
    }
}

pub struct LocatorGraphStory {
    started: Option<f32>,
    found: Vec<Lookup>,
    made: Vec<Read>,
}

impl Default for LocatorGraphStory {
    fn default() -> Self {
        let found = roster();
        let made = fired(&found);
        Self {
            started: None,
            found,
            made,
        }
    }
}

impl LocatorGraphStory {
    /// The loop's clock, started on the story's first frame.
    fn now(&mut self, ui: &egui::Ui) -> f32 {
        let clock = ui.input(|i| i.time) as f32;
        let started = *self.started.get_or_insert(clock);
        (clock - started) % LOOP
    }

    /// The feed as the browser would hold it at `now`: every read that has gone out,
    /// with a flight on the ones that have come back and **none** on the ones still in
    /// the air — which is the difference this story exists to show.
    fn feed_at(&self, now: f32) -> Vec<Read> {
        self.made
            .iter()
            .map(|read| Read {
                flight: match read.flight {
                    Some(flight) if read.at + flight <= now => Some(flight),
                    _ => None,
                },
                ..*read
            })
            .collect()
    }

    /// And the lookups: nothing at all until the run is over, then the run replayed over
    /// the length it took — the same shift `App::replay` makes, for the same reason.
    fn found_at(&self, now: f32) -> Vec<Lookup> {
        if now < RUN {
            return Vec::new();
        }
        self.found
            .iter()
            .map(|lookup| Lookup {
                at: RUN + lookup.at,
                ..*lookup
            })
            .collect()
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut LocatorGraphStory) {
    let now = state.now(ui);
    let width = ui.available_width().min(880.0);

    ui.label(
        RichText::new(
            "Each read advances the pipeline by ONE gap: the index is read to learn which \
             shard holds a bucket, the shard to get the bucket's entries, and only the \
             body read crosses to the corpus — because only the corpus is ordered by \
             position rather than by hash. A wire goes out dim with its far end outlined \
             and comes back at full strength; motion is opacity, because how long a read \
             will take is not known until it is back.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(10.0);

    LocatorGraph::new(&state.found_at(now), now)
        .fired(&state.feed_at(now))
        .size(vec2(width, 280.0))
        .show(ui);

    ui.add_space(18.0);
    ui.label(
        RichText::new("Placed: drawn at full strength, faded to a background")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(6.0);

    let (rect, _) = ui.allocate_exact_size(vec2(width, 220.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(6), crate::highlight(ui));

    let mut faded = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    faded.set_opacity(0.18);
    LocatorGraph::new(&state.found_at(now), now)
        .fired(&state.feed_at(now))
        .size(rect.size())
        .show(&mut faded);

    let mut over = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink(18.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    over.label(
        RichText::new("1,037 of 1,037 AGREE")
            .strong()
            .color(crate::ink(&over)),
    );
    over.label(
        RichText::new("Nothing above is meant to be read — the wires are the run, not a report.")
            .color(crate::muted(&over))
            .size(11.0),
    );

    ui.add_space(18.0);
    ui.label(
        RichText::new("A still, 0.9 s into the run: what the browser knows mid-flight")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(6.0);
    // Every mark is a pure function of the clock, so a fixed clock is a fixed frame.
    // Nothing here is "mid-animation": this is the instant, drawn again.
    //
    // Tall on purpose: the app draws this across a page, and a 200pt strip puts four
    // times the ink per point of height on screen than the placement ever will.
    LocatorGraph::new(&state.found_at(RUN * 0.8), RUN * 0.8)
        .fired(&state.feed_at(RUN * 0.8))
        .size(vec2(width, 440.0))
        .show(ui);
}
