//! Step-based UTxO shelf viewer for optimization visualization.
//!
//! Manages an optimization plan and renders the wallet state at each step
//! as a static shelf. No animation — just step navigation.

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use crate::theme;
use crate::utxo_map::policy_color;
use crate::utxo_shelf::{classify_utxos, ShelfConfig, ShelfData, ShelfTier, ShelfUtxo};
use utxo_optimizer::plan::OptimizationPlan;

/// Protocol parameter for shelf classification.
const COINS_PER_UTXO_BYTE: u64 = 4310;

// ============================================================================
// Public types
// ============================================================================

/// Computed block position from shelf layout.
#[derive(Clone, Debug)]
pub struct BlockLayout {
    pub utxo_ref: String,
    pub rect: Rect,
    pub tier: ShelfTier,
    pub policies: Vec<(String, u64)>,
    pub lovelace: u64,
}

/// Persistent state for the optimizer step viewer.
#[derive(Default)]
pub struct ShelfStepViewer {
    /// The optimization plan being visualized.
    plan: Option<OptimizationPlan>,
    /// Which step we're currently viewing.
    current_step: usize,
    /// Cached shelf data for the current step's resulting state.
    cached_data: Option<ShelfData>,
    /// Step index the cache was built for.
    cached_step: Option<usize>,
}

impl ShelfStepViewer {
    /// Set a new optimization plan. Shows the final (end) state by default.
    pub fn set_plan(&mut self, plan: OptimizationPlan) {
        let last_step = plan.steps.len().saturating_sub(1);
        self.plan = Some(plan);
        self.current_step = last_step;
        self.cached_data = None;
        self.cached_step = None;
    }

    /// Current step index.
    pub fn current_step(&self) -> usize {
        self.current_step
    }

    /// Total number of steps in the plan.
    pub fn num_steps(&self) -> usize {
        self.plan.as_ref().map(|p| p.steps.len()).unwrap_or(0)
    }

    /// Access the current plan (if set).
    pub fn plan(&self) -> Option<&OptimizationPlan> {
        self.plan.as_ref()
    }

    /// Go to a specific step.
    pub fn go_to_step(&mut self, step: usize) {
        let max = self.num_steps();
        let new_step = step.min(max.saturating_sub(1));
        if new_step != self.current_step {
            self.current_step = new_step;
            self.cached_data = None;
            self.cached_step = None;
        }
    }

    /// Advance to the next step.
    pub fn next_step(&mut self) {
        if self.current_step + 1 < self.num_steps() {
            self.current_step += 1;
            self.cached_data = None;
            self.cached_step = None;
        }
    }

    /// Go to previous step.
    pub fn prev_step(&mut self) {
        if self.current_step > 0 {
            self.current_step -= 1;
            self.cached_data = None;
            self.cached_step = None;
        }
    }

    /// Render the shelf for the current step.
    ///
    /// `initial_utxos` are the raw UTxOs before any optimization
    /// (used for the "before" state at step 0).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        config: &ShelfConfig,
        initial_utxos: &[cardano_assets::utxo::UtxoApi],
    ) -> f32 {
        let data = self.get_shelf_data(initial_utxos);
        render_shelf(ui, config, &data)
    }

    /// Get or compute the shelf data for the current step.
    fn get_shelf_data(&mut self, initial_utxos: &[cardano_assets::utxo::UtxoApi]) -> ShelfData {
        // Return cached data if valid
        if let Some(ref data) = self.cached_data {
            if self.cached_step == Some(self.current_step) {
                return data.clone();
            }
        }

        let data = match &self.plan {
            Some(plan) if !plan.steps.is_empty() && self.current_step < plan.steps.len() => {
                let utxos: Vec<cardano_assets::utxo::UtxoApi> = plan.steps[self.current_step]
                    .resulting_utxos
                    .iter()
                    .map(snapshot_to_utxo_api)
                    .collect();
                classify_utxos(&utxos, COINS_PER_UTXO_BYTE)
            }
            _ => classify_utxos(initial_utxos, COINS_PER_UTXO_BYTE),
        };

        self.cached_data = Some(data.clone());
        self.cached_step = Some(self.current_step);
        data
    }
}

// ============================================================================
// Layout computation
// ============================================================================

/// Compute block layout positions for a ShelfData without rendering.
pub fn compute_layout(config: &ShelfConfig, data: &ShelfData) -> Vec<BlockLayout> {
    let mut layouts = Vec::new();

    let mut by_tier: HashMap<ShelfTier, Vec<&ShelfUtxo>> = HashMap::new();
    for utxo in &data.utxos {
        by_tier.entry(utxo.tier).or_default().push(utxo);
    }

    let max_lovelace = data.utxos.iter().map(|u| u.lovelace).max().unwrap_or(1);
    let blocks_width = (config.width - config.label_width - config.block_gap).max(100.0);

    let mut y = 0.0f32;

    for &tier in ShelfTier::all() {
        let utxos = by_tier.get(&tier).map(|v| v.as_slice()).unwrap_or(&[]);
        if utxos.is_empty() {
            let bh = config.block_height(tier);
            y += bh + config.shelf_padding * 2.0;
            continue;
        }

        let bh = config.block_height(tier);
        let bg = config.block_gap(tier);
        let right_edge = config.label_width + bg + blocks_width;

        let mut row = 0usize;
        let mut x = config.label_width + bg;
        let blocks_top = y + config.shelf_padding;

        for utxo in utxos {
            let has_assets = !utxo.policies.is_empty();
            let w = config.block_width(utxo.lovelace, max_lovelace, blocks_width, has_assets, tier);

            if x + w > right_edge && x > config.label_width + bg {
                row += 1;
                x = config.label_width + bg;
            }

            let block_y = blocks_top + row as f32 * (bh + bg);
            let block_rect = Rect::from_min_size(Pos2::new(x, block_y), Vec2::new(w, bh));

            layouts.push(BlockLayout {
                utxo_ref: utxo.utxo_ref.clone(),
                rect: block_rect,
                tier: utxo.tier,
                policies: utxo.policies.clone(),
                lovelace: utxo.lovelace,
            });

            x += w + bg;
        }

        let total_rows = row + 1;
        let tier_height = total_rows as f32 * (bh + bg) + config.shelf_padding * 2.0 - bg;
        let single_row_height = bh + config.shelf_padding * 2.0;
        y += tier_height.max(single_row_height);
    }

    layouts
}

// ============================================================================
// Static shelf rendering
// ============================================================================

/// Render a shelf from ShelfData.
fn render_shelf(ui: &mut Ui, config: &ShelfConfig, data: &ShelfData) -> f32 {
    let layouts = compute_layout(config, data);

    let max_y = layouts
        .iter()
        .map(|bl| bl.rect.max.y)
        .fold(0.0f32, f32::max);
    let total_height = max_y + config.shelf_padding;

    let (rect, _response) =
        ui.allocate_exact_size(Vec2::new(config.width, total_height), egui::Sense::hover());

    if !ui.is_rect_visible(rect) {
        return total_height;
    }

    let painter = ui.painter_at(rect);
    let offset = rect.left_top().to_vec2();

    // Background
    painter.rect_filled(rect, 4.0, Color32::from_rgba_premultiplied(15, 15, 25, 200));

    // Tier backgrounds + labels
    let mut y = 0.0f32;
    for &tier in ShelfTier::all() {
        let tier_blocks: Vec<&BlockLayout> = layouts.iter().filter(|bl| bl.tier == tier).collect();

        let bh = config.block_height(tier);
        let bg = config.block_gap(tier);
        let single_row_height = bh + config.shelf_padding * 2.0;

        let total_rows = if tier_blocks.is_empty() {
            1
        } else {
            tier_blocks
                .iter()
                .map(|bl| ((bl.rect.top() - y - config.shelf_padding) / (bh + bg)) as usize)
                .max()
                .unwrap_or(0)
                + 1
        };

        let tier_height = (total_rows as f32 * (bh + bg) + config.shelf_padding * 2.0 - bg)
            .max(single_row_height);

        let shelf_rect = Rect::from_min_size(
            Pos2::new(0.0, y) + offset,
            Vec2::new(config.width, tier_height),
        );

        painter.rect_filled(shelf_rect, 2.0, tier.bg_tint());

        painter.text(
            Pos2::new(8.0, y + tier_height / 2.0) + offset,
            egui::Align2::LEFT_CENTER,
            tier.label(),
            egui::FontId::proportional(11.0),
            tier.color(),
        );

        let count = tier_blocks.len();
        painter.text(
            Pos2::new(config.label_width - 8.0, y + tier_height / 2.0) + offset,
            egui::Align2::RIGHT_CENTER,
            format!("{count}"),
            egui::FontId::proportional(10.0),
            theme::TEXT_MUTED,
        );

        painter.line_segment(
            [
                Pos2::new(0.0, y + tier_height) + offset,
                Pos2::new(config.width, y + tier_height) + offset,
            ],
            Stroke::new(0.5, Color32::from_rgba_premultiplied(80, 80, 100, 40)),
        );

        y += tier_height;
    }

    // Render blocks
    for bl in &layouts {
        let block_rect = bl.rect.translate(offset);

        let bg = if bl.policies.is_empty() {
            let c = bl.tier.color();
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 120)
        } else {
            Color32::from_rgba_premultiplied(30, 30, 45, 200)
        };
        painter.rect_filled(block_rect, 3.0, bg);

        // Policy segments
        if !bl.policies.is_empty() {
            let total_assets: u64 = bl.policies.iter().map(|(_, c)| *c).sum();
            let mut seg_y = block_rect.top();

            for (pid, count) in &bl.policies {
                let seg_h = (*count as f32 / total_assets as f32) * block_rect.height();
                let seg_rect = Rect::from_min_size(
                    Pos2::new(block_rect.left() + 1.0, seg_y),
                    Vec2::new(block_rect.width() - 2.0, seg_h.max(2.0)),
                );
                painter.rect_filled(seg_rect, 1.5, policy_color(pid));
                seg_y += seg_h;
            }
        }

        // ADA label
        if block_rect.width() > 36.0 {
            let ada = bl.lovelace as f64 / 1_000_000.0;
            let label = if ada >= 100.0 {
                format!("{ada:.0}")
            } else if ada >= 10.0 {
                format!("{ada:.1}")
            } else {
                format!("{ada:.2}")
            };
            painter.text(
                block_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(9.0),
                Color32::from_rgba_unmultiplied(220, 220, 235, 220),
            );
        }
    }

    total_height
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a UtxoSnapshot to a UtxoApi for re-classification.
fn snapshot_to_utxo_api(
    snap: &utxo_optimizer::plan::UtxoSnapshot,
) -> cardano_assets::utxo::UtxoApi {
    let (tx_hash, output_index) = if let Some((h, i)) = snap.utxo_ref.split_once('#') {
        (h.to_string(), i.parse().unwrap_or(0))
    } else {
        (snap.utxo_ref.clone(), 0)
    };

    cardano_assets::utxo::UtxoApi {
        tx_hash,
        output_index,
        lovelace: snap.lovelace,
        assets: snap.assets.clone(),
        tags: snap.tags.clone(),
    }
}
