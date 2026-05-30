//! `IdPill` storybook story.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::IdPill;

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

    // ── Typical use cases ──────────────────────────────────────────────
    ui.label(egui::RichText::new("Common uses").color(ACCENT).strong());
    ui.add_space(4.0);
    let policy = "8532f316dd0973a8e2c5b7d0fa194deebd4451aabdfe3a8c2bd45d87a1b";
    let stake = "stake1u9k5j8d4xv5c98xy7d3jhgf2pvqd0u4ldyk2pz0t3w8nye6jyq25fr";
    let addr = "addr_test1qpx5d3p6vhwl8sqkek74p2lz5w2sse5e0wpzm2ynj3aqlk4qfn7lm";
    let tx = "fa9bfb46c8d3f4e5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8fe407d";

    IdPill::new("policy", policy).show(ui);
    ui.add_space(3.0);
    IdPill::new("wallet", addr).with_widths(14, 8).show(ui);
    ui.add_space(3.0);
    IdPill::new("stake", stake).with_widths(14, 8).show(ui);
    ui.add_space(3.0);
    IdPill::new("tx", tx).with_widths(12, 6).show(ui);

    ui.add_space(16.0);

    // ── No label / short value ─────────────────────────────────────────
    ui.label(
        egui::RichText::new("No label, short value, no copy")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Pass `\"\"` for the label to drop it. Short values render verbatim \
             (no truncation needed). `copyable(false)` suppresses the copy button \
             for read-only contexts.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    IdPill::new("", "abc123").show(ui);
    ui.add_space(3.0);
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
    ui.add_space(4.0);
    IdPill::new("policy", policy)
        .with_short("8532…d87a1b")
        .show(ui);
}
