//! Story: `Breakpoint` — what the current viewport classifies as, and what
//! every layout decision derived from it resolves to right now.
//!
//! This story is a **readout**, not a gallery: the widget has no marks of its
//! own, and the only useful thing to show is what it answers at the width you
//! are looking at. Resize the window (or drive it with `cdp-shot.mjs` at
//! 390 / 834 / 1440) and watch the highlighted row move.

use egui_widgets::Breakpoint;

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Breakpoint").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "How wide the surface is, as three named sizes rather than a number \
             every call site re-compares. Read from content_rect, so the notch \
             and status bar are already excluded.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let width = ui.ctx().content_rect().width();
    let current = Breakpoint::from_ctx(ui.ctx());

    ui.label(
        egui::RichText::new(format!("{width:.0}pt → {}", current.label()))
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(10.0);

    // The readout renders ITSELF through the decision it is documenting. That
    // is not a flourish: the 8-column grid is ~560pt wide, and at 390 it
    // widened the containing Ui past the viewport, which stopped every
    // wrapping label BELOW it from wrapping. The story shipped demonstrating
    // the exact failure it exists to explain.
    let rows: Vec<(Breakpoint, [String; 6])> = Breakpoint::ALL
        .iter()
        .map(|&bp| {
            (
                bp,
                [
                    format!("{:?}", bp.panel_mode()),
                    format!("{:?}", bp.record_layout()),
                    format!("{:?}", bp.header_layout()),
                    format!("{:.0}pt", bp.min_touch()),
                    format!("{:.0}pt", bp.gutter()),
                    format!("{}", bp.columns()),
                ],
            )
        })
        .collect();
    const FIELDS: [&str; 6] = [
        "panel",
        "records",
        "header",
        "min touch",
        "gutter",
        "columns",
    ];

    match current.record_layout() {
        egui_widgets::RecordLayout::Table => {
            egui::Grid::new("breakpoint_table")
                .num_columns(FIELDS.len() + 2)
                .spacing([14.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for h in FIELDS {
                        ui.label(egui::RichText::new(h).color(TEXT_MUTED).small());
                    }
                    ui.label("");
                    ui.end_row();

                    for (bp, values) in &rows {
                        let is_current = *bp == current;
                        let colour = if is_current { ACCENT } else { TEXT_MUTED };
                        ui.label(egui::RichText::new(bp.label()).color(colour).strong());
                        for v in values {
                            ui.label(egui::RichText::new(v).color(colour).small());
                        }
                        ui.label(
                            egui::RichText::new(if is_current { "you are here" } else { "" })
                                .color(colour)
                                .small(),
                        );
                        ui.end_row();
                    }
                });
        }
        egui_widgets::RecordLayout::Cards => {
            for (bp, values) in &rows {
                let is_current = *bp == current;
                let colour = if is_current { ACCENT } else { TEXT_MUTED };
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(bp.label()).color(colour).strong());
                        if is_current {
                            ui.label(egui::RichText::new("you are here").color(ACCENT).small());
                        }
                    });
                    for (field, value) in FIELDS.iter().zip(values) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(*field).color(TEXT_MUTED).small());
                            ui.label(egui::RichText::new(value).color(colour).small());
                        });
                    }
                });
            }
        }
    }

    ui.add_space(14.0);
    ui.label(
        egui::RichText::new(
            "Compact tops out at 700pt because that is a 320pt side panel plus a \
             380pt content column — the width below which having both stops \
             being possible. The number follows the decision it exists to make, \
             not a device.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "There is no is_compact(). The first three columns are the whole \
             point: a call site matches on the DECISION, so where Medium was \
             grouped with Wide is visible here rather than re-decided (and \
             silently forgotten) at each use.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
}
