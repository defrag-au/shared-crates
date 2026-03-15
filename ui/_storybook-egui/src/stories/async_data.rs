//! Async Data story — demonstrates `egui_inbox` driving widgets from
//! simulated external data sources (API polling via spawn_local + gloo_timers).

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::egui_inbox::{UiInbox, UiInboxSender};

/// Simulated balance snapshot from an API.
#[derive(Clone, Debug)]
pub struct BalanceSnapshot {
    pub accrued_total: u64,
    pub effective_rate: f64,
    pub holder_count: u32,
}

/// Mutable state for the async data demo.
pub struct AsyncDataState {
    inbox: UiInbox<BalanceSnapshot>,
    sender: UiInboxSender<BalanceSnapshot>,
    latest: Option<BalanceSnapshot>,
    update_count: u32,
    last_update_time: f64,
    polling: bool,
    polling_started: bool,
    counter: egui_widgets::FlipCounter,
}

impl Default for AsyncDataState {
    fn default() -> Self {
        let (sender, inbox) = UiInbox::channel();
        let counter = egui_widgets::FlipCounter::new(8)
            .text_color(egui_widgets::theme::TEXT_PRIMARY)
            .card_height(50.0);

        Self {
            inbox,
            sender,
            latest: None,
            update_count: 0,
            last_update_time: 0.0,
            polling: false,
            polling_started: false,
            counter,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut AsyncDataState) {
    // Drain inbox — replace with latest value
    let mut got_update = false;
    for snapshot in state.inbox.read(ui) {
        state.latest = Some(snapshot);
        state.update_count += 1;
        state.last_update_time = egui_widgets::utils::now_secs();
        got_update = true;
    }

    // Update flip counter when new data arrives
    if got_update {
        if let Some(ref snap) = state.latest {
            state.counter.set_value(&format!("{}", snap.accrued_total));
        }
    }

    // --- Controls ---
    ui.label(
        egui::RichText::new("Async Data Source")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Demonstrates egui_inbox driving widgets from a simulated API poller. \
             A spawn_local loop sends BalanceSnapshot every 3s through a UiInboxSender.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui
            .button(if state.polling {
                "Stop Polling"
            } else {
                "Start Polling"
            })
            .clicked()
        {
            state.polling = !state.polling;

            if state.polling && !state.polling_started {
                state.polling_started = true;
                start_polling(state.sender.clone());
            }
        }

        if ui.button("Reset").clicked() {
            state.latest = None;
            state.update_count = 0;
            state.last_update_time = 0.0;
            state.counter.set_value("");
        }
    });

    ui.add_space(4.0);

    // Status line
    let status = if !state.polling {
        "Idle".to_string()
    } else if state.update_count == 0 {
        "Waiting for first update...".to_string()
    } else {
        let ago = egui_widgets::utils::now_secs() - state.last_update_time;
        format!(
            "Last update: {ago:.1}s ago  |  Updates received: {}",
            state.update_count
        )
    };
    ui.label(egui::RichText::new(status).color(TEXT_MUTED).small());

    // Request repaint while polling so the "Xs ago" counter ticks
    if state.polling {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    // --- Widgets driven by inbox data ---

    if let Some(ref snap) = state.latest {
        // Metric cards row
        ui.label(egui::RichText::new("Metric Cards").color(ACCENT).strong());
        ui.label(
            egui::RichText::new("Updated via inbox.read() each frame — only latest value kept")
                .color(TEXT_MUTED)
                .small(),
        );
        ui.add_space(4.0);

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            let card_width = 180.0;

            ui.allocate_ui(egui::vec2(card_width, 80.0), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 50))
                    .corner_radius(6.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Accrued Total")
                                    .color(TEXT_MUTED)
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(egui_widgets::format_number(
                                    snap.accrued_total as i64,
                                ))
                                .color(egui_widgets::theme::TEXT_PRIMARY)
                                .size(20.0)
                                .strong(),
                            );
                        });
                    });
            });

            ui.allocate_ui(egui::vec2(card_width, 80.0), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 50))
                    .corner_radius(6.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Rate/hr").color(TEXT_MUTED).size(11.0));
                            ui.label(
                                egui::RichText::new(format!("{:.1}", snap.effective_rate))
                                    .color(egui_widgets::theme::ACCENT_CYAN)
                                    .size(20.0)
                                    .strong(),
                            );
                        });
                    });
            });

            ui.allocate_ui(egui::vec2(card_width, 80.0), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 50))
                    .corner_radius(6.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Holders").color(TEXT_MUTED).size(11.0));
                            ui.label(
                                egui::RichText::new(format!("{}", snap.holder_count))
                                    .color(egui_widgets::theme::SUCCESS)
                                    .size(20.0)
                                    .strong(),
                            );
                        });
                    });
            });
        });

        ui.add_space(16.0);

        // Flip counter
        ui.label(egui::RichText::new("Flip Counter").color(ACCENT).strong());
        ui.label(
            egui::RichText::new("Animates to new value when inbox delivers a snapshot")
                .color(TEXT_MUTED)
                .small(),
        );
        ui.add_space(4.0);
        state.counter.show(ui);

        ui.add_space(16.0);

        // Seven segment rate display
        ui.label(
            egui::RichText::new("Seven Segment Rate")
                .color(ACCENT)
                .strong(),
        );
        ui.label(
            egui::RichText::new("Shows effective_rate as integer from latest snapshot")
                .color(TEXT_MUTED)
                .small(),
        );
        ui.add_space(4.0);
        let rate_text = format!("{:>5}", snap.effective_rate as u64);
        egui_widgets::SevenSegmentDisplay::new(&rate_text)
            .digit_height(24.0)
            .color(egui_widgets::theme::ACCENT_CYAN)
            .off_color(egui::Color32::from_rgb(25, 25, 45))
            .show(ui);
    } else {
        ui.add_space(24.0);
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(30, 30, 50))
            .corner_radius(8.0)
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("No data yet")
                            .color(TEXT_MUTED)
                            .size(14.0),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Click \"Start Polling\" to begin receiving simulated API snapshots",
                        )
                        .color(TEXT_MUTED)
                        .small(),
                    );
                });
            });
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);

    // Pattern explanation
    ui.label(egui::RichText::new("Pattern:").color(ACCENT).strong());
    ui.label("1. UiInbox::channel() creates (sender, inbox) pair");
    ui.label("2. sender.clone() is moved into spawn_local async loop");
    ui.label("3. sender.send(data) auto-triggers ctx.request_repaint()");
    ui.label("4. inbox.read(ui) drains messages in the render loop");
    ui.label("5. No Rc<RefCell<>> — no runtime borrow panics");
}

/// Spawn a polling loop that sends simulated snapshots every 3 seconds.
fn start_polling(sender: UiInboxSender<BalanceSnapshot>) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut base_value: u64 = 10_000;

        loop {
            gloo_timers::future::sleep(std::time::Duration::from_secs(3)).await;

            // Simulate growing balance with some randomness
            base_value += 50 + (js_sys::Math::random() * 200.0) as u64;
            let rate = 120.0 + js_sys::Math::random() * 80.0;
            let holders = 150 + (js_sys::Math::random() * 50.0) as u32;

            let snapshot = BalanceSnapshot {
                accrued_total: base_value,
                effective_rate: rate,
                holder_count: holders,
            };

            let _ = sender.send(snapshot);
        }
    });
}
