//! `LocatorGraph` — the ambient form, at full strength and faded behind content.
//!
//! The widget draws itself at full strength on purpose: fading is the
//! *placement*'s job, so this story shows both — the graph on its own, and the
//! same graph under a card at `set_opacity(0.18)`. A still is included because
//! the widget claims one is reproducible: every mark is a pure function of the
//! clock it is handed.

use egui::{RichText, Sense, UiBuilder, vec2};
use egui_widgets::locator_graph::{LocatorGraph, Lookup, Span};

/// Seconds the roster covers, and where it starts again.
///
/// The roster is longer than the loop so the marks at the seam are already in
/// the air when the clock wraps back to zero.
const LOOP: f32 = 5.0;
const ROSTER: f32 = 7.0;

/// Lookups in one loop — the measured wallet run: 729 transactions located.
const BODIES: u32 = 729;

/// Seconds one lookup is in the air: a cold read is ~400 ms and a lookup makes
/// two of them.
const FLIGHT: f32 = 0.9;

/// The one lookup recorded end to end — bucket 64, shard 0, a six-entry run,
/// chunk 9150, offset 3,426,006, len 156 — and the re-hash agreed.
///
/// Everything else in the roster is generated, and labelled as such: the
/// coordinates are spread over the REAL axes (24-bit buckets, 32,768 shards,
/// 9,203 chunks) by a seeded generator, because a run's own trace is what the
/// browser will feed it.
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
        flight: FLIGHT,
    }
}

/// SplitMix64, so the roster is identical on every run and every machine.
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

fn roster() -> Vec<Lookup> {
    let mut seed = Seeded(0x10CA_7021);
    let count = (BODIES as f32 * ROSTER / LOOP) as usize;
    let mut out = Vec::with_capacity(count + 1);
    out.push(recorded());
    for i in 0..count {
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
            at: i as f32 * (LOOP / BODIES as f32),
            flight: FLIGHT,
        });
    }
    out
}

pub struct LocatorGraphStory {
    started: Option<f32>,
    roster: Vec<Lookup>,
}

impl Default for LocatorGraphStory {
    fn default() -> Self {
        Self {
            started: None,
            roster: roster(),
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
}

pub fn show(ui: &mut egui::Ui, state: &mut LocatorGraphStory) {
    let now = state.now(ui);
    let width = ui.available_width().min(880.0);

    ui.label(
        RichText::new(
            "The index keyed by hash on the left, the corpus ordered by position on the \
             right, and one wire per OBJECT read through the middle. No block is drawn \
             because no block has an address; the wires end in a re-hash because eight \
             bytes of the entry are all the index can promise.",
        )
        .color(crate::muted(ui))
        .size(11.0),
    );
    ui.add_space(10.0);

    LocatorGraph::new(&state.roster, now)
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
    LocatorGraph::new(&state.roster, now)
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
        RichText::new("A still: the same clock handed in every frame")
            .color(crate::muted(ui))
            .size(11.0),
    );
    ui.add_space(6.0);
    LocatorGraph::new(&state.roster, 1.0)
        .size(vec2(width * 0.55, 150.0))
        .show(ui);
}
