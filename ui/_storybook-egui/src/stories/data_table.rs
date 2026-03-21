//! Storybook demo for the DataTable widget.
//!
//! Shows an ExposureBar summary at the top, a dense DataTable below with
//! mock loan data, and a detail panel on row selection.

use egui_widgets::data_table::{self, DataRowItem, DataRowStatus, DataTableConfig, DataTableState};
use egui_widgets::exposure_bar::{self, ExposureBarConfig, ExposureSegment};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

// ============================================================================
// Story state
// ============================================================================

pub struct DataTableStoryState {
    pub table_state: DataTableState,
}

impl Default for DataTableStoryState {
    fn default() -> Self {
        Self {
            table_state: DataTableState::default(),
        }
    }
}

// ============================================================================
// Mock data
// ============================================================================

struct MockLoan {
    token: &'static str,
    principal: &'static str,
    collateral: &'static str,
    ltv_pct: Option<f64>,
    rate: &'static str,
    duration: &'static str,
    interest: &'static str,
    status: DataRowStatus,
    // For exposure bar
    principal_lovelace: u64,
    color: egui::Color32,
}

fn mock_loans() -> Vec<MockLoan> {
    vec![
        MockLoan {
            token: "NIGHT",
            principal: "3,000 ADA",
            collateral: "20,155 NIGHT",
            ltv_pct: Some(89.8),
            rate: "4.0%",
            duration: "14d",
            interest: "120 ADA",
            status: DataRowStatus::Active,
            principal_lovelace: 3_000_000_000,
            color: egui_widgets::theme::ACCENT_MAGENTA,
        },
        MockLoan {
            token: "SNEK",
            principal: "2,500 ADA",
            collateral: "312,500 SNEK",
            ltv_pct: Some(45.2),
            rate: "3.5%",
            duration: "30d",
            interest: "87 ADA",
            status: DataRowStatus::Active,
            principal_lovelace: 2_500_000_000,
            color: egui_widgets::theme::ACCENT_GREEN,
        },
        MockLoan {
            token: "ANGELS",
            principal: "2,000 ADA",
            collateral: "8 ANGELS",
            ltv_pct: Some(72.1),
            rate: "5.0%",
            duration: "7d",
            interest: "100 ADA",
            status: DataRowStatus::Active,
            principal_lovelace: 2_000_000_000,
            color: egui_widgets::theme::ACCENT_CYAN,
        },
        MockLoan {
            token: "HOSKY",
            principal: "355 ADA",
            collateral: "50M HOSKY",
            ltv_pct: Some(35.0),
            rate: "2.0%",
            duration: "14d",
            interest: "7 ADA",
            status: DataRowStatus::Active,
            principal_lovelace: 355_000_000,
            color: egui_widgets::theme::ACCENT_YELLOW,
        },
        MockLoan {
            token: "WMT",
            principal: "1,000 ADA",
            collateral: "5,000 WMT",
            ltv_pct: None, // No price data
            rate: "3.0%",
            duration: "21d",
            interest: "63 ADA",
            status: DataRowStatus::Active,
            principal_lovelace: 1_000_000_000,
            color: egui_widgets::theme::ACCENT_BLUE,
        },
        MockLoan {
            token: "NIGHT",
            principal: "500 ADA",
            collateral: "3,400 NIGHT",
            ltv_pct: Some(95.0),
            rate: "4.0%",
            duration: "14d",
            interest: "20 ADA",
            status: DataRowStatus::PendingCancel,
            principal_lovelace: 500_000_000,
            color: egui_widgets::theme::ACCENT_MAGENTA,
        },
    ]
}

// ============================================================================
// Story
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut DataTableStoryState) {
    ui.label(
        egui::RichText::new("DataTable Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Dense row-based table with column headers, LTV micro-bars, selection, \
             and detail panel. Paired with ExposureBar summary above.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    let loans = mock_loans();

    // ── Exposure bar summary ──
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            // Build segments from loans that have LTV data
            let with_ltv: Vec<&MockLoan> = loans.iter().filter(|l| l.ltv_pct.is_some()).collect();
            let total: u64 = with_ltv.iter().map(|l| l.principal_lovelace).sum();

            let segments: Vec<ExposureSegment> = with_ltv
                .iter()
                .map(|l| ExposureSegment {
                    label: l.token.into(),
                    principal_lovelace: l.principal_lovelace,
                    fraction: l.principal_lovelace as f32 / total as f32,
                    ltv_pct: l.ltv_pct.unwrap_or(0.0),
                    color: l.color,
                })
                .collect();

            exposure_bar::show(ui, &segments, &ExposureBarConfig::default());
        });

    ui.add_space(12.0);

    // ── Data table ──
    egui::Frame::new()
        .fill(BG_MAIN)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
        .show(ui, |ui| {
            let config = DataTableConfig::default();

            let _response = data_table::show(
                ui,
                &mut state.table_state,
                &loans,
                &config,
                // render_row closure
                |loan| DataRowItem {
                    icon_url: None, // No real images in storybook
                    token_name: loan.token,
                    principal: loan.principal,
                    collateral: loan.collateral,
                    ltv_pct: loan.ltv_pct,
                    rate: loan.rate,
                    duration: loan.duration,
                    interest: loan.interest,
                    status: loan.status,
                },
                // render_detail closure
                |ui, _idx, loan| {
                    ui.label(
                        egui::RichText::new(format!("{} Loan Details", loan.token))
                            .color(egui_widgets::theme::TEXT_PRIMARY)
                            .size(14.0)
                            .strong(),
                    );
                    ui.add_space(8.0);

                    let pairs: &[(&str, &str, egui::Color32)] = &[
                        (
                            "Principal",
                            loan.principal,
                            egui_widgets::theme::ACCENT_CYAN,
                        ),
                        (
                            "Collateral",
                            loan.collateral,
                            egui_widgets::theme::TEXT_SECONDARY,
                        ),
                        ("Rate", loan.rate, egui_widgets::theme::TEXT_PRIMARY),
                        ("Duration", loan.duration, egui_widgets::theme::TEXT_MUTED),
                        ("Interest", loan.interest, egui_widgets::theme::ACCENT_GREEN),
                    ];

                    for (label, value, color) in pairs {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{label}:"))
                                    .color(egui_widgets::theme::TEXT_MUTED)
                                    .size(11.0),
                            );
                            ui.label(egui::RichText::new(*value).color(*color).size(11.0));
                        });
                    }

                    if let Some(ltv) = loan.ltv_pct {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("LTV:")
                                    .color(egui_widgets::theme::TEXT_MUTED)
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{ltv:.1}%"))
                                    .color(egui_widgets::exposure_bar::ltv_risk_color(ltv))
                                    .size(11.0)
                                    .strong(),
                            );
                        });
                    }

                    ui.add_space(12.0);

                    if loan.status == DataRowStatus::PendingCancel {
                        ui.label(
                            egui::RichText::new("Cancellation pending...")
                                .color(egui_widgets::theme::WARNING)
                                .size(11.0),
                        );
                    } else {
                        // Mock cancel button
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Cancel Offer")
                                        .color(egui_widgets::theme::ERROR)
                                        .size(11.0),
                                )
                                .fill(egui_widgets::theme::ERROR.linear_multiply(0.15))
                                .corner_radius(4.0),
                            )
                            .clicked()
                        {
                            // no-op in storybook
                        }
                    }
                },
            );
        });
}
