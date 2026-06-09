//! `MintCheckout` storybook story — every state of the buyer mint panel.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{
    BundleOffer, CheckoutState, Eligibility, MintCheckout, MintCheckoutAction, MintCheckoutVm,
};

/// The one interactive panel needs a live quantity.
pub struct MintCheckoutStoryState {
    pub qty: u32,
}

impl Default for MintCheckoutStoryState {
    fn default() -> Self {
        Self { qty: 3 }
    }
}

const UNIT: u64 = 100_000_000; // 100 ADA

pub fn show(ui: &mut egui::Ui, state: &mut MintCheckoutStoryState) {
    ui.label(egui::RichText::new("MintCheckout").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Buyer-facing mint offer + CTA as one composed widget — phase + \
             eligibility chips, QuantityStepper, price-each / total, optional \
             fixed-price bundle cards, purchase summary, Mint button, and the \
             working / submitted / error states. VM-driven; returns \
             QtyChanged / Mint / SelectBundle actions. Hosts never hand-roll \
             (and mis-nest) this layout.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── 1. Interactive — eligible, the stepper drives the total ─────────
    section(ui, "Eligible — interactive (stepper drives the total)");
    let vm = MintCheckoutVm {
        phase_label: Some("public".into()),
        eligibility: Eligibility::Eligible { max_per_wallet: 10 },
        unit_price_lovelace: UNIT,
        qty: state.qty,
        bundles: vec![],
        state: CheckoutState::Idle,
    };
    for action in MintCheckout::new(&vm).show(ui).actions {
        match action {
            MintCheckoutAction::QtyChanged(n) => state.qty = n,
            MintCheckoutAction::Mint => surface(ui, format!("Mint {} clicked", state.qty)),
            MintCheckoutAction::SelectBundle(sku) => surface(ui, format!("Bundle {sku}")),
        }
    }
    ui.add_space(16.0);

    // ── 2. Eligible with bundles ────────────────────────────────────────
    section(ui, "Eligible — with fixed-price bundles");
    let vm = MintCheckoutVm {
        phase_label: Some("public".into()),
        eligibility: Eligibility::Eligible { max_per_wallet: 20 },
        unit_price_lovelace: 70_000_000,
        qty: 1,
        bundles: vec![
            BundleOffer {
                sku_key: "five_pack".into(),
                label: "5-pack".into(),
                size: 5,
                price_lovelace: 320_000_000,
                available: true,
            },
            BundleOffer {
                sku_key: "eleven_pack".into(),
                label: "11-pack".into(),
                size: 11,
                price_lovelace: 700_000_000,
                available: true,
            },
            BundleOffer {
                sku_key: "mega".into(),
                label: "Mega (sold out)".into(),
                size: 50,
                price_lovelace: 3_000_000_000,
                available: false,
            },
        ],
        state: CheckoutState::Idle,
    };
    let _ = MintCheckout::new(&vm).show(ui);
    ui.add_space(16.0);

    // ── 3. Ineligible — chips only ──────────────────────────────────────
    section(ui, "Ineligible — chips only, no controls");
    let vm = MintCheckoutVm {
        phase_label: Some("allowlist".into()),
        eligibility: Eligibility::Ineligible {
            reason: "Not eligible — wrong phase, sold out, or per-wallet limit reached".into(),
        },
        unit_price_lovelace: UNIT,
        qty: 1,
        bundles: vec![],
        state: CheckoutState::Idle,
    };
    let _ = MintCheckout::new(&vm).show(ui);
    ui.add_space(16.0);

    // ── 4. Working ──────────────────────────────────────────────────────
    section(ui, "Working — awaiting signature");
    let vm = base_vm(
        2,
        CheckoutState::Working("Awaiting signature for 200 ADA…".into()),
    );
    let _ = MintCheckout::new(&vm).show(ui);
    ui.add_space(16.0);

    // ── 5. Submitted (order polling) ────────────────────────────────────
    section(ui, "Submitted — order polling");
    let vm = base_vm(
        2,
        CheckoutState::Submitted {
            tx: "bf989cec3c3d80e5239d676f24a5e793291b6ff5109c2af1244fab8125ed104e".into(),
            status: "submitted".into(),
        },
    );
    let _ = MintCheckout::new(&vm).show(ui);
    ui.add_space(16.0);

    // ── 6. Error ────────────────────────────────────────────────────────
    section(ui, "Error");
    let vm = base_vm(
        1,
        CheckoutState::Error("signTx: user declined the transaction".into()),
    );
    let _ = MintCheckout::new(&vm).show(ui);
}

fn base_vm(qty: u32, state: CheckoutState) -> MintCheckoutVm {
    MintCheckoutVm {
        phase_label: Some("public".into()),
        eligibility: Eligibility::Eligible { max_per_wallet: 10 },
        unit_price_lovelace: UNIT,
        qty,
        bundles: vec![],
        state,
    }
}

fn section(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).color(ACCENT).strong());
    ui.add_space(4.0);
}

fn surface(ui: &mut egui::Ui, msg: String) {
    ui.label(
        egui::RichText::new(msg)
            .small()
            .color(egui::Color32::LIGHT_GREEN),
    );
}
