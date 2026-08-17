//! `FlowMatrix` story — the discovery face for a multi-wallet project.
//!
//! Fixture shaped like the real Mekka flow ledger: a declared treasury, a
//! derived royalty address, a handful of ops wallets, and a long tail of
//! payees — plus the two patterns the widget exists to surface.
//!
//! **What to look for:**
//!
//! 1. A **shared-payee column** — `contractor-01` is paid by four different
//!    project wallets. In a node-link graph that is four edges lost among
//!    hundreds; here it is a column your eye lands on, and it is how a set of
//!    wallets is shown to be one interest.
//! 2. The **`unresolved payer` column**, which is the biggest inbound block on
//!    screen. Offline walks cannot name the funder of most receipts, and the
//!    honest picture of that is a large, clearly-labelled column rather than a
//!    footnote or a silent omission.

use crate::TEXT_MUTED;
use egui_widgets::{FlowMatrix, MatrixFlow, Selection, SpineState};

const DAY: i64 = 86_400;
const T0: i64 = 1_750_000_000;

pub struct FlowMatrixState {
    spine: Option<SpineState>,
    selection: Selection,
}

impl Default for FlowMatrixState {
    fn default() -> Self {
        Self {
            spine: None,
            selection: Selection::default(),
        }
    }
}

fn wallets() -> Vec<&'static str> {
    vec![
        "S1 treasury",
        "royalty (CIP-27)",
        "ops-payments",
        "ops-secondary",
        "team-1",
        "team-2",
    ]
}

fn flows() -> Vec<MatrixFlow<'static>> {
    let mut out = Vec::new();
    let w = wallets();

    // Receipts nobody can attribute — the offline-walk reality, and the
    // column that has to stay visible.
    for (i, p) in w.iter().enumerate() {
        for k in 0..(6 - i) {
            out.push(MatrixFlow {
                timestamp: T0 + (k as i64 * 9 + i as i64) * DAY,
                party: p,
                counterparty: "",
                quantity: 40_000_000_000 - (i as i64 * 3_000_000_000) + k as i64 * 900_000_000,
            });
        }
    }

    // THE FINDING: one contractor paid by four separate project wallets.
    for (i, p) in w.iter().take(4).enumerate() {
        for k in 0..(3 + i) {
            out.push(MatrixFlow {
                timestamp: T0 + (20 + k as i64 * 11 + i as i64 * 3) * DAY,
                party: p,
                counterparty: "contractor-01",
                quantity: -(4_000_000_000 + (i as i64) * 1_500_000_000 + k as i64 * 400_000_000),
            });
        }
    }

    // Ordinary spending: each wallet has its own payees, most only once.
    let payees = [
        "exchange-hot",
        "off-ramp script",
        "artist",
        "infra",
        "audit",
        "marketing",
        "legal",
        "grants",
        "misc-a",
        "misc-b",
        "misc-c",
    ];
    for (i, p) in w.iter().enumerate() {
        for (j, c) in payees.iter().enumerate() {
            if (i + j) % 3 != 0 {
                continue;
            }
            out.push(MatrixFlow {
                timestamp: T0 + (30 + (i * payees.len() + j) as i64) * DAY,
                party: p,
                counterparty: c,
                quantity: -((1 + (i * j) as i64) * 620_000_000),
            });
        }
    }
    out.sort_by_key(|f| f.timestamp);
    out
}

pub fn show(ui: &mut egui::Ui, state: &mut FlowMatrixState) {
    let flows = flows();
    let lo = flows.iter().map(|f| f.timestamp).min().unwrap_or(T0);
    let hi = flows.iter().map(|f| f.timestamp).max().unwrap_or(T0 + DAY);
    let spine = state.spine.get_or_insert_with(|| SpineState::new((lo, hi)));

    ui.label(
        egui::RichText::new(
            "Rows are the project's own wallets, columns are who they dealt with, ranked by \
             activity. Out is warm, in is cool, intensity is magnitude on a LOG scale (treasury \
             flows span orders of magnitude — a linear ramp shows one cell). Hover a cell for \
             the exact figures; click one to watch that counterparty everywhere.",
        )
        .small()
        .color(TEXT_MUTED),
    );
    ui.add_space(8.0);

    let r = FlowMatrix::new(&flows, "ADA", spine, &mut state.selection).show(ui);

    ui.add_space(8.0);
    let sel = state
        .selection
        .active()
        .map(|s| format!("watching: {s}"))
        .unwrap_or_else(|| "nothing pinned — click a cell".into());
    ui.label(
        egui::RichText::new(format!(
            "{sel}   ·   {} wallets x {} counterparties   ·   {} flows in view",
            r.rows_shown, r.cols_shown, r.flows_in_window
        ))
        .small()
        .color(TEXT_MUTED),
    );
}
