//! `BackgroundToasts` story — the decision table, not the animation.
//!
//! A toast that only appears after a delay is awkward to demonstrate live: the
//! interesting moments are "too soon to say", "now it is worth saying" and
//! "stopped, take it down", and they are a second apart. So this drives
//! [`BackgroundToasts::plan`] over a scripted clock and shows what it decided
//! at each step — which is also exactly what the unit tests assert.
//!
//! The live half underneath lets you start and stop real work and watch the
//! toasts follow.

use crate::{ACCENT, TEXT_MUTED};
use egui::RichText;
use egui_widgets::background::{BackgroundToasts, Job};
use egui_widgets::theme;

pub fn show(ui: &mut egui::Ui) {
    ui.label(RichText::new("Background Toasts").color(ACCENT).strong());
    ui.label(
        RichText::new(
            "Declare which jobs are running; the toasts follow. Owns the settle delay (so quick \
             work finishes silently), the quiet dismissal (finishing is not news), and the \
             repaint scheduling that makes the delay mean something on an idle surface.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── The decision table ─────────────────────────────────────────────
    ui.label(
        RichText::new("What it decides, over time")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        RichText::new(
            "A scripted clock through one job's life. `wake_after` is the part a hand-rolled \
             version always forgets: during the delay no toast exists, so nothing is asking for \
             frames, and the notice lands whenever the host next happens to redraw.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);

    let mut bg = BackgroundToasts::new().settle_after(0.4);
    let script: [(f64, bool, &str); 5] = [
        (0.0, true, "work starts"),
        (0.2, true, "still under the delay"),
        (0.5, true, "delay served"),
        (0.9, true, "still running"),
        (1.1, false, "work stops"),
    ];

    egui::Grid::new("bg_decisions")
        .num_columns(5)
        .spacing([16.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for h in ["t", "running", "show", "dismiss", "wake_after"] {
                ui.label(RichText::new(h).color(TEXT_MUTED).small());
            }
            ui.end_row();

            for (t, running, note) in script {
                let jobs: Vec<Job<'_>> = if running {
                    vec![Job::new("demo", "loading…")]
                } else {
                    Vec::new()
                };
                let plan = bg.plan(t, &jobs);
                ui.label(RichText::new(format!("{t:.1}s")).color(theme::TEXT_SECONDARY));
                ui.label(
                    RichText::new(if running { note } else { "—" })
                        .color(theme::TEXT_SECONDARY)
                        .small(),
                );
                ui.label(mark(!plan.show.is_empty()));
                ui.label(mark(!plan.dismiss.is_empty()));
                ui.label(
                    RichText::new(match plan.wake_after {
                        Some(w) => format!("{w:.2}s"),
                        None => "—".into(),
                    })
                    .color(theme::TEXT_SECONDARY)
                    .small(),
                );
                ui.end_row();
            }
        });

    ui.add_space(16.0);

    // ── Live ───────────────────────────────────────────────────────────
    ui.label(RichText::new("Live").color(ACCENT).strong());
    ui.label(
        RichText::new(
            "Toggle work on and off. The quick one finishes inside the delay and is never \
             announced — which is the point, not a bug.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);

    let id = ui.id().with("bg_live");
    let mut running: Vec<String> = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
    ui.horizontal(|ui| {
        for (key, label) in [
            ("naming", "naming wallets"),
            ("listings", "loading listings"),
        ] {
            let mut on = running.iter().any(|k| k == key);
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    running.push(key.to_string());
                } else {
                    running.retain(|k| k != key);
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, running.clone()));

    let mut live_bg: BackgroundToasts =
        ui.data_mut(|d| d.get_temp(id.with("state")).unwrap_or_default());
    let mut queue: egui_widgets::toast::ToastQueue =
        ui.data_mut(|d| d.get_temp(id.with("toasts")).unwrap_or_default());
    let jobs: Vec<Job<'_>> = running
        .iter()
        .map(|k| {
            Job::new(
                k.as_str(),
                if k == "naming" {
                    "naming wallets — 40/231"
                } else {
                    "loading listings…"
                },
            )
            .progress((k == "naming").then_some(0.17))
        })
        .collect();
    live_bg.sync(ui.ctx(), &mut queue, &jobs);
    ui.data_mut(|d| {
        d.insert_temp(id.with("state"), live_bg);
        d.insert_temp(id.with("toasts"), queue.clone());
    });
    egui_widgets::toast::show_toasts(ui.ctx(), &mut queue);
    ui.data_mut(|d| d.insert_temp(id.with("toasts"), queue));
}

fn mark(on: bool) -> RichText {
    if on {
        RichText::new("●").color(theme::SUCCESS)
    } else {
        RichText::new("·").color(theme::TEXT_MUTED)
    }
}
