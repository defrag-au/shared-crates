//! Horizontal pip row widget — a label on the left and a bar of colored pips
//! (or density heatmap) on the right, each positioned proportionally by value.
//!
//! Useful for showing distributions, market depth, listing spreads, or any
//! scenario where you want to visualize a set of values along a common axis.
//!
//! Two rendering modes:
//! - **Pips**: individual colored rectangles at exact positions (good for sparse data)
//! - **Density**: continuous heatmap where brightness encodes local density (good for clusters)

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

// ============================================================================
// Public types
// ============================================================================

/// A single pip (data point) in the bar.
pub struct Pip {
    /// Value determining horizontal position (in domain units).
    pub value: f64,
    /// Color of this pip.
    pub color: Color32,
}

/// Data for one row: label + pips.
pub struct PipRowData<'a> {
    /// Text label shown at the left edge.
    pub label: &'a str,
    /// Label color.
    pub label_color: Color32,
    /// The pips to draw in the bar, positioned by value.
    pub pips: &'a [Pip],
    /// Text shown when the bar has no pips (e.g. "no listings", "\u{2014}").
    pub empty_text: Option<&'a str>,
    /// Color for the empty text.
    pub empty_color: Color32,
}

/// Rendering mode for the bar area, carrying mode-specific settings.
pub enum PipRowMode {
    /// Individual colored rectangles at exact positions.
    Pips {
        /// Width of each pip.
        pip_width: f32,
        /// Corner radius of each pip.
        pip_rounding: f32,
        /// Overflow text color ("+N more").
        overflow_color: Color32,
    },
    /// Continuous heatmap where brightness encodes local density.
    Density {
        /// Number of bins across the bar width.
        bins: usize,
        /// Base color (brightness/alpha modulated by density).
        color: Color32,
        /// Minimum opacity for bins with at least one value (0.0–1.0).
        min_alpha: f32,
    },
}

impl Default for PipRowMode {
    fn default() -> Self {
        Self::Pips {
            pip_width: 4.0,
            pip_rounding: 1.0,
            overflow_color: Color32::from_rgb(160, 160, 180),
        }
    }
}

impl PipRowMode {
    /// Convenience constructor for density mode with sensible defaults.
    pub fn density() -> Self {
        Self::Density {
            bins: 40,
            color: Color32::from_rgb(125, 207, 255),
            min_alpha: 0.15,
        }
    }
}

/// Appearance configuration for the pip row widget.
pub struct PipRowConfig {
    /// Rendering mode (carries mode-specific settings).
    pub mode: PipRowMode,
    /// Width of the label column in pixels.
    pub label_width: f32,
    /// Height of each row.
    pub row_height: f32,
    /// Height of the bar within each row.
    pub bar_height: f32,
    /// Bar background color.
    pub bar_color: Color32,
    /// Bar corner radius.
    pub bar_rounding: f32,
    /// Label font size.
    pub label_font_size: f32,
    /// Empty-text font size.
    pub empty_font_size: f32,
}

impl Default for PipRowConfig {
    fn default() -> Self {
        Self {
            mode: PipRowMode::default(),
            label_width: 200.0,
            row_height: 26.0,
            bar_height: 18.0,
            bar_color: Color32::from_rgb(40, 43, 55),
            bar_rounding: 3.0,
            label_font_size: 12.0,
            empty_font_size: 10.0,
        }
    }
}

// ============================================================================
// Hover info types
// ============================================================================

/// Info about a single hovered pip (Pips mode).
pub struct HoveredPip {
    /// Index into the original `pips` slice.
    pub index: usize,
    /// The pip's domain value.
    pub value: f64,
}

/// Info about a hovered density bin (Density mode).
pub struct HoveredBin {
    /// Bin index (0-based).
    pub index: usize,
    /// Number of pips that fall in this bin.
    pub count: u32,
    /// Domain value at the low edge of the bin.
    pub range_lo: f64,
    /// Domain value at the high edge of the bin.
    pub range_hi: f64,
}

/// What the user is hovering over in the bar.
pub enum HoverInfo {
    /// One or more nearby pips (Pips mode). Sorted by proximity (nearest first).
    Pips(Vec<HoveredPip>),
    /// A density bin (Density mode).
    Bin(HoveredBin),
}

/// Result returned per row so the caller can attach tooltips or handle clicks.
pub struct PipRowResponse {
    /// The egui response for the entire row (for hover/click).
    pub response: egui::Response,
    /// The rect of the bar area (for custom overlays).
    pub bar_rect: Rect,
    /// What's under the cursor, if hovering the bar area.
    pub hover: Option<HoverInfo>,
    /// Anchor point for a positioned tooltip (crosshair x, just below the bar).
    /// Present only when `hover` is `Some`.
    pub tooltip_anchor: Option<Pos2>,
}

impl PipRowResponse {
    /// Show a tooltip anchored at the hover position (below the crosshair).
    /// Only shows when the bar is being hovered. Returns `None` if not hovering.
    pub fn show_tooltip(&self, add_contents: impl FnOnce(&mut egui::Ui, &HoverInfo)) -> Option<()> {
        let hover = self.hover.as_ref()?;
        let anchor = self.tooltip_anchor?;
        egui::Tooltip::always_open(
            self.response.ctx.clone(),
            self.response.layer_id,
            self.response.id,
            anchor,
        )
        .show(|ui| add_contents(ui, hover));
        Some(())
    }
}

// ============================================================================
// Constants
// ============================================================================

/// Pixel radius for pip proximity detection.
const PIP_HOVER_RADIUS: f32 = 12.0;

/// Crosshair line color.
const CROSSHAIR_COLOR: Color32 = Color32::from_rgb(160, 160, 180);

/// Highlight stroke for hovered pips.
const PIP_HIGHLIGHT_COLOR: Color32 = Color32::from_rgb(240, 240, 255);

/// Highlight stroke for hovered density bin.
const BIN_HIGHLIGHT_COLOR: Color32 = Color32::from_rgb(220, 220, 240);

// ============================================================================
// Drawing
// ============================================================================

/// Draw a single pip row and return its response.
///
/// `global_max` is the maximum value across all rows — used to scale pip
/// positions consistently across multiple rows.
pub fn show(
    ui: &mut egui::Ui,
    data: &PipRowData<'_>,
    global_max: f64,
    config: &PipRowConfig,
) -> PipRowResponse {
    let bar_area_width = (ui.available_width() - config.label_width - 8.0).max(100.0);

    let (row_rect, row_resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), config.row_height),
        Sense::hover(),
    );

    let label_rect = Rect::from_min_size(
        row_rect.min,
        Vec2::new(config.label_width, config.row_height),
    );
    let bar_rect = Rect::from_min_size(
        Pos2::new(
            row_rect.min.x + config.label_width + 4.0,
            row_rect.min.y + (config.row_height - config.bar_height) / 2.0,
        ),
        Vec2::new(bar_area_width, config.bar_height),
    );

    // Label
    ui.painter().with_clip_rect(label_rect).text(
        Pos2::new(label_rect.min.x, label_rect.center().y),
        Align2::LEFT_CENTER,
        data.label,
        FontId::proportional(config.label_font_size),
        data.label_color,
    );

    // Bar background
    ui.painter()
        .rect_filled(bar_rect, config.bar_rounding, config.bar_color);

    // Detect hover position within the bar
    let bar_hover_x = row_resp
        .hover_pos()
        .filter(|pos| bar_rect.contains(*pos))
        .map(|pos| pos.x);

    let mut hover_info: Option<HoverInfo> = None;
    let mut tooltip_anchor: Option<Pos2> = None;

    if data.pips.is_empty() {
        // Empty state
        if let Some(text) = data.empty_text {
            ui.painter().text(
                bar_rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(config.empty_font_size),
                data.empty_color,
            );
        }
    } else if global_max > 0.0 {
        match &config.mode {
            PipRowMode::Pips {
                pip_width,
                pip_rounding,
                overflow_color,
            } => {
                let max_pips = ((bar_area_width / (pip_width + 1.0)) as usize).min(data.pips.len());

                // Collect pip screen positions for hover detection
                let mut pip_positions: Vec<(usize, f32, f64)> = Vec::with_capacity(max_pips);

                for (idx, pip) in data.pips.iter().take(max_pips).enumerate() {
                    let x_frac = (pip.value / global_max).min(1.0) as f32;
                    let x = bar_rect.min.x + x_frac * (bar_area_width - pip_width);
                    let x_center = x + pip_width / 2.0;
                    pip_positions.push((idx, x_center, pip.value));
                }

                // Find nearby pips if hovering
                let nearby: Vec<(usize, f64, f32)> = if let Some(hover_x) = bar_hover_x {
                    let mut near: Vec<(usize, f64, f32)> = pip_positions
                        .iter()
                        .filter_map(|&(idx, x_center, value)| {
                            let dist = (hover_x - x_center).abs();
                            if dist <= PIP_HOVER_RADIUS {
                                Some((idx, value, dist))
                            } else {
                                None
                            }
                        })
                        .collect();
                    near.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
                    near
                } else {
                    Vec::new()
                };

                // Draw pips, highlighting nearby ones
                for &(idx, x_center, _value) in &pip_positions {
                    let x = x_center - pip_width / 2.0;
                    let pip_rect = Rect::from_min_size(
                        Pos2::new(x, bar_rect.min.y + 1.0),
                        Vec2::new(*pip_width, config.bar_height - 2.0),
                    );

                    let pip_color = data.pips[idx].color;
                    ui.painter().rect_filled(pip_rect, *pip_rounding, pip_color);

                    // Highlight if nearby
                    if nearby.iter().any(|&(ni, _, _)| ni == idx) {
                        ui.painter().rect_stroke(
                            pip_rect.expand(1.0),
                            *pip_rounding + 1.0,
                            Stroke::new(1.0, PIP_HIGHLIGHT_COLOR),
                            egui::StrokeKind::Outside,
                        );
                    }
                }

                if data.pips.len() > max_pips {
                    ui.painter().text(
                        Pos2::new(bar_rect.max.x - 2.0, bar_rect.center().y),
                        Align2::RIGHT_CENTER,
                        format!("+{}", data.pips.len() - max_pips),
                        FontId::proportional(9.0),
                        *overflow_color,
                    );
                }

                // Build hover info
                if !nearby.is_empty() {
                    if let Some(hover_x) = bar_hover_x {
                        tooltip_anchor = Some(Pos2::new(hover_x, bar_rect.bottom() + 4.0));
                    }
                    hover_info = Some(HoverInfo::Pips(
                        nearby
                            .iter()
                            .map(|&(idx, value, _)| HoveredPip { index: idx, value })
                            .collect(),
                    ));
                }
            }
            PipRowMode::Density {
                bins,
                color,
                min_alpha,
            } => {
                let counts = draw_density(
                    ui,
                    data.pips,
                    global_max,
                    bar_rect,
                    *bins,
                    *color,
                    *min_alpha,
                    bar_hover_x,
                );

                // Build hover info for the hovered bin
                if let Some(hover_x) = bar_hover_x {
                    let bin_count = counts.len();
                    let rel_x = (hover_x - bar_rect.left()) / bar_rect.width();
                    let bin_idx = ((rel_x * bin_count as f32) as usize).min(bin_count - 1);
                    let count = counts[bin_idx];
                    let range_lo = (bin_idx as f64 / bin_count as f64) * global_max;
                    let range_hi = ((bin_idx + 1) as f64 / bin_count as f64) * global_max;
                    tooltip_anchor = Some(Pos2::new(hover_x, bar_rect.bottom() + 4.0));
                    hover_info = Some(HoverInfo::Bin(HoveredBin {
                        index: bin_idx,
                        count,
                        range_lo,
                        range_hi,
                    }));
                }
            }
        }
    }

    // Draw crosshair if hovering the bar
    if let Some(hover_x) = bar_hover_x {
        ui.painter().line_segment(
            [
                Pos2::new(hover_x, bar_rect.top()),
                Pos2::new(hover_x, bar_rect.bottom()),
            ],
            Stroke::new(0.5, CROSSHAIR_COLOR),
        );
    }

    PipRowResponse {
        response: row_resp,
        bar_rect,
        hover: hover_info,
        tooltip_anchor,
    }
}

/// Render pips as a continuous density heatmap. Returns bin counts for hover.
#[allow(clippy::too_many_arguments)]
fn draw_density(
    ui: &mut egui::Ui,
    pips: &[Pip],
    global_max: f64,
    bar_rect: Rect,
    bins: usize,
    base_color: Color32,
    min_alpha: f32,
    hover_x: Option<f32>,
) -> Vec<u32> {
    let bins = bins.max(1);
    let mut counts = vec![0u32; bins];

    // Bin each pip value
    for pip in pips {
        let frac = (pip.value / global_max).min(1.0);
        let idx = ((frac * bins as f64) as usize).min(bins - 1);
        counts[idx] += 1;
    }

    let max_count = counts.iter().copied().max().unwrap_or(1).max(1);
    let bin_width = bar_rect.width() / bins as f32;
    let inner_height = bar_rect.height() - 2.0;

    // Determine which bin is hovered
    let hovered_bin = hover_x.map(|hx| {
        let rel_x = (hx - bar_rect.left()) / bar_rect.width();
        ((rel_x * bins as f32) as usize).min(bins - 1)
    });

    for (i, &count) in counts.iter().enumerate() {
        let bin_rect = Rect::from_min_size(
            Pos2::new(bar_rect.min.x + i as f32 * bin_width, bar_rect.min.y + 1.0),
            Vec2::new(bin_width, inner_height),
        );

        if count > 0 {
            // Map count to alpha: min_alpha at 1 pip, 1.0 at max_count
            let t = if max_count <= 1 {
                1.0
            } else {
                (count - 1) as f32 / (max_count - 1) as f32
            };
            let alpha = min_alpha + (1.0 - min_alpha) * t;
            ui.painter()
                .rect_filled(bin_rect, 0.0, base_color.gamma_multiply(alpha));
        }

        // Highlight hovered bin
        if hovered_bin == Some(i) {
            ui.painter().rect_stroke(
                bin_rect,
                0.0,
                Stroke::new(1.0, BIN_HIGHLIGHT_COLOR),
                egui::StrokeKind::Inside,
            );
        }
    }

    counts
}

/// Map a value to a green-yellow-red gradient color.
///
/// Useful for coloring pips by price or intensity:
/// - 0.0 → green
/// - 0.5 → yellow
/// - 1.0 → red
pub fn heat_color(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let s = t * 2.0;
        Color32::from_rgb(
            (158.0 + (224.0 - 158.0) * s) as u8,
            (206.0 + (175.0 - 206.0) * s) as u8,
            (106.0 + (104.0 - 106.0) * s) as u8,
        )
    } else {
        let s = (t - 0.5) * 2.0;
        Color32::from_rgb(
            (224.0 + (247.0 - 224.0) * s) as u8,
            (175.0 + (118.0 - 175.0) * s) as u8,
            (104.0 + (142.0 - 104.0) * s) as u8,
        )
    }
}
