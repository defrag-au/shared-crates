//! Focus list — a fixed-geometry master–detail widget for inspecting one item
//! out of many in a constrained surface (typically a pinned chart tooltip).
//!
//! A stable, windowed list of compact rows (one highlighted) sits above a
//! detail pane showing the focused item in full. The focus is driven by the
//! CALLER (e.g. `price_timeline`'s scroll-to-cycle), so moving it only slides
//! the highlight and swaps the detail pane — the list itself never reflows,
//! reorders, or changes height. Every slot (rows, "more above/below" markers)
//! has a fixed height precisely so cycling causes zero layout shift.
//!
//! Domain-free: the caller renders each row and the detail pane via closures.

use egui::{Align, Color32, Layout, Rect, Sense, UiBuilder, Vec2};

/// Appearance / geometry configuration.
pub struct FocusListConfig {
    /// Number of compact row slots — the window size. The window slides to
    /// keep the focused row visible (roughly centered).
    pub visible_rows: usize,
    /// Fixed height of each compact row.
    pub row_height: f32,
    /// Minimum width of the whole widget — keeps the popup from resizing as
    /// the focus moves across items with different content widths.
    pub min_width: f32,
    /// Background fill for the focused row.
    pub highlight: Color32,
    /// Accent stripe drawn at the left edge of the focused row — the strong
    /// signal; the fill alone is easy to miss on dark themes.
    pub accent: Color32,
    /// Color of the "…n above / below" marker text.
    pub marker_color: Color32,
    /// Marker slot height (the slots are reserved even when empty, so the
    /// list height never changes as the window reaches the ends).
    pub marker_height: f32,
}

impl Default for FocusListConfig {
    fn default() -> Self {
        Self {
            visible_rows: 7,
            row_height: 17.0,
            min_width: 300.0,
            highlight: Color32::from_rgb(56, 61, 82),
            accent: Color32::from_rgb(125, 207, 255),
            marker_color: Color32::from_rgb(120, 126, 150),
            marker_height: 12.0,
        }
    }
}

/// Draw the list + detail pane. `focus` is clamped to `len`; `row` renders one
/// compact row (`is_focused` for styling beyond the highlight fill), `detail`
/// renders the focused item's full view below the list.
pub fn show(
    ui: &mut egui::Ui,
    len: usize,
    focus: usize,
    config: &FocusListConfig,
    mut row: impl FnMut(&mut egui::Ui, usize, bool),
    detail: impl FnOnce(&mut egui::Ui, usize),
) {
    if len == 0 {
        return;
    }
    ui.set_min_width(config.min_width);
    let focus = focus.min(len - 1);
    let (start, end) = window(len, focus, config.visible_rows);
    let width = ui.available_width().max(config.min_width);

    let marker = |ui: &mut egui::Ui, count: usize, label: &str| {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(width, config.marker_height), Sense::hover());
        if count > 0 {
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                format!("\u{2026}{count} {label}"),
                egui::FontId::proportional(9.0),
                config.marker_color,
            );
        }
    };

    ui.scope(|ui| {
        // Tight rows — the fixed slot heights are the geometry; default
        // vertical item spacing between them reads as sprawl.
        ui.spacing_mut().item_spacing.y = 2.0;
        marker(ui, start, "above");
        for pos in start..end {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, config.row_height), Sense::hover());
            let is_focused = pos == focus;
            if is_focused {
                let band = rect.expand2(Vec2::new(2.0, 1.0));
                ui.painter().rect_filled(band, 3.0, config.highlight);
                ui.painter().rect_filled(
                    Rect::from_min_size(band.min, Vec2::new(2.5, band.height())),
                    1.0,
                    config.accent,
                );
            }
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(shrink_x(rect, 6.0))
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().interact_size.y = 0.0;
                    row(ui, pos, is_focused);
                },
            );
        }
        marker(ui, len - end, "below");
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    detail(ui, focus);
}

/// The window `[start, end)` of `visible` rows that keeps `focus` roughly
/// centered, clamped to the list bounds.
fn window(len: usize, focus: usize, visible: usize) -> (usize, usize) {
    let visible = visible.max(1).min(len);
    let start = focus.saturating_sub(visible / 2).min(len - visible);
    (start, start + visible)
}

fn shrink_x(rect: Rect, by: f32) -> Rect {
    Rect::from_min_max(
        egui::pos2(rect.min.x + by, rect.min.y),
        egui::pos2(rect.max.x - by, rect.max.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keeps_focus_visible_and_clamped() {
        // Centered in the middle of a long list.
        assert_eq!(window(50, 20, 7), (17, 24));
        // Clamped at the start / end.
        assert_eq!(window(50, 0, 7), (0, 7));
        assert_eq!(window(50, 49, 7), (43, 50));
        // Short lists show everything.
        assert_eq!(window(4, 2, 7), (0, 4));
        for focus in 0..50 {
            let (s, e) = window(50, focus, 7);
            assert!(s <= focus && focus < e, "focus {focus} outside {s}..{e}");
            assert_eq!(e - s, 7);
        }
    }
}
