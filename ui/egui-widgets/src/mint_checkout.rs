//! `MintCheckout` — the buyer-facing mint offer + CTA, composed as one widget.
//!
//! Owns the full offer layout so hosts don't hand-roll (and mis-nest) it:
//! phase + eligibility chips, a [`QuantityStepper`], price-each / total cards,
//! optional fixed-price **bundle** cards, a purchase summary, the Mint button,
//! and the post-submit progress / submitted / error states.
//!
//! VM-driven and stateless: the host projects its quote + flow state into a
//! [`MintCheckoutVm`], and `show` returns [`MintCheckoutAction`]s for the host
//! to dispatch (re-quote, build+sign, select a bundle). The widget never spawns
//! async work and never formats wall-clock or i18n-sensitive values beyond ADA.
//!
//! ```ignore
//! let vm = MintCheckoutVm { /* projected from the engine quote + flow */ };
//! for action in MintCheckout::new(&vm).show(ui).actions {
//!     match action {
//!         MintCheckoutAction::QtyChanged(n) => { /* total recomputes locally */ }
//!         MintCheckoutAction::Mint => { /* build → sign → submit */ }
//!         MintCheckoutAction::SelectBundle(sku) => { /* build the bundle */ }
//!     }
//! }
//! ```

use egui::{Color32, RichText, Ui};

use crate::chip::{Chip, ChipVariant};
use crate::error_note::ErrorNote;
use crate::quantity_stepper::QuantityStepper;
use crate::theme::{Radius, Space, SpaceExt, ThemeExt};
use crate::utils::{format_lovelace, truncate_hex};

/// Whether this wallet may mint right now, in the active phase.
pub enum Eligibility {
    /// Eligible — `max_per_wallet` is the remaining per-wallet allocation.
    Eligible { max_per_wallet: u32 },
    /// Not eligible — `reason` is shown verbatim (host owns the wording).
    Ineligible { reason: String },
}

/// A fixed-size, fixed-price bundle offered in the active phase (e.g. "11 for
/// 700"). The host derives these from the engine's bundle offers.
pub struct BundleOffer {
    /// Stable id sent back on selection (the `phase_bundles.sku_key`).
    pub sku_key: String,
    /// Display label, e.g. "11-pack".
    pub label: String,
    /// Units delivered (the fulfilment quota).
    pub size: u32,
    /// Fixed total for the pack, in lovelace.
    pub price_lovelace: u64,
    /// `false` → rendered disabled ("sold out").
    pub available: bool,
}

/// Where the mint flow is, post-Mint.
pub enum CheckoutState {
    /// Nothing in flight — the Mint button is live.
    Idle,
    /// Async work in progress; the string is a human status line.
    Working(String),
    /// Payment submitted; `status` is the order's current lifecycle status.
    Submitted { tx: String, status: String },
    /// A failure to surface (rendered via [`ErrorNote`]).
    Error(String),
}

/// Everything `MintCheckout` renders — projected by the host from its quote +
/// flow state.
pub struct MintCheckoutVm {
    /// Active phase label (e.g. "public"); `None` hides the phase chip.
    pub phase_label: Option<String>,
    pub eligibility: Eligibility,
    /// Per-unit price for singles, in lovelace.
    pub unit_price_lovelace: u64,
    /// Current single quantity (the stepper value).
    pub qty: u32,
    /// Bundles offered this phase (empty = none).
    pub bundles: Vec<BundleOffer>,
    pub state: CheckoutState,
}

/// What the host should act on after `show`.
pub enum MintCheckoutAction {
    /// The stepper changed; re-derive the total (locally — no re-quote).
    QtyChanged(u32),
    /// Mint the current single quantity.
    Mint,
    /// Mint the named bundle SKU.
    SelectBundle(String),
}

pub struct MintCheckoutResponse {
    pub actions: Vec<MintCheckoutAction>,
}

/// The buyer-facing mint offer widget. See module docs.
pub struct MintCheckout<'a> {
    vm: &'a MintCheckoutVm,
    /// `None` asks the theme at render time — a `new` has no `Ui` to ask.
    accent: Option<Color32>,
}

impl<'a> MintCheckout<'a> {
    pub fn new(vm: &'a MintCheckoutVm) -> Self {
        Self { vm, accent: None }
    }

    /// Accent for the total, stepper readout, and Mint/bundle buttons.
    pub fn accent(mut self, accent: Color32) -> Self {
        self.accent = Some(accent);
        self
    }

    pub fn show(self, ui: &mut Ui) -> MintCheckoutResponse {
        crate::install_phosphor_font(ui.ctx());
        let vm = self.vm;
        let accent = self.accent.unwrap_or(ui.tokens().color.accent_green);
        let mut actions = Vec::new();

        // ── Phase + eligibility chips ───────────────────────────────────
        ui.horizontal_wrapped(|ui| {
            if let Some(phase) = &vm.phase_label {
                Chip::new(phase)
                    .variant(ChipVariant::Info)
                    .upper_case(true)
                    .show(ui);
            }
            match &vm.eligibility {
                Eligibility::Eligible { max_per_wallet } => {
                    Chip::new(&format!("Eligible · up to {max_per_wallet} per wallet"))
                        .variant(ChipVariant::Success)
                        .show(ui);
                }
                Eligibility::Ineligible { reason } => {
                    Chip::new(reason).variant(ChipVariant::Warning).show(ui);
                }
            }
        });

        // Ineligible → chips only, nothing to mint.
        let Eligibility::Eligible { max_per_wallet } = &vm.eligibility else {
            return MintCheckoutResponse { actions };
        };
        let max_per_wallet = (*max_per_wallet).max(1);
        let busy = matches!(vm.state, CheckoutState::Working(_));

        ui.gap(Space::Xl);

        // ── Quantity ────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(RichText::new("Quantity").color(ui.tokens().color.text_secondary));
            ui.gap(Space::Lg);
            ui.add_enabled_ui(!busy, |ui| {
                let resp = QuantityStepper::new(vm.qty)
                    .range(1, max_per_wallet)
                    .accent(accent)
                    .show(ui);
                if resp.changed {
                    actions.push(MintCheckoutAction::QtyChanged(resp.value));
                }
            });
        });

        ui.gap(Space::Xl);

        // ── Price each / Total — fixed-width cards, top-aligned ──────────
        let unit = vm.unit_price_lovelace;
        let total = unit.saturating_mul(vm.qty as u64);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            ui.set_item_gap_x(Space::Lg);
            price_card(
                ui,
                "Price each",
                &format_lovelace(unit as i64),
                ui.tokens().color.text_primary,
            );
            price_card(ui, "Total", &format_lovelace(total as i64), accent);
        });

        // ── Bundles (optional) ──────────────────────────────────────────
        if !vm.bundles.is_empty() {
            ui.gap(Space::Xl2);
            ui.label(
                RichText::new("Bundles")
                    .color(ui.tokens().color.text_secondary)
                    .strong(),
            );
            ui.gap(Space::Base);
            for bundle in &vm.bundles {
                if bundle_card(ui, bundle, unit, busy, accent) {
                    actions.push(MintCheckoutAction::SelectBundle(bundle.sku_key.clone()));
                }
            }
        }

        ui.gap(Space::Xl2);

        // ── Purchase summary + Mint ─────────────────────────────────────
        let noun = if vm.qty == 1 { "NFT" } else { "NFTs" };
        ui.label(
            RichText::new(format!(
                "Minting {} {noun} for {}",
                vm.qty,
                format_lovelace(total as i64)
            ))
            .size(15.0)
            .color(ui.tokens().color.text_primary),
        );
        ui.gap(Space::Md);

        let can_mint = matches!(vm.state, CheckoutState::Idle | CheckoutState::Error(_));
        let mint = egui::Button::new(
            RichText::new("Mint")
                .strong()
                .size(15.0)
                .color(ui.tokens().color.bg_primary),
        )
        .fill(accent)
        .min_size(egui::vec2(ui.available_width(), 44.0))
        .corner_radius(ui.tokens().corner(Radius::Lg));
        if ui.add_enabled(can_mint, mint).clicked() {
            actions.push(MintCheckoutAction::Mint);
        }

        // ── Post-submit state ───────────────────────────────────────────
        ui.gap(Space::Lg);
        match &vm.state {
            CheckoutState::Idle => {}
            CheckoutState::Working(msg) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(msg).color(ui.tokens().color.accent_cyan));
                });
            }
            CheckoutState::Submitted { tx, status } => {
                Chip::new("Payment submitted")
                    .variant(ChipVariant::Success)
                    .show(ui);
                ui.gap(Space::Base);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Order").color(ui.tokens().color.text_secondary));
                    let (variant, label) = order_status_view(status);
                    Chip::new(&label).variant(variant).show(ui);
                });
                ui.gap(Space::Sm);
                ui.hyperlink_to(
                    RichText::new(format!("view tx {}", truncate_hex(tx, 8, 6)))
                        .color(ui.tokens().color.accent_blue),
                    format!("/{tx}"),
                );
            }
            CheckoutState::Error(err) => {
                ErrorNote::new(err).show(ui);
            }
        }

        MintCheckoutResponse { actions }
    }
}

/// A fixed-width stat card (label over value). Used side-by-side for
/// price-each / total — fixed width + top alignment so they never stagger.
fn price_card(ui: &mut Ui, label: &str, value: &str, value_color: Color32) {
    egui::Frame::new()
        .fill(ui.tokens().color.bg_highlight)
        .stroke(egui::Stroke::new(1.0_f32, ui.tokens().color.border))
        .corner_radius(ui.tokens().corner(Radius::Lg))
        .inner_margin(ui.tokens().margin(Space::Xl))
        .show(ui, |ui| {
            ui.set_width(150.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(label)
                        .color(ui.tokens().color.text_secondary)
                        .size(12.0),
                );
                ui.gap(Space::Xs);
                ui.label(RichText::new(value).color(value_color).size(22.0).strong());
            });
        });
}

/// A full-width bundle row: label + saving on the left, a "{size} for {price}"
/// buy button on the right. Returns `true` when the buy button is clicked.
fn bundle_card(ui: &mut Ui, bundle: &BundleOffer, unit: u64, busy: bool, accent: Color32) -> bool {
    let saving = (bundle.size as u64)
        .saturating_mul(unit)
        .saturating_sub(bundle.price_lovelace);
    let mut clicked = false;

    egui::Frame::new()
        .fill(ui.tokens().color.bg_secondary)
        .stroke(egui::Stroke::new(1.0_f32, ui.tokens().color.border))
        .corner_radius(ui.tokens().corner(Radius::Lg))
        .inner_margin(ui.tokens().margin(Space::Lg))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&bundle.label)
                            .color(ui.tokens().color.text_primary)
                            .strong(),
                    );
                    if saving > 0 {
                        ui.label(
                            RichText::new(format!("save {}", format_lovelace(saving as i64)))
                                .color(ui.tokens().color.accent_green)
                                .small(),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if bundle.available {
                        format!(
                            "{} for {}",
                            bundle.size,
                            format_lovelace(bundle.price_lovelace as i64)
                        )
                    } else {
                        "Sold out".to_string()
                    };
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .strong()
                            .color(ui.tokens().color.bg_primary),
                    )
                    .fill(accent)
                    .corner_radius(ui.tokens().corner(Radius::Md));
                    if ui.add_enabled(bundle.available && !busy, btn).clicked() {
                        clicked = true;
                    }
                });
            });
        });
    ui.gap(Space::Base);
    clicked
}

/// Chip variant + label for an order status string.
fn order_status_view(status: &str) -> (ChipVariant, String) {
    let variant = match status {
        "delivered" => ChipVariant::Success,
        "failed" | "unfulfilled" => ChipVariant::Danger,
        _ => ChipVariant::Info,
    };
    (variant, status.to_string())
}
