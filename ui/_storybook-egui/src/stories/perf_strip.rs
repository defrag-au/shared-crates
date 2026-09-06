//! `PerfStrip` — everything on this page is real.
//!
//! A perf widget demonstrated on fixture data proves nothing: the interesting
//! question is whether the numbers *move correctly* when the app gets slower,
//! and a hardcoded `FrameStats` cannot answer it. So this story has no
//! fixtures. The strip is reading the storybook's own frame clock (wired in
//! `StorybookApp::ui`), the storybook's own wasm linear memory, and a gauge
//! that the controls below actually open and close.
//!
//! Which means the story doubles as the acceptance test for the design claim in
//! the widget's docs: drag the load slider up and watch **frame build** climb
//! through green → amber → red while **fps** stays where it was. Those are
//! genuinely different measurements and this is the surface that shows it.
//!
//! It is also the check on the widget's *size*. Both orientations are shown
//! bare, then again in the corner of a mock surface — because the only way to
//! tell whether a HUD is unobtrusive is to put it next to content it must not
//! obtrude on, and the two axes fail that differently: a row spans the surface,
//! a column eats its margin.

use egui_widgets::{PerfOrientation, PerfStrip, PerfStripState};
use perf_probe::Gauge;

/// Stands in for whatever a real app has in flight. Named like a real one
/// because it appears on the strip verbatim.
static DEMO_WORK: Gauge = Gauge::new("demo work");

/// Room reserved for the vertical strip in the mock surface's right margin.
const MARGIN_STRIP_WIDTH: f32 = 130.0;

/// One `PerfStripState` per strip on screen.
///
/// They cannot share one: the state carries the sampling deadline, and two
/// strips sharing it would leave whichever drew second re-reading the cache the
/// first had just refreshed — the same numbers, but a second strip added to a
/// real app would silently halve nothing and look right, so the trap is worth
/// spelling out here rather than discovering later. The probes behind them ARE
/// global, so all four show the same figures.
#[derive(Default)]
pub struct PerfStripStory {
    vertical: PerfStripState,
    horizontal: PerfStripState,
    in_situ_footer: PerfStripState,
    in_situ_corner: PerfStripState,
    /// Roughly how much synthetic work to do per frame, as a budget fraction.
    load: f32,
    /// Guards held open, to drive the gauge. Dropping one closes it.
    held: Vec<perf_probe::Guard>,
    /// Where the synthetic load's result lands, so it cannot be optimised away.
    burned: f64,
}

pub fn show(ui: &mut egui::Ui, state: &mut PerfStripStory) {
    // Register up front so the gauge reads a real zero from the first frame,
    // rather than appearing out of nowhere on the first button press — the
    // difference `Gauge::register` exists for.
    DEMO_WORK.register();

    ui.label(
        egui::RichText::new(
            "Live — these are the storybook's own numbers, not a fixture. The strip \
             samples four times a second and asks for no repaints in between. \
             Hover ANY reading for the numbers behind it and what it means; the \
             text is not selectable, so a stray drag cannot highlight it.",
        )
        .color(egui_widgets::theme::TEXT_MUTED),
    );
    ui.add_space(10.0);

    // Side by side, so the size difference is a comparison rather than a claim.
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            caption(ui, "Vertical (default)");
            PerfStrip::new(&mut state.vertical)
                .orientation(PerfOrientation::Vertical)
                .show(ui);
        });
        ui.add_space(48.0);
        ui.vertical(|ui| {
            caption(ui, "Horizontal");
            PerfStrip::new(&mut state.horizontal)
                .orientation(PerfOrientation::Horizontal)
                .show(ui);
        });
    });

    ui.add_space(16.0);
    in_situ(ui, state);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.strong("Make it worse");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Synthetic load");
        ui.add(
            egui::Slider::new(&mut state.load, 0.0..=2.0)
                .fixed_decimals(2)
                .suffix(" × frame budget"),
        );
    });
    ui.label(
        egui::RichText::new(
            "Real work, not a sleep — a busy loop is the only thing the frame clock \
             can honestly measure. Note that pushing this up reddens the BUILD time \
             and leaves fps alone.",
        )
        .size(11.0)
        .color(egui_widgets::theme::TEXT_MUTED),
    );

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(format!("Work in flight: {}", state.held.len()));
        if ui.button("Start one").clicked() {
            state.held.push(DEMO_WORK.enter());
        }
        if ui.button("Finish one").clicked() {
            state.held.pop();
        }
        if ui.button("Finish all").clicked() {
            state.held.clear();
        }
    });
    ui.label(
        egui::RichText::new(
            "`peak` and `total` keep climbing after the work closes — that is what \
             makes a gauge readable by something sampling at 4 Hz, which would \
             otherwise miss every burst that fits between two samples.",
        )
        .size(11.0)
        .color(egui_widgets::theme::TEXT_MUTED),
    );

    // The load itself, at the END of the story so it lands inside the frame the
    // scope is timing. Work done here shows up on the NEXT sample, which is the
    // honest ordering — a strip cannot report a cost it has not paid yet.
    burn(state);

    // Only spin the repaint loop while there is something to see move. Left on
    // unconditionally, this story would hold the whole storybook at full frame
    // rate for as long as it is open.
    if state.load > 0.0 {
        ui.ctx().request_repaint();
    }
}

/// A small muted caption above an example.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(11.0)
            .color(egui_widgets::theme::TEXT_MUTED),
    );
    ui.add_space(3.0);
}

/// The strip where it is actually meant to live: on a surface that is busy doing
/// something else.
///
/// A widget whose whole design constraint is "must not obtrude" cannot be judged
/// on its own against an empty page — the first version of this looked perfectly
/// reasonable in isolation and swamped everything the moment it was put beside
/// real content.
///
/// Both placements at once, because they are what the orientations are FOR:
/// horizontal joins the footer the surface already has, vertical takes the right
/// margin the content was never using. Seeing them apart makes each look fine;
/// seeing them together is what tells you which one a given surface can afford.
fn in_situ(ui: &mut egui::Ui, state: &mut PerfStripStory) {
    use egui_widgets::theme;

    caption(
        ui,
        "In situ — horizontal on the footer, vertical in the margin:",
    );

    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(6)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    // The content column takes everything the margin does not,
                    // so the vertical strip displaces content rather than
                    // floating over it — which is the honest way to show what it
                    // costs a layout.
                    ui.set_width(ui.available_width() - MARGIN_STRIP_WIDTH);
                    ui.heading(egui::RichText::new("Wallet flow").size(16.0));
                    ui.label(
                        egui::RichText::new(
                            "stake1u8n…7q4z · 412 movements · 38 counterparties · last seen 4 min ago",
                        )
                        .color(theme::TEXT_SECONDARY),
                    );
                    ui.add_space(6.0);
                    for (label, value) in [
                        ("Received", "128,400 ₳"),
                        ("Sent", "121,905 ₳"),
                        ("Net", "+6,495 ₳"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(label).color(theme::TEXT_MUTED));
                            ui.label(value);
                        });
                    }
                });
                PerfStrip::new(&mut state.in_situ_corner)
                    .orientation(PerfOrientation::Vertical)
                    .show(ui);
            });

            ui.add_space(10.0);
            ui.separator();
            // On its own row under a rule — out of the reading path, still on
            // screen. Not wrapped in a `right_to_left` layout: the strip pins
            // its own reading order, so that would no longer move it right, it
            // would just look like it was meant to.
            PerfStrip::new(&mut state.in_situ_footer)
                .orientation(PerfOrientation::Horizontal)
                .show(ui);
        });
}

/// Do `load × 16.6 ms` worth of arithmetic, approximately.
///
/// Calibration is deliberately crude — the point is a knob that visibly moves
/// the number, not a precise millisecond. The first version was out by a factor
/// of thirty: "2.00 × frame budget" moved frame build from 1.7 ms to 2.0 ms,
/// which demonstrates nothing.
///
/// Arithmetic only, no allocation. The first version pushed into a `Vec`, which
/// grew wasm linear memory as you dragged the slider and made the *Memory* card
/// move too — so the one control on the page appeared to affect every number,
/// which is the opposite of what it is here to show.
fn burn(state: &mut PerfStripStory) {
    if state.load <= 0.0 {
        return;
    }
    // ~1.5M f64 ops ≈ one 60 Hz frame budget in a debug wasm build. Wrong on
    // native and on a faster machine, which does not matter for a knob.
    let iterations = (state.load * 1_500_000.0) as usize;
    let mut acc = 0.0_f64;
    for i in 0..iterations {
        acc += (i as f64).sqrt();
    }
    // Keeps `acc` observable so the loop cannot be optimised away — without
    // this the slider does nothing at all in a release build.
    state.burned = acc;
}
