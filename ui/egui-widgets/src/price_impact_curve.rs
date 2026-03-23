//! Price impact curve chart — visualizes why split routing helps.
//!
//! Shows per-pool AMM price impact curves (impact % vs ADA input) with
//! the optimizer's allocation points overlaid. Makes it visually obvious
//! that splitting keeps you in the cheap region of each pool's curve.
//!
//! The AMM math is **injected** via a `price_impact_fn` closure parameter —
//! the widget itself has no dependency on `cardano-tx`. Callers provide a
//! function `(input_lovelace, &ImpactCurvePool) -> impact_fraction` that
//! computes the price impact for a given input amount and pool.

use egui::{Color32, CornerRadius, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Pool data needed to compute the price impact curve.
pub struct ImpactCurvePool {
    /// DEX or pool label (e.g. "Splash", "CSWAP").
    pub label: String,
    /// Color for this pool's curve.
    pub color: Color32,
    /// ADA reserves in lovelace.
    pub ada_reserves: u64,
    /// Token reserves.
    pub token_reserves: u64,
    /// Pool fee in basis points (e.g. 30 = 0.3%).
    pub fee_bps: u64,
    /// Optimizer's chosen allocation for this pool in lovelace.
    /// `None` if the pool was not allocated to.
    pub allocation: Option<u64>,
}

/// Configuration for the price impact curve chart.
pub struct PriceImpactCurveConfig {
    /// Chart height in pixels.
    pub chart_height: f32,
    /// Total ADA being swapped in lovelace (defines x-axis range).
    pub total_input: u64,
    /// Number of sample points per curve.
    pub curve_resolution: usize,
    /// Whether to show semi-transparent fill below curves.
    pub show_fill: bool,
    /// Whether to show a dashed reference for the best single pool at total input.
    pub show_single_pool_ref: bool,
    /// Whether to show the legend row below the chart.
    pub show_legend: bool,
    /// Whether to show axis labels and grid lines.
    pub show_axes: bool,
}

impl Default for PriceImpactCurveConfig {
    fn default() -> Self {
        Self {
            chart_height: 200.0,
            total_input: 1_000_000_000, // 1000 ADA
            curve_resolution: 100,
            show_fill: true,
            show_single_pool_ref: true,
            show_legend: true,
            show_axes: true,
        }
    }
}

// ============================================================================
// Price impact function type
// ============================================================================

/// Computes price impact as a fraction (0.0 = no impact, 1.0 = 100% impact)
/// for a given `input_lovelace` into the specified pool.
///
/// Callers inject their own AMM math — e.g. wrapping
/// `cardano_tx::dex::cswap::pool::constant_product_swap`.
pub type PriceImpactFn = dyn Fn(u64, &ImpactCurvePool) -> f64;

/// Helper: build a [`PriceImpactFn`] from a constant-product swap function.
///
/// `swap_fn(input, input_reserves, output_reserves, fee_bps) -> output`
///
/// This computes impact as `1 - (actual_rate / spot_rate)` where spot rate
/// is the marginal price at a tiny (1 ADA) reference swap.
pub fn constant_product_impact_fn(
    swap_fn: impl Fn(u64, u64, u64, u64) -> u64 + 'static,
) -> Box<PriceImpactFn> {
    Box::new(move |input, pool| {
        if input == 0 || pool.ada_reserves == 0 || pool.token_reserves == 0 {
            return 0.0;
        }
        let spot_ref = 1_000_000u64; // 1 ADA
        let spot_output = swap_fn(
            spot_ref,
            pool.ada_reserves,
            pool.token_reserves,
            pool.fee_bps,
        );
        if spot_output == 0 {
            return 0.0;
        }
        let spot_rate = spot_output as f64 / spot_ref as f64;
        let actual_output = swap_fn(input, pool.ada_reserves, pool.token_reserves, pool.fee_bps);
        let actual_rate = actual_output as f64 / input as f64;
        (1.0 - actual_rate / spot_rate).max(0.0)
    })
}

// ============================================================================
// Widget
// ============================================================================

/// Render the price impact curve chart.
///
/// `price_impact_fn(input_lovelace, pool) -> impact_fraction` is injected
/// by the caller — the widget has no AMM math dependency.
pub fn show(
    ui: &mut Ui,
    pools: &[ImpactCurvePool],
    config: &PriceImpactCurveConfig,
    price_impact_fn: &PriceImpactFn,
) {
    if pools.is_empty() || config.total_input == 0 {
        return;
    }

    let padding = 4.0;
    let left_margin = if config.show_axes { 38.0 } else { padding };
    let bottom_margin = if config.show_axes { 20.0 } else { padding };

    let chart_width = ui.available_width();
    let total_height = config.chart_height + bottom_margin;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(chart_width, total_height), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    let rounding = CornerRadius::same(4);

    // Background
    painter.rect_filled(rect, rounding, theme::BG_SECONDARY);
    painter.rect_stroke(
        rect,
        rounding,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Outside,
    );

    // Plot area (inside margins)
    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + left_margin, rect.min.y + padding),
        egui::pos2(rect.max.x - padding, rect.max.y - bottom_margin),
    );

    // ── Compute all curves ─────────────────────────────────────────────

    let n = config.curve_resolution.max(2);
    let total = config.total_input;

    // Sample points for each pool: Vec<Vec<(input_lovelace, impact_fraction)>>
    let curves: Vec<Vec<(u64, f64)>> = pools
        .iter()
        .map(|pool| {
            (0..=n)
                .map(|i| {
                    let input = (total as u128 * i as u128 / n as u128) as u64;
                    let impact = price_impact_fn(input, pool);
                    (input, impact)
                })
                .collect()
        })
        .collect();

    // Find the maximum impact across all curves for y-axis scaling
    let max_impact = curves
        .iter()
        .flat_map(|c| c.iter().map(|(_, impact)| *impact))
        .fold(0.0f64, f64::max)
        .max(0.001); // floor to avoid zero range

    // Add 20% headroom for readability
    let y_max = max_impact * 1.2;

    // ── Axis grid and labels ───────────────────────────────────────────

    if config.show_axes {
        // Y-axis: impact percentage grid lines
        let y_tick_count = 4;
        for i in 0..=y_tick_count {
            let frac = i as f32 / y_tick_count as f32;
            let y = plot_rect.bottom() - frac * plot_rect.height();
            let impact_pct = y_max * frac as f64 * 100.0;

            // Grid line
            if i > 0 {
                let dash_len = 3.0;
                let gap_len = 3.0;
                let mut x = plot_rect.left();
                while x < plot_rect.right() {
                    let end_x = (x + dash_len).min(plot_rect.right());
                    painter.line_segment(
                        [Pos2::new(x, y), Pos2::new(end_x, y)],
                        Stroke::new(0.5, theme::BG_HIGHLIGHT),
                    );
                    x += dash_len + gap_len;
                }
            }

            // Label
            let label = if impact_pct < 0.1 {
                "0%".to_string()
            } else {
                format!("{impact_pct:.1}%")
            };
            painter.text(
                Pos2::new(rect.min.x + left_margin - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                label,
                egui::FontId::proportional(9.0),
                theme::TEXT_MUTED,
            );
        }

        // X-axis: ADA amount labels
        let x_tick_count = 4;
        for i in 0..=x_tick_count {
            let frac = i as f32 / x_tick_count as f32;
            let x = plot_rect.left() + frac * plot_rect.width();
            let ada = (total as f64 * frac as f64) / 1_000_000.0;

            // Tick mark
            painter.line_segment(
                [
                    Pos2::new(x, plot_rect.bottom()),
                    Pos2::new(x, plot_rect.bottom() + 3.0),
                ],
                Stroke::new(0.5, theme::TEXT_MUTED),
            );

            // Label
            let label = if ada >= 1000.0 {
                format!("{:.0}K", ada / 1000.0)
            } else {
                format!("{ada:.0}")
            };
            painter.text(
                Pos2::new(x, plot_rect.bottom() + 5.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::proportional(9.0),
                theme::TEXT_MUTED,
            );
        }

        // X-axis title
        painter.text(
            Pos2::new(plot_rect.center().x, rect.max.y - 2.0),
            egui::Align2::CENTER_BOTTOM,
            "ADA Input",
            egui::FontId::proportional(9.0),
            theme::TEXT_MUTED,
        );
    }

    // Baseline (x-axis line)
    painter.line_segment(
        [
            Pos2::new(plot_rect.left(), plot_rect.bottom()),
            Pos2::new(plot_rect.right(), plot_rect.bottom()),
        ],
        Stroke::new(1.0, theme::BORDER),
    );

    // ── Draw curves ────────────────────────────────────────────────────

    for (pool_idx, (pool, curve)) in pools.iter().zip(curves.iter()).enumerate() {
        // Map curve points to pixel positions
        let points: Vec<Pos2> = curve
            .iter()
            .map(|&(input, impact)| {
                let x_frac = input as f32 / total as f32;
                let y_frac = (impact / y_max) as f32;
                Pos2::new(
                    plot_rect.left() + x_frac * plot_rect.width(),
                    plot_rect.bottom() - y_frac * plot_rect.height(),
                )
            })
            .collect();

        // Fill area below curve
        if config.show_fill {
            let fill_color = pool.color.gamma_multiply(0.15);
            for window in points.windows(2) {
                let p0 = window[0];
                let p1 = window[1];
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 0, 2, 3],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: p0,
                            uv: egui::epaint::WHITE_UV,
                            color: fill_color,
                        },
                        egui::epaint::Vertex {
                            pos: p1,
                            uv: egui::epaint::WHITE_UV,
                            color: fill_color,
                        },
                        egui::epaint::Vertex {
                            pos: Pos2::new(p1.x, plot_rect.bottom()),
                            uv: egui::epaint::WHITE_UV,
                            color: Color32::TRANSPARENT,
                        },
                        egui::epaint::Vertex {
                            pos: Pos2::new(p0.x, plot_rect.bottom()),
                            uv: egui::epaint::WHITE_UV,
                            color: Color32::TRANSPARENT,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
        }

        // Curve line
        let line_stroke = Stroke::new(2.0, pool.color);
        for window in points.windows(2) {
            painter.line_segment([window[0], window[1]], line_stroke);
        }

        // Allocation marker (dot on curve at optimizer's chosen amount)
        if let Some(alloc) = pool.allocation {
            let impact_at_alloc = price_impact_fn(alloc, pool);
            let x_frac = alloc as f32 / total as f32;
            let y_frac = (impact_at_alloc / y_max) as f32;
            let marker_pos = Pos2::new(
                plot_rect.left() + x_frac * plot_rect.width(),
                plot_rect.bottom() - y_frac * plot_rect.height(),
            );

            // Outer ring + filled dot
            painter.circle_filled(marker_pos, 5.0, pool.color);
            painter.circle_stroke(marker_pos, 5.0, Stroke::new(1.5, theme::BG_PRIMARY));

            // Label next to marker
            let impact_pct = impact_at_alloc * 100.0;
            let label = format!("{impact_pct:.2}%");
            // Offset label to avoid overlapping with other curves
            let label_offset = if pool_idx % 2 == 0 { -12.0 } else { 12.0 };
            painter.text(
                Pos2::new(marker_pos.x + 8.0, marker_pos.y + label_offset),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(10.0),
                pool.color,
            );
        }
    }

    // ── Single-pool reference (dashed) ─────────────────────────────────

    if config.show_single_pool_ref && pools.len() > 1 {
        // Find the best single pool (lowest impact at total input)
        let best_single = pools
            .iter()
            .map(|p| {
                let impact = price_impact_fn(total, p);
                (p, impact)
            })
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((_, single_impact)) = best_single {
            let y_frac = (single_impact / y_max) as f32;
            let ref_y = plot_rect.bottom() - y_frac * plot_rect.height();

            // Dashed horizontal line
            let dash_len = 5.0;
            let gap_len = 3.0;
            let mut x = plot_rect.left();
            while x < plot_rect.right() {
                let end_x = (x + dash_len).min(plot_rect.right());
                painter.line_segment(
                    [Pos2::new(x, ref_y), Pos2::new(end_x, ref_y)],
                    Stroke::new(1.0, theme::TEXT_MUTED),
                );
                x += dash_len + gap_len;
            }

            // Label at right edge
            let ref_pct = single_impact * 100.0;
            painter.text(
                Pos2::new(plot_rect.right() - 2.0, ref_y - 8.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("single: {ref_pct:.2}%"),
                egui::FontId::proportional(9.0),
                theme::TEXT_MUTED,
            );
        }
    }

    // ── Hover interaction ──────────────────────────────────────────────

    if response.hovered() {
        if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
            if plot_rect.contains(hover_pos) {
                let rel_x = (hover_pos.x - plot_rect.left()) / plot_rect.width();
                let hover_input = (total as f64 * rel_x as f64) as u64;

                // Vertical crosshair
                painter.line_segment(
                    [
                        Pos2::new(hover_pos.x, plot_rect.top()),
                        Pos2::new(hover_pos.x, plot_rect.bottom()),
                    ],
                    Stroke::new(0.5, theme::TEXT_MUTED),
                );

                // Find impact for each pool at this x position and show tooltip
                let mut tooltip_lines =
                    vec![format!("{:.0} ADA", hover_input as f64 / 1_000_000.0)];
                for pool in pools {
                    let impact = price_impact_fn(hover_input, pool);
                    tooltip_lines.push(format!("{}: {:.2}%", pool.label, impact * 100.0));

                    // Highlight dot on each curve at hover position
                    let y_frac = (impact / y_max) as f32;
                    let dot_pos = Pos2::new(
                        hover_pos.x,
                        plot_rect.bottom() - y_frac * plot_rect.height(),
                    );
                    painter.circle_filled(dot_pos, 3.0, pool.color);
                }

                response.clone().on_hover_text(tooltip_lines.join("\n"));
            }
        }
    }

    // ── Legend ──────────────────────────────────────────────────────────

    if config.show_legend {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for pool in pools {
                // Colored dot
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                if ui.is_rect_visible(dot_rect) {
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, pool.color);
                }

                let text = if let Some(alloc) = pool.allocation {
                    let ada = alloc as f64 / 1_000_000.0;
                    let impact = price_impact_fn(alloc, pool) * 100.0;
                    format!("{}: {impact:.2}% at {ada:.0} ADA", pool.label)
                } else {
                    pool.label.clone()
                };

                ui.label(RichText::new(text).color(theme::TEXT_SECONDARY).size(10.0));
                ui.add_space(8.0);
            }

            // Single pool reference in legend
            if config.show_single_pool_ref && pools.len() > 1 {
                let best_impact = pools
                    .iter()
                    .map(|p| price_impact_fn(total, p))
                    .fold(f64::INFINITY, f64::min);

                // Draw a small dashed line before the label
                let (dash_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 8.0), Sense::hover());
                if ui.is_rect_visible(dash_rect) {
                    let y = dash_rect.center().y;
                    let painter = ui.painter();
                    // Two short dashes
                    painter.line_segment(
                        [
                            Pos2::new(dash_rect.left(), y),
                            Pos2::new(dash_rect.left() + 6.0, y),
                        ],
                        Stroke::new(1.5, theme::TEXT_MUTED),
                    );
                    painter.line_segment(
                        [
                            Pos2::new(dash_rect.left() + 10.0, y),
                            Pos2::new(dash_rect.right(), y),
                        ],
                        Stroke::new(1.5, theme::TEXT_MUTED),
                    );
                }

                ui.label(
                    RichText::new(format!("single pool: {:.2}%", best_impact * 100.0))
                        .color(theme::TEXT_MUTED)
                        .size(10.0),
                );
            }
        });
    }
}
