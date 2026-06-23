//! `NamedGroupList` story — named groups with member multiselects + a flag.

use egui_widgets::named_group_list::{NamedGroup, NamedGroupList};
use egui_widgets::theme;

pub struct NamedGroupListState {
    pub options: Vec<String>,
    pub groups: Vec<NamedGroup>,
}

impl Default for NamedGroupListState {
    fn default() -> Self {
        Self {
            options: vec![
                "hair".into(),
                "headwear".into(),
                "accessory".into(),
                "glasses".into(),
                "mask".into(),
            ],
            groups: vec![NamedGroup {
                name: "head_covering".into(),
                members: vec!["hair".into(), "headwear".into()],
                flag: true,
            }],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut NamedGroupListState) {
    ui.label(
        egui::RichText::new("Named Group List")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Named groups, each with a member multiselect + an optional flag — \
             backs exclusive groups (allow_none), bundled sets, linked traits. \
             Composes the Token Multiselect widget.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = NamedGroupListState::default();
    }
    ui.add_space(8.0);

    NamedGroupList::new(&mut state.groups, &state.options)
        .flag("allow none")
        .add_label("Add group")
        .show(ui);

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("{} group(s)", state.groups.len()))
            .color(theme::TEXT_SECONDARY)
            .small(),
    );
}
