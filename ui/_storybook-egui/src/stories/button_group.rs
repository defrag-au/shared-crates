//! `ButtonGroup` storybook story — text-only, with-icons, disabled,
//! single-row (no wrap), and a realistic collection-card action bar.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{ButtonGroup, ButtonGroupButton, PhosphorIcon};

#[derive(Default)]
pub struct ButtonGroupState {
    pub last_clicked: Option<u64>,
}

pub fn show(ui: &mut egui::Ui, state: &mut ButtonGroupState) {
    ui.label(egui::RichText::new("ButtonGroup").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Row of related action buttons with shared layout. Each button \
             gets a caller-supplied `id`; the group's response carries the \
             clicked id so the host dispatches on a single matched value. \
             Default layout is horizontal_wrapped — narrow surfaces spill \
             onto a second line instead of overlapping siblings.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── 1. Text-only action group ──────────────────────────────────────
    section(ui, "Plain text labels");
    let resp = ButtonGroup::new()
        .add(ButtonGroupButton::new(11, "Save"))
        .add(ButtonGroupButton::new(12, "Cancel"))
        .add(ButtonGroupButton::new(13, "Reset"))
        .show(ui);
    capture(state, resp.clicked);

    ui.add_space(12.0);

    // ── 2. With icons + tooltips ──────────────────────────────────────
    section(ui, "Icon + label + tooltip (per-button)");
    let resp = ButtonGroup::new()
        .add(
            ButtonGroupButton::new(21, "Configure")
                .icon(PhosphorIcon::Gear)
                .hover_text("Edit phases, gates, and allowlist"),
        )
        .add(
            ButtonGroupButton::new(22, "Scan")
                .icon(PhosphorIcon::MagnifyingGlass)
                .hover_text("Walk the deposit address for unspent payments"),
        )
        .add(ButtonGroupButton::new(23, "Copy policy_id").icon(PhosphorIcon::Copy))
        .show(ui);
    capture(state, resp.clicked);

    ui.add_space(12.0);

    // ── 3. Disabled state ─────────────────────────────────────────────
    section(ui, "Disabled (e.g. action in flight)");
    ui.label(
        egui::RichText::new(
            "Disabled buttons render greyed-out and unclickable. Their \
             `hover_text` becomes the disabled-hover hint so the operator \
             sees the reason a button is unavailable.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    let resp = ButtonGroup::new()
        .add(
            ButtonGroupButton::new(31, "Refuelling…")
                .enabled(false)
                .hover_text("Refuel tx in flight — wait for the ack"),
        )
        .add(ButtonGroupButton::new(32, "Activity"))
        .add(
            ButtonGroupButton::new(33, "Scanning…")
                .icon(PhosphorIcon::MagnifyingGlass)
                .enabled(false)
                .hover_text("A scan is in flight"),
        )
        .show(ui);
    capture(state, resp.clicked);

    ui.add_space(12.0);

    // ── 4. Single-row (no wrap) — toolbar use case ────────────────────
    section(ui, "Single row (no wrap) — toolbar");
    let resp = ButtonGroup::new()
        .wrap(false)
        .spacing(8.0)
        .add(ButtonGroupButton::new(41, "New").icon(PhosphorIcon::Plus))
        .add(ButtonGroupButton::new(42, "Open").icon(PhosphorIcon::Eye))
        .add(
            ButtonGroupButton::new(43, "Delete")
                .icon(PhosphorIcon::Trash)
                .hover_text("Permanent — no undo"),
        )
        .show(ui);
    capture(state, resp.clicked);

    ui.add_space(12.0);

    // ── 5. Realistic collection-card action bar ───────────────────────
    section(ui, "Collection-card action bar (real use case)");
    ui.label(
        egui::RichText::new(
            "The action bar from the portal's collection card: Test mint, \
             Activity, Configure, Scan, + Ingest, + Seed stubs. This is the \
             pattern `collection_list.rs` will use after the refactor.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    let resp = ButtonGroup::new()
        .add(ButtonGroupButton::new(51, "Test mint"))
        .add(ButtonGroupButton::new(52, "Activity"))
        .add(
            ButtonGroupButton::new(53, "Configure")
                .icon(PhosphorIcon::Gear)
                .hover_text("Edit phases / gates / allowlist"),
        )
        .add(
            ButtonGroupButton::new(54, "Scan")
                .icon(PhosphorIcon::MagnifyingGlass)
                .hover_text("Walk deposit address for unspent payments"),
        )
        .add(ButtonGroupButton::new(55, "+ Ingest"))
        .add(ButtonGroupButton::new(56, "+ Seed stubs"))
        .show(ui);
    capture(state, resp.clicked);

    // ── Action receipt ────────────────────────────────────────────────
    if let Some(id) = state.last_clicked {
        ui.add_space(12.0);
        ui.colored_label(egui::Color32::LIGHT_GREEN, format!("last clicked: id {id}"));
    }
}

fn section(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).color(ACCENT).strong());
    ui.add_space(4.0);
}

fn capture(state: &mut ButtonGroupState, clicked: Option<u64>) {
    if let Some(id) = clicked {
        state.last_clicked = Some(id);
    }
}
