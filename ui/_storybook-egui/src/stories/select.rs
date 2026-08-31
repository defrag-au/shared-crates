//! `Select` story — the react-select-shaped control, in the states that
//! decide whether it feels solid.
//!
//! Put two of them side by side with values of different lengths: the point of
//! the fixed value column is that the `×` and `⌄` land in the same place on
//! every row. The old three-loose-buttons row failed exactly there.

use egui_widgets::select::{Select, SelectOption, SelectState, SelectValue};
use egui_widgets::theme;

pub struct SelectStory {
    pub roles: Vec<SelectOption>,
    pub tiers: Vec<(String, SelectState)>,
    pub plain: SelectState,
    pub plain_value: String,
    pub subtitled: SelectState,
    pub subtitled_value: String,
    pub last: String,
}

impl Default for SelectStory {
    fn default() -> Self {
        Self {
            roles: vec![
                SelectOption::new("1291038099278790738", "crew")
                    .swatch(Some(egui::Color32::from_rgb(0x58, 0x65, 0xf2))),
                SelectOption::new("1291038099278790999", "deckhand")
                    .swatch(Some(egui::Color32::from_rgb(0x57, 0xf2, 0x87))),
                SelectOption::new("1291038099278791111", "quartermaster")
                    .swatch(Some(egui::Color32::from_rgb(0xfe, 0xe7, 0x5c))),
                SelectOption::new("1291038099278792222", "bilge rat"),
            ],
            // Three rows: resolved short, resolved long, and an id whose role
            // no longer exists — the case a select that blanks unknown values
            // would quietly destroy.
            tiers: vec![
                ("1291038099278790738".to_string(), SelectState::default()),
                ("1291038099278790999".to_string(), SelectState::default()),
                ("999000111222333444".to_string(), SelectState::default()),
            ],
            plain: SelectState::default(),
            plain_value: String::new(),
            subtitled: SelectState::default(),
            subtitled_value: "https://api.x.ai/v1".to_string(),
            last: String::new(),
        }
    }
}

fn providers() -> Vec<SelectOption> {
    vec![
        SelectOption::new("https://api.x.ai/v1", "x.ai").subtitle("grok-4.3"),
        SelectOption::new("https://api.openai.com/v1", "OpenAI").subtitle("gpt-4.1"),
        SelectOption::new("https://api.deepseek.com", "DeepSeek").subtitle("deepseek-chat"),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut SelectStory) {
    ui.label(
        egui::RichText::new("Select")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "One bordered control — value, clear, separator, chevron — with a floating \
             filtered menu. Modelled on react-select; click anywhere in the box, type to \
             filter, ↑↓ to move, Enter to choose, Esc to close.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    if !state.last.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, &state.last);
    }
    ui.add_space(12.0);

    // ── Empty + placeholder ───────────────────────────────────────────────
    ui.strong("Empty");
    ui.label(
        egui::RichText::new("Muted placeholder, no clear affordance until there is something to clear.")
            .small()
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(4.0);
    let roles = state.roles.clone();
    let resp = Select::new("plain", &mut state.plain, &roles)
        .value_from_id(&state.plain_value, "no such role in this guild")
        .placeholder("Select role…")
        .width(240.0)
        .show(ui);
    if let Some(id) = resp.chosen {
        state.plain_value = id;
        state.last = "chose a role".into();
    }
    if resp.cleared {
        state.plain_value.clear();
        state.last = "cleared".into();
    }

    ui.add_space(16.0);

    // ── Subtitles ─────────────────────────────────────────────────────────
    ui.strong("With subtitles");
    ui.label(
        egui::RichText::new("A row can carry a second line — enough for a model name, not a table.")
            .small()
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(4.0);
    let provider_options = providers();
    let resp = Select::new("providers", &mut state.subtitled, &provider_options)
        .value_from_id(&state.subtitled_value, "unknown endpoint")
        .placeholder("Select provider…")
        .width(280.0)
        .show(ui);
    if let Some(id) = resp.chosen {
        state.subtitled_value = id;
        state.last = "chose a provider".into();
    }
    if resp.cleared {
        state.subtitled_value.clear();
    }

    ui.add_space(16.0);

    // ── The alignment case ────────────────────────────────────────────────
    ui.strong("Rows — the alignment case");
    ui.label(
        egui::RichText::new(
            "Different value lengths, and a third row whose role was deleted. \
             Every ×, separator and chevron should sit in one column; the dead \
             id renders flagged rather than blank.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(6.0);

    let mut chose: Option<(usize, String)> = None;
    let mut cleared: Option<usize> = None;
    egui::Grid::new("select_story_tiers")
        .num_columns(3)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for (index, (value, tier_state)) in state.tiers.iter_mut().enumerate() {
                let resp = Select::new("tier", tier_state, &roles)
                    .value_from_id(value, "no such role in this guild — it may have been deleted")
                    .placeholder("Select role…")
                    .width(220.0)
                    .show(ui);
                if let Some(id) = resp.chosen {
                    chose = Some((index, id));
                }
                if resp.cleared {
                    cleared = Some(index);
                }
                ui.label("500,000");
                ui.colored_label(theme::TEXT_MUTED, "tokens/day");
                ui.end_row();
            }
        });
    if let Some((index, id)) = chose {
        state.tiers[index].0 = id;
        state.last = format!("row {index} chose a role");
    }
    if let Some(index) = cleared {
        state.tiers[index].0.clear();
        state.last = format!("row {index} cleared");
    }

    ui.add_space(16.0);
    ui.strong("Empty option set");
    ui.label(
        egui::RichText::new("Filter to something that matches nothing to see the menu's empty state.")
            .small()
            .color(theme::TEXT_MUTED),
    );
}
