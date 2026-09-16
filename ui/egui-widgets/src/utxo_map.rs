//! UTxO terrain map — a Voronoi-based wallet visualization.
//!
//! Each cell represents a `(utxo_ref, policy_id, asset_count)` tuple.
//! Cells are positioned via force-directed layout (UTxO cohesion + policy gravity)
//! then tessellated with Voronoi to produce an organic territory map.
//!
//! Water = free ADA, land = locked ADA. Policy colors create visual territories.

use std::collections::HashMap;

use egui::{Color32, Pos2, Response, Sense, Shape, Stroke, Ui, Vec2};
use voronoice::{BoundingBox, Point, VoronoiBuilder};

use crate::theme::{Ink, InkExt, TextSize, ThemeExt, Token};

// ============================================================================
// Public types
// ============================================================================

/// A single cell in the terrain map.
///
/// One UTxO with assets from 3 policies produces 3 cells.
/// A pure-ADA UTxO produces 1 cell with an empty `policy_id`.
#[derive(Clone, Debug)]
pub struct UtxoCell {
    /// UTxO reference, e.g. `"tx_hash#index"`.
    pub utxo_ref: String,
    /// 56-char hex policy ID, or empty string for pure-ADA UTxOs.
    pub policy_id: String,
    /// Number of assets from this policy in this UTxO.
    pub asset_count: u64,
    /// Proportional ADA attributed to this cell.
    pub lovelace_share: u64,
}

/// Immutable per-frame configuration.
pub struct UtxoMapConfig {
    /// Widget dimensions.
    pub size: Vec2,
    /// Water color (free ADA). Default: the theme's cyan.
    ///
    /// The literal this replaced was
    /// `from_rgba_premultiplied(125, 207, 255, 30)`, described as "cyan at low
    /// opacity" — but that constructor expects channels already scaled by
    /// alpha, so it blended additively and the blue channel **overflowed and
    /// clipped**: over the primary background it rendered `(148, 231, 255)`,
    /// i.e. a cyan *brighter* than the token, not a 12% tint of it. The comment
    /// described the intent; the pixels did the opposite.
    ///
    /// Taking the token itself reproduces what actually shipped (within ~20 per
    /// channel) while following the theme. Deliberately not the literal reading
    /// of the old comment — `Ink::Wash(Token::AccentCyan, 30)` is a barely-there
    /// tint that would make the sea vanish into the background, and this widget
    /// has no story to review such a change in.
    pub water_color: Ink,
    /// Border color between cells.
    pub border_color: Ink,
    /// Whether to show the consolidation toggle button.
    pub show_consolidation_toggle: bool,
}

impl Default for UtxoMapConfig {
    fn default() -> Self {
        Self {
            size: Vec2::new(400.0, 400.0),
            water_color: Ink::Token(Token::AccentCyan),
            border_color: Ink::Token(Token::Border),
            show_consolidation_toggle: false,
        }
    }
}

/// Data provided by the caller each frame.
pub struct UtxoMapData {
    /// The cells to visualize.
    pub cells: Vec<UtxoCell>,
    /// Total wallet ADA (lovelace). Used to compute water ratio.
    pub total_lovelace: u64,
    /// ADA locked in UTxOs that contain native assets.
    pub locked_lovelace: u64,
}

/// Persisted state across frames (caller-owned).
#[derive(Default)]
pub struct UtxoMapState {
    /// Cached layout from the last computation.
    layout: Option<ComputedLayout>,
    /// Fingerprint of input data — recompute layout when this changes.
    data_fingerprint: u64,
    /// Which UTxO is currently hovered.
    pub hovered_utxo: Option<String>,
}

/// Actions the widget can emit.
#[derive(Debug, Clone)]
pub enum UtxoMapAction {
    /// User hovered over a UTxO group.
    HoveredUtxo(String),
    /// User clicked a UTxO group.
    ClickedUtxo(String),
}

/// Widget response.
pub struct UtxoMapResponse {
    pub response: Response,
    pub action: Option<UtxoMapAction>,
}

// ============================================================================
// Internal types
// ============================================================================

/// A positioned cell after force-directed layout.
#[derive(Clone, Debug)]
struct PositionedCell {
    /// Index back into the input cells array.
    cell_idx: usize,
    /// Position in layout space (centered around origin).
    pos: [f64; 2],
}

/// Voronoi polygons in layout space + bounding box (min_x, min_y, max_x, max_y).
type TessellationResult = (Vec<Vec<(f64, f64)>>, (f64, f64, f64, f64));

/// Cached layout result — stores positioned cells and water count for tessellation.
struct ComputedLayout {
    cells: Vec<PositionedCell>,
    water_count: usize,
}

// ============================================================================
// Color helpers
// ============================================================================

/// Deterministic policy colour — an **identity** encoding.
///
/// # Why this needed a theme envelope rather than a palette
///
/// There is no Nth colour when N is "every policy on chain", so no ramp can
/// serve this: the hue has to come from the hash. What the theme *can* own is
/// the envelope — how saturated and how light those hues are allowed to be —
/// which is what keeps an arbitrary hue legible on this particular surface.
/// Hue is the hash's to choose; legibility is the theme's. See
/// [`crate::encoding::IdentityEnvelope`].
///
/// The old version walked its own HSL cube (72 hues × 3 saturations × 3
/// lightnesses). Varying lightness per-policy is the part that had to go: it
/// meant a third of all policies rendered at L 0.45 regardless of the surface
/// they landed on, so on a near-black theme some policies were simply hard to
/// see, determined by their name. The envelope fixes lightness and lets only
/// hue vary — `every_preset_identity_envelope_clears_its_surface` checks all 72.
pub fn policy_color(ui: &Ui, policy_id: &str) -> Color32 {
    let series = ui.tokens().series;
    if policy_id.is_empty() {
        return series.unobserved;
    }
    series.identity.color(simple_hash(policy_id) as u64)
}

// `hsl_to_rgb` lived here, walking a 72×3×3 HSL cube for policy colours. It is
// gone: `IdentityEnvelope` generates in Oklab, where equal steps are
// perceptually equal, and fixes lightness so the theme — not the policy's name —
// decides whether a colour clears the surface.

pub fn simple_hash(s: &str) -> usize {
    let mut h: usize = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as usize);
    }
    h
}

fn brighten(c: Color32, factor: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * factor).clamp(0.0, 255.0) as u8,
        (c.g() as f32 * factor).clamp(0.0, 255.0) as u8,
        (c.b() as f32 * factor).clamp(0.0, 255.0) as u8,
    )
}

// ============================================================================
// Data fingerprinting
// ============================================================================

fn fingerprint(data: &UtxoMapData) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for cell in &data.cells {
        for b in cell.utxo_ref.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        for b in cell.policy_id.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= cell.asset_count;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= data.total_lovelace;
    h = h.wrapping_mul(0x100000001b3);
    h ^= data.locked_lovelace;
    h
}

// ============================================================================
// Force-directed layout (self-contained, no external crate)
// ============================================================================

/// Run force-directed layout and return positioned cells.
fn compute_layout(cells: &[UtxoCell]) -> Vec<PositionedCell> {
    if cells.is_empty() {
        return vec![];
    }
    let n = cells.len();

    // Initial positions: scatter based on hash to break symmetry
    let mut pos: Vec<[f64; 2]> = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let h = simple_hash(&format!("{}{}{i}", cell.utxo_ref, cell.policy_id));
            let angle = (h % 1000) as f64 / 1000.0 * std::f64::consts::TAU;
            let r = ((h / 1000) % 100) as f64 / 100.0 * 50.0 + 10.0;
            [angle.cos() * r, angle.sin() * r]
        })
        .collect();

    let mut vel: Vec<[f64; 2]> = vec![[0.0; 2]; n];

    // Mass proportional to sqrt(asset_count)
    let mass: Vec<f64> = cells
        .iter()
        .map(|c| (c.asset_count as f64).sqrt().max(1.0))
        .collect();

    // Precompute groups
    let mut utxo_groups: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut policy_groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, cell) in cells.iter().enumerate() {
        utxo_groups
            .entry(cell.utxo_ref.as_str())
            .or_default()
            .push(i);
        if !cell.policy_id.is_empty() {
            policy_groups
                .entry(cell.policy_id.as_str())
                .or_default()
                .push(i);
        }
    }

    let iterations = 300;
    let dt = 0.035;
    let cooloff = 0.92;

    // Force parameters
    let repulsion_k = 2000.0;
    let utxo_cohesion = 6.0;
    let policy_gravity = 1.2;
    let center_strength = 2.0;

    for _ in 0..iterations {
        // 1. Repulsion: all pairs (O(n^2) — fine for <500 cells)
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i][0] - pos[j][0];
                let dy = pos[i][1] - pos[j][1];
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                let force = repulsion_k / dist_sq;
                let dist = dist_sq.sqrt();
                let fx = dx / dist * force * dt;
                let fy = dy / dist * force * dt;
                vel[i][0] += fx / mass[i];
                vel[i][1] += fy / mass[i];
                vel[j][0] -= fx / mass[j];
                vel[j][1] -= fy / mass[j];
            }
        }

        // 2. UTxO cohesion: pull toward group centroid
        for group in utxo_groups.values() {
            if group.len() < 2 {
                continue;
            }
            let mut cx = 0.0;
            let mut cy = 0.0;
            for &i in group {
                cx += pos[i][0];
                cy += pos[i][1];
            }
            cx /= group.len() as f64;
            cy /= group.len() as f64;
            for &i in group {
                let dx = cx - pos[i][0];
                let dy = cy - pos[i][1];
                vel[i][0] += dx * utxo_cohesion * dt;
                vel[i][1] += dy * utxo_cohesion * dt;
            }
        }

        // 3. Policy gravity: weaker pull toward policy centroid
        for group in policy_groups.values() {
            if group.len() < 2 {
                continue;
            }
            let mut cx = 0.0;
            let mut cy = 0.0;
            for &i in group {
                cx += pos[i][0];
                cy += pos[i][1];
            }
            cx /= group.len() as f64;
            cy /= group.len() as f64;
            for &i in group {
                let dx = cx - pos[i][0];
                let dy = cy - pos[i][1];
                vel[i][0] += dx * policy_gravity * dt;
                vel[i][1] += dy * policy_gravity * dt;
            }
        }

        // 4. Centering: gentle pull toward origin
        for i in 0..n {
            vel[i][0] -= pos[i][0] * center_strength * dt;
            vel[i][1] -= pos[i][1] * center_strength * dt;
        }

        // Integrate & damp
        for i in 0..n {
            pos[i][0] += vel[i][0] * dt;
            pos[i][1] += vel[i][1] * dt;
            vel[i][0] *= cooloff;
            vel[i][1] *= cooloff;
        }
    }

    pos.into_iter()
        .enumerate()
        .map(|(i, p)| PositionedCell {
            cell_idx: i,
            pos: p,
        })
        .collect()
}

// ============================================================================
// Voronoi tessellation
// ============================================================================

/// Build Voronoi from positioned cells + water points.
/// Returns polygon vertices in layout space and the bounding box.
fn tessellate(positioned: &[PositionedCell], water_count: usize) -> TessellationResult {
    if positioned.is_empty() {
        return (vec![], (0.0, 0.0, 0.0, 0.0));
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for pc in positioned {
        min_x = min_x.min(pc.pos[0]);
        min_y = min_y.min(pc.pos[1]);
        max_x = max_x.max(pc.pos[0]);
        max_y = max_y.max(pc.pos[1]);
    }

    let width = (max_x - min_x).max(1.0);
    let height = (max_y - min_y).max(1.0);
    let margin = width.max(height) * 0.3;
    let bbox = (
        min_x - margin,
        min_y - margin,
        max_x + margin,
        max_y + margin,
    );

    let mut sites: Vec<Point> = positioned
        .iter()
        .map(|pc| Point {
            x: pc.pos[0],
            y: pc.pos[1],
        })
        .collect();

    // Water cells distributed around the perimeter
    if water_count > 0 {
        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;
        let r = width.max(height) / 2.0 + margin * 0.6;
        for i in 0..water_count {
            let angle = (i as f64 / water_count as f64) * std::f64::consts::TAU;
            sites.push(Point {
                x: cx + angle.cos() * r,
                y: cy + angle.sin() * r,
            });
        }
    }

    if sites.len() < 2 {
        return (vec![vec![]; positioned.len()], bbox);
    }

    let bb_cx = (bbox.0 + bbox.2) / 2.0;
    let bb_cy = (bbox.1 + bbox.3) / 2.0;
    let bb_w = bbox.2 - bbox.0;
    let bb_h = bbox.3 - bbox.1;
    let bb = BoundingBox::new(Point { x: bb_cx, y: bb_cy }, bb_w, bb_h);

    let voronoi = match VoronoiBuilder::default()
        .set_sites(sites)
        .set_bounding_box(bb)
        .build()
    {
        Some(v) => v,
        None => return (vec![vec![]; positioned.len()], bbox),
    };

    let polygons: Vec<Vec<(f64, f64)>> = voronoi
        .iter_cells()
        .map(|cell| cell.iter_vertices().map(|p| (p.x, p.y)).collect())
        .collect();

    (polygons, bbox)
}

// ============================================================================
// Widget rendering
// ============================================================================

impl UtxoMapConfig {
    /// Render the UTxO terrain map.
    pub fn show(
        &self,
        ui: &mut Ui,
        data: &UtxoMapData,
        state: &mut UtxoMapState,
    ) -> UtxoMapResponse {
        let fp = fingerprint(data);

        // Recompute layout when data changes
        if state.data_fingerprint != fp || state.layout.is_none() {
            state.data_fingerprint = fp;

            if data.cells.is_empty() {
                state.layout = Some(ComputedLayout {
                    cells: vec![],
                    water_count: 0,
                });
            } else {
                let positioned = compute_layout(&data.cells);

                let free_lovelace = data.total_lovelace.saturating_sub(data.locked_lovelace);
                let free_ratio = free_lovelace as f64 / data.total_lovelace.max(1) as f64;
                let water_count = (positioned.len() as f64 * free_ratio * 1.5).round() as usize;
                let water_count = water_count.max(4).min(positioned.len() * 2);

                state.layout = Some(ComputedLayout {
                    cells: positioned,
                    water_count,
                });
            }
        }

        let (rect, response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 4.0, ui.tokens().color.bg_primary);

        let mut action = None;

        if let Some(layout) = &state.layout {
            if layout.cells.is_empty() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No UTxOs",
                    egui::FontId::proportional(ui.text_size(TextSize::Lg)),
                    ui.tokens().color.text_muted,
                );
            } else {
                let (polygons_layout, bounds) = tessellate(&layout.cells, layout.water_count);
                let land_count = layout.cells.len();

                // Map layout space -> screen space (aspect-preserving)
                let (bx0, by0, bx1, by1) = bounds;
                let bw = (bx1 - bx0).max(0.001);
                let bh = (by1 - by0).max(0.001);
                let scale = (rect.width() as f64 / bw).min(rect.height() as f64 / bh);
                let ox = rect.left() as f64 + (rect.width() as f64 - bw * scale) / 2.0;
                let oy = rect.top() as f64 + (rect.height() as f64 - bh * scale) / 2.0;

                let to_screen = |lx: f64, ly: f64| -> Pos2 {
                    Pos2::new(
                        (ox + (lx - bx0) * scale) as f32,
                        (oy + (ly - by0) * scale) as f32,
                    )
                };

                let mouse_pos = response.hover_pos();
                let mut hovered_cell_idx: Option<usize> = None;

                // Draw all polygons
                for (poly_idx, verts) in polygons_layout.iter().enumerate() {
                    if verts.len() < 3 {
                        continue;
                    }

                    let screen_verts: Vec<Pos2> =
                        verts.iter().map(|&(x, y)| to_screen(x, y)).collect();

                    let is_water = poly_idx >= land_count;

                    let fill = if is_water {
                        self.water_color.of(ui)
                    } else {
                        let cell = &data.cells[layout.cells[poly_idx].cell_idx];
                        policy_color(ui, &cell.policy_id)
                    };

                    if !is_water
                        && let Some(mp) = mouse_pos
                        && point_in_polygon(mp, &screen_verts)
                    {
                        hovered_cell_idx = Some(poly_idx);
                    }

                    painter.add(Shape::convex_polygon(
                        screen_verts,
                        fill,
                        Stroke::new(0.5_f32, self.border_color.of(ui)),
                    ));
                }

                // Highlight hovered UTxO group
                if let Some(hi) = hovered_cell_idx {
                    let hovered_utxo_ref = &data.cells[layout.cells[hi].cell_idx].utxo_ref;
                    state.hovered_utxo = Some(hovered_utxo_ref.clone());

                    for (poly_idx, verts) in polygons_layout.iter().enumerate() {
                        if poly_idx >= land_count || verts.len() < 3 {
                            continue;
                        }
                        let cell = &data.cells[layout.cells[poly_idx].cell_idx];
                        if cell.utxo_ref == *hovered_utxo_ref {
                            let screen_verts: Vec<Pos2> =
                                verts.iter().map(|&(x, y)| to_screen(x, y)).collect();
                            painter.add(Shape::convex_polygon(
                                screen_verts,
                                brighten(policy_color(ui, &cell.policy_id), 1.3),
                                Stroke::new(2.0_f32, ui.tokens().color.accent),
                            ));
                        }
                    }

                    // Tooltip
                    if response.hovered() {
                        let cell = &data.cells[layout.cells[hi].cell_idx];
                        let utxo_ref = cell.utxo_ref.clone();
                        let utxo_assets: u64 = data
                            .cells
                            .iter()
                            .filter(|c| c.utxo_ref == utxo_ref)
                            .map(|c| c.asset_count)
                            .sum();
                        let utxo_ada: u64 = data
                            .cells
                            .iter()
                            .filter(|c| c.utxo_ref == utxo_ref)
                            .map(|c| c.lovelace_share)
                            .sum();
                        let policy_count = data
                            .cells
                            .iter()
                            .filter(|c| c.utxo_ref == utxo_ref && !c.policy_id.is_empty())
                            .count();

                        response.clone().on_hover_ui(|ui| {
                            ui.label(
                                egui::RichText::new(truncate_ref(&utxo_ref))
                                    .monospace()
                                    .size(ui.text_size(TextSize::Base))
                                    .color(ui.tokens().color.text_secondary),
                            );
                            ui.label(format!("{utxo_assets} assets, {policy_count} policies"));
                            ui.label(format!("{:.2} ADA", utxo_ada as f64 / 1_000_000.0));
                        });
                    }

                    if response.clicked() {
                        action = Some(UtxoMapAction::ClickedUtxo(hovered_utxo_ref.clone()));
                    } else {
                        action = Some(UtxoMapAction::HoveredUtxo(hovered_utxo_ref.clone()));
                    }
                } else {
                    state.hovered_utxo = None;
                }
            }
        }

        UtxoMapResponse { response, action }
    }
}

// ============================================================================
// Geometry helpers
// ============================================================================

/// Point-in-polygon test (ray casting).
fn point_in_polygon(point: Pos2, polygon: &[Pos2]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn truncate_ref(utxo_ref: &str) -> String {
    if utxo_ref.len() > 20 {
        format!("{}...{}", &utxo_ref[..8], &utxo_ref[utxo_ref.len() - 8..])
    } else {
        utxo_ref.to_string()
    }
}

// ============================================================================
// UTxO → Map Data conversion
// ============================================================================

/// Convert decoded CIP-30 UTxOs into terrain map data.
///
/// Groups assets by policy within each UTxO, producing one cell per
/// `(utxo_ref, policy_id)` pair. Pure-ADA UTxOs produce a single cell
/// with an empty `policy_id`.
pub fn utxos_to_map_data(utxos: &[cardano_assets::utxo::UtxoApi]) -> UtxoMapData {
    let mut cells = Vec::new();
    let mut total_lovelace: u64 = 0;
    let mut locked_lovelace: u64 = 0;

    for utxo in utxos {
        let utxo_ref = format!("{}#{}", utxo.tx_hash, utxo.output_index);
        total_lovelace += utxo.lovelace;

        if utxo.assets.is_empty() {
            cells.push(UtxoCell {
                utxo_ref,
                policy_id: String::new(),
                asset_count: 0,
                lovelace_share: utxo.lovelace,
            });
        } else {
            locked_lovelace += utxo.lovelace;
            // Group assets by policy_id within this UTxO
            let mut by_policy: HashMap<&str, u64> = HashMap::new();
            for aq in &utxo.assets {
                *by_policy.entry(aq.asset_id.policy_id.as_str()).or_default() += 1;
            }
            let policy_count = by_policy.len() as u64;
            let lovelace_per_policy = utxo.lovelace / policy_count.max(1);
            for (policy_id, count) in by_policy {
                cells.push(UtxoCell {
                    utxo_ref: utxo_ref.clone(),
                    policy_id: policy_id.to_string(),
                    asset_count: count,
                    lovelace_share: lovelace_per_policy,
                });
            }
        }
    }

    UtxoMapData {
        cells,
        total_lovelace,
        locked_lovelace,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cells(utxo_count: usize, policies_per_utxo: usize) -> Vec<UtxoCell> {
        let mut cells = Vec::new();
        for u in 0..utxo_count {
            for p in 0..policies_per_utxo {
                cells.push(UtxoCell {
                    utxo_ref: format!("tx_{u}#0"),
                    policy_id: format!("policy_{p:0>56}"),
                    asset_count: 3,
                    lovelace_share: 2_000_000,
                });
            }
        }
        cells
    }

    #[test]
    fn test_layout_basic() {
        let cells = make_cells(5, 3);
        let positioned = compute_layout(&cells);
        assert_eq!(positioned.len(), 15);
        for pc in &positioned {
            assert!(pc.pos[0].is_finite());
            assert!(pc.pos[1].is_finite());
        }
    }

    #[test]
    fn test_layout_single_cell() {
        let cells = vec![UtxoCell {
            utxo_ref: "tx_0#0".into(),
            policy_id: String::new(),
            asset_count: 1,
            lovelace_share: 5_000_000,
        }];
        let positioned = compute_layout(&cells);
        assert_eq!(positioned.len(), 1);
    }

    #[test]
    fn test_layout_utxo_cohesion() {
        // Cells sharing a UTxO should cluster closer than unrelated cells
        let cells = vec![
            UtxoCell {
                utxo_ref: "tx_a#0".into(),
                policy_id: "policy_1".into(),
                asset_count: 5,
                lovelace_share: 2_000_000,
            },
            UtxoCell {
                utxo_ref: "tx_a#0".into(),
                policy_id: "policy_2".into(),
                asset_count: 3,
                lovelace_share: 2_000_000,
            },
            UtxoCell {
                utxo_ref: "tx_b#0".into(),
                policy_id: "policy_3".into(),
                asset_count: 4,
                lovelace_share: 2_000_000,
            },
        ];
        let positioned = compute_layout(&cells);
        let dist_same_utxo = {
            let a = positioned[0].pos;
            let b = positioned[1].pos;
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
        };
        let dist_diff_utxo = {
            let a = positioned[0].pos;
            let b = positioned[2].pos;
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
        };
        assert!(
            dist_same_utxo < dist_diff_utxo,
            "same-UTxO cells should be closer: {dist_same_utxo:.2} vs {dist_diff_utxo:.2}"
        );
    }

    #[test]
    fn test_tessellate_basic() {
        let cells = make_cells(5, 3);
        let positioned = compute_layout(&cells);
        let (polygons, _bounds) = tessellate(&positioned, 8);
        assert_eq!(polygons.len(), 15 + 8);
        for poly in &polygons {
            assert!(poly.len() >= 3, "polygon has {} vertices", poly.len());
        }
    }

    /// Identity colours get compared between screenshots and across sessions,
    /// so the same policy must keep its colour. Now needs a `Ui` because the
    /// envelope is a theme axis — the determinism claim is unchanged.
    #[test]
    fn test_policy_color_deterministic() {
        egui::__run_test_ui(|ui| {
            assert_eq!(policy_color(ui, "abc123"), policy_color(ui, "abc123"));
            assert_ne!(policy_color(ui, "abc123"), policy_color(ui, "zzz999"));
        });
    }

    #[test]
    fn test_point_in_polygon() {
        let square = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ];
        assert!(point_in_polygon(Pos2::new(5.0, 5.0), &square));
        assert!(!point_in_polygon(Pos2::new(15.0, 5.0), &square));
        assert!(!point_in_polygon(Pos2::new(-1.0, 5.0), &square));
    }

    #[test]
    fn test_fingerprint_changes() {
        let data1 = UtxoMapData {
            cells: make_cells(3, 2),
            total_lovelace: 10_000_000,
            locked_lovelace: 6_000_000,
        };
        let data2 = UtxoMapData {
            cells: make_cells(3, 3),
            total_lovelace: 10_000_000,
            locked_lovelace: 6_000_000,
        };
        assert_ne!(fingerprint(&data1), fingerprint(&data2));
    }

    #[test]
    fn test_utxos_to_map_data_pure_ada() {
        use cardano_assets::utxo::UtxoApi;

        let utxos = vec![UtxoApi {
            tx_hash: "aabb".into(),
            output_index: 0,
            lovelace: 10_000_000,
            assets: vec![],
            tags: vec![],
        }];
        let data = utxos_to_map_data(&utxos);
        assert_eq!(data.cells.len(), 1);
        assert_eq!(data.total_lovelace, 10_000_000);
        assert_eq!(data.locked_lovelace, 0);
        assert!(data.cells[0].policy_id.is_empty());
        assert_eq!(data.cells[0].utxo_ref, "aabb#0");
    }

    #[test]
    fn test_utxos_to_map_data_multi_policy() {
        use cardano_assets::AssetId;
        use cardano_assets::utxo::{AssetQuantity, UtxoApi};

        let utxos = vec![UtxoApi {
            tx_hash: "ccdd".into(),
            output_index: 1,
            lovelace: 6_000_000,
            assets: vec![
                AssetQuantity {
                    asset_id: AssetId::new_unchecked("policy_a".into(), "asset1".into()),
                    quantity: 1,
                },
                AssetQuantity {
                    asset_id: AssetId::new_unchecked("policy_a".into(), "asset2".into()),
                    quantity: 5,
                },
                AssetQuantity {
                    asset_id: AssetId::new_unchecked("policy_b".into(), "asset3".into()),
                    quantity: 1,
                },
            ],
            tags: vec![],
        }];
        let data = utxos_to_map_data(&utxos);
        // 2 policies → 2 cells
        assert_eq!(data.cells.len(), 2);
        assert_eq!(data.total_lovelace, 6_000_000);
        assert_eq!(data.locked_lovelace, 6_000_000);
        // policy_a has 2 assets, policy_b has 1
        let pa = data.cells.iter().find(|c| c.policy_id == "policy_a");
        let pb = data.cells.iter().find(|c| c.policy_id == "policy_b");
        assert!(pa.is_some());
        assert!(pb.is_some());
        assert_eq!(pa.unwrap().asset_count, 2);
        assert_eq!(pb.unwrap().asset_count, 1);
        assert_eq!(pa.unwrap().lovelace_share, 3_000_000);
    }
}
