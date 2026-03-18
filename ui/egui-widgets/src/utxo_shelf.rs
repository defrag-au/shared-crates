//! UTxO Shelf — wallet health visualization.
//!
//! Each UTxO is classified into a health tier and rendered as a discrete block
//! on the corresponding shelf. Shelves are ordered top-to-bottom from healthiest
//! (Collateral, Liquid) to most problematic (Bloated, Dust).
//!
//! The metaphor: a shelving unit where neatly organized wallets have items on
//! upper shelves, and fragmented wallets have everything piled at the bottom.

use std::collections::{HashMap, HashSet};

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;
use crate::utxo_map::policy_color;

// ============================================================================
// Public types
// ============================================================================

/// Which shelf tier a UTxO belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShelfTier {
    /// Pure ADA >= 5 ADA. DApp-ready collateral for Plutus script interactions.
    Collateral,
    /// Has datum or script reference — locked by a smart contract.
    ScriptLocked,
    /// Pure ADA (no native assets). Freely spendable, low fees.
    Liquid,
    /// Assets from exactly 1 policy. Minimal locked ADA.
    Clean,
    /// Assets from 2-3 policies. More ADA locked than necessary.
    Cluttered,
    /// Assets from 4+ policies. High locked ADA, bloats TX size.
    Bloated,
    /// Near min-UTxO threshold with assets. Costs more to spend than it holds.
    Dust,
}

impl ShelfTier {
    /// All tiers in display order (top shelf to bottom).
    pub fn all() -> &'static [Self] {
        &[
            Self::Collateral,
            Self::ScriptLocked,
            Self::Liquid,
            Self::Clean,
            Self::Cluttered,
            Self::Bloated,
            Self::Dust,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Collateral => "Collateral",
            Self::ScriptLocked => "Script",
            Self::Liquid => "Liquid",
            Self::Clean => "Clean",
            Self::Cluttered => "Cluttered",
            Self::Bloated => "Bloated",
            Self::Dust => "Dust",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Collateral => "Pure ADA 5-15. Reserved for DApp interactions.",
            Self::ScriptLocked => "Locked by smart contract. Requires redeemer to spend.",
            Self::Liquid => "Pure ADA. Freely spendable, minimal fees.",
            Self::Clean => "Single policy. Minimal locked ADA.",
            Self::Cluttered => "2-3 policies. Excess ADA locked.",
            Self::Bloated => "4+ policies. High locked ADA, consolidation target.",
            Self::Dust => "Near min-UTxO. Costs more to spend than it holds.",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Collateral => theme::ACCENT_GREEN,
            Self::ScriptLocked => theme::ACCENT_ORANGE,
            Self::Liquid => theme::ACCENT_BLUE,
            Self::Clean => theme::ACCENT_CYAN,
            Self::Cluttered => theme::ACCENT_YELLOW,
            Self::Bloated => theme::ACCENT_RED,
            Self::Dust => theme::TEXT_MUTED,
        }
    }

    /// Subtle background tint for the shelf row.
    pub fn bg_tint(&self) -> Color32 {
        let c = self.color();
        Color32::from_rgba_premultiplied(c.r() / 8, c.g() / 8, c.b() / 8, 20)
    }
}

/// A UTxO classified for shelf display.
#[derive(Clone, Debug)]
pub struct ShelfUtxo {
    /// UTxO reference, e.g. `"tx_hash#index"`.
    pub utxo_ref: String,
    /// Which shelf this UTxO belongs to.
    pub tier: ShelfTier,
    /// Total lovelace in this UTxO.
    pub lovelace: u64,
    /// Policy segments: `(policy_id, asset_count)`. Empty for pure-ADA UTxOs.
    /// Used for rendering shelf block segments (only needs policy + count).
    pub policies: Vec<(String, u64)>,
    /// Full asset list from the original UTxO. Empty for pure-ADA UTxOs.
    /// Used for detail panel display when a UTxO is selected.
    pub assets: Vec<cardano_assets::utxo::AssetQuantity>,
    /// Tags from the decoded UTxO (datum, script ref, script address, etc.)
    pub tags: Vec<cardano_assets::utxo::UtxoTag>,
}

impl ShelfUtxo {
    /// Check if this UTxO has a specific tag.
    pub fn has_tag(&self, tag: cardano_assets::utxo::UtxoTag) -> bool {
        self.tags.contains(&tag)
    }

    /// Whether this UTxO is script-locked (datum, script ref, or script address).
    pub fn is_script_locked(&self) -> bool {
        use cardano_assets::utxo::UtxoTag;
        self.has_tag(UtxoTag::HasDatum)
            || self.has_tag(UtxoTag::HasScriptRef)
            || self.has_tag(UtxoTag::ScriptAddress)
    }
}

/// Classified wallet data for the shelf widget.
#[derive(Clone)]
pub struct ShelfData {
    /// All UTxOs, classified by tier.
    pub utxos: Vec<ShelfUtxo>,
    /// Total lovelace across all UTxOs.
    pub total_lovelace: u64,
    /// Whether at least one collateral-ready UTxO exists.
    pub has_collateral: bool,
    /// Spendable ADA: sum of (lovelace - min_utxo) for all non-script-locked UTxOs.
    /// Includes excess ADA from asset-bearing UTxOs.
    pub spendable_lovelace: u64,
}

/// Estimate min-UTxO lovelace for an output based on its asset count and policy count.
///
/// Uses the Babbage/Conway formula: `coinsPerUTxOByte * (160 + output_size_estimate)`.
fn estimate_min_lovelace(coins_per_utxo_byte: u64, num_assets: usize, num_policies: usize) -> u64 {
    const FIXED_OVERHEAD: u64 = 160; // base output size in bytes

    if num_assets == 0 {
        // Pure ADA output: ~27 bytes address + 8 bytes value
        return coins_per_utxo_byte * FIXED_OVERHEAD;
    }

    // Each policy adds ~28 bytes (policy hash), each asset adds ~12 bytes (name + quantity)
    let policy_bytes = num_policies as u64 * 28;
    let asset_bytes = num_assets as u64 * 12;
    coins_per_utxo_byte * (FIXED_OVERHEAD + policy_bytes + asset_bytes)
}

/// Immutable per-frame configuration.
pub struct ShelfConfig {
    /// Total widget width. Height is computed from content.
    pub width: f32,
    /// Height of each UTxO block on a shelf.
    pub block_height: f32,
    /// Vertical padding between shelves.
    pub shelf_padding: f32,
    /// Horizontal gap between blocks on a shelf.
    pub block_gap: f32,
    /// Width reserved for shelf labels on the left.
    pub label_width: f32,
    /// Show ADA amounts on blocks.
    pub show_labels: bool,
    /// Render empty shelf tiers.
    pub show_empty_shelves: bool,
}

impl Default for ShelfConfig {
    fn default() -> Self {
        Self {
            width: 600.0,
            block_height: 40.0,
            shelf_padding: 6.0,
            block_gap: 4.0,
            label_width: 90.0,
            show_labels: true,
            show_empty_shelves: true,
        }
    }
}

/// Persisted state across frames.
#[derive(Default)]
pub struct ShelfState {
    /// Which UTxO is currently hovered.
    pub hovered_utxo: Option<String>,
    /// Which policy is currently hovered (for cross-shelf highlighting).
    pub hovered_policy: Option<String>,
    /// Which UTxO is selected (click to toggle).
    pub selected_utxo: Option<String>,
    /// Which tiers have been manually expanded (show all rows instead of collapsed).
    pub expanded_tiers: HashSet<ShelfTier>,
}

/// Actions the widget can emit.
#[derive(Debug, Clone)]
pub enum ShelfAction {
    HoveredUtxo(String),
    HoveredPolicy(String),
    SelectedUtxo(String),
    Deselected,
}

/// Widget response.
pub struct ShelfResponse {
    pub response: Response,
    pub action: Option<ShelfAction>,
}

// ============================================================================
// Classification
// ============================================================================

const COLLATERAL_THRESHOLD: u64 = 5_000_000;
/// Upper bound for collateral — above this, ADA is more useful as Liquid.
const COLLATERAL_CEILING: u64 = 15_000_000;
const DUST_THRESHOLD: u64 = 1_500_000;
/// Max collateral UTxOs to keep — enough for concurrent DApp interactions.
const MAX_COLLATERAL: usize = 3;

/// Classify raw CIP-30 UTxOs into shelf tiers.
///
/// `coins_per_utxo_byte` is the Cardano protocol parameter (`coinsPerUTxOByte`,
/// mainnet = 4310) used to estimate min-UTxO requirements for spendable ADA calculation.
pub fn classify_utxos(
    utxos: &[cardano_assets::utxo::UtxoApi],
    coins_per_utxo_byte: u64,
) -> ShelfData {
    let mut shelf_utxos = Vec::with_capacity(utxos.len());
    let mut total_lovelace: u64 = 0;

    // Pass 1: provisional classification
    for utxo in utxos {
        let utxo_ref = format!("{}#{}", utxo.tx_hash, utxo.output_index);
        total_lovelace += utxo.lovelace;

        // Group assets by policy
        let mut by_policy: HashMap<&str, u64> = HashMap::new();
        for aq in &utxo.assets {
            *by_policy.entry(aq.asset_id.policy_id.as_str()).or_default() += 1;
        }

        let is_script = utxo.has_tag(cardano_assets::utxo::UtxoTag::HasDatum)
            || utxo.has_tag(cardano_assets::utxo::UtxoTag::HasScriptRef)
            || utxo.has_tag(cardano_assets::utxo::UtxoTag::ScriptAddress);

        let tier = if is_script {
            // Script-locked UTxOs (DEX orders, marketplace listings, staking) — not freely spendable
            ShelfTier::ScriptLocked
        } else if utxo.assets.is_empty() {
            if utxo.lovelace >= COLLATERAL_THRESHOLD && utxo.lovelace <= COLLATERAL_CEILING {
                ShelfTier::Collateral
            } else {
                ShelfTier::Liquid
            }
        } else if utxo.lovelace <= DUST_THRESHOLD {
            ShelfTier::Dust
        } else {
            match by_policy.len() {
                1 => ShelfTier::Clean,
                2 | 3 => ShelfTier::Cluttered,
                _ => ShelfTier::Bloated,
            }
        };

        let policies: Vec<(String, u64)> = by_policy
            .into_iter()
            .map(|(pid, count)| (pid.to_string(), count))
            .collect();

        shelf_utxos.push(ShelfUtxo {
            utxo_ref,
            tier,
            lovelace: utxo.lovelace,
            policies,
            assets: utxo.assets.clone(),
            tags: utxo.tags.clone(),
        });
    }

    // Pass 2: cap Collateral at MAX_COLLATERAL, preferring smaller (collateral-sized) UTxOs.
    // Large pure-ADA UTxOs are more useful as Liquid than locked as collateral.
    let mut collateral_indices: Vec<usize> = shelf_utxos
        .iter()
        .enumerate()
        .filter(|(_, u)| u.tier == ShelfTier::Collateral)
        .map(|(i, _)| i)
        .collect();

    if collateral_indices.len() > MAX_COLLATERAL {
        // Sort by lovelace ascending — keep smallest as collateral
        collateral_indices.sort_by_key(|&i| shelf_utxos[i].lovelace);
        // Reclassify excess (the larger ones) as Liquid
        for &i in &collateral_indices[MAX_COLLATERAL..] {
            shelf_utxos[i].tier = ShelfTier::Liquid;
        }
    }

    let has_collateral = shelf_utxos.iter().any(|u| u.tier == ShelfTier::Collateral);

    // Compute spendable ADA: for each non-locked UTxO, excess above min-UTxO is spendable.
    let spendable_lovelace: u64 = shelf_utxos
        .iter()
        .filter(|u| u.tier != ShelfTier::ScriptLocked && u.tier != ShelfTier::Collateral)
        .map(|u| {
            let num_assets: usize = u.policies.iter().map(|(_, c)| *c as usize).sum();
            let num_policies = u.policies.len();
            let min = estimate_min_lovelace(coins_per_utxo_byte, num_assets, num_policies);
            u.lovelace.saturating_sub(min)
        })
        .sum();

    // Sort within each tier: group by primary policy, then largest ADA first.
    // This clusters same-policy UTxOs together (especially visible in Dust).
    shelf_utxos.sort_by(|a, b| {
        a.tier.cmp(&b.tier).then_with(|| {
            let pa = a.policies.first().map(|(p, _)| p.as_str()).unwrap_or("");
            let pb = b.policies.first().map(|(p, _)| p.as_str()).unwrap_or("");
            pa.cmp(pb).then(b.lovelace.cmp(&a.lovelace))
        })
    });

    ShelfData {
        utxos: shelf_utxos,
        total_lovelace,
        has_collateral,
        spendable_lovelace,
    }
}

// ============================================================================
// Rendering
// ============================================================================

/// Max visible rows per tier before collapsing. Click to expand.
const DEFAULT_MAX_ROWS: usize = 2;

/// Fixed width for pure-ADA blocks (Collateral, Liquid, ScriptLocked without assets).
/// The ADA label communicates the amount; proportional sizing only helps asset blocks.
const PURE_ADA_BLOCK_WIDTH: f32 = 48.0;

/// Compact block size for Dust tier — small squares since all near min-UTxO.
const DUST_BLOCK_SIZE: f32 = 12.0;

/// Gap between Dust blocks (tighter than normal).
const DUST_BLOCK_GAP: f32 = 2.0;

impl ShelfConfig {
    /// Compute the width of a single block.
    /// Dust blocks are compact squares; pure-ADA blocks get fixed width;
    /// asset blocks scale proportionally.
    pub(crate) fn block_width(
        &self,
        lovelace: u64,
        max_lovelace: u64,
        blocks_width: f32,
        has_assets: bool,
        tier: ShelfTier,
    ) -> f32 {
        if tier == ShelfTier::Dust {
            return DUST_BLOCK_SIZE;
        }
        if !has_assets {
            return PURE_ADA_BLOCK_WIDTH;
        }
        let fraction = lovelace as f32 / max_lovelace as f32;
        let min_block_width = 24.0;
        (fraction * blocks_width * 0.6)
            .max(min_block_width)
            .min(blocks_width)
    }

    /// Block height for a given tier.
    pub(crate) fn block_height(&self, tier: ShelfTier) -> f32 {
        if tier == ShelfTier::Dust {
            DUST_BLOCK_SIZE
        } else {
            self.block_height
        }
    }

    /// Gap between blocks for a given tier.
    pub(crate) fn block_gap(&self, tier: ShelfTier) -> f32 {
        if tier == ShelfTier::Dust {
            DUST_BLOCK_GAP
        } else {
            self.block_gap
        }
    }

    /// Pre-compute how many rows each tier needs when blocks wrap.
    /// Returns `(row_count, block_widths)` per tier.
    fn compute_tier_rows(
        &self,
        utxos: &[&ShelfUtxo],
        max_lovelace: u64,
        blocks_width: f32,
        tier: ShelfTier,
    ) -> (usize, Vec<f32>) {
        if utxos.is_empty() {
            return (1, vec![]);
        }

        let gap = self.block_gap(tier);
        let right_edge = self.label_width + gap + blocks_width;
        let mut rows = 1usize;
        let mut x = self.label_width + gap;
        let mut widths = Vec::with_capacity(utxos.len());

        for utxo in utxos {
            let has_assets = !utxo.policies.is_empty();
            let w = self.block_width(utxo.lovelace, max_lovelace, blocks_width, has_assets, tier);
            widths.push(w);

            if x + w > right_edge && x > self.label_width + gap {
                // Wrap to next row
                rows += 1;
                x = self.label_width + gap;
            }
            x += w + gap;
        }

        (rows, widths)
    }

    /// Render the UTxO shelf widget.
    pub fn show(&self, ui: &mut Ui, data: &ShelfData, state: &mut ShelfState) -> ShelfResponse {
        // Group UTxOs by tier
        let mut by_tier: HashMap<ShelfTier, Vec<&ShelfUtxo>> = HashMap::new();
        for utxo in &data.utxos {
            by_tier.entry(utxo.tier).or_default().push(utxo);
        }

        // Compute max ADA across all UTxOs for proportional block widths
        let max_lovelace = data.utxos.iter().map(|u| u.lovelace).max().unwrap_or(1);

        // Available width for blocks (after label column)
        let blocks_width = (self.width - self.label_width - self.block_gap).max(100.0);

        // Visible tiers
        let visible_tiers: Vec<ShelfTier> = ShelfTier::all()
            .iter()
            .copied()
            .filter(|t| self.show_empty_shelves || by_tier.contains_key(t))
            .collect();

        // Pre-compute row counts per tier (with wrapping)
        let mut tier_info: Vec<(ShelfTier, usize, usize, Vec<f32>)> = Vec::new(); // (tier, total_rows, visible_rows, widths)
        let mut total_height = 0.0f32;

        for &tier in &visible_tiers {
            let utxos = by_tier.get(&tier).map(|v| v.as_slice()).unwrap_or(&[]);
            let (total_rows, widths) =
                self.compute_tier_rows(utxos, max_lovelace, blocks_width, tier);
            let bh = self.block_height(tier);
            let bg = self.block_gap(tier);
            let single_row_height = bh + self.shelf_padding * 2.0;

            // Dust shows all rows (compact); other tiers collapse after DEFAULT_MAX_ROWS
            let is_expanded = state.expanded_tiers.contains(&tier);
            let visible_rows =
                if tier == ShelfTier::Dust || is_expanded || total_rows <= DEFAULT_MAX_ROWS {
                    total_rows
                } else {
                    DEFAULT_MAX_ROWS
                };
            let tier_height = visible_rows as f32 * (bh + bg) + self.shelf_padding * 2.0 - bg; // no trailing gap
                                                                                               // Add a small extra for the "+N more" indicator when collapsed
            let tier_height =
                if !is_expanded && tier != ShelfTier::Dust && total_rows > DEFAULT_MAX_ROWS {
                    tier_height + 16.0 // space for the "+N more" label
                } else {
                    tier_height
                };
            total_height += tier_height.max(single_row_height);
            tier_info.push((tier, total_rows, visible_rows, widths));
        }

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(self.width, total_height), Sense::click_and_drag());

        if !ui.is_rect_visible(rect) {
            return ShelfResponse {
                response,
                action: None,
            };
        }

        let painter = ui.painter_at(rect);
        let mouse_pos = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| rect.contains(*p));

        let mut action: Option<ShelfAction> = None;
        let mut new_hovered_utxo: Option<String> = None;
        let mut new_hovered_policy: Option<String> = None;

        // First pass: compute all block rects with wrapping
        struct BlockRect {
            rect: Rect,
            utxo_ref: String,
            policies: Vec<(String, u64)>,
            lovelace: u64,
            tier: ShelfTier,
            visible: bool, // false if in collapsed rows
        }
        let mut all_blocks: Vec<BlockRect> = Vec::new();

        // Also track per-tier shelf rects and "+N more" hit areas
        struct TierLayout {
            tier: ShelfTier,
            shelf_rect: Rect,
            total_rows: usize,
            visible_rows: usize,
            utxo_count: usize,
            /// The "+N more" label rect (for click-to-expand)
            more_label_rect: Option<Rect>,
        }
        let mut tier_layouts: Vec<TierLayout> = Vec::new();

        let mut y = rect.top();

        for (tier, total_rows, visible_rows, ref widths) in &tier_info {
            let tier = *tier;
            let total_rows = *total_rows;
            let visible_rows = *visible_rows;
            let is_expanded = state.expanded_tiers.contains(&tier);
            let bh = self.block_height(tier);
            let bg = self.block_gap(tier);
            let right_edge = rect.right() - bg;

            let utxos = by_tier.get(&tier).map(|v| v.as_slice()).unwrap_or(&[]);
            let single_row_height = bh + self.shelf_padding * 2.0;
            let tier_height_rows = visible_rows as f32 * (bh + bg) + self.shelf_padding * 2.0 - bg;
            let has_more = !is_expanded && tier != ShelfTier::Dust && total_rows > DEFAULT_MAX_ROWS;
            let tier_height = if has_more {
                tier_height_rows + 16.0
            } else {
                tier_height_rows
            }
            .max(single_row_height);

            let shelf_rect = Rect::from_min_size(
                Pos2::new(rect.left(), y),
                Vec2::new(self.width, tier_height),
            );

            // Lay out blocks with wrapping
            let mut row = 0usize;
            let mut x = rect.left() + self.label_width + bg;
            let blocks_top = y + self.shelf_padding;

            for (i, utxo) in utxos.iter().enumerate() {
                let w = widths.get(i).copied().unwrap_or(24.0);

                // Wrap check
                if x + w > right_edge && x > rect.left() + self.label_width + bg {
                    row += 1;
                    x = rect.left() + self.label_width + bg;
                }

                let visible = row < visible_rows;
                let block_y = blocks_top + row as f32 * (bh + bg);

                let block_rect = Rect::from_min_size(Pos2::new(x, block_y), Vec2::new(w, bh));

                all_blocks.push(BlockRect {
                    rect: block_rect,
                    utxo_ref: utxo.utxo_ref.clone(),
                    policies: utxo.policies.clone(),
                    lovelace: utxo.lovelace,
                    tier,
                    visible,
                });

                x += w + bg;
            }

            // "+N more" label rect (not used for Dust — always fully shown)
            let more_label_rect = if has_more {
                let hidden_count = utxos.len()
                    - all_blocks
                        .iter()
                        .filter(|b| b.tier == tier && b.visible)
                        .count();
                if hidden_count > 0 {
                    let label_y = y + tier_height - 14.0;
                    Some(Rect::from_min_size(
                        Pos2::new(rect.left() + self.label_width + bg, label_y),
                        Vec2::new(120.0, 14.0),
                    ))
                } else {
                    None
                }
            } else {
                None
            };

            tier_layouts.push(TierLayout {
                tier,
                shelf_rect,
                total_rows,
                visible_rows,
                utxo_count: utxos.len(),
                more_label_rect,
            });

            y += tier_height;
        }

        // Detect hover (only on visible blocks).
        // When the pointer is within the widget but between blocks (in gaps),
        // keep the previous hover state to avoid flicker from rapid dim/undim toggles.
        if let Some(mp) = mouse_pos {
            for block in &all_blocks {
                if block.visible && block.rect.contains(mp) {
                    new_hovered_utxo = Some(block.utxo_ref.clone());
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);

                    if !block.policies.is_empty() {
                        let total_assets: u64 = block.policies.iter().map(|(_, c)| *c).sum();
                        let mut seg_y = block.rect.top();
                        for (pid, count) in &block.policies {
                            let seg_h = (*count as f32 / total_assets as f32) * block.rect.height();
                            if mp.y >= seg_y && mp.y < seg_y + seg_h {
                                new_hovered_policy = Some(pid.clone());
                                break;
                            }
                            seg_y += seg_h;
                        }
                    }
                    break;
                }
            }
            // Pointer is within widget but not on a block — keep previous hover (sticky)
            if new_hovered_utxo.is_none() {
                new_hovered_utxo = state.hovered_utxo.clone();
                new_hovered_policy = state.hovered_policy.clone();
            }
        }
        // Pointer outside widget → clear hover (mouse_pos is None)

        state.hovered_utxo = new_hovered_utxo;
        state.hovered_policy = new_hovered_policy.clone();

        // Second pass: render shelf backgrounds, labels, dividers
        for layout in &tier_layouts {
            let tier = layout.tier;
            let shelf_rect = layout.shelf_rect;
            let shelf_y = shelf_rect.top();
            let shelf_h = shelf_rect.height();

            // Shelf background
            painter.rect_filled(shelf_rect, 2.0, tier.bg_tint());

            // Shelf label (vertically centered in shelf)
            let label_pos = Pos2::new(rect.left() + 8.0, shelf_y + shelf_h / 2.0);
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                tier.label(),
                egui::FontId::proportional(11.0),
                tier.color(),
            );

            // UTxO count badge
            let count_pos = Pos2::new(
                rect.left() + self.label_width - 8.0,
                shelf_y + shelf_h / 2.0,
            );
            painter.text(
                count_pos,
                egui::Align2::RIGHT_CENTER,
                format!("{}", layout.utxo_count),
                egui::FontId::proportional(10.0),
                theme::TEXT_MUTED,
            );

            // Empty shelf indicator
            if layout.utxo_count == 0 {
                let empty_pos = Pos2::new(
                    rect.left() + self.label_width + self.block_gap + 8.0,
                    shelf_y + shelf_h / 2.0,
                );
                let (empty_text, empty_color) = if tier == ShelfTier::Collateral {
                    ("No collateral UTxO", theme::ACCENT_RED)
                } else {
                    ("empty", Color32::from_rgba_premultiplied(80, 80, 100, 60))
                };
                painter.text(
                    empty_pos,
                    egui::Align2::LEFT_CENTER,
                    empty_text,
                    egui::FontId::proportional(10.0),
                    empty_color,
                );
            }

            // "+N more" indicator
            if let Some(more_rect) = layout.more_label_rect {
                let hidden = layout.utxo_count
                    - all_blocks
                        .iter()
                        .filter(|b| b.tier == tier && b.visible)
                        .count();
                let hidden_rows = layout.total_rows - layout.visible_rows;
                let is_hovered = mouse_pos.is_some_and(|mp| more_rect.contains(mp));
                let label_color = if is_hovered {
                    theme::ACCENT
                } else {
                    theme::TEXT_MUTED
                };
                painter.text(
                    more_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    format!("+{hidden} more ({hidden_rows} rows)"),
                    egui::FontId::proportional(10.0),
                    label_color,
                );
            }

            // Shelf divider line
            painter.line_segment(
                [
                    Pos2::new(rect.left(), shelf_rect.bottom()),
                    Pos2::new(rect.right(), shelf_rect.bottom()),
                ],
                Stroke::new(0.5, Color32::from_rgba_premultiplied(80, 80, 100, 40)),
            );
        }

        // Render blocks (only visible ones)
        for block in &all_blocks {
            if !block.visible {
                continue;
            }

            let is_hovered_utxo = state
                .hovered_utxo
                .as_ref()
                .is_some_and(|h| *h == block.utxo_ref);

            let is_policy_highlighted = new_hovered_policy
                .as_ref()
                .is_some_and(|hp| block.policies.iter().any(|(pid, _)| pid == hp));

            let has_active_highlight = state.hovered_utxo.is_some() || new_hovered_policy.is_some();
            let is_dimmed = has_active_highlight && !is_hovered_utxo && !is_policy_highlighted;

            let dim_alpha = if is_dimmed { 0.3f32 } else { 1.0 };

            // Block background
            let bg = if block.policies.is_empty() {
                let c = block.tier.color();
                Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (120.0 * dim_alpha) as u8)
            } else {
                Color32::from_rgba_premultiplied(30, 30, 45, (200.0 * dim_alpha) as u8)
            };
            painter.rect_filled(block.rect, 3.0, bg);

            // Policy segments (stacked vertically within the block)
            if !block.policies.is_empty() {
                let total_assets: u64 = block.policies.iter().map(|(_, c)| *c).sum();
                let mut seg_y = block.rect.top();

                for (pid, count) in &block.policies {
                    let seg_h = (*count as f32 / total_assets as f32) * block.rect.height();
                    let seg_rect = Rect::from_min_size(
                        Pos2::new(block.rect.left() + 1.0, seg_y),
                        Vec2::new(block.rect.width() - 2.0, seg_h.max(2.0)),
                    );

                    let mut c = policy_color(pid);
                    if is_dimmed {
                        c = Color32::from_rgba_unmultiplied(
                            c.r(),
                            c.g(),
                            c.b(),
                            (c.a() as f32 * dim_alpha) as u8,
                        );
                    }
                    painter.rect_filled(seg_rect, 1.5, c);
                    seg_y += seg_h;
                }
            }

            // Selection + policy-highlight border (no hover border)
            let is_selected = state
                .selected_utxo
                .as_ref()
                .is_some_and(|s| *s == block.utxo_ref);

            let (border_color, border_width) = if is_selected {
                (theme::ACCENT, 2.5)
            } else if is_policy_highlighted && block.tier != ShelfTier::Dust {
                let c = new_hovered_policy
                    .as_ref()
                    .map(|p| policy_color(p))
                    .unwrap_or(theme::ACCENT);
                (c, 1.0)
            } else {
                (Color32::TRANSPARENT, 0.0)
            };
            if border_color != Color32::TRANSPARENT {
                painter.rect_stroke(
                    block.rect,
                    3.0,
                    Stroke::new(border_width, border_color),
                    StrokeKind::Outside,
                );
            }

            // ADA label on block
            if self.show_labels && block.rect.width() > 36.0 {
                let ada = block.lovelace as f64 / 1_000_000.0;
                let label = if ada >= 100.0 {
                    format!("{:.0}", ada)
                } else if ada >= 10.0 {
                    format!("{:.1}", ada)
                } else {
                    format!("{:.2}", ada)
                };
                let text_alpha = if is_dimmed { 100 } else { 220 };
                painter.text(
                    block.rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::monospace(9.0),
                    Color32::from_rgba_unmultiplied(220, 220, 235, text_alpha),
                );
            }
        }

        // Click detection — toggle selection OR expand/collapse tiers
        if response.clicked() {
            let mut handled = false;

            // Check "+N more" clicks first
            if let Some(mp) = mouse_pos {
                for layout in &tier_layouts {
                    if let Some(more_rect) = layout.more_label_rect {
                        if more_rect.contains(mp) {
                            state.expanded_tiers.insert(layout.tier);
                            handled = true;
                            break;
                        }
                    }
                }
            }

            // Then check block selection
            if !handled {
                if let Some(ref hovered_ref) = state.hovered_utxo {
                    if state.selected_utxo.as_ref() == Some(hovered_ref) {
                        state.selected_utxo = None;
                        action = Some(ShelfAction::Deselected);
                    } else {
                        state.selected_utxo = Some(hovered_ref.clone());
                        action = Some(ShelfAction::SelectedUtxo(hovered_ref.clone()));
                    }
                }
            }
        }

        // Hover action (only if no click happened)
        if action.is_none() {
            if let Some(ref hovered_ref) = state.hovered_utxo {
                action = Some(ShelfAction::HoveredUtxo(hovered_ref.clone()));
            } else if let Some(ref hovered_pol) = new_hovered_policy {
                action = Some(ShelfAction::HoveredPolicy(hovered_pol.clone()));
            }
        }

        ShelfResponse { response, action }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::utxo::{AssetQuantity, UtxoApi, UtxoTag};
    use cardano_assets::AssetId;

    /// Mainnet coinsPerUTxOByte for tests.
    const TEST_COINS_PER_UTXO_BYTE: u64 = 4310;

    fn pure_ada_utxo(lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: format!("tx_{lovelace}"),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn asset_utxo(lovelace: u64, policies: &[(&str, u64)]) -> UtxoApi {
        let mut assets = Vec::new();
        for (pid, count) in policies {
            for i in 0..*count {
                assets.push(AssetQuantity {
                    asset_id: AssetId::new_unchecked(pid.to_string(), format!("asset_{i}")),
                    quantity: 1,
                });
            }
        }
        UtxoApi {
            tx_hash: format!("tx_{lovelace}"),
            output_index: 0,
            lovelace,
            assets,
            tags: vec![],
        }
    }

    #[test]
    fn test_classify_collateral() {
        let data = classify_utxos(&[pure_ada_utxo(5_000_000)], TEST_COINS_PER_UTXO_BYTE);
        assert_eq!(data.utxos.len(), 1);
        assert_eq!(data.utxos[0].tier, ShelfTier::Collateral);
        assert!(data.has_collateral);
    }

    #[test]
    fn test_classify_liquid() {
        let data = classify_utxos(&[pure_ada_utxo(2_000_000)], TEST_COINS_PER_UTXO_BYTE);
        assert_eq!(data.utxos[0].tier, ShelfTier::Liquid);
        assert!(!data.has_collateral);
    }

    #[test]
    fn test_classify_clean() {
        let data = classify_utxos(
            &[asset_utxo(3_000_000, &[("policy_a", 5)])],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Clean);
        assert_eq!(data.utxos[0].policies.len(), 1);
    }

    #[test]
    fn test_classify_cluttered() {
        let data = classify_utxos(
            &[asset_utxo(4_000_000, &[("policy_a", 2), ("policy_b", 3)])],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Cluttered);
    }

    #[test]
    fn test_classify_bloated() {
        let data = classify_utxos(
            &[asset_utxo(
                10_000_000,
                &[
                    ("policy_a", 1),
                    ("policy_b", 1),
                    ("policy_c", 1),
                    ("policy_d", 1),
                ],
            )],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Bloated);
    }

    #[test]
    fn test_classify_dust() {
        let data = classify_utxos(
            &[asset_utxo(1_200_000, &[("policy_a", 1)])],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Dust);
    }

    #[test]
    fn test_classify_dust_threshold_boundary() {
        // Exactly at threshold — still dust
        let data = classify_utxos(
            &[asset_utxo(1_500_000, &[("policy_a", 1)])],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Dust);

        // Just above threshold — clean
        let data = classify_utxos(
            &[asset_utxo(1_500_001, &[("policy_a", 1)])],
            TEST_COINS_PER_UTXO_BYTE,
        );
        assert_eq!(data.utxos[0].tier, ShelfTier::Clean);
    }

    #[test]
    fn test_classify_script_locked_datum() {
        let mut utxo = asset_utxo(5_000_000, &[("p1", 2)]);
        utxo.tags.push(UtxoTag::HasDatum);
        let data = classify_utxos(&[utxo], TEST_COINS_PER_UTXO_BYTE);
        assert_eq!(data.utxos[0].tier, ShelfTier::ScriptLocked);
        assert!(data.utxos[0].has_tag(UtxoTag::HasDatum));
    }

    #[test]
    fn test_classify_script_ref_only() {
        let mut utxo = pure_ada_utxo(10_000_000);
        utxo.tags.push(UtxoTag::HasScriptRef);
        let data = classify_utxos(&[utxo], TEST_COINS_PER_UTXO_BYTE);
        assert_eq!(data.utxos[0].tier, ShelfTier::ScriptLocked);
    }

    #[test]
    fn test_classify_script_address() {
        let mut utxo = asset_utxo(3_000_000, &[("p1", 1)]);
        utxo.tags.push(UtxoTag::ScriptAddress);
        let data = classify_utxos(&[utxo], TEST_COINS_PER_UTXO_BYTE);
        // Franken address → ScriptLocked even without datum
        assert_eq!(data.utxos[0].tier, ShelfTier::ScriptLocked);
        assert!(data.utxos[0].has_tag(UtxoTag::ScriptAddress));
    }

    #[test]
    fn test_classify_mixed_wallet() {
        let utxos = vec![
            pure_ada_utxo(10_000_000),                      // Collateral
            pure_ada_utxo(3_000_000),                       // Liquid
            asset_utxo(2_000_000, &[("p1", 3)]),            // Clean
            asset_utxo(4_000_000, &[("p1", 2), ("p2", 1)]), // Cluttered
            asset_utxo(8_000_000, &[("p1", 1), ("p2", 1), ("p3", 1), ("p4", 1)]), // Bloated
            asset_utxo(1_100_000, &[("p1", 1)]),            // Dust
        ];
        let data = classify_utxos(&utxos, TEST_COINS_PER_UTXO_BYTE);
        assert_eq!(data.utxos.len(), 6);
        assert!(data.has_collateral);

        let tiers: Vec<ShelfTier> = data.utxos.iter().map(|u| u.tier).collect();
        assert_eq!(
            tiers,
            vec![
                ShelfTier::Collateral,
                ShelfTier::Liquid,
                ShelfTier::Clean,
                ShelfTier::Cluttered,
                ShelfTier::Bloated,
                ShelfTier::Dust,
            ]
        );
    }

    #[test]
    fn test_sort_within_tier_by_ada() {
        let utxos = vec![
            pure_ada_utxo(2_000_000),
            pure_ada_utxo(8_000_000),
            pure_ada_utxo(5_000_000),
        ];
        let data = classify_utxos(&utxos, TEST_COINS_PER_UTXO_BYTE);
        // 5 ADA and 8 ADA both qualify for Collateral, but only up to MAX_COLLATERAL (3).
        // Both fit under the cap, so both stay as Collateral.
        // 2 ADA is Liquid (below threshold).
        // Within Collateral: largest first in display order.
        let collateral: Vec<u64> = data
            .utxos
            .iter()
            .filter(|u| u.tier == ShelfTier::Collateral)
            .map(|u| u.lovelace)
            .collect();
        assert_eq!(collateral, vec![8_000_000, 5_000_000]);
    }

    #[test]
    fn test_collateral_cap_at_max() {
        // 5 pure-ADA UTxOs within collateral range (5-15 ADA) — only 3 smallest kept
        let utxos = vec![
            pure_ada_utxo(14_000_000), // 14 ADA — over cap, reclassified as Liquid
            pure_ada_utxo(12_000_000), // 12 ADA — over cap, reclassified as Liquid
            pure_ada_utxo(10_000_000), // 10 ADA — Collateral (3rd smallest)
            pure_ada_utxo(5_000_000),  // 5 ADA — Collateral (smallest)
            pure_ada_utxo(7_000_000),  // 7 ADA — Collateral (2nd smallest)
        ];
        let data = classify_utxos(&utxos, TEST_COINS_PER_UTXO_BYTE);
        assert!(data.has_collateral);

        let collateral: Vec<u64> = data
            .utxos
            .iter()
            .filter(|u| u.tier == ShelfTier::Collateral)
            .map(|u| u.lovelace)
            .collect();
        // Kept: 5M, 7M, 10M (3 smallest). Display order: largest first.
        assert_eq!(collateral, vec![10_000_000, 7_000_000, 5_000_000]);

        let liquid: Vec<u64> = data
            .utxos
            .iter()
            .filter(|u| u.tier == ShelfTier::Liquid)
            .map(|u| u.lovelace)
            .collect();
        // Excess: 12M, 14M reclassified as Liquid. Display order: largest first.
        assert_eq!(liquid, vec![14_000_000, 12_000_000]);
    }

    #[test]
    fn test_collateral_ceiling() {
        // UTxOs above the 15 ADA ceiling are Liquid, not Collateral
        let utxos = vec![
            pure_ada_utxo(34_000_000), // 34 ADA — above ceiling → Liquid
            pure_ada_utxo(5_000_000),  // 5 ADA — Collateral
        ];
        let data = classify_utxos(&utxos, TEST_COINS_PER_UTXO_BYTE);
        assert!(data.has_collateral);
        assert_eq!(
            data.utxos
                .iter()
                .filter(|u| u.tier == ShelfTier::Collateral)
                .count(),
            1
        );
        assert_eq!(
            data.utxos
                .iter()
                .find(|u| u.tier == ShelfTier::Collateral)
                .unwrap()
                .lovelace,
            5_000_000
        );
        assert_eq!(
            data.utxos
                .iter()
                .find(|u| u.tier == ShelfTier::Liquid)
                .unwrap()
                .lovelace,
            34_000_000
        );
    }
}
