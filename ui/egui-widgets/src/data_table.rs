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

const COL_CHEVRON: f32 = 20.0;
const COL_ICON: f32 = 28.0;
const COL_TOKEN: f32 = 80.0;
const COL_PRINCIPAL: f32 = 100.0;
const COL_COLLATERAL: f32 = 160.0;
const COL_LTV: f32 = 60.0;
const COL_RATE: f32 = 50.0;
const COL_DURATION: f32 = 50.0;
const COL_INTEREST: f32 = 80.0;
const COL_STATUS: f32 = 80.0;

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
pub struct DataRowItem {
    /// Small token icon URL (rendered at icon_size). `None` = no icon.
    pub icon_url: Option<String>,
    /// Token/pool name (e.g. "NIGHT").
    pub token_name: String,
    /// Pre-formatted principal (e.g. "3,000 ADA").
    pub principal: String,
    /// Pre-formatted collateral (e.g. "20,155 NIGHT").
    pub collateral: String,
    /// LTV percentage. `None` if no price data available.
    pub ltv_pct: Option<f64>,
    /// Pre-formatted interest rate (e.g. "4.0%").
    pub rate: String,
    /// Pre-formatted duration (e.g. "14d").
    pub duration: String,
    /// Pre-formatted interest amount (e.g. "120 ADA").
    pub interest: String,
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
    /// Background color for the expanded accordion detail area.
    pub detail_bg: Color32,
}

impl Default for DataTableConfig {
    fn default() -> Self {
        Self {
            row_height: 36.0,
            header_height: 28.0,
            icon_size: 20.0,
            scroll_id: "data_table",
            show_ltv_bar: true,
            detail_bg: Color32::from_rgb(30, 32, 42),
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
    mut render_row: impl FnMut(&T) -> DataRowItem,
    mut render_detail: impl FnMut(&mut Ui, usize, &T),
) -> DataTableResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut response = DataTableResponse {
        selected_changed: false,
        selected_index: state.selected_index,
    };

    // Column headers
    draw_header(ui, config);

    // Scrollable rows with inline accordion detail
    egui::ScrollArea::vertical()
        .id_salt(config.scroll_id)
        .show(ui, |ui| {
            for (idx, item) in items.iter().enumerate() {
                let row_data = render_row(item);
                let is_selected = state.selected_index == Some(idx);

                let clicked = draw_row(ui, &row_data, config, idx, is_selected);

                if clicked {
                    if is_selected {
                        state.selected_index = None;
                    } else {
                        state.selected_index = Some(idx);
                    }
                    response.selected_changed = true;
                    response.selected_index = state.selected_index;
                }

                // Accordion detail — rendered inline below the selected row
                if state.selected_index == Some(idx) {
                    egui::Frame::new()
                        .fill(config.detail_bg)
                        .corner_radius(4.0)
                        .inner_margin(14.0)
                        .outer_margin(egui::Margin {
                            left: COL_CHEVRON as i8,
                            right: 0,
                            top: 0,
                            bottom: 4,
                        })
                        .show(ui, |ui| {
                            render_detail(ui, idx, item);
                        });
                }
            }
        });

    response
}

// ============================================================================
// Header
// ============================================================================

fn draw_header(ui: &mut Ui, config: &DataTableConfig) {
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
        ("", COL_CHEVRON),
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
    item: &DataRowItem,
    config: &DataTableConfig,
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

    // ── Chevron ──
    let chevron = if is_selected {
        crate::PhosphorIcon::CaretDown
    } else {
        crate::PhosphorIcon::CaretRight
    };
    let chevron_color = if is_selected || is_hovered {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_MUTED
    };
    chevron.paint(
        painter,
        egui::pos2(x + COL_CHEVRON / 2.0, cy),
        egui::Align2::CENTER_CENTER,
        10.0,
        chevron_color,
    );
    x += COL_CHEVRON;

    // ── Icon ──
    if let Some(ref url) = item.icon_url {
        let icon_rect = Rect::from_center_size(
            egui::pos2(x + COL_ICON / 2.0, cy),
            Vec2::splat(config.icon_size),
        );
        let image = egui::Image::new(url.as_str())
            .fit_to_exact_size(Vec2::splat(config.icon_size))
            .corner_radius(CornerRadius::same(2));
        image.paint_at(ui, icon_rect);
    }
    x += COL_ICON;

    // ── Token name ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        &item.token_name,
        egui::FontId::proportional(11.0),
        theme::TEXT_PRIMARY,
    );
    x += COL_TOKEN;

    // ── Principal ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        &item.principal,
        egui::FontId::monospace(11.0),
        theme::ACCENT_CYAN,
    );
    x += COL_PRINCIPAL;

    // ── Collateral ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        &item.collateral,
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
        &item.rate,
        egui::FontId::monospace(10.0),
        theme::TEXT_PRIMARY,
    );
    x += COL_RATE;

    // ── Duration ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        &item.duration,
        egui::FontId::proportional(10.0),
        theme::TEXT_MUTED,
    );
    x += COL_DURATION;

    // ── Interest ──
    painter.text(
        egui::pos2(x + 4.0, cy),
        egui::Align2::LEFT_CENTER,
        &item.interest,
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
