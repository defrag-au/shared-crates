//! Split allocation bar — segmented horizontal bar showing ADA allocation
//! across multiple DEXes.
//!
//! Each segment's width is proportional to its allocation fraction. Percentage
//! labels are painted inside segments when wide enough. Hover tooltips show
//! DEX name and ADA amount. Optional legend row below.

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single segment in the allocation bar.
pub struct AllocationSegment {
    /// DEX or pool label (e.g. "Splash", "CSWAP").
    pub label: String,
    /// ADA amount allocated to this segment in lovelace.
    pub amount_lovelace: u64,
    /// Fraction of total allocation (0.0..=1.0).
    pub fraction: f32,
    /// Color for this segment.
    pub color: Color32,
}

/// Configuration for the split allocation bar.
pub struct SplitAllocationBarConfig {
    /// Bar height in pixels.
    pub bar_height: f32,
    /// Corner radius.
    pub corner_radius: u8,
    /// Background color for empty/unfilled region.
    pub bg_color: Color32,
    /// Minimum segment width (pixels) to show percentage label inside.
    pub min_label_width: f32,
    /// Whether to show the legend row below the bar.
    pub show_legend: bool,
    /// Whether to show the built-in hover tooltip (segment amounts as ADA).
    /// Set to `false` when providing a custom tooltip via the returned `Response`.
    pub show_tooltip: bool,
    /// Font size for percentage labels inside segments.
    pub label_size: f32,
    /// Font size for legend text.
    pub legend_size: f32,
}

impl Default for SplitAllocationBarConfig {
    fn default() -> Self {
        Self {
            bar_height: 20.0,
            corner_radius: 4,
            bg_color: theme::BG_SECONDARY,
            min_label_width: 40.0,
            show_legend: true,
            show_tooltip: true,
            label_size: 10.0,
            legend_size: 10.0,
        }
    }
}

// ============================================================================
// DEX color palette
// ============================================================================

/// Get a color for a DEX by index. Cycles through the accent palette.
pub fn dex_color(index: usize) -> Color32 {
    const PALETTE: &[Color32] = &[
        theme::ACCENT_CYAN,
        theme::ACCENT_MAGENTA,
        theme::ACCENT_YELLOW,
        theme::ACCENT_ORANGE,
        theme::ACCENT_GREEN,
        theme::ACCENT_BLUE,
    ];
    PALETTE[index % PALETTE.len()]
}

// ============================================================================
// Widget
// ============================================================================

/// Render the split allocation bar. Returns the bar's hover `Response` so
/// callers can attach custom tooltips (set `show_tooltip: false` to suppress
/// the built-in one).
pub fn show(
    ui: &mut Ui,
    segments: &[AllocationSegment],
    config: &SplitAllocationBarConfig,
) -> egui::Response {
    if segments.is_empty() {
        // Allocate zero-size so we always return a Response
        let (_, r) = ui.allocate_exact_size(Vec2::ZERO, Sense::hover());
        return r;
    }

    let available_width = ui.available_width();
    let desired_size = Vec2::new(available_width, config.bar_height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rounding = CornerRadius::same(config.corner_radius);

        // Background track
        painter.rect_filled(rect, rounding, config.bg_color);

        // Paint segments left to right
        let mut x = rect.min.x;
        for (i, seg) in segments.iter().enumerate() {
            if seg.fraction <= 0.0 {
                continue;
            }

            let seg_width = rect.width() * seg.fraction;
            let seg_rect = Rect::from_min_size(
                egui::pos2(x, rect.min.y),
                Vec2::new(seg_width, rect.height()),
            );

            // Corner rounding: first segment gets left corners, last gets right
            let is_first = i == 0 || segments[..i].iter().all(|s| s.fraction <= 0.0);
            let is_last =
                i == segments.len() - 1 || segments[i + 1..].iter().all(|s| s.fraction <= 0.0);

            let cr = config.corner_radius;
            let seg_rounding = CornerRadius {
                nw: if is_first { cr } else { 0 },
                sw: if is_first { cr } else { 0 },
                ne: if is_last { cr } else { 0 },
                se: if is_last { cr } else { 0 },
            };

            painter.rect_filled(seg_rect, seg_rounding, seg.color);

            // Percentage label inside segment (if wide enough)
            if seg_width >= config.min_label_width {
                let pct_text = format!("{}%", (seg.fraction * 100.0).round() as u32);
                painter.text(
                    seg_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    pct_text,
                    egui::FontId::proportional(config.label_size),
                    theme::BG_PRIMARY,
                );
            }

            x += seg_width;
        }
    }

    // Hover tooltip — single summary with one line per segment
    let response = if config.show_tooltip && response.hovered() {
        let mut lines = Vec::new();
        for seg in segments {
            if seg.fraction <= 0.0 {
                continue;
            }
            let ada = seg.amount_lovelace as f64 / 1_000_000.0;
            let pct = (seg.fraction * 100.0).round() as u32;
            lines.push(format!("{}: {:.1} ADA ({pct}%)", seg.label, ada));
        }
        if !lines.is_empty() {
            response.on_hover_text(lines.join("\n"))
        } else {
            response
        }
    } else {
        response
    };

    // Legend row
    if config.show_legend {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for seg in segments {
                if seg.fraction <= 0.0 {
                    continue;
                }
                // Colored dot
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                if ui.is_rect_visible(dot_rect) {
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, seg.color);
                }
                let pct = (seg.fraction * 100.0).round() as u32;
                ui.label(
                    RichText::new(format!("{} {pct}%", seg.label))
                        .color(theme::TEXT_SECONDARY)
                        .size(config.legend_size),
                );
                ui.add_space(8.0);
            }
        });
    }

    response
}
