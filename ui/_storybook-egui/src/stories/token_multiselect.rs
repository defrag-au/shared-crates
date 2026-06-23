//! `TokenMultiselect` story — pick a subset from a known option set; selected
//! show as removable chips, an "add" menu offers the rest.

use egui_widgets::theme;
use egui_widgets::token_multiselect::TokenMultiselect;

pub struct TokenMultiselectState {
    pub options: Vec<String>,
    pub selected: Vec<String>,
}

impl Default for TokenMultiselectState {
    fn default() -> Self {
        let options = vec![
            "01 - backgrounds".into(),
            "02 - skin".into(),
            "07 - neck".into(),
            "09 - clothes".into(),
            "11 - hair a".into(),
            "13 - hair b".into(),
            "14 - headwear".into(),
            "08 - eyes".into(),
            "12 - eyewear".into(),
        ];
        Self {
            selected: vec!["11 - hair a".into(), "13 - hair b".into()],
            options,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut TokenMultiselectState) {
    ui.label(
        egui::RichText::new("Token Multiselect")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Pick a subset from a known set of options — selected items render as \
             removable chips (wraps cleanly), and an \"add\" menu lists the rest. \
             Foundation for group/member/required-slot pickers in the config editor.",
        )
        .color(theme::TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    if ui.button("Reset").clicked() {
        *state = TokenMultiselectState::default();
    }
    ui.add_space(8.0);

    let resp = TokenMultiselect::new(&state.selected, &state.options)
        .add_label("+ add slot")
        .clearable("Clear")
        .show(ui);
    if let Some(opt) = resp.added {
        if !state.selected.contains(&opt) {
            state.selected.push(opt);
        }
    }
    if let Some(i) = resp.removed {
        if i < state.selected.len() {
            state.selected.remove(i);
        }
    }
    if resp.cleared {
        state.selected.clear();
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!(
            "{} of {} selected",
            state.selected.len(),
            state.options.len()
        ))
        .color(theme::TEXT_SECONDARY)
        .small(),
    );
}
