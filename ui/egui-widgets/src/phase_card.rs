//! `PhaseCard` — read-only display of one mint phase row.
//!
//! Composes [`Chip`], [`PropertyList`], and gate chips into the visual
//! unit operators land on in the portal's Configure window. Header
//! shows the phase name + status + priority + Edit/Delete; body shows
//! Price / Window / Per-wallet via `PropertyList`; gates strip lists the
//! phase's gates as removable `Chip`s with an "+ Add gate" affordance.
//!
//! **Read-only by design.** The edit form is host-state-heavy (text
//! buffers, validation, submit-on-Enter) and lives in the application
//! beside the rest of its form state. The card surfaces an `Edit`
//! action so the host can swap it for its form in the layout.
//!
//! The allowlist sub-section is intentionally *not* part of this widget
//! — allowlist rows are a list, not a phase property, and they have
//! their own forms. Host stacks them below the card.
//!
//! ## VM
//!
//! The host projects its `SalePhaseConfig` (and matching gate rows) into
//! a [`PhaseCardRow`] + a `Vec<GateChip>`. Pre-formatted strings (price
//! as ADA, window as ISO) live in the VM — the widget renders verbatim
//! and never does formatting that needs i18n or wall-clock arithmetic.
//!
//! ## Example
//!
//! ```ignore
//! let row = PhaseCardRow {
//!     phase_key: "public".into(),
//!     name: "Public".into(),
//!     price_display: "FREE".into(),
//!     window_display: "unbounded → unbounded".into(),
//!     per_wallet_display: "unlimited".into(),
//!     active: true,
//!     priority: 0,
//! };
//! let gates = vec![GateChip { gate_id: 1, label: "public".into() }];
//! let resp = PhaseCard::new(&row, &gates).show(ui);
//! for action in resp.actions {
//!     match action {
//!         PhaseCardAction::Edit => { … }
//!         PhaseCardAction::Delete => { … }
//!         PhaseCardAction::AddGate => { … }
//!         PhaseCardAction::RemoveGate { gate_id } => { … }
//!     }
//! }
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::chip::{Chip, ChipVariant};
use crate::property_list::PropertyList;

/// View-model for the header + properties section. The widget renders
/// `*_display` strings verbatim — the host owns formatting.
#[derive(Clone, Debug)]
pub struct PhaseCardRow {
    /// Immutable identifier from the data layer. Used for egui Id
    /// salting + as the gate-list key.
    pub phase_key: String,
    pub name: String,
    /// Pre-formatted price (e.g. `"FREE"`, `"50 ADA"`, `"3.5 ADA"`).
    pub price_display: String,
    /// Pre-formatted time window (e.g.
    /// `"2026-05-30T09:00:00 → unbounded"`).
    pub window_display: String,
    /// Pre-formatted per-wallet cap (e.g. `"3"`, `"unlimited"`).
    pub per_wallet_display: String,
    /// Drives the status chip's variant (Success vs Muted).
    pub active: bool,
    /// `sort_order` from the data layer. Rendered as `priority N`.
    pub priority: i32,
}

/// One gate as the widget renders it. The host pre-formats the chip
/// label (e.g. `"token_held(8532f316…, min 3)"`) so the widget doesn't
/// know about gate-type semantics.
#[derive(Clone, Debug)]
pub struct GateChip {
    pub gate_id: i64,
    pub label: String,
}

/// Builder.
pub struct PhaseCard<'a> {
    row: &'a PhaseCardRow,
    gates: &'a [GateChip],
    show_edit: bool,
    show_delete: bool,
    show_add_gate: bool,
    show_gate_remove: bool,
}

/// Action emitted by an interaction inside the card.
#[derive(Clone, Debug)]
pub enum PhaseCardAction {
    /// Edit button clicked. Host opens its phase-edit form.
    Edit,
    /// Delete button clicked AND already confirmed (the widget owns the
    /// click-to-confirm flow via egui memory). Host runs the delete.
    Delete,
    /// `+ Add gate` clicked. Host opens its new-gate form.
    AddGate,
    /// `×` on a gate chip clicked. Host removes the gate row.
    RemoveGate { gate_id: i64 },
}

/// Drained actions for this frame — empty on a pure-render pass.
#[derive(Default, Debug)]
pub struct PhaseCardResponse {
    pub actions: Vec<PhaseCardAction>,
}

impl<'a> PhaseCard<'a> {
    /// Construct a card. All affordances default to enabled — call
    /// `with_*(false)` to suppress on a read-only audit view.
    pub fn new(row: &'a PhaseCardRow, gates: &'a [GateChip]) -> Self {
        Self {
            row,
            gates,
            show_edit: true,
            show_delete: true,
            show_add_gate: true,
            show_gate_remove: true,
        }
    }

    /// Show or hide the Edit button.
    pub fn with_edit(mut self, enabled: bool) -> Self {
        self.show_edit = enabled;
        self
    }

    /// Show or hide the Delete button.
    pub fn with_delete(mut self, enabled: bool) -> Self {
        self.show_delete = enabled;
        self
    }

    /// Show or hide the `+ Add gate` button.
    pub fn with_add_gate(mut self, enabled: bool) -> Self {
        self.show_add_gate = enabled;
        self
    }

    /// Show or hide the `×` remove affordance on individual gate chips.
    pub fn with_gate_remove(mut self, enabled: bool) -> Self {
        self.show_gate_remove = enabled;
        self
    }

    /// Render the card.
    pub fn show(self, ui: &mut Ui) -> PhaseCardResponse {
        let mut response = PhaseCardResponse::default();
        // Scope every child Id to this phase_key so the internal `Grid`
        // (used by `PropertyList`) + the delete-confirm flag stay unique
        // when multiple phase cards stack in one Ui. egui's id system is
        // hierarchical — `push_id` salts every descendant.
        let salt = ("phase_card", self.row.phase_key.clone());
        ui.push_id(salt, |ui| {
            self.show_inner(ui, &mut response);
        });
        response
    }

    fn show_inner(self, ui: &mut Ui, response: &mut PhaseCardResponse) {
        Frame::new()
            .fill(Color32::from_rgb(22, 24, 32))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 60)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                // ── Header: name + status chip + priority + Edit/Delete ──
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.row.name).strong());
                    let chip = if self.row.active {
                        Chip::new("active").variant(ChipVariant::Success)
                    } else {
                        Chip::new("paused").variant(ChipVariant::Muted)
                    };
                    chip.show(ui);
                    ui.label(
                        RichText::new(format!("priority {}", self.row.priority))
                            .small()
                            .color(Color32::from_gray(140)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.show_delete && delete_button(ui, &self.row.phase_key) {
                            response.actions.push(PhaseCardAction::Delete);
                        }
                        if self.show_edit && ui.small_button("Edit").clicked() {
                            response.actions.push(PhaseCardAction::Edit);
                        }
                    });
                });

                ui.add_space(4.0);

                // ── Properties ──────────────────────────────────────
                PropertyList::new()
                    .id("phase_card_props")
                    .add("Price", &self.row.price_display)
                    .add("Window", &self.row.window_display)
                    .add("Per wallet", &self.row.per_wallet_display)
                    .show(ui);

                ui.add_space(8.0);

                // ── Gates strip ─────────────────────────────────────
                ui.label(
                    RichText::new("Gates")
                        .small()
                        .color(Color32::from_gray(150)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if self.gates.is_empty() {
                        Chip::new("none — phase ineligible")
                            .variant(ChipVariant::Danger)
                            .show(ui);
                    }
                    for gate in self.gates {
                        let chip_resp = Chip::new(&gate.label)
                            .variant(ChipVariant::Tag)
                            .removable(self.show_gate_remove)
                            .show(ui);
                        if chip_resp.removed {
                            response.actions.push(PhaseCardAction::RemoveGate {
                                gate_id: gate.gate_id,
                            });
                        }
                    }
                });

                if self.show_add_gate {
                    ui.add_space(2.0);
                    if ui.small_button("+ Add gate").clicked() {
                        response.actions.push(PhaseCardAction::AddGate);
                    }
                }
            });
    }
}

/// Click-to-confirm Delete button. Returns `true` when the click is the
/// confirmation (second click). Confirmation state lives in egui memory
/// keyed by `phase_key`, so multiple cards on the same screen don't
/// share a flag.
fn delete_button(ui: &mut Ui, phase_key: &str) -> bool {
    let confirm_id = ui.id().with(("phase_card_delete_confirm", phase_key));
    let confirming = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(confirm_id))
        .unwrap_or(false);
    let (label, colour) = if confirming {
        ("Confirm delete", Color32::LIGHT_RED)
    } else {
        ("Delete", Color32::from_gray(180))
    };
    let clicked = ui
        .small_button(RichText::new(label).small().color(colour))
        .on_hover_text(if confirming {
            "Click again to delete this phase + its gates + allowlist"
        } else {
            "Delete this phase"
        })
        .clicked();
    if clicked {
        if confirming {
            ui.ctx().data_mut(|d| d.remove::<bool>(confirm_id));
            return true;
        } else {
            ui.ctx().data_mut(|d| d.insert_temp(confirm_id, true));
        }
    }
    false
}
