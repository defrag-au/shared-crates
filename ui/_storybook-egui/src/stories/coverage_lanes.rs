//! `CoverageLanes` story — the third state is the whole story.
//!
//! Fixtures are shaped from a real watched fleet (15 ASIC miners on a hosted
//! site, 12 days at hourly buckets): a daily curtailment window roughly
//! 21:00–02:00 UTC, one machine that died mid-run and never came back, one that
//! ramps down early every day.
//!
//! **The thing to look at:** day 4's midday orange stretch and day 7's grey
//! column cover comparable spans and make completely different claims — day 4
//! the fleet was watched and idle, day 7 the poller was broken and nobody
//! knows. Fold those together and every ingest outage becomes recorded
//! downtime, which is the bug this widget exists to make impossible to draw.
//!
//! Second read: `miner-06` dies on day 5 and never comes back. You find it by
//! the shape of its lane against its neighbours, not by reading fifteen rows.

use crate::TEXT_MUTED;
use egui_widgets::{
    coverage_tint, Coverage, CoverageLane, CoverageLanes, Run, Selection, SpineState, TimeSpine,
};

const HOUR: i64 = 3_600;
const DAY: i64 = 24 * HOUR;
const DAYS: i64 = 12;
const T0: i64 = 1_786_838_400; // a Monday 00:00 UTC, so the ruler reads cleanly
const MACHINES: usize = 15;

pub struct CoverageLanesState {
    spine: Option<SpineState>,
    selection: Selection,
    per_machine: bool,
    fleet: Vec<Run>,
    machines: Vec<(String, Vec<Run>)>,
    active: Vec<bool>,
}

impl Default for CoverageLanesState {
    fn default() -> Self {
        Self {
            spine: None,
            selection: Selection::default(),
            per_machine: true,
            fleet: fleet_runs(),
            machines: machine_runs(),
            active: vec![true; MACHINES],
        }
    }
}

/// Is this hour inside the daily curtailment window (21:00–02:00 UTC)?
fn curtailed(hour_of_day: i64) -> bool {
    !(2..21).contains(&hour_of_day)
}

/// The ingest outage: a whole day nobody successfully polled. Emitting NO runs
/// is how a caller says "unobserved" — there is deliberately no `Run` variant
/// for it, so absence cannot be confused with a claim.
fn observed(day: i64, _hour_of_day: i64) -> bool {
    day != 7
}

/// Day 4's midday stop: watched the whole time, produced nothing. The visual
/// twin of day 7's gap and the comparison this story exists to make — so it
/// must be *observed*, not merely absent.
fn daytime_stop(day: i64, hour_of_day: i64) -> bool {
    day == 4 && (10..15).contains(&hour_of_day)
}

/// Day 4's window is a genuine, observed idle stretch — the visual twin of the
/// day-7 gap, and the comparison the story is built around.
fn fleet_runs() -> Vec<Run> {
    let mut runs = Vec::new();
    for day in 0..DAYS {
        for h in 0..24 {
            if !observed(day, h) {
                continue;
            }
            let start = T0 + day * DAY + h * HOUR;
            let end = start + HOUR;
            if curtailed(h) || daytime_stop(day, h) {
                runs.push(Run::idle(start, end));
            } else {
                // A taper into the curtailment window rather than a cliff.
                let level = match h {
                    20 => 0.35,
                    19 => 0.72,
                    _ => 0.88 + ((day * 7 + h) % 5) as f32 * 0.024,
                };
                runs.push(Run::producing(start, end, level));
            }
        }
    }
    runs
}

fn machine_runs() -> Vec<(String, Vec<Run>)> {
    (0..MACHINES)
        .map(|m| {
            // One machine dies on day 5 and never returns; one ramps down an
            // hour early every day. The rest follow the site schedule.
            let dead_from = (m == 6).then_some(5i64);
            let early = m == 11;
            let mut runs = Vec::new();
            for day in 0..DAYS {
                for h in 0..24 {
                    if !observed(day, h) {
                        continue;
                    }
                    let start = T0 + day * DAY + h * HOUR;
                    let end = start + HOUR;
                    if dead_from.is_some_and(|d| day >= d) {
                        runs.push(Run::idle(start, end));
                    } else if curtailed(h) || daytime_stop(day, h) || (early && h == 19) {
                        runs.push(Run::idle(start, end));
                    } else {
                        let level = match h {
                            20 => 0.35,
                            19 => 0.72,
                            _ => 0.86 + ((m * 3 + h as usize) % 6) as f32 * 0.023,
                        };
                        runs.push(Run::producing(start, end, level));
                    }
                }
            }
            (format!("miner-{m:02}"), runs)
        })
        .collect()
}

pub fn show(ui: &mut egui::Ui, state: &mut CoverageLanesState) {
    let domain = (T0, T0 + DAYS * DAY);
    let spine = state.spine.get_or_insert_with(|| SpineState::new(domain));

    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.per_machine, false, "Fleet");
        ui.selectable_value(&mut state.per_machine, true, "Per machine");
        ui.add_space(12.0);
        // The legend. Colour has to be decodable somewhere, and a three-state
        // encoding where one state is "no data" cannot rely on the reader
        // inferring it.
        legend_swatch(ui, coverage_tint(Coverage::Producing), "hashing");
        legend_swatch(ui, coverage_tint(Coverage::Idle), "dark (observed)");
        legend_swatch(ui, egui_widgets::UNOBSERVED, "unobserved");
    });
    ui.add_space(6.0);

    let label_w = 108.0;
    let sp = TimeSpine::new(spine)
        .left_inset(label_w)
        .brushing(true)
        .show(ui);

    let out = if state.per_machine {
        let lanes: Vec<CoverageLane<'_>> = state
            .machines
            .iter()
            .enumerate()
            .map(|(i, (name, runs))| CoverageLane {
                key: name,
                name: name.clone(),
                runs,
                active: state.active[i],
            })
            .collect();
        CoverageLanes::new(&lanes, &sp.scale, spine, &mut state.selection)
            .label_width(label_w)
            .lane_height(16.0)
            .show(ui)
    } else {
        let lanes = [CoverageLane {
            key: "fleet",
            name: "beartooth".into(),
            runs: &state.fleet,
            active: true,
        }];
        CoverageLanes::new(&lanes, &sp.scale, spine, &mut state.selection)
            .label_width(label_w)
            .lane_height(44.0)
            .show(ui)
    };

    // Nested rather than a let-chain: this crate is edition 2021.
    if let Some(key) = &out.toggled {
        if let Some(i) = state.machines.iter().position(|(n, _)| n == key) {
            state.active[i] = !state.active[i];
        }
    }

    ui.add_space(8.0);
    let w = out.window;
    ui.horizontal(|ui| {
        match w.uptime() {
            Some(u) => ui.label(format!("uptime {:.1}%", u * 100.0)),
            None => ui.colored_label(TEXT_MUTED, "uptime — nothing observed"),
        };
        ui.colored_label(TEXT_MUTED, "·");
        // Uptime divides by observed time, so it has to travel with the share
        // of the window nobody watched — otherwise 100% over one good hour
        // reads the same as 100% over a month.
        ui.colored_label(
            TEXT_MUTED,
            format!("blind {:.1}% of window", w.blind_spot() * 100.0),
        );
    });
}

fn legend_swatch(ui: &mut egui::Ui, col: egui::Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, col);
    ui.colored_label(TEXT_MUTED, label);
    ui.add_space(6.0);
}
