//! `SliderGroup` story — a bank of faders, and the stack of plain sliders it
//! replaces, side by side.
//!
//! The comparison IS the story. "Visually average" is not a claim you can
//! settle by looking at the new thing alone: the three problems being fixed —
//! ragged labels, a readout that outweighs its control, a rail that stays
//! 100px however much room it has — are only obvious next to the version that
//! has them.

use egui_widgets::slider_group::{Budget, Fader, SliderGroup};

use crate::{accent, muted, secondary};

pub struct SliderGroupStoryState {
    // The bank.
    pub size: f32,
    pub tension: f32,
    pub rings: u32,
    pub dot: f32,
    pub width: f32,
    // The rarity-style budget bank.
    pub purple: f32,
    pub pink: f32,
    pub red: f32,
    pub blue: f32,
    // A lone channel.
    pub solo: f32,
    pub last: String,
}

impl Default for SliderGroupStoryState {
    fn default() -> Self {
        Self {
            size: 240.0,
            tension: 0.3,
            rings: 4,
            dot: 3.0,
            width: 1.5,
            purple: 36.0,
            pink: 25.0,
            red: 25.0,
            blue: 25.0,
            solo: 0.6,
            last: "—".into(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut SliderGroupStoryState) {
    crate::caption(
        ui,
        "A mixing desk rotated a quarter turn. Labels right-aligned to one \
         edge, every rail starting and ending at the same x, readouts \
         right-aligned in a monospace column wide enough for the widest value \
         the range can print — so a number does not shift while you drag the \
         fader you are watching.",
    );
    ui.add_space(10.0);

    crate::heading(ui, "A bank");
    crate::caption(
        ui,
        "Mixed numeric types in one group — `rings` is a u32, the rest are f32. \
         Splitting them would break the spine that is the point.",
    );
    ui.add_space(4.0);
    let r = SliderGroup::new()
        .slider("Size", &mut state.size, 120.0..=400.0)
        .slider("Tension", &mut state.tension, 0.0..=0.5)
        .slider("Rings", &mut state.rings, 2..=8)
        .slider("Dot radius", &mut state.dot, 1.0..=6.0)
        .fader(Fader::new("Line width", &mut state.width, 0.5..=4.0).suffix("px"))
        .show(ui);
    if let Some(i) = r.changed_row {
        state.last = format!("row {i} moved");
    }

    ui.add_space(14.0);
    crate::heading(ui, "With a budget — the master meter");
    crate::caption(
        ui,
        "The running total against what it is supposed to sum to. This is the \
         whole of what `RarityTargetEditor` now is: percentages and a budget.",
    );
    ui.add_space(4.0);
    SliderGroup::new()
        .fader(Fader::new("Background: Purple", &mut state.purple, 0.0..=100.0).suffix("%"))
        .fader(Fader::new("Background: Pink", &mut state.pink, 0.0..=100.0).suffix("%"))
        .fader(Fader::new("Background: Red", &mut state.red, 0.0..=100.0).suffix("%"))
        .fader(Fader::new("Background: Blue", &mut state.blue, 0.0..=100.0).suffix("%"))
        .budget(Budget::new(100.0))
        .show(ui);

    ui.add_space(14.0);
    crate::heading(ui, "Constrained by its caller");
    crate::caption(
        ui,
        "The widget fills what it is given and takes no view on a maximum — a \
         360px cap baked in here would be this widget forming a private opinion \
         about layout in a codebase that has a theme and a layout engine for \
         exactly that. A caller that wants a shorter throw says so, with the \
         idiom that already exists:  ui.set_max_width(ui.fit_width(420.0)) — \
         which clamps to the CONTAINER, so this stays 420 in a wide pane and \
         shrinks with a narrow one.",
    );
    ui.add_space(4.0);
    ui.scope(|ui| {
        use egui_widgets::viewport::LayoutExt as _;
        ui.set_max_width(ui.fit_width(420.0));
        SliderGroup::new()
            .fader(Fader::new("Purple", &mut state.purple, 0.0..=100.0).suffix("%"))
            .fader(Fader::new("Pink", &mut state.pink, 0.0..=100.0).suffix("%"))
            .show(ui);
    });

    ui.add_space(14.0);
    crate::heading(ui, "One channel");
    crate::caption(
        ui,
        "A bank that happens to have one row today should not need rewriting \
         when it gets a second.",
    );
    ui.add_space(4.0);
    SliderGroup::new()
        .fader(Fader::new("Opacity", &mut state.solo, 0.0..=1.0).decimals(2))
        .show(ui);

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(8.0);

    crate::heading(ui, "What it replaces");
    crate::caption(
        ui,
        "The same five controls as plain `Slider::new(..).text(..)`. Note the \
         ragged left edge, the boxed DragValue outweighing each rail, and the \
         rails all stopping at 100px no matter how wide the pane is.",
    );
    ui.add_space(4.0);
    // Deliberately NOT migrated — this is the control group.
    let mut size = state.size;
    let mut tension = state.tension;
    let mut rings = state.rings;
    let mut dot = state.dot;
    let mut width = state.width;
    ui.add(egui::Slider::new(&mut size, 120.0..=400.0).text("Size"));
    ui.add(egui::Slider::new(&mut tension, 0.0..=0.5).text("Tension"));
    ui.add(egui::Slider::new(&mut rings, 2..=8).text("Rings"));
    ui.add(egui::Slider::new(&mut dot, 1.0..=6.0).text("Dot radius"));
    ui.add(egui::Slider::new(&mut width, 0.5..=4.0).text("Line width"));

    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(format!("last interaction: {}", state.last))
            .color(muted(ui))
            .small(),
    );
    ui.label(
        egui::RichText::new(format!(
            "bank total {:.1}  ·  changed this frame: {}",
            r.total, r.changed
        ))
        .color(secondary(ui))
        .small(),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Try it under compact / spacious — the rail, the row height and the gutters all move.",
        )
        .color(accent(ui))
        .small(),
    );
}
