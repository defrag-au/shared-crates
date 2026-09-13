//! `OptionGroup` story — related choices as one control.
//!
//! The comparison at the bottom is the point. "One compound control" is not a
//! claim you can judge from the compound version alone: the thing being fixed —
//! several borders implying several unrelated objects — is only obvious next to
//! the version that has them.

use egui_widgets::PhosphorIcon;
use egui_widgets::option_group::{GroupDensity, GroupFlow, OptionGroup, OptionGroupItem};

pub struct OptionGroupStoryState {
    pub flow_inline: bool,
    pub compact: bool,
    pub selected: u64,
    pub last: String,
}

impl Default for OptionGroupStoryState {
    fn default() -> Self {
        Self {
            flow_inline: false,
            compact: false,
            selected: 1,
            last: "—".into(),
        }
    }
}

/// Stand-ins for wallet extension icons, which arrive as data URLs the widget
/// cannot invent. Glyphs exercise the same layout path.
const CHOICES: [(u64, &str, PhosphorIcon); 4] = [
    (0, "eternl", PhosphorIcon::Wallet),
    (1, "VESPR", PhosphorIcon::Lightning),
    (2, "Lace", PhosphorIcon::Star),
    (3, "Nami", PhosphorIcon::Compass),
];

pub fn show(ui: &mut egui::Ui, state: &mut OptionGroupStoryState) {
    crate::heading(ui, "OptionGroup");
    crate::caption(
        ui,
        "A set of related choices drawn as ONE control: a single border, \
         hairline separators, no gaps. Not `ButtonGroup`, which is an action bar \
         of independent buttons doing unrelated things — the tell is whether the \
         items are alternatives to each other.",
    );
    crate::caption(
        ui,
        "The craft is the corner arithmetic: each cell's fill is rounded to \
         match its POSITION, so the first and last rows' hover fill cannot poke \
         its square corners through the container's curve. Hover the top and \
         bottom rows and watch the edges.",
    );
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.flow_inline, "inline");
        ui.checkbox(&mut state.compact, "compact");
    });
    ui.add_space(8.0);

    let flow = match state.flow_inline {
        true => GroupFlow::Inline,
        false => GroupFlow::Stacked,
    };
    let density = match state.compact {
        true => GroupDensity::Compact,
        false => GroupDensity::Full,
    };

    crate::heading(ui, "As a picker");
    crate::caption(
        ui,
        "Nothing is current — you are choosing an action, so no cell is marked.",
    );
    ui.add_space(4.0);
    crate::controls(ui, |ui| {
        let r = OptionGroup::new()
            .flow(flow)
            .density(density)
            .items(
                CHOICES
                    .iter()
                    .map(|(id, label, icon)| OptionGroupItem::new(*id, label).icon(*icon)),
            )
            .show(ui);
        if let Some(id) = r.clicked {
            state.last = format!("picked {id}");
        }
    });

    ui.add_space(14.0);
    crate::heading(ui, "As a selector");
    crate::caption(
        ui,
        "The same control with one choice marked current. A picker and a \
         selector differ by one field, so they are one widget — two would drift.",
    );
    ui.add_space(4.0);
    crate::controls(ui, |ui| {
        let r = OptionGroup::new()
            .flow(flow)
            .density(density)
            .selected(state.selected)
            .items(
                CHOICES
                    .iter()
                    .map(|(id, label, icon)| OptionGroupItem::new(*id, label).icon(*icon)),
            )
            .show(ui);
        if let Some(id) = r.clicked {
            state.selected = id;
            state.last = format!("selected {id}");
        }
    });

    ui.add_space(14.0);
    crate::heading(ui, "A disabled choice");
    crate::caption(
        ui,
        "Muted and unclickable, but still in the group — a choice that vanishes \
         when unavailable makes the reader wonder what they misremembered.",
    );
    ui.add_space(4.0);
    crate::controls(ui, |ui| {
        OptionGroup::new()
            .flow(flow)
            .density(density)
            .item(OptionGroupItem::new(0, "eternl").icon(PhosphorIcon::Wallet))
            .item(
                OptionGroupItem::new(1, "VESPR")
                    .icon(PhosphorIcon::Lightning)
                    .enabled(false)
                    .hover("not detected in this browser"),
            )
            .item(OptionGroupItem::new(2, "Lace").icon(PhosphorIcon::Star))
            .show(ui);
    });

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(8.0);

    crate::heading(ui, "What it replaces");
    crate::caption(
        ui,
        "The same four choices as separate bordered buttons — the shape the \
         wallet picker had. Each border draws a box around one item, so four \
         alternatives read as four unrelated things that happen to be stacked.",
    );
    ui.add_space(4.0);
    // Deliberately NOT migrated — this is the control group.
    crate::controls(ui, |ui| {
        for (_, label, _) in CHOICES {
            ui.add_sized(
                [ui.available_width(), 30.0],
                egui::Button::new(label)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(0.5_f32, crate::muted(ui))),
            );
        }
    });

    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(format!("last: {}", state.last))
            .color(crate::secondary(ui))
            .small(),
    );
}
