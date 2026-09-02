//! `Skeleton` storybook story — the two reasons side by side, because the
//! whole point of the widget is that they must not look alike.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{Skeleton, SkeletonReason};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Skeleton").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Placeholder shapes for content that is not on screen — and a statement of WHY. \
             The reason is a constructor argument, not a setter, so a call site cannot draw one \
             without saying which it means.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── The distinction the widget exists for ───────────────────────────
    ui.label(
        egui::RichText::new("Loading vs Withheld")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Watch them: the left PULSES, the right does not. Loading means \"wait\", and motion \
             is how a surface says so. Withheld means waiting produces nothing — a shimmer over \
             a paywall is a lie told in animation, training a reader to sit for rows that will \
             never arrive. The withheld rows also recede, which says the list continues past \
             what is readable.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        for (label, reason) in [
            ("Loading — on its way", SkeletonReason::Loading),
            ("Withheld — not coming", SkeletonReason::Withheld),
        ] {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                ui.label(egui::RichText::new(label).color(TEXT_MUTED).small());
                ui.add_space(6.0);
                Skeleton::rows(3, reason).show(ui);
            });
            ui.add_space(24.0);
        }
    });

    ui.add_space(20.0);

    // ── In place, which is the only way to judge the handover ───────────
    ui.label(
        egui::RichText::new("In place: real rows, then the gate")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A list that stops looks identical to a list that was always empty, and the \
             difference can be enormous — one wallet has three thousand transactions behind a \
             window, the other has never done anything. Match `row_height` to the real row above \
             or the handover reads as a layout jump rather than the same list continuing.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .inner_margin(egui::Margin::same(12))
        .corner_radius(6.0)
        .show(ui, |ui| {
            for (title, detail) in [
                ("Offer accepted · wayup", "175 ₳ · 2 items"),
                ("Sent · $boef", "63.84 ₳"),
            ] {
                fake_row(ui, title, detail);
            }
            Skeleton::rows(3, SkeletonReason::Withheld).show(ui);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("3,201 more before 2026-08-26")
                    .color(TEXT_MUTED)
                    .small(),
            );
        });

    ui.add_space(20.0);

    // ── The other shape ─────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Blocks — thumbnails and panels")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "One rectangle, sized by the caller, reserving the space so the layout does not jump \
             when the image lands. An asset still being fetched is Loading; one the reader is \
             not entitled to see is Withheld.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for reason in [SkeletonReason::Loading, SkeletonReason::Withheld] {
            Skeleton::block(egui::vec2(72.0, 72.0), reason).show(ui);
        }
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("loading · withheld")
                    .color(TEXT_MUTED)
                    .small(),
            );
            ui.label(
                egui::RichText::new("same shape, different motion")
                    .color(TEXT_MUTED)
                    .small(),
            );
        });
    });

    ui.add_space(20.0);

    // ── Shaping it to what it replaces ──────────────────────────────────
    ui.label(
        egui::RichText::new("Shaped to its content")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "`bars` is the rhythm of the row it stands in for. The COUNT of rows is a visual \
             quantity chosen by the caller — never the number of hidden items. A placeholder \
             that leaked the shape of its content would defeat the gate it illustrates.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        for (label, widths, height) in [
            ("plain list", vec![0.42], 34.0),
            ("card + detail", vec![0.34, 0.18], 54.0),
            ("dense", vec![0.5, 0.3, 0.16], 72.0),
        ] {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.label(egui::RichText::new(label).color(TEXT_MUTED).small());
                ui.add_space(4.0);
                Skeleton::rows(2, SkeletonReason::Withheld)
                    .bars(widths)
                    .row_height(height)
                    .show(ui);
            });
            ui.add_space(16.0);
        }
    });
}

/// A stand-in for a real row, so the handover into placeholders can be judged.
fn fake_row(ui: &mut egui::Ui, title: &str, detail: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).strong());
                ui.label(egui::RichText::new(detail).color(TEXT_MUTED).small());
            });
        });
    ui.add_space(6.0);
}
