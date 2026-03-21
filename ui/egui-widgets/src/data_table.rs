//! Data table — dense row-based table with column headers, selection, and
//! optional detail panel.
//!
//! Designed for data-oriented dashboards where scannable numbers matter more
//! than visual thumbnails. The caller provides closures to map items into
//! [`DataRowItem`] structs and render the detail panel, keeping the widget
//! fully generic.

use egui::{Color32, CornerRadius, Rect, Sense, Ui, Vec2};

use crate::exposure_bar::ltv_risk_color;
use crate::theme;

// ============================================================================
// Column widths
// ============================================================================

const COL_ICON: f32 = 28.0;
const COL_TOKEN: f32 = 80.0;
const COL_PRINCIPAL: f32 = 100.0;
const COL_COLLATERAL: f32 = 160.0;
const COL_LTV: f32 = 60.0;
const COL_RATE: f32 = 50.0;
const COL_DURATION: f32 = 50.0;
const COL_INTEREST: f32 = 80.0;
const COL_STATUS: f32 = 80.0;

/// Sum of all fixed column widths (excluding status which gets the remainder).
const FIXED_WIDTH: f32 = COL_ICON
    + COL_TOKEN
    + COL_PRINCIPAL
    + COL_COLLATERAL
    + COL_LTV
    + COL_RATE
    + COL_DURATION
    + COL_INTEREST
    + COL_STATUS;

// ============================================================================
// Types
// ============================================================================

/// Status of a data row entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataRowStatus {
    Active,
    PendingCancel,
}

/// Data for a single dense row in the table.
///
/// All string fields are pre-formatted by the caller — the widget does no
/// number formatting itself, keeping it reusable across domains.
pub struct DataRowItem<'a> {
    /// Small token icon URL (rendered at icon_size). `None` = no icon.
    pub icon_url: Option<&'a str>,
    /// Token/pool name (e.g. "NIGHT").
    pub token_name: &'a str,
    /// Pre-formatted principal (e.g. "3,000 ADA").
    pub principal: &'a str,
    /// Pre-formatted collateral (e.g. "20,155 NIGHT").
    pub collateral: &'a str,
    /// LTV percentage. `None` if no price data available.
    pub ltv_pct: Option<f64>,
    /// Pre-formatted interest rate (e.g. "4.0%").
    pub rate: &'a str,
    /// Pre-formatted duration (e.g. "14d").
    pub duration: &'a str,
    /// Pre-formatted interest amount (e.g. "120 ADA").
    pub interest: &'a str,
    /// Row lifecycle status.
    pub status: DataRowStatus,
}

/// Persistent table state — selection tracking.
#[derive(Default)]
pub struct DataTableState {
    /// Currently selected row index.
    pub selected_index: Option<usize>,
}

/// Configuration for the data table.
pub struct DataTableConfig {
    /// Width of the detail panel (0 = no detail panel).
    pub detail_width: f32,
    /// Height of each data row in pixels.
    pub row_height: f32,
    /// Height of the column header row.
    pub header_height: f32,
    /// Size of the token icon in pixels.
    pub icon_size: f32,
    /// Scroll area ID salt.
    pub scroll_id: &'static str,
    /// Whether to show a micro LTV bar behind the LTV text.
    pub show_ltv_bar: bool,
}

impl Default for DataTableConfig {
    fn default() -> Self {
        Self {
            detail_width: 340.0,
            row_height: 36.0,
            header_height: 28.0,
            icon_size: 20.0,
            scroll_id: "data_table",
            show_ltv_bar: true,
        }
    }
}

/// Response from the data table widget.
pub struct DataTableResponse {
    /// True if the selection changed this frame.
    pub selected_changed: bool,
    /// Current selected index (if any).
    pub selected_index: Option<usize>,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the data table.
///
/// - `render_row`: maps each `&T` to a [`DataRowItem`] for painting.
/// - `render_detail`: paints the detail panel when a row is selected.
pub fn show<T>(
    ui: &mut Ui,
    state: &mut DataTableState,
    items: &[T],
    config: &DataTableConfig,
    mut render_row: impl FnMut(&T) -> DataRowItem<'_>,
    mut render_detail: impl FnMut(&mut Ui, usize, &T),
) -> DataTableResponse {
    crate::install_phosphor_font(ui.ctx());

    let has_selection = state.selected_index.is_some_and(|idx| idx < items.len());
    let detail_width = if has_selection {
        config.detail_width
    } else {
        0.0
    };

    let mut response = DataTableResponse {
        selected_changed: false,
        selected_index: state.selected_index,
    };

    ui.horizontal_top(|ui| {
        // LEFT: table
        let grid_width = if has_selection {
            (ui.available_width() - detail_width - 12.0).max(FIXED_WIDTH)
        } else {
            ui.available_width()
        };

        ui.vertical(|ui| {
            ui.set_max_width(grid_width);

            // Column headers
            draw_header(ui, config, grid_width);

            // Scrollable rows
            egui::ScrollArea::vertical()
                .id_salt(config.scroll_id)
                .show(ui, |ui| {
                    for (idx, item) in items.iter().enumerate() {
                        let row_data = render_row(item);
                        let is_selected = state.selected_index == Some(idx);

                        let clicked = draw_row(ui, &row_data, config, grid_width, idx, is_selected);

                        if clicked {
                            if is_selected {
                                state.selected_index = None;
                            } else {
                                state.selected_index = Some(idx);
                            }
                            response.selected_changed = true;
                            response.selected_index = state.selected_index;
                        }
                    }
                });
        });

        // RIGHT: detail panel
        if let Some(sel_idx) = state.selected_index {
            if sel_idx < items.len() {
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_max_width(config.detail_width);
                    ui.set_min_width(config.detail_width);
                    let frame_resp = egui::Frame::new()
                        .fill(Color32::from_rgb(30, 32, 42))
                        .corner_radius(6.0)
                        .inner_margin(14.0)
                        .show(ui, |ui| {
                            render_detail(ui, sel_idx, &items[sel_idx]);
                        });

                    // Close button overlay at top-right
                    let panel_rect = frame_resp.response.rect;
                    let btn_size = Vec2::splat(20.0);
                    let btn_rect = Rect::from_min_size(
                        egui::pos2(panel_rect.max.x - btn_size.x - 4.0, panel_rect.min.y + 4.0),
                        btn_size,
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    crate::PhosphorIcon::X.rich_text(14.0, theme::TEXT_SECONDARY),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            state.selected_index = None;
                            response.selected_changed = true;
                            response.selected_index = None;
                        }
                    });
                });
            }
        }
    });

    response
}

// ============================================================================
// Header
// ============================================================================

fn draw_header(ui: &mut Ui, config: &DataTableConfig, _grid_width: f32) {
    let desired = Vec2::new(ui.available_width(), config.header_height);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    let font = egui::FontId::proportional(9.0);
    let color = theme::TEXT_MUTED;
    let y = rect.center().y;

    let headers = [
        ("", COL_ICON),
        ("Pool", COL_TOKEN),
        ("Principal", COL_PRINCIPAL),
        ("Collateral", COL_COLLATERAL),
        ("LTV", COL_LTV),
        ("Rate", COL_RATE),
        ("Dur", COL_DURATION),
        ("Interest", COL_INTEREST),
        ("Status", COL_STATUS),
    ];

    let mut x = rect.min.x;
    for (label, width) in headers {
        if !label.is_empty() {
            painter.text(
                egui::pos2(x + 4.0, y),
                egui::Align2::LEFT_CENTER,
                label,
                font.clone(),
                color,
            );
        }
        x += width;
    }

    // Bottom separator
    painter.hline(
        rect.min.x..=rect.max.x,
        rect.max.y,
        egui::Stroke::new(1.0, theme::BG_HIGHLIGHT),
    );
}

// ============================================================================
// Row
// ============================================================================

fn draw_row(
    ui: &mut Ui,
    item: &DataRowItem<'_>,
    config: &DataTableConfig,
    _grid_width: f32,
    idx: usize,
    is_selected: bool,
) -> bool {
    let desired = Vec2::new(ui.available_width(), config.row_height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());

    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }

    let painter = ui.painter();
    let is_hovered = response.hovered();

    // Row background
    let bg = if is_selected {
        theme::BG_HIGHLIGHT
    } else if is_hovered {
        Color32::from_rgba_premultiplied(41, 46, 66, 128)
    } else if idx.is_multiple_of(2) {
        Color32::TRANSPARENT
    } else {
        Color32::from_rgba_premultiplied(36, 40, 59, 80)
    };
    painter.rect_filled(rect, 0.0, bg);

    let cy = rect.center().y;
    let mut x = rect.min.x;

    // ── Icon ──
    if let Some(url) = item.icon_url {
        let icon_rect = Rect::from_center_size(
            egui::pos2(x + COL_ICON / 2.0, cy),
            Vec2::splat(config.icon_size),
        );
        let image = egui::Image::new(url)
            .fit_to_exact_size(Vec2::splat(config.icon_size))
            .corner_radius(CornerRadius::same(2));
        image.paint_at(ui, icon_rect);
    }
    x += COL_ICON;

    // ── Token name ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.token_name,
        egui::FontId::proportional(11.0),
        theme::TEXT_PRIMARY,
    );
    x += COL_TOKEN;

    // ── Principal ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.principal,
        egui::FontId::monospace(11.0),
        theme::ACCENT_CYAN,
    );
    x += COL_PRINCIPAL;

    // ── Collateral ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.collateral,
        egui::FontId::proportional(10.0),
        theme::TEXT_SECONDARY,
    );
    x += COL_COLLATERAL;

    // ── LTV ──
    if let Some(ltv) = item.ltv_pct {
        let ltv_color = ltv_risk_color(ltv);

        // Optional micro-bar behind the text
        if config.show_ltv_bar {
            let bar_rect =
                Rect::from_min_size(egui::pos2(x + 2.0, cy + 6.0), Vec2::new(COL_LTV - 8.0, 3.0));
            painter.rect_filled(bar_rect, 1.0, theme::BG_SECONDARY);
            let fill_width = (bar_rect.width() * (ltv as f32 / 100.0).min(1.0)).max(0.0);
            let fill_rect = Rect::from_min_size(bar_rect.min, Vec2::new(fill_width, 3.0));
            painter.rect_filled(fill_rect, 1.0, ltv_color.linear_multiply(0.5));
        }

        painter.text(
            egui::pos2(x + 4.0, cy - if config.show_ltv_bar { 2.0 } else { 0.0 }),
            egui::Align2::LEFT_CENTER,
            format!("{ltv:.1}%"),
            egui::FontId::monospace(10.0),
            ltv_color,
        );
    } else {
        painter.text(
            egui::pos2(x + 4.0, cy),
            egui::Align2::LEFT_CENTER,
            "\u{2014}",
            egui::FontId::proportional(10.0),
            theme::TEXT_MUTED,
        );
    }
    x += COL_LTV;

    // ── Rate ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.rate,
        egui::FontId::monospace(10.0),
        theme::TEXT_PRIMARY,
    );
    x += COL_RATE;

    // ── Duration ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.duration,
        egui::FontId::proportional(10.0),
        theme::TEXT_MUTED,
    );
    x += COL_DURATION;

    // ── Interest ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        item.interest,
        egui::FontId::monospace(10.0),
        theme::ACCENT_GREEN,
    );
    x += COL_INTEREST;

    // ── Status ──
    if item.status == DataRowStatus::PendingCancel {
        let pill_rect = Rect::from_min_size(egui::pos2(x + 4.0, cy - 8.0), Vec2::new(72.0, 16.0));
        painter.rect_filled(pill_rect, 8.0, theme::WARNING.linear_multiply(0.2));
        painter.text(
            pill_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Cancelling\u{2026}",
            egui::FontId::proportional(9.0),
            theme::WARNING,
        );
    }

    response.clicked()
}
