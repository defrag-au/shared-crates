//! `PhaseCard` storybook story — every common state of a phase row.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{GateChip, PhaseCard, PhaseCardAction, PhaseCardRow};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("PhaseCard").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Read-only display of one mint phase row — header (name + status + \
             priority + Edit/Delete), properties (price / window / per-wallet), \
             gates strip with × remove + Add gate. The edit form lives in the \
             host beside its form state; the card emits Edit/Delete/AddGate/RemoveGate \
             actions for the host to dispatch.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── 1. Public phase, FREE, unbounded, one public gate ──────────────
    section(ui, "Active public phase (default seed)");
    let row = PhaseCardRow {
        phase_key: "public".into(),
        name: "Public".into(),
        price_display: "FREE".into(),
        window_display: "unbounded to unbounded".into(),
        per_wallet_display: "unlimited".into(),
        active: true,
        priority: 0,
    };
    let gates = vec![GateChip {
        gate_id: 1,
        label: "public".into(),
    }];
    let resp = PhaseCard::new(&row, &gates).show(ui);
    surface_actions(ui, &resp.actions);

    ui.add_space(12.0);

    // ── 2. Allowlist phase with multiple gates ─────────────────────────
    section(ui, "Allowlist phase, 50 ADA, time-bounded, two gates");
    let row = PhaseCardRow {
        phase_key: "allowlist".into(),
        name: "Allowlist".into(),
        price_display: "50 ADA".into(),
        window_display: "2026-06-01T09:00:00 to 2026-06-01T18:00:00".into(),
        per_wallet_display: "3".into(),
        active: true,
        priority: 20,
    };
    let gates = vec![
        GateChip {
            gate_id: 12,
            label: "allowlist".into(),
        },
        GateChip {
            gate_id: 13,
            label: "token_held(8532f316…, min 3)".into(),
        },
    ];
    let resp = PhaseCard::new(&row, &gates).show(ui);
    surface_actions(ui, &resp.actions);

    ui.add_space(12.0);

    // ── 3. Paused (active=false) — operator's emergency stop ───────────
    section(ui, "Paused — operator-flipped `active = false`");
    let row = PhaseCardRow {
        phase_key: "og".into(),
        name: "OG".into(),
        price_display: "30 ADA".into(),
        window_display: "2026-06-01T08:00:00 to 2026-06-01T09:00:00".into(),
        per_wallet_display: "5".into(),
        active: false,
        priority: 30,
    };
    let gates = vec![GateChip {
        gate_id: 5,
        label: "token_held(b3dab69f…, min 1)".into(),
    }];
    let resp = PhaseCard::new(&row, &gates).show(ui);
    surface_actions(ui, &resp.actions);

    ui.add_space(12.0);

    // ── 4. Phase with zero gates — fail-safe warning ───────────────────
    section(ui, "Phase with zero gates (fail-safe: nobody eligible)");
    let row = PhaseCardRow {
        phase_key: "draft".into(),
        name: "Stealth drop".into(),
        price_display: "25 ADA".into(),
        window_display: "unbounded to 2026-12-31T23:59:59".into(),
        per_wallet_display: "1".into(),
        active: true,
        priority: 10,
    };
    let resp = PhaseCard::new(&row, &[]).show(ui);
    surface_actions(ui, &resp.actions);

    ui.add_space(12.0);

    // ── 5. Read-only audit (no Edit/Delete/Add) ────────────────────────
    section(ui, "Audit mode — Edit/Delete/AddGate suppressed");
    let row = PhaseCardRow {
        phase_key: "archived".into(),
        name: "Public (archived)".into(),
        price_display: "FREE".into(),
        window_display: "2025-12-01T00:00:00 to 2025-12-31T23:59:59".into(),
        per_wallet_display: "unlimited".into(),
        active: false,
        priority: 0,
    };
    let gates = vec![GateChip {
        gate_id: 99,
        label: "public".into(),
    }];
    let _ = PhaseCard::new(&row, &gates)
        .with_edit(false)
        .with_delete(false)
        .with_add_gate(false)
        .with_gate_remove(false)
        .show(ui);
}

fn section(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).color(ACCENT).strong());
    ui.add_space(4.0);
}

fn surface_actions(ui: &mut egui::Ui, actions: &[PhaseCardAction]) {
    for action in actions {
        let msg = match action {
            PhaseCardAction::Edit => "Edit clicked".to_string(),
            PhaseCardAction::Delete => "Delete confirmed".to_string(),
            PhaseCardAction::AddGate => "+ Add gate clicked".to_string(),
            PhaseCardAction::RemoveGate { gate_id } => {
                format!("Remove gate {gate_id}")
            }
        };
        ui.label(
            egui::RichText::new(msg)
                .small()
                .color(egui::Color32::LIGHT_GREEN),
        );
    }
}
