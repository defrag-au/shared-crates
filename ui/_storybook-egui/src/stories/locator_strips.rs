//! `LocatorStrips` — a run as heat: a flat hash band, a chain band that clusters, and the
//! reads themselves as sparks under both.
//!
//! The claim is a comparison, so the story has to show both bands at once and at the same
//! scale. The still is the point of the widget; the live one is the same thing moving; the
//! faded one is how it lands behind a page.

use egui::{RichText, Sense, UiBuilder, vec2};
use egui_widgets::locator_graph::{Read, ReadTo, Span};
use egui_widgets::locator_strips::LocatorStrips;

/// The run's length and its three waves, from the phase table of the wallet arm measured
/// on 2026-09-27.
const RUN: f32 = 1.108;
const INDEX_UNTIL: f32 = 0.270;
const RUNS_UNTIL: f32 = 0.813;

/// How long the loop runs for. Longer than the run plus the widget's own hold, so the
/// bands clear before the next run starts.
const LOOP: f32 = 5.0;

/// The measured counts: 729 transactions located, from 6 index reads, 719 entry reads
/// and 129 body reads.
const INDEX_READS: usize = 6;
const RUN_READS: usize = 719;
const BODY_READS: usize = 129;

/// How many chunks the wallet's activity spans.
///
/// ⚠️ **The one number here that is not the run's.** 129 chunks is measured; how *wide* a
/// stretch of chain they occupy is not something the run reports, and a wallet's
/// transactions being spread over ~240 chunks is a stand-in. The count and the total are
/// real, and they are what the two bands are compared by.
const SPAN: u16 = 240;
const SPAN_FROM: u16 = 6_000;

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

/// One run's reads, on the three waves it actually made them in.
fn reads() -> Vec<Read> {
    let mut seed = Seeded(0x10CA_7021);
    let mut out = Vec::with_capacity(INDEX_READS + RUN_READS + BODY_READS);

    for i in 0..INDEX_READS {
        out.push(Read {
            to: ReadTo::Index,
            from_bucket: seed.next() % (1 << 24),
            at: i as f32 * (INDEX_UNTIL / INDEX_READS as f32),
            flight: Some(0.4),
        });
    }
    for i in 0..RUN_READS {
        let bucket = seed.next() % (1 << 24);
        out.push(Read {
            to: ReadTo::Run {
                shard: bucket / 512,
            },
            from_bucket: bucket,
            at: INDEX_UNTIL + (RUNS_UNTIL - INDEX_UNTIL) * i as f32 / RUN_READS as f32,
            flight: Some(0.4),
        });
    }
    for i in 0..BODY_READS {
        out.push(Read {
            to: ReadTo::Body {
                span: Span {
                    chunk: SPAN_FROM + (i as u16 * 7) % SPAN,
                    offset: 1 << 20,
                    len: 156,
                },
            },
            from_bucket: seed.next() % (1 << 24),
            at: RUNS_UNTIL + (RUN - RUNS_UNTIL) * i as f32 / BODY_READS as f32,
            flight: Some(0.4),
        });
    }
    out
}

/// The run as it was at `now`.
///
/// ⚠️ **The strip draws what is in the air, so a replay has to put reads back in it.**
/// A read that had not been answered by `now` had no flight yet — that is what `flight:
/// None` means — and a feed that kept every measured flight would show a run whose every
/// request was already answered, which is a strip with nothing on it.
fn as_of(reads: &[Read], now: f32) -> Vec<Read> {
    reads
        .iter()
        .map(|read| Read {
            flight: read.flight.filter(|flight| read.at + flight <= now),
            ..*read
        })
        .collect()
}

pub struct LocatorStripsStory {
    started: Option<f32>,
    reads: Vec<Read>,
}

impl Default for LocatorStripsStory {
    fn default() -> Self {
        Self {
            started: None,
            reads: reads(),
        }
    }
}

impl LocatorStripsStory {
    /// The loop's clock, started on the story's first frame.
    fn now(&mut self, ui: &egui::Ui) -> f32 {
        let clock = ui.input(|i| i.time) as f32;
        let started = *self.started.get_or_insert(clock);
        (clock - started) % LOOP
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut LocatorStripsStory) {
    let now = state.now(ui);
    let width = ui.available_width().min(880.0);

    ui.label(
        RichText::new(
            "One band per address space, drawn with the same mapping so they can be \
             compared: the hash side fills in evenly, the chain side clusters. The strip \
             under both is the reads — one tick as each one goes out.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(10.0);

    // The still: a fixed clock is a fixed frame, because heat is a pure function of it.
    // The run completed, so every read is in play and the two shapes can be compared.
    ui.label(
        RichText::new("The shape, at the end of the run")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    let still = as_of(&state.reads, RUN);
    LocatorStrips::new(&still, RUN)
        .size(vec2(width, 150.0))
        .show(ui);

    ui.add_space(18.0);
    ui.label(
        RichText::new("Live, on the loop — request ticks fire and go as they are answered")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    let live = as_of(&state.reads, now);
    LocatorStrips::new(&live, now)
        .size(vec2(width, 150.0))
        .show(ui);

    ui.add_space(18.0);
    ui.label(
        RichText::new("Placed: at full strength, then faded to a background")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(4.0);
    LocatorStrips::new(&live, now)
        .size(vec2(width, 110.0))
        .show(ui);

    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 110.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(6), crate::highlight(ui));

    let inner = rect.shrink(12.0);
    let mut faded = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    faded.set_opacity(0.18);
    LocatorStrips::new(&live, now)
        .size(inner.size())
        .show(&mut faded);

    let mut over = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink(16.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    over.label(
        RichText::new("1,037 of 1,037 AGREE")
            .strong()
            .color(crate::ink(&over)),
    );
    over.label(
        RichText::new("Nothing above is meant to be read — it is the shape of the run.")
            .color(crate::muted(&over))
            .size(11.0),
    );
}
