//! Pool liquidity indicator — per-pool depth and health context cards.
//!
//! Shows per-pool cards with: DEX name + fee badge, relative depth bar
//! (comparing pool sizes), TVL/spot price stats, price impact at current
//! amount (green/yellow/red by threshold), and allocation fraction.

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// Information about a single liquidity pool.
pub struct PoolInfo {
    /// DEX label (e.g. "Splash", "CSWAP").
    pub dex_label: String,
    /// Color for this pool.
    pub color: Color32,
    /// ADA reserves in this pool (lovelace).
    pub ada_reserves: u64,
    /// Token reserves in this pool.
    pub token_reserves: u64,
    /// Pool fee in basis points (e.g. 30 = 0.3%).
    pub fee_bps: u32,
    /// Spot price (ADA per token).
    pub spot_price: f64,
    /// Estimated price impact at the current swap amount (0.0..1.0).
    pub price_impact: f64,
    /// Fraction of the split allocated to this pool (0.0..=1.0).
    pub allocation_fraction: f32,
}

/// Configuration for the pool liquidity indicator.
pub struct PoolLiquidityConfig {
    /// Price impact threshold for warning (yellow). Default: 0.01 (1%).
    pub impact_warn_threshold: f64,
    /// Price impact threshold for danger (red). Default: 0.03 (3%).
    pub impact_danger_threshold: f64,
    /// Height of the depth bar in pixels.
    pub depth_bar_height: f32,
    /// Font size for labels.
    pub font_size: f32,
}

impl Default for PoolLiquidityConfig {
    fn default() -> Self {
        Self {
            impact_warn_threshold: 0.01,
            impact_danger_threshold: 0.03,
            depth_bar_height: 8.0,
            font_size: 11.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the pool liquidity indicator for all pools.
pub fn show(ui: &mut Ui, pools: &[PoolInfo], config: &PoolLiquidityConfig) {
    if pools.is_empty() {
        return;
    }

    // Find the max ADA reserves across all pools for relative depth bars
    let max_reserves = pools.iter().map(|p| p.ada_reserves).max().unwrap_or(1);

    for (i, pool) in pools.iter().enumerate() {
        if i > 0 {
            ui.add_space(4.0);
        }

        egui::Frame::new()
            .fill(theme::BG_SECONDARY)
            .corner_radius(6.0)
            .inner_margin(10.0)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                draw_pool_card(ui, pool, max_reserves, config);
            });
    }
}

/// Draw a single pool card.
fn draw_pool_card(ui: &mut Ui, pool: &PoolInfo, max_reserves: u64, config: &PoolLiquidityConfig) {
    // Header row: DEX name + fee badge + allocation
    ui.horizontal(|ui| {
        // Colored dot
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        if ui.is_rect_visible(dot_rect) {
            ui.painter()
                .circle_filled(dot_rect.center(), 4.0, pool.color);
        }

        // DEX label
        ui.label(
            RichText::new(&pool.dex_label)
                .color(theme::TEXT_PRIMARY)
                .strong()
                .size(config.font_size),
        );

        // Fee badge
        let fee_pct = pool.fee_bps as f64 / 100.0;
        let fee_text = format!("{fee_pct:.1}%");
        ui.label(
            RichText::new(fee_text)
                .color(theme::TEXT_MUTED)
                .size(config.font_size - 1.0),
        );

        // Right-aligned: allocation fraction
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if pool.allocation_fraction > 0.0 {
                let alloc_pct = (pool.allocation_fraction * 100.0).round() as u32;
                ui.label(
                    RichText::new(format!("{alloc_pct}% allocated"))
                        .color(pool.color)
                        .size(config.font_size),
                );
            }
        });
    });

    ui.add_space(4.0);

    // Relative depth bar
    let depth_fraction = if max_reserves > 0 {
        pool.ada_reserves as f32 / max_reserves as f32
    } else {
        0.0
    };

    let (bar_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), config.depth_bar_height),
        Sense::hover(),
    );

    if ui.is_rect_visible(bar_rect) {
        let painter = ui.painter();
        let rounding = CornerRadius::same(2);

        // Track
        painter.rect_filled(bar_rect, rounding, theme::BG_HIGHLIGHT);

        // Fill
        if depth_fraction > 0.0 {
            let fill_width = bar_rect.width() * depth_fraction;
            let fill_rect =
                Rect::from_min_size(bar_rect.min, Vec2::new(fill_width, bar_rect.height()));
            painter.rect_filled(
                fill_rect,
                CornerRadius {
                    nw: 2,
                    sw: 2,
                    ne: if depth_fraction > 0.98 { 2 } else { 0 },
                    se: if depth_fraction > 0.98 { 2 } else { 0 },
                },
                pool.color.gamma_multiply(0.6),
            );
        }
    }

    ui.add_space(4.0);

    // Stats row: TVL + spot price
    ui.horizontal(|ui| {
        let tvl_ada = pool.ada_reserves as f64 / 1_000_000.0;
        ui.label(
            RichText::new(format!("TVL: {}", format_ada_compact(tvl_ada)))
                .color(theme::TEXT_SECONDARY)
                .size(config.font_size),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("Spot: {:.6} ADA", pool.spot_price))
                    .color(theme::TEXT_SECONDARY)
                    .size(config.font_size),
            );
        });
    });

    // Price impact row
    let impact_color = if pool.price_impact >= config.impact_danger_threshold {
        theme::ACCENT_RED
    } else if pool.price_impact >= config.impact_warn_threshold {
        theme::ACCENT_YELLOW
    } else {
        theme::ACCENT_GREEN
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Price impact")
                .color(theme::TEXT_MUTED)
                .size(config.font_size),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{:.2}%", pool.price_impact * 100.0))
                    .color(impact_color)
                    .size(config.font_size),
            );
        });
    });
}

// ============================================================================
// Helpers
// ============================================================================

/// Format ADA amount compactly (e.g. "1.2M ADA", "450K ADA").
fn format_ada_compact(ada: f64) -> String {
    if ada >= 1_000_000.0 {
        format!("{:.1}M ADA", ada / 1_000_000.0)
    } else if ada >= 1_000.0 {
        format!("{:.0}K ADA", ada / 1_000.0)
    } else {
        format!("{ada:.0} ADA")
    }
}
