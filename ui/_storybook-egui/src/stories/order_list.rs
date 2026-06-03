//! Story: `OrderList` — the mint-orders dashboard. Interactive: the status
//! filter chips, the search box, the "show submitted" toggle, and the per-row
//! history drawer all work. A pinned "now" keeps the relative times stable.
//!
//! Covers the realistic mix (delivered / submitted / pending / failed +
//! refunds), the "wall of failed" case from the bug report, and the empty /
//! loading / error states.

use std::collections::HashSet;

use egui_widgets::{OrderEventRow, OrderList, OrderListAction, OrderRow};

use crate::{ACCENT, TEXT_MUTED};

/// Pinned clock so the relative times ("9s ago", "2d ago") are deterministic.
const NOW: i64 = 1_780_000_000;

#[derive(Default)]
pub struct OrderListState {
    /// Order ids whose history drawer is open (toggled by the history button).
    open: HashSet<String>,
    show_submitted: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut OrderListState) {
    ui.label(egui::RichText::new("Order List").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Mint-orders dashboard. Click the status chips to filter, type in the \
             box to search id/address, toggle a row's history, and hover a time \
             for the absolute UTC. Replaces the old flat list (no dates, \
             \"refund: not_required\" on every line).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(10.0);

    // ── Interactive instance — a realistic mix + a run of failures ───────
    let rows = demo_rows(&state.open);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 440.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let resp = OrderList::new(&rows)
                    .now(NOW)
                    .fetched_at(Some(NOW - 9))
                    .show_submitted(state.show_submitted)
                    .show(ui);
                for action in resp.actions {
                    match action {
                        OrderListAction::ToggleHistory { order_id } => {
                            if !state.open.remove(&order_id) {
                                state.open.insert(order_id);
                            }
                        }
                        OrderListAction::SetShowSubmitted(b) => state.show_submitted = b,
                        OrderListAction::Refresh => {}
                    }
                }
            },
        );
    });

    ui.add_space(18.0);
    ui.label(
        egui::RichText::new("Empty · loading · error")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(6.0);

    // ── Empty ────────────────────────────────────────────────────────────
    small_panel(ui, 70.0, |ui| {
        OrderList::new(&[])
            .now(NOW)
            .fetched_at(Some(NOW - 3))
            .show(ui);
    });
    ui.add_space(8.0);
    // ── Loading ──────────────────────────────────────────────────────────
    small_panel(ui, 60.0, |ui| {
        OrderList::new(&[]).now(NOW).loading(true).show(ui);
    });
    ui.add_space(8.0);
    // ── Error ────────────────────────────────────────────────────────────
    small_panel(ui, 70.0, |ui| {
        OrderList::new(&[])
            .now(NOW)
            .error(Some("engine unreachable (502)"))
            .show(ui);
    });
}

fn small_panel(ui: &mut egui::Ui, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            add,
        );
    });
}

/// A spread of orders across the lifecycle, plus a run of failures so the
/// "500 failed" headline + filtering read true. Times fan out so the relative
/// labels show a range (seconds → days).
fn demo_rows(open: &HashSet<String>) -> Vec<OrderRow> {
    // (mins_ago, id_suffix, status, refund, qty, paid_ada, addr_suffix)
    let spec: &[(i64, &str, &str, &str, u32, Option<u64>, &str)] = &[
        (0, "01", "pending", "none", 2, Some(200), "t7wq32"),
        (1, "02", "fulfilling", "none", 1, Some(100), "lwfqva"),
        (3, "03", "submitted", "none", 2, Some(200), "8wrtp6"),
        (5, "04", "submitted", "none", 1, Some(100), "sgggrd"),
        (
            12,
            "05",
            "confirmed",
            "not_required",
            2,
            Some(200),
            "meml2g",
        ),
        (
            44,
            "06",
            "delivered",
            "not_required",
            1,
            Some(100),
            "pd5xxj",
        ),
        (
            58,
            "07",
            "delivered",
            "not_required",
            3,
            Some(300),
            "glvfvy",
        ),
        (130, "08", "failed", "pending", 2, Some(200), "tpq8md"),
        (190, "09", "failed", "refunded", 1, Some(100), "ge2q9t"),
        (240, "10", "unfulfilled", "refunded", 1, Some(100), "20xuc3"),
        (300, "11", "failed", "required", 2, Some(200), "ykrqnp"),
    ];
    let mut rows: Vec<OrderRow> = spec
        .iter()
        .map(|(mins, id, status, refund, qty, paid, addr)| {
            order(open, *mins, id, status, refund, *qty, *paid, addr)
        })
        .collect();

    // The "wall of failures" tail — what the bug report screenshot showed.
    for i in 0..24 {
        let mins = 320 + i * 2;
        let id = format!("3{i:02}");
        let addr = format!("f{i:02}fail");
        rows.push(order(
            open,
            mins,
            &id,
            "failed",
            "not_required",
            if i % 2 == 0 { 2 } else { 1 },
            Some(if i % 2 == 0 { 200 } else { 100 }),
            &addr,
        ));
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn order(
    open: &HashSet<String>,
    mins_ago: i64,
    id_suffix: &str,
    status: &str,
    refund: &str,
    qty: u32,
    paid_ada: Option<u64>,
    addr_suffix: &str,
) -> OrderRow {
    let order_id = format!("test:ta_2f9c1b04:{id_suffix}");
    let created = NOW - mins_ago * 60;
    let mut r = OrderRow::new(
        order_id.clone(),
        format!("addr_test1qz9k3v8x2m7p4n6c0d5{addr_suffix}"),
        qty,
        status,
        created,
        created + 8, // a small "updated" delta
    );
    r.refund_status = refund.to_string();
    r.paid_lovelace = paid_ada.map(|a| a * 1_000_000);
    if open.contains(&order_id) {
        r.detail_open = true;
        r.events = Some(demo_events(status, refund, created));
        if status == "failed" {
            r.note = Some("submit failed: inputs already spent (stale fuel)".to_string());
        }
    }
    r
}

fn demo_events(status: &str, refund: &str, created: i64) -> Vec<OrderEventRow> {
    let mut ev = vec![OrderEventRow {
        event: "created".to_string(),
        detail: None,
        tx_hash: None,
        at: created,
    }];
    let tx = Some("9f38a7c2e1b4d6058a2c4e7d1b9f6a3c0e5d8b2f7a4c1e6d3b9f0a8c".to_string());
    match status {
        "submitted" | "confirmed" | "delivered" => {
            ev.push(OrderEventRow {
                event: "submitted".to_string(),
                detail: None,
                tx_hash: tx.clone(),
                at: created + 6,
            });
            if status != "submitted" {
                ev.push(OrderEventRow {
                    event: "confirmed".to_string(),
                    detail: None,
                    tx_hash: tx.clone(),
                    at: created + 90,
                });
            }
            if status == "delivered" {
                ev.push(OrderEventRow {
                    event: "delivered".to_string(),
                    detail: None,
                    tx_hash: None,
                    at: created + 120,
                });
            }
        }
        "failed" => {
            ev.push(OrderEventRow {
                event: "failed".to_string(),
                detail: Some("inputs already spent (stale fuel)".to_string()),
                tx_hash: None,
                at: created + 5,
            });
            if refund == "refunded" {
                ev.push(OrderEventRow {
                    event: "refunded".to_string(),
                    detail: None,
                    tx_hash: tx,
                    at: created + 40,
                });
            } else if refund == "pending" || refund == "required" {
                ev.push(OrderEventRow {
                    event: "refund_required".to_string(),
                    detail: None,
                    tx_hash: None,
                    at: created + 6,
                });
            }
        }
        "unfulfilled" => {
            ev.push(OrderEventRow {
                event: "unfulfilled".to_string(),
                detail: Some("collection sold out".to_string()),
                tx_hash: None,
                at: created + 5,
            });
            if refund == "refunded" {
                ev.push(OrderEventRow {
                    event: "refunded".to_string(),
                    detail: None,
                    tx_hash: tx,
                    at: created + 30,
                });
            }
        }
        _ => {}
    }
    ev
}
