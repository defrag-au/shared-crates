//! `IdPill` storybook story.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{id_pill_stacked_width_for, IdPill, IdPillLayout};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("IdPill").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Truncated identifier + copy button. The recurring \
             `policy 8532f316dd09…45d87a1b 📋` pattern used for policy_id, \
             wallet addresses, deposit addresses, tx hashes. Hover the value \
             to see the full string in a tooltip.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── Stacked (default) ──────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Stacked — default")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Three-row framed pill: small muted label header, monospace value body, \
             right-aligned copy footer. Shows the FULL value when it fits the parent's \
             available width; falls back to middle-elision only when constrained. \
             Best for standalone display stacks (the typical policy / wallet / \
             stake / tx group on a dashboard).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    let policy = "8532f316dd0973a8e2c5b7d0fa194deebd4451aabdfe3a8c2bd45d87a1b";
    let stake = "stake1u9k5j8d4xv5c98xy7d3jhgf2pvqd0u4ldyk2pz0t3w8nye6jyq25fr";
    let addr = "addr_test1qpx5d3p6vhwl8sqkek74p2lz5w2sse5e0wpzm2ynj3aqlk4qfn7lm";
    let tx = "fa9bfb46c8d3f4e5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8fe407d";

    // Normalize frame width across the stack — measure each value's
    // natural Stacked width and take the max so every pill's right
    // border lines up regardless of value length.
    let stack_w = [policy, addr, stake, tx]
        .iter()
        .map(|v| id_pill_stacked_width_for(ui, v, true))
        .fold(0.0_f32, f32::max);
    IdPill::new("policy", policy).min_width(stack_w).show(ui);
    ui.add_space(4.0);
    IdPill::new("wallet", addr).min_width(stack_w).show(ui);
    ui.add_space(4.0);
    IdPill::new("stake", stake).min_width(stack_w).show(ui);
    ui.add_space(4.0);
    IdPill::new("tx", tx).min_width(stack_w).show(ui);

    ui.add_space(16.0);

    // ── Stacked, constrained ───────────────────────────────────────────
    ui.label(
        egui::RichText::new("Stacked — constrained width")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Wrap the pill in a narrow container and it auto-elides without any \
             extra config. The width budget is what the parent hands the frame.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    ui.allocate_ui(egui::vec2(220.0, 0.0), |ui| {
        IdPill::new("policy", policy).show(ui);
    });

    ui.add_space(16.0);

    // ── Inline ─────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("Inline").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Single-row compact shape: `label  value [Copy]`, no frame. \
             Always truncates per `with_widths`. Use this inside header bars \
             where vertical budget is tight (e.g. the portal's Configure window \
             header). `label_min_width` reserves a column so a vertical stack of \
             Inline pills aligns its values.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    const LABEL_W: f32 = 48.0;
    IdPill::new("policy", policy)
        .layout(IdPillLayout::Inline)
        .label_min_width(LABEL_W)
        .show(ui);
    ui.add_space(3.0);
    IdPill::new("wallet", addr)
        .layout(IdPillLayout::Inline)
        .label_min_width(LABEL_W)
        .show(ui);
    ui.add_space(3.0);
    IdPill::new("stake", stake)
        .layout(IdPillLayout::Inline)
        .label_min_width(LABEL_W)
        .show(ui);
    ui.add_space(3.0);
    IdPill::new("tx", tx)
        .layout(IdPillLayout::Inline)
        .label_min_width(LABEL_W)
        .show(ui);

    ui.add_space(16.0);

    // ── No label / short value ─────────────────────────────────────────
    ui.label(
        egui::RichText::new("No label, short value, no copy")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Pass `\"\"` for the label to drop the header row. Short values render \
             verbatim (no truncation needed). `copyable(false)` suppresses the copy \
             button for read-only contexts.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    IdPill::new("", "abc123").show(ui);
    ui.add_space(4.0);
    IdPill::new("ref", policy).copyable(false).show(ui);

    ui.add_space(16.0);

    // ── Pre-truncated value (host-provided) ────────────────────────────
    ui.label(
        egui::RichText::new("Host-provided truncation")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Hosts that already have a pre-formatted display string (e.g. from \
             a row VM) pass it via `with_short`. The full value still goes to \
             the clipboard on copy.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);
    IdPill::new("policy", policy)
        .with_short("8532…d87a1b")
        .show(ui);
}
