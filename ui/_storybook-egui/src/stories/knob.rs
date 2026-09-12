//! `Knob` story — the prototype bench.
//!
//! The point of this story is the **bottom section**, not the top one: the same
//! value driven by a knob, by a `DragValue` and by a `SliderGroup` fader, so the
//! question "does this actually feel better" can be answered by moving all three
//! rather than by looking at a picture of one.
//!
//! Everything about the feel is a control here on purpose. `travel` especially —
//! it is the one number that decides whether the knob feels twitchy or gluey,
//! and no amount of reasoning settles it as fast as dragging at 60px and at
//! 300px back to back.

use egui_widgets::knob::{Knob, KnobFace, KnobSize, KnobTouch, Readout};
use egui_widgets::slider_group::{Fader, SliderGroup};

pub struct KnobState {
    // The bench controls.
    pub diameter: f32,
    pub travel: f32,
    pub readout_index: usize,
    pub touch_index: usize,
    pub ticks: u32,
    // One value per face, so each can be left somewhere different.
    pub per_face: [f32; 4],
    // The head-to-head.
    pub knob_value: f32,
    pub drag_value: f32,
    pub fader_value: f32,
    // A bank, to see whether a row of them reads.
    pub bank: [f32; 5],
}

impl Default for KnobState {
    fn default() -> Self {
        Self {
            diameter: 44.0,
            travel: 160.0,
            readout_index: 0,
            touch_index: 0,
            ticks: 11,
            per_face: [0.35, 0.6, 0.8, 0.5],
            knob_value: 0.4,
            drag_value: 0.4,
            fader_value: 0.4,
            bank: [0.8, 0.35, 0.5, 0.62, 0.2],
        }
    }
}

const READOUTS: [(Readout, &str); 3] = [
    (Readout::Below, "below"),
    (Readout::Centre, "centre"),
    (Readout::None, "none"),
];

const TOUCHES: [(KnobTouch, &str); 2] = [
    (KnobTouch::HoldToEngage, "hold to engage"),
    (KnobTouch::Direct, "direct"),
];

pub fn show(ui: &mut egui::Ui, state: &mut KnobState) {
    crate::heading(ui, "Knob — prototype");
    crate::caption(
        ui,
        "Exploratory. Four faces and an adjustable feel, so the choice gets made \
         by using it. Nothing depends on this yet and the shape is expected to \
         move.",
    );
    crate::caption(
        ui,
        "Drag up and down — not in a circle, which is miserable with a mouse. \
         Hold shift for fine, double-click to reset, scroll while hovered to \
         nudge.",
    );
    crate::caption(
        ui,
        "On a touch screen: rest a finger until the ring fills, then drag. Pull \
         sideways as you drag to get finer — that is the finger's shift key, and \
         being continuous it is arguably better than one. The throw is longer \
         under touch, not shorter.",
    );
    ui.add_space(12.0);

    // ── The bench controls ──────────────────────────────────────────────────
    crate::heading(ui, "Feel");
    crate::caption(
        ui,
        "`travel` is the whole gain story: the pixels of drag that span the full \
         range, in the unit the hand actually works in. Try 60 against 300.",
    );
    ui.add_space(4.0);
    crate::controls(ui, |ui| {
        SliderGroup::new()
            .fader(Fader::new("diameter", &mut state.diameter, 20.0..=120.0).suffix("px"))
            .fader(Fader::new("travel", &mut state.travel, 40.0..=400.0).suffix("px"))
            .slider("ticks", &mut state.ticks, 3..=21)
            .show(ui);
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("readout").color(crate::muted(ui)));
        for (i, (_, name)) in READOUTS.iter().enumerate() {
            if ui
                .selectable_label(state.readout_index == i, *name)
                .clicked()
            {
                state.readout_index = i;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("touch").color(crate::muted(ui)));
        for (i, (_, name)) in TOUCHES.iter().enumerate() {
            if ui.selectable_label(state.touch_index == i, *name).clicked() {
                state.touch_index = i;
            }
        }
    });
    let readout = READOUTS[state.readout_index].0;
    let touch = TOUCHES[state.touch_index].0;

    ui.add_space(4.0);
    crate::caption(
        ui,
        "The touch policy does nothing under a mouse — a mouse scrolls with the \
         wheel, so nothing contests its drag. On a phone, this story pane is a \
         vertical ScrollArea, which is exactly the case that matters: with \
         `direct`, a finger landing on a knob cannot scroll the page and the \
         bank below becomes a band you are stuck on. With `hold to engage`, a \
         flick scrolls and a short rest takes the knob — watch for the ring \
         filling under your finger.",
    );

    ui.add_space(14.0);

    // ── The faces ───────────────────────────────────────────────────────────
    crate::heading(ui, "Four faces");
    crate::caption(
        ui,
        "Same value, same interaction — they differ in what they make easy to \
         see. `ring` survives smallest; `dial` reads most like a physical \
         control but is the least precise; `ticks` is for a parameter you count \
         rather than estimate.",
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        for (i, face) in KnobFace::ALL.iter().enumerate() {
            ui.vertical(|ui| {
                Knob::new(&mut state.per_face[i], 0.0..=1.0)
                    .face(*face)
                    .size(KnobSize::Fixed(state.diameter))
                    .travel(state.travel)
                    .readout(readout)
                    .touch(touch)
                    .ticks(state.ticks as usize)
                    .label(face.label())
                    .default_value(0.5)
                    .show(ui);
            });
        }
    });

    ui.add_space(14.0);

    // ── A bank ──────────────────────────────────────────────────────────────
    crate::heading(ui, "A bank");
    crate::caption(
        ui,
        "Five channels in a row. The question this answers is whether a row of \
         knobs is scannable — a `SliderGroup` gives you a spine to run your eye \
         down, and a row of knobs deliberately does not.",
    );
    ui.add_space(6.0);
    const CHANNELS: [&str; 5] = ["mount", "spacing", "lift", "tilt", "shadow"];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        for (i, name) in CHANNELS.iter().enumerate() {
            Knob::new(&mut state.bank[i], 0.0..=1.0)
                .face(KnobFace::Ring)
                .size(KnobSize::Fixed(state.diameter))
                .travel(state.travel)
                .readout(readout)
                .touch(touch)
                .label(name)
                .default_value(0.5)
                .show(ui);
        }
    });

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(8.0);

    // ── The head-to-head ────────────────────────────────────────────────────
    crate::heading(ui, "Against what it would replace");
    crate::caption(
        ui,
        "The same 0–1 parameter, three ways. Drag each. The `DragValue` is what \
         the effect editor's weight column uses today: a filled box, no sense of \
         range, and a gain that comes from a per-call `speed` nobody can pick \
         correctly for every range.",
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 28.0;
        ui.vertical(|ui| {
            Knob::new(&mut state.knob_value, 0.0..=1.0)
                .size(KnobSize::Fixed(state.diameter))
                .travel(state.travel)
                .readout(readout)
                .touch(touch)
                .label("knob")
                .default_value(0.4)
                .show(ui);
        });
        ui.vertical(|ui| {
            ui.add(
                egui::DragValue::new(&mut state.drag_value)
                    .speed(0.01)
                    .range(0.0..=1.0),
            );
            ui.label(
                egui::RichText::new("DragValue")
                    .color(crate::muted(ui))
                    .small(),
            );
        });
        ui.vertical(|ui| {
            ui.set_max_width(260.0);
            SliderGroup::new()
                .fader(Fader::new("fader", &mut state.fader_value, 0.0..=1.0).decimals(2))
                .show(ui);
        });
    });

    ui.add_space(14.0);
    crate::caption(
        ui,
        "Open question worth settling here: a knob is compact and feels good for \
         ONE value, but a bank of them has no shared edge. If the effect \
         editor's weight column becomes knobs, the column stops being scannable \
         — which may be a fair trade for weights nobody reads across, and a bad \
         one for a set that has to sum.",
    );
}
