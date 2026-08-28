//! Theme states story — the interaction states, side by side, on every
//! background they can land on.
//!
//! **This is the template for contrast bugs.** Widget stories show a widget in
//! its resting state, which is exactly where contrast problems don't live:
//! they live in *selected*, *hovered*, *active* and *disabled*, and in the
//! translucent washes those states paint underneath text. A selected tab
//! shipped as accent-on-accent because no story ever drew one.
//!
//! Pair this with `tests/contrast.rs`, which pins the same combinations
//! numerically. The story is for seeing it; the test is for keeping it. Add a
//! row here whenever the theme gains a state, and an assertion there.

use crate::{ACCENT, TEXT_MUTED};
use egui::{Color32, RichText, Stroke};

/// The backgrounds a widget can sit on. Every state below is drawn on each.
const SURFACES: [(&str, Color32); 3] = [
    ("BG_PRIMARY", egui_widgets::theme::BG_PRIMARY),
    ("BG_SECONDARY", egui_widgets::theme::BG_SECONDARY),
    ("BG_HIGHLIGHT", egui_widgets::theme::BG_HIGHLIGHT),
];

pub fn show(ui: &mut egui::Ui) {
    ui.label(RichText::new("Theme States").color(ACCENT).strong());
    ui.label(
        RichText::new(
            "Interaction states on every surface. Contrast bugs hide in selected/hovered/\
             disabled and in the translucent washes under them — a resting-state story cannot \
             show them. Mirrored by assertions in tests/contrast.rs.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── Selectable tabs, the case that shipped broken ──────────────────
    ui.label(RichText::new("Selectable (tabs)").color(ACCENT).strong());
    ui.label(
        RichText::new(
            "egui's interact_selectable takes the selected label's TEXT colour from \
             visuals.selection.stroke and its fill from visuals.selection.bg_fill — so those two \
             are a foreground/background pair, not a border and a fill.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    for (name, surface) in SURFACES {
        surface_row(ui, name, surface, |ui| {
            let mut selected = 0usize;
            ui.horizontal(|ui| {
                for (i, label) in ["Summary", "Feed", "Stave"].iter().enumerate() {
                    // Draw index 0 as selected regardless of clicks: this is a
                    // specimen, not a control.
                    selected = 0;
                    let _ = ui.selectable_label(i == selected, *label);
                }
            });
        });
    }

    ui.add_space(16.0);

    // ── Buttons across states ──────────────────────────────────────────
    ui.label(RichText::new("Buttons").color(ACCENT).strong());
    ui.add_space(6.0);
    for (name, surface) in SURFACES {
        surface_row(ui, name, surface, |ui| {
            ui.horizontal(|ui| {
                let _ = ui.button("enabled");
                ui.add_enabled(false, egui::Button::new("disabled"));
                // Hover and active can't be forced from here, so paint the
                // fills directly beside a label of the text colour they carry.
                let v = ui.visuals().clone();
                swatch(
                    ui,
                    "hovered",
                    v.widgets.hovered.bg_fill,
                    v.widgets.hovered.fg_stroke,
                );
                swatch(
                    ui,
                    "active",
                    v.widgets.active.bg_fill,
                    v.widgets.active.fg_stroke,
                );
            });
        });
    }

    ui.add_space(16.0);

    // ── The selection wash, isolated ───────────────────────────────────
    ui.label(RichText::new("Selection wash").color(ACCENT).strong());
    ui.label(
        RichText::new(
            "Color32 stores PREMULTIPLIED channels: each must be <= alpha. from_rgba_premultiplied \
             with larger channels blends additively and renders far lighter than the tint intended \
             — the original cause of the unreadable tab.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    let sel = ui.visuals().selection;
    ui.horizontal(|ui| {
        for (name, surface) in SURFACES {
            ui.vertical(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, surface);
                ui.painter().rect_filled(rect, 4.0, sel.bg_fill);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "selected text",
                    egui::TextStyle::Body.resolve(ui.style()),
                    sel.stroke.color,
                );
                ui.label(RichText::new(name).color(TEXT_MUTED).small());
            });
        }
    });
}

/// A labelled strip painted on `surface`, so a state is judged against the
/// background it actually lands on rather than the panel default.
fn surface_row(
    ui: &mut egui::Ui,
    name: &str,
    surface: Color32,
    contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(surface)
        .inner_margin(8.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(TEXT_MUTED).small());
                ui.add_space(8.0);
                contents(ui);
            });
        });
    ui.add_space(4.0);
}

/// A fill with its own foreground colour written on it — for states that
/// can't be triggered from a static specimen.
fn swatch(ui: &mut egui::Ui, label: &str, fill: Color32, fg: Stroke) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 22.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        fg.color,
    );
}
