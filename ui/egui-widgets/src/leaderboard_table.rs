//! `LeaderboardTable` — a dense, virtual-scrolled ranked table.
//!
//! The recurring "ranked list of entities with a value and share" pattern:
//! token holders, top traders, wallet rankings, leaderboards. Each row is
//! a rank, an identity label (optionally accent-colored, e.g. a resolved
//! `$handle`), an optional semantic [`Chip`] badge, a pre-formatted value,
//! and a percentage. Rows are virtualized via [`egui_extras::TableBuilder`]
//! so 10k+ entries scroll cheaply.
//!
//! The widget does **no** number formatting — the caller pre-formats
//! `value` (and provides `percent` as a plain `f64`) so it stays reusable
//! across domains (tokens, ADA, counts).
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{LeaderboardTable, LeaderboardRow, ChipVariant};
//!
//! let rows = vec![LeaderboardRow {
//!     rank: 1,
//!     label: "$whale".into(),
//!     accent: true,
//!     badge: Some(("LP".into(), ChipVariant::Info)),
//!     value: "148.45M".into(),
//!     value_detail: Some("Free: 124M\nLP: 24M".into()),
//!     percent: 32.6,
//!     copy_value: Some("stake1u9d7…".into()),
//! }];
//! LeaderboardTable::new(&rows)
//!     .headers("Holder", "Amount")
//!     .id_salt("holders")
//!     .show(ui);
//! ```

use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::theme;
use crate::{Chip, ChipVariant, PhosphorIcon};

/// One ranked row. All display strings are caller-formatted.
pub struct LeaderboardRow {
    /// 1-based rank shown in the `#` column.
    pub rank: usize,
    /// Identity label (handle, name, truncated address, …).
    pub label: String,
    /// Render the label in the accent color (e.g. a resolved `$handle`).
    pub accent: bool,
    /// Optional semantic badge (label + variant), e.g. `("LP", Info)`.
    pub badge: Option<(String, ChipVariant)>,
    /// Pre-formatted value (e.g. `"148.45M"`, `"3,000 ADA"`).
    pub value: String,
    /// Optional multi-line breakdown shown when hovering the value (e.g.
    /// `"Free: 30M\nLP: 24M"`). `None` hides the tooltip for that row.
    pub value_detail: Option<String>,
    /// Share of total, as a percentage (e.g. `32.6`).
    pub percent: f64,
    /// Full identifier to copy when set (e.g. the row's stake / enterprise
    /// address). Renders a copy button left of the label; the widget owns
    /// the clipboard write. `None` hides the button for that row.
    pub copy_value: Option<String>,
}

/// Builder for the leaderboard table.
pub struct LeaderboardTable<'a> {
    rows: &'a [LeaderboardRow],
    label_header: &'a str,
    value_header: &'a str,
    show_badge: bool,
    show_percent: bool,
    row_height: f32,
    header_height: f32,
    accent_color: Color32,
    id_salt: &'a str,
}

impl<'a> LeaderboardTable<'a> {
    /// Construct a table over `rows`.
    pub fn new(rows: &'a [LeaderboardRow]) -> Self {
        Self {
            rows,
            label_header: "Name",
            value_header: "Value",
            show_badge: true,
            show_percent: true,
            row_height: 26.0,
            header_height: 22.0,
            accent_color: theme::ACCENT_CYAN,
            id_salt: "leaderboard_table",
        }
    }

    /// Set the identity and value column header labels.
    pub fn headers(mut self, label: &'a str, value: &'a str) -> Self {
        self.label_header = label;
        self.value_header = value;
        self
    }

    /// Hide the badge column (no rows carry badges).
    pub fn show_badge(mut self, show: bool) -> Self {
        self.show_badge = show;
        self
    }

    /// Hide the percentage column.
    pub fn show_percent(mut self, show: bool) -> Self {
        self.show_percent = show;
        self
    }

    /// Row height in pixels (default 26).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Color used for accent-flagged labels (default cyan).
    pub fn accent_color(mut self, color: Color32) -> Self {
        self.accent_color = color;
        self
    }

    /// Scroll-area id salt — give each instance a distinct value when
    /// several tables share a view.
    pub fn id_salt(mut self, salt: &'a str) -> Self {
        self.id_salt = salt;
        self
    }

    /// Render the table.
    pub fn show(self, ui: &mut Ui) {
        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .id_salt(self.id_salt)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(44.0))
            .column(Column::remainder().at_least(140.0));
        if self.show_badge {
            builder = builder.column(Column::exact(80.0));
        }
        builder = builder.column(Column::exact(120.0));
        if self.show_percent {
            builder = builder.column(Column::exact(70.0));
        }

        builder
            .auto_shrink([false, false])
            .header(self.header_height, |mut header| {
                header.col(|ui| {
                    ui.label(muted_header("#"));
                });
                header.col(|ui| {
                    ui.label(muted_header(self.label_header));
                });
                if self.show_badge {
                    header.col(|ui| {
                        ui.label(muted_header("Tag"));
                    });
                }
                header.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted_header(self.value_header));
                    });
                });
                if self.show_percent {
                    header.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(muted_header("%"));
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(self.row_height, self.rows.len(), |mut row| {
                    let r = &self.rows[row.index()];
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format!("{}", r.rank))
                                .monospace()
                                .color(theme::TEXT_MUTED),
                        );
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            // Copy affordance left of the identity — copies the
                            // full address for use in other tooling.
                            if let Some(addr) = &r.copy_value {
                                let btn = egui::Button::new(
                                    PhosphorIcon::Copy.rich_text(12.0, theme::TEXT_MUTED),
                                )
                                .frame(false);
                                if ui.add(btn).on_hover_text("Copy address").clicked() {
                                    ui.ctx().copy_text(addr.clone());
                                }
                            }
                            let color = if r.accent {
                                self.accent_color
                            } else {
                                theme::TEXT_PRIMARY
                            };
                            ui.label(RichText::new(&r.label).monospace().color(color));
                        });
                    });
                    if self.show_badge {
                        row.col(|ui| {
                            if let Some((label, variant)) = &r.badge {
                                Chip::new(label).variant(*variant).show(ui);
                            }
                        });
                    }
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let resp = ui.label(
                                RichText::new(&r.value)
                                    .monospace()
                                    .color(theme::TEXT_PRIMARY),
                            );
                            if let Some(detail) = &r.value_detail {
                                resp.on_hover_text(detail);
                            }
                        });
                    });
                    if self.show_percent {
                        row.col(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{:.2}%", r.percent))
                                            .monospace()
                                            .color(theme::TEXT_SECONDARY),
                                    );
                                },
                            );
                        });
                    }
                });
            });
    }
}

fn muted_header(text: &str) -> RichText {
    RichText::new(text).small().color(theme::TEXT_MUTED)
}
