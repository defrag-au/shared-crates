//! Flame chart story — a synthetic frame, so the widget can be built and read
//! without running a profiler.
//!
//! The spans below are made up, and that is the point twice over. It means the
//! chart is reviewable here rather than only in a browser with
//! `--features profiling` and a real capture; and it means the awkward cases —
//! a span too narrow to draw, a label too long for its bar, a stack deeper than
//! anyone expects — can be put on screen deliberately instead of waited for.
//!
//! The shape is the one that was actually measured: a ~4.2ms frame whose cost
//! is dominated by a listing grid, with the block train ticking beside it.

use egui::Ui;
use egui_widgets::flame_chart::{FlameChart, Span};
use egui_widgets::theme::{Ink, InkExt, TextSize, ThemeExt, Token};
use std::sync::Arc;

pub struct FlameChartState {
    shape: Shape,
    row_height: f32,
    spans: Vec<Span>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// The measured frame: a grid dominating, a train beside it.
    Frame,
    /// Deep and narrow — a recursive walk. Exercises the depth axis and the
    /// labels that cannot fit.
    Deep,
    /// Thousands of tiny scopes, most of them sub-pixel at full zoom. The cull
    /// is what keeps this drawable at all; zoom in and they appear.
    Swarm,
    /// Nothing captured. A profiler that has not been switched on looks like
    /// this, and it should say so rather than draw an empty box.
    Empty,
}

impl Default for FlameChartState {
    fn default() -> Self {
        let mut state = Self {
            shape: Shape::Frame,
            row_height: 18.0,
            spans: Vec::new(),
        };
        state.rebuild();
        state
    }
}

impl FlameChartState {
    fn rebuild(&mut self) {
        self.spans = match self.shape {
            Shape::Frame => a_measured_frame(),
            Shape::Deep => a_deep_stack(),
            Shape::Swarm => a_swarm(),
            Shape::Empty => Vec::new(),
        };
    }
}

fn span(depth: u16, start_ns: i64, duration_ns: i64, label: &str) -> Span {
    Span {
        depth,
        start_ns,
        duration_ns,
        label: Arc::from(label),
    }
}

/// The frame the vitals line reported: 4.2ms, most of it one widget.
fn a_measured_frame() -> Vec<Span> {
    let mut spans = vec![
        span(0, 0, 4_200_000, "App::ui"),
        span(1, 20_000, 120_000, "process_messages"),
        span(1, 150_000, 3_600_000, "ui::draw"),
        span(2, 180_000, 3_100_000, "listing_grid::show"),
        span(2, 3_320_000, 400_000, "block_train::show"),
        span(3, 3_340_000, 180_000, "block_train::plot"),
        span(3, 3_530_000, 150_000, "block_train::riders"),
        span(1, 3_780_000, 380_000, "Context::end_pass"),
        span(2, 3_800_000, 250_000, "tessellate"),
        span(2, 4_060_000, 90_000, "texture upload"),
    ];

    // The cards: enough of them that the cull matters, sized so a few carry a
    // label and most do not.
    let mut t = 200_000;
    for i in 0..48 {
        let d = 55_000 + (i % 7) * 9_000;
        spans.push(span(3, t, d, "card"));
        if i % 4 == 0 {
            spans.push(span(4, t + 8_000, d / 3, "try_load_texture"));
        }
        t += d + 2_000;
    }
    spans
}

fn a_deep_stack() -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut duration = 2_000_000;
    for depth in 0..24u16 {
        spans.push(span(
            depth,
            start,
            duration,
            "collection::walk::recurse_into_children",
        ));
        start += duration / 20;
        duration -= duration / 12;
    }
    spans
}

fn a_swarm() -> Vec<Span> {
    let mut spans = vec![span(0, 0, 3_000_000, "frame")];
    let mut t = 0;
    for i in 0..2_000 {
        // Deliberately sub-pixel at full zoom: ~1.5µs on a 3ms domain.
        spans.push(span(1, t, 1_200, if i % 3 == 0 { "hash" } else { "cmp" }));
        t += 1_500;
    }
    spans
}

pub fn show(ui: &mut Ui, state: &mut FlameChartState) {
    ui.heading("FlameChart");
    ui.label(
        egui::RichText::new(
            "Nested spans on a zoomable time axis. Wheel zooms about the pointer, \
             drag pans, double-click restores the whole frame.",
        )
        .color(Ink::Token(Token::TextMuted).of(ui))
        .size(ui.text_size(TextSize::Sm)),
    );
    ui.add_space(8.0);

    let before = state.shape;
    ui.horizontal_wrapped(|ui| {
        ui.label("Capture");
        ui.selectable_value(&mut state.shape, Shape::Frame, "Measured frame");
        ui.selectable_value(&mut state.shape, Shape::Deep, "Deep stack");
        ui.selectable_value(&mut state.shape, Shape::Swarm, "Swarm (2,000 spans)");
        ui.selectable_value(&mut state.shape, Shape::Empty, "Empty");
        ui.add(egui::Slider::new(&mut state.row_height, 10.0..=32.0).text("row height"));
    });
    if state.shape != before {
        state.rebuild();
    }

    ui.add_space(8.0);
    let response = FlameChart::new(&state.spans)
        .row_height(state.row_height)
        .show(ui);

    ui.add_space(8.0);
    // The hovered index is returned as well as tooltipped, so a real surface
    // can drive a detail panel from it. Showing it here is what proves that.
    let hovered = match response.hovered {
        Some(i) => format!("{} ({})", state.spans[i].label, i),
        None => "—".to_owned(),
    };
    ui.label(
        egui::RichText::new(format!(
            "{} span(s) · hovered: {hovered}",
            state.spans.len()
        ))
        .color(Ink::Token(Token::TextMuted).of(ui))
        .size(ui.text_size(TextSize::Sm)),
    );
}
