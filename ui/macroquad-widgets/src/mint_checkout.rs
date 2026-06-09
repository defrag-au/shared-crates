//! `MintCheckout` organism — the mint initiator: phase + eligibility, a
//! [`quantity_stepper`], the live total, and the Mint CTA. The PRE-mint half of
//! the flow; once submitted the host hands off to `OrderFulfilment` (the
//! heartbeat). Mirrors the egui `MintCheckout` (VM in → actions out).

use macroquad::prelude::*;

use crate::button::Button;
use crate::painter::Painter;
use crate::quantity_stepper::{quantity_stepper, QuantityStepperVm, StepperAction};
use crate::theme;

pub enum Eligibility {
    Eligible { max_per_wallet: u32 },
    Ineligible { reason: String },
}

pub enum CheckoutState {
    Idle,
    /// Build/sign/submit in flight; the string is a human status line.
    Working(String),
}

pub struct MintCheckoutVm {
    pub phase_label: Option<String>,
    pub eligibility: Eligibility,
    pub unit_price_lovelace: u64,
    pub qty: u32,
    pub state: CheckoutState,
}

pub enum CheckoutAction {
    QtyChanged(u32),
    Mint,
}

pub struct CheckoutResponse {
    pub bottom: f32,
    pub action: Option<CheckoutAction>,
}

pub fn mint_checkout(
    p: &Painter,
    vm: &MintCheckoutVm,
    x: f32,
    mut y: f32,
    w: f32,
) -> CheckoutResponse {
    let mut action = None;

    if let Some(phase) = &vm.phase_label {
        p.text_top(&phase.to_uppercase(), x, y, 12.0, p.theme.link);
        y += 20.0;
    }

    let max = match &vm.eligibility {
        Eligibility::Eligible { max_per_wallet } => (*max_per_wallet).max(1),
        Eligibility::Ineligible { reason } => {
            p.text_top(reason, x, y, 14.0, p.theme.warn);
            y += 24.0;
            return CheckoutResponse { bottom: y, action };
        }
    };

    let busy = matches!(vm.state, CheckoutState::Working(_));

    p.text_top("quantity", x, y, 13.0, p.theme.muted);
    y += 24.0;
    let svm = QuantityStepperVm {
        qty: vm.qty,
        min: 1,
        max,
    };
    let resp = quantity_stepper(p, &svm, x, y, 34.0, !busy);
    if let Some(StepperAction::Changed(n)) = resp.action {
        action = Some(CheckoutAction::QtyChanged(n));
    }
    y += 34.0 + 16.0;

    let total = vm.unit_price_lovelace.saturating_mul(vm.qty as u64);
    p.text_top(
        &format!(
            "{} x {}  =  {}",
            vm.qty,
            format_ada(vm.unit_price_lovelace),
            format_ada(total)
        ),
        x,
        y,
        14.0,
        p.theme.fg,
    );
    y += 28.0;

    match &vm.state {
        CheckoutState::Working(msg) => {
            let pulse = (get_time() * 2.0).sin() as f32 * 0.5 + 0.5;
            draw_circle(
                x + 6.0,
                y + 9.0,
                5.0,
                theme::with_alpha(p.theme.link, 0.3 + 0.7 * pulse),
            );
            p.text_top(msg, x + 22.0, y, 15.0, p.theme.link);
            y += 30.0;
        }
        CheckoutState::Idle => {
            if Button::new(&format!("Mint {} for {}", vm.qty, format_ada(total)))
                .font_size(17.0)
                .show(p, Rect::new(x, y, w, 48.0))
            {
                action = Some(CheckoutAction::Mint);
            }
            y += 56.0;
        }
    }

    CheckoutResponse { bottom: y, action }
}

fn format_ada(lovelace: u64) -> String {
    let ada = lovelace as f64 / 1_000_000.0;
    if ada.fract().abs() < 1e-9 {
        format!("{} ADA", ada as u64)
    } else {
        format!("{ada:.2} ADA")
    }
}
