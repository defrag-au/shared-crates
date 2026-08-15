//! `Leaderboard` story — ranked standings with a share bar.
//!
//! Presets cover the three shapes that actually change how the widget reads:
//! a runaway leader (one bar dominates), a tight race (bars near-equal), and
//! the long tail past the podium. Plus the empty state, which is the one a
//! live view hits most often on a quiet day.
//!
//! "Long tail" is also the column-alignment check: its spends span two orders
//! of magnitude, so any per-row right-alignment shows up immediately as a
//! wandering value column.

use egui::{RichText, Ui};
use egui_widgets::leaderboard::{
    self, LeaderboardConfig, LeaderboardPrize, LeaderboardRow, LeaderboardStat,
};

#[derive(Default)]
pub struct LeaderboardState {
    preset: Preset,
    podium_tint: bool,
    viewer_row: bool,
    prizes: bool,
    last_click: Option<String>,
    initialised: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Preset {
    #[default]
    RunawayLeader,
    TightRace,
    LongTail,
    Empty,
}

impl Preset {
    fn label(self) -> &'static str {
        match self {
            Self::RunawayLeader => "Runaway leader",
            Self::TightRace => "Tight race",
            Self::LongTail => "Long tail",
            Self::Empty => "Empty",
        }
    }
}

/// (display name, spend in ADA, assets, trades)
fn data(preset: Preset) -> Vec<(&'static str, u64, usize, usize)> {
    match preset {
        Preset::RunawayLeader => vec![
            ("$whale", 128_400, 214, 41),
            ("$damo", 21_050, 33, 12),
            ("stake1u9…f3kq", 18_200, 29, 9),
            ("$pirate", 9_400, 14, 6),
            ("stake1uy…7t2v", 4_100, 7, 3),
        ],
        Preset::TightRace => vec![
            ("$damo", 12_400, 20, 8),
            ("$pirate", 12_100, 19, 7),
            ("stake1u9…f3kq", 11_850, 21, 9),
            ("$collector", 11_400, 18, 6),
            ("stake1uy…7t2v", 10_900, 17, 7),
        ],
        Preset::LongTail => (0..14)
            .map(|i| {
                let names = [
                    "$whale",
                    "$damo",
                    "$pirate",
                    "$collector",
                    "stake1u9…f3kq",
                    "stake1uy…7t2v",
                    "$flipper",
                    "$hodler",
                    "$sailor",
                    "$captain",
                    "$swab",
                    "$gunner",
                    "$cook",
                    "$navigator",
                ];
                (names[i], 40_000 / (i as u64 + 1), 30 - i, 12 - i / 2)
            })
            .collect(),
        Preset::Empty => Vec::new(),
    }
}

fn rows(state: &LeaderboardState) -> Vec<LeaderboardRow> {
    let raw = data(state.preset);
    let spends: Vec<u64> = raw.iter().map(|(_, spend, _, _)| *spend).collect();
    let shares = leaderboard::shares(&spends);

    raw.iter()
        .zip(shares)
        .enumerate()
        .map(
            |(i, ((name, spend, assets, trades), share))| LeaderboardRow {
                rank: i + 1,
                name: name.to_string(),
                tooltip: Some(format!(
                    "stake1u9qq{i:0>44}\n{trades} trades \u{00b7} {assets} assets"
                )),
                value: format!("{} ADA", thousands(*spend)),
                share,
                stats: vec![
                    LeaderboardStat::new(assets.to_string(), "assets"),
                    LeaderboardStat::new(trades.to_string(), "trades"),
                ],
                // No image URL: exercises the placeholder path, since a story
                // shouldn't depend on a live image host to be reviewable.
                prize: state.prizes.then(|| LeaderboardPrize {
                    image_url: None,
                    label: format!("Pirate #{}", 1000 + i * 37),
                    value: format!("{} ADA", thousands(spend / (*trades as u64).max(1))),
                }),
                // Second place is "you" — shows the accent treatment without
                // stealing the podium.
                is_viewer: state.viewer_row && i == 1,
            },
        )
        .collect()
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn show(ui: &mut Ui, state: &mut LeaderboardState) {
    if !state.initialised {
        state.initialised = true;
        state.podium_tint = true;
        state.prizes = true;
    }

    ui.label(
        RichText::new(
            "Ranked standings: rank gutter (podium by colour, not emoji), prize \
             thumbnail, name, headline metric, stats, and a thin share bar beneath \
             each row. The host formats every string — the widget is domain-free.",
        )
        .size(11.0),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        for preset in [
            Preset::RunawayLeader,
            Preset::TightRace,
            Preset::LongTail,
            Preset::Empty,
        ] {
            if ui
                .selectable_label(state.preset == preset, preset.label())
                .clicked()
            {
                state.preset = preset;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.podium_tint, "podium tint");
        ui.checkbox(&mut state.prizes, "prize thumbnails");
        ui.checkbox(&mut state.viewer_row, "highlight viewer row");
    });
    ui.add_space(8.0);

    let config = LeaderboardConfig {
        podium_tint: state.podium_tint,
        empty_text: "No buys in this window".to_string(),
        ..Default::default()
    };

    let rows = rows(state);
    ui.group(|ui| {
        ui.set_width(460.0);
        leaderboard::header(ui, "buyer", "spend", &config);
        if let Some(leaderboard::LeaderboardAction::RowClicked(idx)) =
            leaderboard::show(ui, &rows, &config)
        {
            state.last_click = rows.get(idx).map(|r| r.name.clone());
        }
    });

    if let Some(name) = &state.last_click {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("clicked: {name}")).size(10.0));
    }

    ui.add_space(10.0);
    ui.label(
        RichText::new(
            "Note the difference between 'Runaway leader' and 'Tight race': rank alone \
             reads identically, the bars don't. That's the reason the bar exists.\n\
             The value and stat columns are measured across ALL rows before drawing, so \
             they stay aligned however wide each row's numbers are \u{2014} check the \
             'Long tail' preset, where spends span two orders of magnitude.",
        )
        .size(10.0)
        .weak(),
    );
}
